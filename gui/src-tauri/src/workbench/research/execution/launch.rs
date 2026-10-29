//! Resolve and fingerprint the same executable and environment used at launch.
use super::types::ExecutionProfile;
use crate::workbench::research::papers::{hash_file, hash_resolved_file};
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
    let (hash, size) = hash_file(&executable)?;
    let mut launch_files = Vec::new();
    for argument in prefix.iter().chain(profile.argv.iter().skip(1)) {
        let path = Path::new(argument);
        if !path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| {
                [
                    "sh", "bash", "zsh", "py", "r", "jl", "do", "pl", "rb", "tex", "js", "cjs",
                    "mjs",
                ]
                .contains(&ext.to_ascii_lowercase().as_str())
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
    if profile.argv[0] == "/bin/zsh" {
        if let Some(home) = profile
            .environment
            .get("ZDOTDIR")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("ZDOTDIR").map(PathBuf::from))
            .or_else(dirs::home_dir)
        {
            for name in [".zshenv", ".zprofile", ".zshrc", ".zlogin"] {
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
    Ok(
        json!({"executable":executable,"invocationPath":invocation_path,"prefixArgs":prefix,"path":path,"pathExt":pathext.map(|s| s.to_string_lossy().into_owned()),"pythonEnvironment":python_environment,"hash":hash,"size":size,"shellStartupFiles":startup,"launchFiles":launch_files,"coverage":"executable, directly referenced launch scripts, known shell startup files and declared inputs; shell-string and transitive dependencies require explicit input declarations"}),
    )
}
