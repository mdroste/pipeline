//! Optional per-user command installation. Only the tiny launcher is copied;
//! the app remains the single owner of the execution engine and PDF resources.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const OWNER: &str = "pipeline-cli.install.json";
const LOCATOR: &str = "pipeline-cli-app";
const MAX_LAUNCHER_BYTES: u64 = 2_000_000;
static OPERATION: Mutex<()> = Mutex::new(());

#[derive(Serialize)]
pub struct CliToolsStatus {
    pub installed: bool,
    pub available: bool,
    pub reason: Option<String>,
    pub path: String,
    pub command: String,
    pub terminal_setup: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ownership {
    schema: u32,
    component: String,
    sha256: String,
}

fn name() -> &'static str {
    if cfg!(windows) {
        "pipeline-cli.exe"
    } else {
        "pipeline-cli"
    }
}

fn bin_dir() -> Result<PathBuf, String> {
    Ok(dirs::home_dir()
        .ok_or("Cannot determine home directory")?
        .join(".pipeline/bin"))
}

fn app_paths() -> Result<(PathBuf, PathBuf), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let source = executable
        .parent()
        .ok_or("Missing application directory")?
        .join(name());
    // AppImage mount paths disappear when the app exits. The stable outer
    // AppImage mounts itself and forwards our headless switch on invocation.
    let application = if cfg!(target_os = "linux") {
        std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute() && path.is_file())
            .unwrap_or(executable)
    } else {
        executable
    };
    Ok((application, source))
}

fn regular(path: &Path, max: u64) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && m.len() > 0 && m.len() <= max)
}

fn ownership(bin: &Path) -> Option<Ownership> {
    let file = bin.join(OWNER);
    if !regular(&file, 4096) {
        return None;
    }
    let owner: Ownership = serde_json::from_slice(&std::fs::read(file).ok()?).ok()?;
    (owner.schema == 1 && owner.component == "pipeline-cli" && owner.sha256.len() == 64)
        .then_some(owner)
}

fn digest(path: &Path) -> Result<String, String> {
    if !regular(path, MAX_LAUNCHER_BYTES) {
        return Err("Invalid command launcher".into());
    }
    Ok(format!(
        "{:x}",
        Sha256::digest(std::fs::read(path).map_err(|e| e.to_string())?)
    ))
}

fn source_check(source: &Path) -> Result<(), String> {
    if !regular(source, MAX_LAUNCHER_BYTES) {
        return Err("Command installation needs a packaged Pipeline app. Source builds can run cargo run --bin pipeline-cli.".into());
    }
    let output = crate::process::run_bounded(
        std::process::Command::new(source).arg("--pipeline-launcher-info"),
        std::time::Duration::from_secs(5),
        1024,
    )
    .map_err(|e| format!("Cannot check command launcher: {e}"))?;
    if !output.status.success()
        || output.stdout_truncated
        || output.stdout != b"pipeline-cli-launcher-v1\n"
    {
        return Err("The packaged command launcher is invalid. Reinstall Pipeline.".into());
    }
    Ok(())
}

fn quote(value: &str) -> String {
    format!(
        "'{}'",
        if cfg!(windows) {
            value.replace('\'', "''")
        } else {
            value.replace('\'', "'\"'\"'")
        }
    )
}

