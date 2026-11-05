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

/// The first `wsl.exe` call after boot has to start the VM.
const DETECT_TIMEOUT: Duration = Duration::from_secs(45);
const STARTING: &str =
    "WSL is still starting, so this message was not sent. Try again in a moment.";

/// One turn's answer about WSL, taken once so a turn can't mix two results.
pub(super) enum Detection {
    Ready(Environment),
    /// Not installed or not set up for Claude: use native Windows.
    Missing,
    /// No answer yet. Native Windows would ask someone signed in inside WSL to
    /// sign in again, so this never falls back.
    Starting,
}

impl Detection {
    /// The WSL environment to use, `None` for native Windows, or the message
    /// to try again.
    pub(super) fn environment(self) -> Result<Option<Environment>, String> {
        match self {
            Self::Ready(environment) => Ok(Some(environment)),
            Self::Missing => Ok(None),
            Self::Starting => Err(STARTING.into()),
        }
    }
}

pub(super) async fn detect_async() -> Detection {
    tokio::task::spawn_blocking(detect)
        .await
        .unwrap_or(Detection::Missing)
}

fn detect() -> Detection {
    if !cfg!(windows) {
        return Detection::Missing;
    }
    // Windows ships a `wsl.exe` stub even when WSL is not installed. Started
    // without a distribution, it can wait on a hidden "press any key to
    // install" prompt until the probe times out, which would then read as a
    // cold start on every turn. Only run it once the user has a distribution.
    if !wsl2_distribution_registered() {
        return Detection::Missing;
    }
    let Some(wsl) = crate::deps::resolve_command("wsl.exe") else {
        return Detection::Missing;
    };
    let script="import os,shutil,json; p=os.path.expanduser('~/.local/bin')+':/usr/local/bin:/usr/bin:/bin'; names={n:shutil.which(n,path=p) for n in ['claude','python3','bwrap','socat','setsid']}; k=open('/proc/sys/kernel/osrelease').read().lower(); print(json.dumps(dict(home=os.path.expanduser('~'),kernel=k,**names)))";
    let mut command = wsl.command(["--exec", "python3", "-c", script]);
    classify(
        wsl,
        crate::process::run_bounded_independent(&mut command, DETECT_TIMEOUT, 8192),
    )
}

/// The per-user WSL distribution registry, written by `wsl --install` and
/// `wsl --import` for both the inbox and the Store/MSI WSL.
#[cfg(any(windows, test))]
const LXSS: &str = r"Software\Microsoft\Windows\CurrentVersion\Lxss";

/// Whether the default distribution is registered and not WSL1. The probe
/// itself still checks the kernel, tools, and sign-in; this only decides
/// whether starting `wsl.exe` can do anything but offer to install WSL.
#[cfg(any(windows, test))]
fn default_distribution_registered(
    string: impl Fn(&str, &str) -> Option<String>,
    dword: impl Fn(&str, &str) -> Option<u32>,
) -> bool {
    let Some(id) = string(LXSS, "DefaultDistribution") else {
        return false;
    };
    let id = id.trim();
    if id.is_empty() || id.contains(['\\', '/']) {
        return false;
    }
    let key = format!(r"{LXSS}\{id}");
    string(&key, "DistributionName").is_some_and(|name| !name.trim().is_empty())
        && dword(&key, "Version") != Some(1)
}

#[cfg(windows)]
fn wsl2_distribution_registered() -> bool {
    default_distribution_registered(registry::string, registry::dword)
}

#[cfg(not(windows))]
fn wsl2_distribution_registered() -> bool {
    false
}

/// Minimal `HKEY_CURRENT_USER` value reads. Missing keys, missing values, and
/// unexpected types all read as `None`.
#[cfg(windows)]
mod registry {
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
    };

    const MAX_STRING_BYTES: u32 = 64 * 1024;

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    pub(super) fn string(key: &str, name: &str) -> Option<String> {
        let (key, name) = (wide(key), wide(name));
        let mut bytes = 0u32;
        // SAFETY: both names are NUL-terminated UTF-16 and outlive the call;
        // a null data pointer asks only for the value's size.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut bytes,
            )
        };
        if status != 0 || bytes == 0 || bytes > MAX_STRING_BYTES {
            return None;
        }
        let mut buffer = vec![0u16; (bytes as usize).div_ceil(2)];
        // SAFETY: `buffer` holds at least `bytes` bytes, which RegGetValueW
        // will not exceed; a value that grew since the size query fails.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if status != 0 {
            return None;
        }
        buffer.truncate(bytes as usize / 2);
        while buffer.last() == Some(&0) {
            buffer.pop();
        }
        String::from_utf16(&buffer).ok()
    }

    pub(super) fn dword(key: &str, name: &str) -> Option<u32> {
        let (key, name) = (wide(key), wide(name));
        let mut value = 0u32;
        let mut bytes = std::mem::size_of::<u32>() as u32;
        // SAFETY: names are NUL-terminated; `value` is a writable u32 and
        // `bytes` gives its exact size.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut value as *mut u32).cast(),
                &mut bytes,
            )
        };
        (status == 0).then_some(value)
    }
}

