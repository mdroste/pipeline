//! Resolve and fingerprint the same executable and environment used at launch.
use super::adapters;
use super::file_identity::{hash_regular_file, hash_resolved_file};
use super::types::ExecutionProfile;
use crate::workbench::store::{WorkbenchError, WorkbenchResult};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

fn profile_environment<'a>(profile: &'a ExecutionProfile, key: &str) -> Option<&'a str> {
    profile
        .environment
        .as_object()?
        .iter()
        .find(|(k, _)| {
            if cfg!(windows) {
                k.eq_ignore_ascii_case(key)
            } else {
                *k == key
            }
        })
        .and_then(|(_, value)| value.as_str())
}

fn effective_path(profile: &ExecutionProfile) -> String {
    profile_environment(profile, "PATH")
        .map(str::to_owned)
        .unwrap_or_else(crate::env::full_path)
}

pub(crate) fn launch_manifest(profile: &ExecutionProfile) -> WorkbenchResult<Value> {
    let requested = Path::new(&profile.argv[0]);
    let path = effective_path(profile);
    let pathext = profile_environment(profile, "PATHEXT")
        .map(std::ffi::OsString::from)
        .or_else(|| std::env::var_os("PATHEXT"));
    let resolved = crate::deps::resolve_command_at(&profile.argv[0], std::ffi::OsStr::new(&path),
        Path::new(&profile.cwd), cfg!(windows), pathext.as_deref())
        .ok_or_else(|| WorkbenchError::invalid("Execution program is missing or not executable. Install it or configure its absolute path; arbitrary batch wrappers are unsupported."))?;
    let prefix: Vec<String> = resolved
        .prefix_args()
        .iter()
        .map(|arg| {
            arg.to_str()
                .map(str::to_owned)
                .ok_or_else(|| WorkbenchError::invalid("Execution arguments must be UTF-8"))
        })
        .collect::<WorkbenchResult<_>>()?;
    // Preserve the invocation path: Python discovers a venv beside that path,
    // and multicall binaries may also distinguish their aliases. Hash the target.
    let invocation_path = resolved.program().to_owned();
    let executable = fs::canonicalize(&invocation_path)
        .map_err(|e| WorkbenchError::storage("Failed to resolve execution program", e))?;
    let python_environment = if requested.file_name().is_some_and(|s| {
        s.to_string_lossy()
            .to_ascii_lowercase()
            .starts_with("python")
    }) {
        let config = invocation_path
            .parent()
            .and_then(Path::parent)
            .map(|p| p.join("pyvenv.cfg"));
        if let Some(config) = config.filter(|p| p.is_file()) {
            let (_, hash, size) = hash_resolved_file(&config, "Python environment")?;
            json!({"configuration":config,"hash":hash,"size":size})
        } else {
            Value::Null
        }
    } else {
        Value::Null
    };
    let (hash, size) = hash_regular_file(&executable).map_err(|problem| {
        WorkbenchError::invalid(format!("The execution program {}", problem.reason()))
    })?;
    let mut launch_files = Vec::new();
    let document_extensions = adapters::launch_file_extensions(&profile.adapter);
    for argument in prefix.iter().chain(profile.argv.iter().skip(1)) {
        let path = Path::new(argument);
        if !path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| {
                let ext = ext.to_ascii_lowercase();
                [
                    "sh", "bash", "zsh", "py", "r", "jl", "do", "pl", "rb", "tex", "js", "cjs",
                    "mjs",
                ]
                .contains(&ext.as_str())
                    || document_extensions.contains(&ext.as_str())
            })
        {
            continue;
        }
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            Path::new(&profile.cwd).join(path)
        };
        if path.is_file() {
            let (path, hash, size) = hash_resolved_file(&path, "launch script")?;
            launch_files.push(json!({"path":path,"hash":hash,"size":size}));
        }
    }
    let mut startup = Vec::new();
    const ZSH_STARTUP: [&str; 4] = [".zshenv", ".zprofile", ".zshrc", ".zlogin"];
    const BASH_STARTUP: [&str; 4] = [".bash_profile", ".bash_login", ".profile", ".bashrc"];
    // `/bin/zsh` keeps its original identity for every adapter. Other launcher
    // shells are recognized only for Stata custom launchers.
    let startup_files = if profile.argv[0] == "/bin/zsh"
        || (adapters::waits_for_stata_launcher(&profile.adapter, &profile.argv)
            && profile.argv[0].ends_with("/zsh"))
    {
        Some((ZSH_STARTUP, true))
    } else if adapters::waits_for_stata_launcher(&profile.adapter, &profile.argv) {
        Some((BASH_STARTUP, false))
    } else {
        None
    };
    if let Some((names, honors_zdotdir)) = startup_files {
        let zdotdir = honors_zdotdir
            .then(|| {
                profile
                    .environment
                    .get("ZDOTDIR")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .or_else(|| std::env::var_os("ZDOTDIR").map(PathBuf::from))
            })
            .flatten();
        if let Some(home) = zdotdir.or_else(dirs::home_dir) {
            for name in names {
                let path = home.join(name);
                startup.push(if path.is_file() {
                    let (_, hash, size) = hash_resolved_file(&path, "shell startup file")?;
                    json!({"path":path,"hash":hash,"size":size})
                } else {
                    json!({"path":path,"missing":true})
                });
            }
        }
    }
    let mut manifest = json!({"executable":executable,"invocationPath":invocation_path,"prefixArgs":prefix,"path":path,"pathExt":pathext.map(|s| s.to_string_lossy().into_owned()),"pythonEnvironment":python_environment,"hash":hash,"size":size,"shellStartupFiles":startup,"launchFiles":launch_files,"coverage":"executable, directly referenced launch scripts, known shell startup files and declared inputs; shell-string and transitive dependencies require explicit input declarations"});
    // Structured adapters add their environment files. Profiles without one
    // keep the exact launch identity they were authorized with.
    if let Some(names) = adapters::environment_files(&profile.adapter, &profile.argv) {
        if let Some(script) =
            adapters::launch_script(&profile.adapter, &profile.argv, cfg!(windows))
        {
            if !Path::new(&profile.cwd).join(&script).is_file() {
                return Err(WorkbenchError::invalid(format!(
                    "{script} was not found in the working folder. Create it or correct the profile before reviewing the command."
                )));
            }
        }
        manifest["environmentIdentity"] =
            super::toolchains::environment_identity(Path::new(&profile.cwd), names)?;
    }
    Ok(manifest)
}
