//! Windows Claude shell transport. WSL2 owns Linux authentication and sandbox
//! dependencies. All arguments remain argv entries; no prompt enters shell code.
//! A Linux supervisor records its process-group identity for cancellation.
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(Clone)]
pub(super) struct Environment {
    wsl: crate::deps::ResolvedCommand,
    home: String,
    claude: String,
    python: String,
}

pub(super) async fn detect_async() -> Option<Environment> {
    tokio::task::spawn_blocking(detect).await.ok().flatten()
}

pub(super) fn detect() -> Option<Environment> {
    if !cfg!(windows) {
        return None;
    }
    let wsl = crate::deps::resolve_command("wsl.exe")?;
    let script="import os,shutil,json; p=os.path.expanduser('~/.local/bin')+':/usr/local/bin:/usr/bin:/bin'; names={n:shutil.which(n,path=p) for n in ['claude','python3','bwrap','socat','setsid']}; k=open('/proc/sys/kernel/osrelease').read().lower(); print(json.dumps(dict(home=os.path.expanduser('~'),kernel=k,**names)))";
    let mut command = wsl.command(["--exec", "python3", "-c", script]);
    let output =
        crate::process::run_bounded_independent(&mut command, Duration::from_secs(5), 8192).ok()?;
    if !output.status.success() || output.stdout_truncated {
        return None;
    }
    let value: Value = serde_json::from_slice(&output.stdout).ok()?;
    if !value["kernel"].as_str()?.contains("wsl2")
        || ["claude", "python3", "bwrap", "socat"]
            .iter()
            .any(|key| value[key].as_str().is_none_or(|v| !v.starts_with('/')))
    {
        return None;
    }
    Some(Environment {
        wsl,
        home: value["home"].as_str()?.into(),
        claude: value["claude"].as_str()?.into(),
        python: value["python3"].as_str()?.into(),
    })
}