fn classify(
    wsl: crate::deps::ResolvedCommand,
    probe: Result<crate::process::BoundedOutput, String>,
) -> Detection {
    match probe {
        Ok(output) => parse(wsl, &output).map_or(Detection::Missing, Detection::Ready),
        Err(error) if error.starts_with("Subprocess timed out") => Detection::Starting,
        Err(_) => Detection::Missing,
    }
}

fn parse(
    wsl: crate::deps::ResolvedCommand,
    output: &crate::process::BoundedOutput,
) -> Option<Environment> {
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
        working: Option<&Path>,
    ) -> Result<(tokio::process::Command, Value, OwnedGroup), String> {
        let status = self.status()?;
        let scratch = self.path(directory.to_str().ok_or("Scratch path is not UTF-8")?)?;
        let cwd = match working {
            Some(path) => self.path(path.to_str().ok_or("Working path is not UTF-8")?)?,
            None => scratch.clone(),
        };
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
            &format!("{scratch}/wsl-launch.py"),
            &format!("{scratch}/wsl-launch.json"),
            &format!("{scratch}/wsl-group.json"),
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
    use std::collections::HashMap;

    /// A fake `HKEY_CURRENT_USER` keyed by (subkey, value name).
    struct Registry {
        strings: HashMap<(String, String), String>,
        dwords: HashMap<(String, String), u32>,
    }

    impl Registry {
        fn new() -> Self {
            Self {
                strings: HashMap::new(),
                dwords: HashMap::new(),
            }
        }
        fn distribution(mut self, id: &str, name: &str, version: Option<u32>) -> Self {
            let key = format!(r"{LXSS}\{id}");
            self.strings
                .insert((key.clone(), "DistributionName".into()), name.into());
            if let Some(version) = version {
                self.dwords.insert((key, "Version".into()), version);
            }
            self
        }
        fn with_default(mut self, id: &str) -> Self {
            self.strings
                .insert((LXSS.into(), "DefaultDistribution".into()), id.into());
            self
        }
        fn registered(&self) -> bool {
            default_distribution_registered(
                |key, name| self.strings.get(&(key.into(), name.into())).cloned(),
                |key, name| self.dwords.get(&(key.into(), name.into())).copied(),
            )
        }
    }

    #[test]
    fn the_inbox_wsl_stub_alone_is_never_started() {
        // Windows 11 without WSL installed: no Lxss key at all.
        assert!(!Registry::new().registered());
        // A distribution with no default, or a default that names nothing.
        let id = "{3e6b5c62-1a2b-4c3d-8e9f-0123456789ab}";
        assert!(!Registry::new()
            .distribution(id, "Ubuntu", Some(2))
            .registered());
        assert!(!Registry::new().with_default(id).registered());
        assert!(!Registry::new()
            .distribution(id, "Ubuntu", Some(2))
            .with_default(r"..\..\Run")
            .registered());
    }

    #[test]
    fn a_default_wsl2_distribution_is_probed_and_wsl1_is_not() {
        let id = "{3e6b5c62-1a2b-4c3d-8e9f-0123456789ab}";
        let wsl2 = Registry::new()
            .distribution(id, "Ubuntu", Some(2))
            .with_default(id);
        assert!(wsl2.registered());
        let wsl1 = Registry::new()
            .distribution(id, "Ubuntu", Some(1))
            .with_default(id);
        assert!(!wsl1.registered());
        // Without a recorded version the probe's kernel check decides.
        let unknown = Registry::new()
            .distribution(id, "Ubuntu", None)
            .with_default(id);
        assert!(unknown.registered());
    }

    #[test]
    fn recognizes_only_windows_paths_for_translation() {
        assert!(windows_path("C:\\Users\\Research\\paper.tex"));
        assert!(windows_path("\\\\?\\C:\\project"));
        assert!(!windows_path("/home/research/paper.tex"));
        assert!(!windows_path("~/.ssh"));
        assert!(!windows_path("example.org"));
    }

    #[cfg(unix)]
    #[test]
    fn a_cold_wsl_asks_for_a_retry_and_never_falls_back_to_native_windows() {
        let wsl = || crate::deps::resolve_command("sh").unwrap();
        let waiting = classify(wsl(), Err("Subprocess timed out after 45s".into()));
        let message = waiting.environment().err().unwrap();
        assert!(message.contains("Try again"));

        // Absence and failure are answers, so native Windows may take over.
        let unavailable = classify(wsl(), Err("Failed to start subprocess: none".into()));
        assert!(unavailable.environment().unwrap().is_none());
        let failed = Command::new("false").status().unwrap();
        let refused = classify(
            wsl(),
            Ok(crate::process::BoundedOutput {
                status: failed,
                stdout: Vec::new(),
                stderr: Vec::new(),
                stdout_truncated: false,
                stderr_truncated: false,
            }),
        );
        assert!(refused.environment().unwrap().is_none());
    }
}