pub fn status() -> Result<CliToolsStatus, String> {
    let bin = bin_dir()?;
    let (_, source) = app_paths()?;
    let error = source_check(&source).err();
    let target = bin.join(name());
    let installed = ownership(&bin)
        .is_some_and(|owner| digest(&target).is_ok_and(|hash| hash == owner.sha256))
        && regular(&bin.join(LOCATOR), 32_768);
    let quoted = quote(&target.to_string_lossy());
    let directory = quote(&bin.to_string_lossy());
    Ok(CliToolsStatus {
        installed,
        available: error.is_none(),
        reason: error,
        path: target.to_string_lossy().into_owned(),
        command: format!("{}{quoted} --help", if cfg!(windows) { "& " } else { "" }),
        terminal_setup: if cfg!(windows) {
            format!("$env:Path = {directory} + ';' + $env:Path")
        } else {
            format!("export PATH={directory}:\"$PATH\"")
        },
    })
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut temp =
        tempfile::NamedTempFile::new_in(path.parent().ok_or("Missing command directory")?)
            .map_err(|e| e.to_string())?;
    temp.write_all(bytes).map_err(|e| e.to_string())?;
    crate::safety::sync_file(temp.as_file()).map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

fn install_at(bin: &Path, application: &Path, source: &Path) -> Result<(), String> {
    let app = application
        .to_str()
        .filter(|s| !s.contains(['\n', '\r']))
        .ok_or("Unsupported application path")?;
    if !application.is_absolute() {
        return Err("Application path must be absolute".into());
    }
    if bin.exists() && !std::fs::symlink_metadata(bin).is_ok_and(|m| m.is_dir()) {
        return Err("The command directory is not a regular directory".into());
    }
    std::fs::create_dir_all(bin).map_err(|e| e.to_string())?;
    let target = bin.join(name());
    let owned = ownership(bin).is_some();
    for path in [&target, &bin.join(LOCATOR), &bin.join(OWNER)] {
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            if !owned || !meta.is_file() {
                return Err(format!(
                    "Refusing to replace an unowned command file: {}",
                    path.display()
                ));
            }
        }
    }
    let hash = digest(source)?;
    // Claim only our three reserved files before activation; a failed copy can
    // be repaired, and the previous launcher continues to work.
    write_atomic(
        &bin.join(OWNER),
        &serde_json::to_vec(&Ownership {
            schema: 1,
            component: "pipeline-cli".into(),
            sha256: hash.clone(),
        })
        .map_err(|e| e.to_string())?,
    )?;
    write_atomic(&bin.join(LOCATOR), app.as_bytes())?;
    if digest(&target).ok().as_ref() != Some(&hash) {
        let temp = tempfile::NamedTempFile::new_in(bin).map_err(|e| e.to_string())?;
        std::fs::copy(source, temp.path()).map_err(|e| e.to_string())?;
        std::fs::set_permissions(
            temp.path(),
            std::fs::metadata(source)
                .map_err(|e| e.to_string())?
                .permissions(),
        )
        .map_err(|e| e.to_string())?;
        crate::safety::sync_file(temp.as_file()).map_err(|e| e.to_string())?;
        temp.persist(&target)
            .map_err(|e| format!("Close running CLI commands, then choose Repair: {e}"))?;
    }
    Ok(())
}

pub fn install() -> Result<CliToolsStatus, String> {
    let _guard = OPERATION
        .lock()
        .map_err(|_| "Command installation is busy")?;
    let (app, source) = app_paths()?;
    source_check(&source)?;
    install_at(&bin_dir()?, &app, &source)?;
    status()
}

fn uninstall_at(bin: &Path) -> Result<(), String> {
    if ownership(bin).is_none() {
        return Err("No Pipeline-owned command installation was found".into());
    }
    let paths = [name(), LOCATOR, OWNER].map(|name| bin.join(name));
    for path in &paths {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => return Err("Refusing to remove a replaced command file".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    for path in paths {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

pub fn uninstall() -> Result<CliToolsStatus, String> {
    let _guard = OPERATION
        .lock()
        .map_err(|_| "Command installation is busy")?;
    uninstall_at(&bin_dir()?)?;
    status()
}

/// No opt-in means no install. Refresh only an existing owned launcher after
/// app relocation/update, so its path and signed bytes follow this app.
pub fn refresh_installed() {
    if bin_dir().is_ok_and(|bin| ownership(&bin).is_some()) {
        if let Err(error) = install() {
            eprintln!("Command launcher refresh: {error}");
        }
    }
}

pub fn run_headless_if_requested() -> bool {
    if std::env::args_os().nth(1).as_deref() != Some(std::ffi::OsStr::new("--pipeline-cli")) {
        return false;
    }
    #[cfg(windows)]
    attach_parent_console();
    crate::cli::run(std::env::args().skip(2).collect())
}

#[cfg(windows)]
fn attach_parent_console() {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(process: u32) -> i32;
        fn GetStdHandle(kind: u32) -> *mut c_void;
        fn SetStdHandle(kind: u32, handle: *mut c_void) -> i32;
        fn GetFileType(handle: *mut c_void) -> u32;
    }
    // Preserve redirected pipes/files when attaching for console I/O and
    // Ctrl+C. Console handles are supplied by AttachConsole itself.
    unsafe {
        let handles = [(-10i32) as u32, (-11i32) as u32, (-12i32) as u32].map(|kind| {
            let handle = GetStdHandle(kind);
            (kind, handle, GetFileType(handle))
        });
        if AttachConsole(u32::MAX) != 0 {
            for (kind, handle, file_type) in handles {
                if file_type == 1 || file_type == 3 {
                    SetStdHandle(kind, handle);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "cli_tools_tests.rs"]
mod tests;