fn windows_path(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':' || value.starts_with("\\\\")
}
impl Environment {
    fn path(&self, value: &str) -> Result<String, String> {
        if !windows_path(value) {
            return Ok(value.into());
        }
        let value = value.strip_prefix("\\\\?\\").unwrap_or(value);
        let mut command = self.wsl.command(["--exec", "wslpath", "-a", "-u", value]);
        let output =
            crate::process::run_bounded_independent(&mut command, Duration::from_secs(5), 8192)?;
        let path = String::from_utf8(output.stdout)
            .map_err(|_| "WSL returned a non-UTF-8 path")?
            .trim()
            .to_string();
        if !output.status.success()
            || !path.starts_with('/')
            || path.contains('\n')
            || path.contains('\0')
        {
            return Err("This project path cannot be mapped into WSL2".into());
        }
        Ok(path)
    }
    fn translated(&self, value: &mut Value) -> Result<(), String> {
        match value {
            Value::String(text) if windows_path(text) => *text = self.path(text)?,
            Value::Array(values) => {
                for value in values {
                    self.translated(value)?;
                }
            }
            Value::Object(values) => {
                for value in values.values_mut() {
                    self.translated(value)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    fn command(&self, args: &[String], cwd: &str) -> Command {
        let mut command = self
            .wsl
            .command(["--cd", cwd, "--exec", "/usr/bin/env", "-i"]);
        command
            .args([
                format!("HOME={}", self.home),
                format!("PATH={}/.local/bin:/usr/local/bin:/usr/bin:/bin", self.home),
                "LANG=C.UTF-8".into(),
            ])
            .arg(&self.claude)
            .args(args);
        crate::agent_runtime::claude::configure_subscription(&mut command);
        command
    }
    pub(super) fn status(&self) -> Result<Value, String> {
        let auth_args =
            crate::agent_runtime::claude::subscription_args(&["auth".into(), "status".into()]);
        let output = crate::process::run_bounded_independent(
            &mut self.command(&auth_args, "/tmp"),
            Duration::from_secs(15),
            64 * 1024,
        )?;
        let status: Value = serde_json::from_slice(&output.stdout).map_err(|_| {
            "Could not read Claude's WSL2 sign-in. Run wsl --exec claude auth login"
        })?;
        crate::agent_runtime::claude::validate_subscription_status(&status,output.status.success()).map_err(|_|"Sign in to Claude inside WSL2: wsl --exec claude auth login. Native Windows sign-in is separate.")?;
        Ok(status)
    }
    pub(super) fn prepare(
        &self,
        sandbox: Option<&Value>,
        args: &[String],
        directory: &Path,
    ) -> Result<(tokio::process::Command, Value, OwnedGroup), String> {
        let status = self.status()?;
        let cwd = self.path(directory.to_str().ok_or("Scratch path is not UTF-8")?)?;
        let mut sandbox = sandbox.cloned();
        if let Some(value) = &mut sandbox {
            self.translated(value)?;
        }
        let mut args = args.to_vec();
        for flag in ["--system-prompt-file", "--append-system-prompt-file"] {
            if let Some(i) = args.iter().position(|v| v == flag) {
                args[i + 1] = self.path(&args[i + 1])?;
            }
        }
        if let Some(root) = sandbox
            .as_ref()
            .and_then(|s| s["filesystem"]["allowRead"][0].as_str())
        {
            // The user-facing project path remains a Windows path; Bash needs
            // this translated root. The instruction file is app-owned scratch.
            if let Some(i) = args.iter().position(|v| v == "--append-system-prompt-file") {
                let native = directory.join("append-system-prompt.txt");
                use std::io::Write;
                std::fs::OpenOptions::new().append(true).open(native).and_then(|mut file|writeln!(file,"\nFor Bash in WSL2, the working root is {root}. Project tools still use project-relative paths.")).map_err(|e|e.to_string())?;
                let _ = &args[i + 1];
            }
        }
        let args = crate::agent_runtime::claude::sandbox_subscription_args(sandbox.as_ref(), &args);
        let config = json!({"cwd":cwd,"executable":self.claude,"args":args,"home":self.home});
        std::fs::write(
            directory.join("wsl-launch.json"),
            serde_json::to_vec(&config).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(
            directory.join("wsl-launch.py"),
            include_str!("wsl-launch.py"),
        )
        .map_err(|e| e.to_string())?;
        let mut command = self.wsl.command([
            "--exec",
            &self.python,
            &format!("{cwd}/wsl-launch.py"),
            &format!("{cwd}/wsl-launch.json"),
            &format!("{cwd}/wsl-group.json"),
        ]);
        crate::agent_runtime::claude::configure_subscription(&mut command);
        let mut command = tokio::process::Command::from(command);
        command.kill_on_drop(true);
        Ok((
            command,
            status,
            OwnedGroup {
                wsl: self.wsl.clone(),
                python: self.python.clone(),
                record: directory.join("wsl-group.json"),
            },
        ))
    }
}

pub(super) struct OwnedGroup {
    wsl: crate::deps::ResolvedCommand,
    python: String,
    record: PathBuf,
}
impl OwnedGroup {
    pub(super) fn stop(&self) {
        let Some(record) = std::fs::read(&self.record)
            .ok()
            .filter(|v| v.len() < 1024)
            .and_then(|v| serde_json::from_slice::<Value>(&v).ok())
        else {
            return;
        };
        let (Some(pid), Some(start)) = (
            record["pid"].as_u64().filter(|v| *v > 1),
            record["start"].as_str(),
        ) else {
            return;
        };
        if !start.bytes().all(|b| b.is_ascii_digit()) {
            return;
        }
        let code="import os,sys,signal; p=int(sys.argv[1]); s=open('/proc/'+str(p)+'/stat').read(); actual=s[s.rfind(')')+2:].split()[19]; actual==sys.argv[2] and os.getpgid(p)==p and os.killpg(p,signal.SIGKILL)";
        let mut command =
            self.wsl
                .command(["--exec", &self.python, "-c", code, &pid.to_string(), start]);
        let _ = crate::process::run_bounded_independent(&mut command, Duration::from_secs(5), 1024);
        let _ = std::fs::remove_file(&self.record);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_only_windows_paths_for_translation() {
        assert!(windows_path("C:\\Users\\Research\\paper.tex"));
        assert!(windows_path("\\\\?\\C:\\project"));
        assert!(!windows_path("/home/research/paper.tex"));
        assert!(!windows_path("~/.ssh"));
        assert!(!windows_path("example.org"));
    }
}
