//! Workspace-specific native configuration.
pub(crate) use crate::agent_runtime::codex::process::{
    capture_stderr, prepare_isolated_command, OwnedProcess,
};
use std::path::Path;
fn toml_basic_string(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() + 2);
    encoded.push('"');
    for character in value.chars() {
        match character {
            '\\' => encoded.push_str("\\\\"),
            '"' => encoded.push_str("\\\""),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            value if value.is_control() => encoded.push_str(&format!("\\u{:04x}", value as u32)),
            value => encoded.push(value),
        }
    }
    encoded.push('"');
    encoded
}

pub(crate) fn write_runtime_config(codex_home: &Path, launcher: &Path) -> Result<(), String> {
    std::fs::create_dir_all(codex_home)
        .map_err(|error| format!("Failed to create Workbench Codex home: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(codex_home, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("Failed to secure Workbench Codex home: {error}"))?;
    }
    let path = codex_home.join("config.toml");
    let pending = codex_home.join("config.toml.pending");
    let launcher = launcher
        .to_str()
        .ok_or("Codex launcher path is not valid UTF-8")?;
    let canonical_launcher = std::fs::canonicalize(launcher)
        .map_err(|error| format!("Failed to resolve Codex launcher: {error}"))?;
    let canonical_launcher = canonical_launcher
        .to_str()
        .ok_or("Resolved Codex launcher path is not valid UTF-8")?;
    let launcher_permissions = if launcher == canonical_launcher {
        format!("{} = \"read\"\n", toml_basic_string(launcher))
    } else {
        format!(
            "{} = \"read\"\n{} = \"read\"\n",
            toml_basic_string(launcher),
            toml_basic_string(canonical_launcher),
        )
    };
    let mut config = format!(
        "cli_auth_credentials_store = \"file\"\n\
         default_permissions = \"workbench-inspect\"\n\
         \n\
         [permissions.workbench-inspect]\n\
         description = \"Workspace inspect mode\"\n\
         [permissions.workbench-inspect.filesystem]\n\
         \":minimal\" = \"read\"\n{}\
         [permissions.workbench-inspect.filesystem.\":workspace_roots\"]\n\
         \".\" = \"read\"\n\
         [permissions.workbench-inspect.network]\n\
         enabled = false\n\
         \n\
         [permissions.workbench-inspect-network]\n\
         description = \"Workspace inspect mode with command network\"\n\
         [permissions.workbench-inspect-network.filesystem]\n\
         \":minimal\" = \"read\"\n{}\
         [permissions.workbench-inspect-network.filesystem.\":workspace_roots\"]\n\
         \".\" = \"read\"\n\
         [permissions.workbench-inspect-network.network]\n\
         enabled = true\n\
         \n\
         [permissions.workbench-edit]\n\
         description = \"Workspace edit mode\"\n\
         [permissions.workbench-edit.filesystem]\n\
         \":minimal\" = \"read\"\n{}\
         [permissions.workbench-edit.filesystem.\":workspace_roots\"]\n\
         \".\" = \"write\"\n\
         [permissions.workbench-edit.network]\n\
         enabled = false\n\
         \n\
         [permissions.workbench-edit-network]\n\
         description = \"Workspace edit mode with command network\"\n\
         [permissions.workbench-edit-network.filesystem]\n\
         \":minimal\" = \"read\"\n{}\
         [permissions.workbench-edit-network.filesystem.\":workspace_roots\"]\n\
         \".\" = \"write\"\n\
         [permissions.workbench-edit-network.network]\n\
         enabled = true\n",
        launcher_permissions, launcher_permissions, launcher_permissions, launcher_permissions,
    );
    // Same confined filesystem rules as ordinary Workspace, with no network.
    // Only host-created discovery role records can select these profiles.
    for (profile, access) in [("discovery-inspect", "read"), ("discovery-edit", "write")] {
        config.push_str(&format!("\n[permissions.{profile}]\ndescription = \"Bounded research automation\"\n[permissions.{profile}.filesystem]\n\":minimal\" = \"read\"\n{launcher_permissions}[permissions.{profile}.filesystem.\":workspace_roots\"]\n\".\" = \"{access}\"\n[permissions.{profile}.network]\nenabled = false\n"));
    }
    std::fs::write(&pending, config)
        .map_err(|error| format!("Failed to create Workbench Codex config: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&pending, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("Failed to secure Workbench Codex config: {error}"))?;
    }
    std::fs::rename(&pending, &path)
        .map_err(|error| format!("Failed to publish Workbench Codex config: {error}"))?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::agent_runtime::codex::process::SECRET_ENVIRONMENT_KEYS;
    use std::time::Duration;
    use tokio::io::{AsyncBufReadExt as _, BufReader};

    fn process_is_running(pid: u32) -> bool {
        if pid == 0 || pid > i32::MAX as u32 {
            return false;
        }
        (unsafe { libc::kill(pid as i32, 0) == 0 })
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    #[test]
    fn forced_shutdown_reaps_the_owned_process_group_but_not_pipeline_children() {
        // This test deliberately invokes global Workflow cancellation. Run it
        // in a separate harness process so it cannot kill unrelated dependency
        // probes that the Rust test runner starts concurrently.
        const CHILD_MARKER: &str = "PIPELINE_CODEX_PROCESS_TEST_CHILD";
        if std::env::var_os(CHILD_MARKER).is_none() {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command.args([
                "--exact",
                "workbench::codex::process::tests::forced_shutdown_reaps_the_owned_process_group_but_not_pipeline_children",
                "--nocapture",
            ]).env(CHILD_MARKER, "1");
            let result =
                crate::process::run_bounded(&mut command, Duration::from_secs(15), 64 * 1024)
                    .unwrap();
            assert!(
                result.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut command = std::process::Command::new("/bin/sh");
            command.args(["-c", "sleep 30 & child=$!; printf '%s\\n' \"$child\"; wait"]);
            let (process, pipes) = OwnedProcess::spawn(command).unwrap();
            let leader = process.pid();
            let mut reader = BufReader::new(pipes.stdout);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let descendant = line.trim().parse::<u32>().unwrap();
            assert!(process_is_running(leader));
            assert!(process_is_running(descendant));
            crate::commands::lifecycle::kill_all_children();
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert!(
                process_is_running(leader) && process_is_running(descendant),
                "ordinary Pipeline cancellation must not terminate the independent Workbench tree"
            );
            process.terminate().await;
            for _ in 0..40 {
                if !process_is_running(leader) && !process_is_running(descendant) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            assert!(!process_is_running(leader));
            assert!(!process_is_running(descendant));
        });
    }

    #[test]
    fn isolated_environment_removes_provider_credentials() {
        let temporary = tempfile::tempdir().unwrap();
        let resolved = crate::deps::ResolvedCommand::direct("/bin/echo".into());
        let command =
            prepare_isolated_command(&resolved, temporary.path(), temporary.path()).unwrap();
        let removals: Vec<_> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(key, _)| key.to_string_lossy().to_string())
            .collect();
        for key in SECRET_ENVIRONMENT_KEYS {
            assert!(removals.iter().any(|removed| removed == key));
        }
    }
}
