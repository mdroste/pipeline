//! Conversation-owned managed Node and locked npm packages. Setup is lazy,
//! cancellable, and publishes only a complete, verified staging directory.
//! Versions live side by side; a failed upgrade cannot damage a working install.
//! Never resolve Node/npm from PATH or execute a model-selected installer.
mod archive;
mod process;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    time::Duration,
};

const MANIFEST: &str = include_str!("../../resources/conversation-tools/node.json");
const PACKAGE: &str = include_str!("../../resources/conversation-tools/package.json");
const LOCK: &str = include_str!("../../resources/conversation-tools/package-lock.json");
static INSTALL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static PHASE: Mutex<Option<String>> = Mutex::new(None);
static CANCEL: OnceLock<tokio::sync::watch::Sender<u64>> = OnceLock::new();
fn cancellation() -> &'static tokio::sync::watch::Sender<u64> {
    CANCEL.get_or_init(|| tokio::sync::watch::channel(0).0)
}
pub(crate) fn phase() -> Option<String> {
    PHASE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}
pub(crate) fn set_phase(value: &str) {
    *PHASE.lock().unwrap_or_else(|e| e.into_inner()) = Some(value.into());
}
struct Progress;
impl Drop for Progress {
    fn drop(&mut self) {
        *PHASE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}
#[tauri::command]
pub fn workbench_tool_setup_cancel() {
    cancellation().send_modify(|n| *n = n.wrapping_add(1));
}
pub(crate) async fn cancellable<T>(
    future: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    let mut cancel = cancellation().subscribe();
    tokio::select! {
        biased;
        _ = cancel.changed() => Err("Setup cancelled. You can retry when ready.".into()),
        result = future => result,
    }
}

#[derive(Deserialize)]
struct Manifest {
    version: String,
    artifacts: Vec<Artifact>,
}
#[derive(Clone, Deserialize)]
struct Artifact {
    os: String,
    arch: String,
    filename: String,
    sha256: String,
}
fn manifest() -> Manifest {
    serde_json::from_str(MANIFEST).expect("compiled Node manifest")
}
fn artifact() -> Result<Artifact, String> {
    manifest()
        .artifacts
        .into_iter()
        .find(|a| a.os == std::env::consts::OS && a.arch == std::env::consts::ARCH)
        .ok_or_else(|| "Managed conversation tools are unavailable on this platform.".into())
}
pub(crate) fn supported() -> bool {
    artifact().is_ok()
}
fn identity() -> String {
    format!(
        "node-{}-{:x}",
        manifest().version,
        Sha256::digest(format!(
            "{LOCK}{MANIFEST}{}{}",
            include_str!("browser/driver.mjs"),
            include_str!("browser/public-request.mjs")
        ))
    )
}
fn parent() -> Result<PathBuf, String> {
    Ok(crate::storage::local_root()?.join("native/conversation-tools"))
}
pub(crate) fn root() -> Result<PathBuf, String> {
    Ok(parent()?.join(identity()))
}
fn node_at(root: &Path) -> PathBuf {
    root.join(if cfg!(windows) {
        "runtime/node.exe"
    } else {
        "runtime/bin/node"
    })
}
fn npm_at(root: &Path) -> PathBuf {
    root.join(if cfg!(windows) {
        "runtime/node_modules/npm/bin/npm-cli.js"
    } else {
        "runtime/lib/node_modules/npm/bin/npm-cli.js"
    })
}
fn ready_at(root: &Path) -> bool {
    crate::safety::open_regular_file(&root.join("complete"))
        .ok()
        .and_then(|f| {
            use std::io::Read;
            let mut text = String::new();
            f.take(512).read_to_string(&mut text).ok()?;
            Some(text)
        })
        .is_some_and(|text| text == identity())
        && [
            node_at(root),
            npm_at(root),
            root.join("node_modules/playwright/cli.js"),
            root.join("node_modules/@anthropic-ai/sandbox-runtime/dist/cli.js"),
        ]
        .iter()
        .all(|p| crate::safety::open_regular_file(p).is_ok())
}
pub(crate) fn ready() -> bool {
    root().is_ok_and(|p| ready_at(&p))
}
pub(crate) fn node() -> Result<PathBuf, String> {
    let root = root()?;
    if !ready_at(&root) {
        return Err("Conversation tools are not ready. Retry setup in Assistant settings.".into());
    }
    Ok(node_at(&root))
}

/// Preserve OS essentials but discard npm configuration, credentials and Node
/// injection variables. Both installers and their children use the managed Node.
pub(crate) fn command_at(root: &Path) -> Command {
    let mut command = Command::new(node_at(root));
    command.env_clear();
    for key in [
        "SYSTEMROOT",
        "WINDIR",
        "TEMP",
        "TMP",
        "TMPDIR",
        "HOME",
        "USERPROFILE",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let mut paths = vec![node_at(root).parent().unwrap().to_path_buf()];
    paths.extend(std::env::split_paths(&crate::env::full_path()));
    if let Ok(path) = std::env::join_paths(paths) {
        command.env("PATH", path);
    }
    command.current_dir(root);
    command
}
pub(crate) async fn run(command: Command, label: &str) -> Result<Vec<u8>, String> {
    process::run(command, label).await
}

async fn download(artifact: &Artifact, path: &Path) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .https_only(true)
        // The pinned checksum below authenticates the archive, not the host.
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!(
        "https://nodejs.org/dist/v{}/{}",
        manifest().version,
        artifact.filename
    );
    let mut response = client
        .get(url)
        .timeout(Duration::from_secs(300))
        .send()
        .await
        .map_err(|e| format!("Runtime download failed: {e}"))?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = 0;
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        bytes += chunk.len();
        if bytes > 128 * 1024 * 1024 {
            return Err("Runtime download exceeds 128 MB".into());
        }
        hash.update(&chunk);
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        set_phase(&format!(
            "Downloading command runtime: {} MB",
            bytes / 1_000_000
        ));
    }
    if format!("{:x}", hash.finalize()) != artifact.sha256 {
        return Err("Runtime checksum mismatch. No software was installed.".into());
    }
    file.sync_all().map_err(|e| e.to_string())
}

pub(crate) async fn ensure() -> Result<(), String> {
    ensure_in(parent()?).await
}

async fn ensure_in(parent: PathBuf) -> Result<(), String> {
    cancellable(async {
        let _guard = INSTALL.lock().await;
        let target = parent.join(identity());
        if ready_at(&target) {
            return Ok(());
        }
        let _progress = Progress;
        let artifact = artifact()?;
        crate::agent_runtime::codex::session::private_directory(&parent)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(parent.join("setup.lock"))
            .map_err(|e| e.to_string())?;
        fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| {
            "Another Pipeline instance is setting up conversation tools. Retry shortly."
        })?;
        let stage = tempfile::Builder::new()
            .prefix("setup-")
            .tempdir_in(&parent)
            .map_err(|e| e.to_string())?;
        set_phase("Downloading command runtime…");
        let archive = stage.path().join("node.archive");
        download(&artifact, &archive).await?;
        set_phase("Preparing command runtime…");
        let stage = tokio::task::spawn_blocking(move || {
            archive::unpack(
                &archive,
                &stage.path().join("runtime"),
                artifact.filename.ends_with(".zip"),
            )?;
            fs::remove_file(archive).map_err(|e| e.to_string())?;
            Ok::<_, String>(stage)
        })
        .await
        .map_err(|e| e.to_string())??;
        fs::write(stage.path().join("package.json"), PACKAGE).map_err(|e| e.to_string())?;
        fs::write(stage.path().join("package-lock.json"), LOCK).map_err(|e| e.to_string())?;
        let empty = stage.path().join("npmrc");
        fs::write(&empty, "").map_err(|e| e.to_string())?;
        let global = stage.path().join("global-npmrc");
        fs::write(&global, "").map_err(|e| e.to_string())?;
        let mut command = command_at(stage.path());
        command
            .arg(npm_at(stage.path()))
            .args(["ci", "--ignore-scripts", "--no-audit", "--no-fund"])
            .arg("--userconfig")
            .arg(&empty)
            .arg("--globalconfig")
            .arg(&global)
            .arg("--cache")
            .arg(stage.path().join("cache"));
        set_phase("Downloading command and browser tools…");
        process::run(command, "Conversation tool setup").await?;
        fs::write(
            stage.path().join("driver.mjs"),
            include_str!("browser/driver.mjs"),
        )
        .map_err(|e| e.to_string())?;
        fs::write(
            stage.path().join("public-request.mjs"),
            include_str!("browser/public-request.mjs"),
        )
        .map_err(|e| e.to_string())?;
        fs::write(stage.path().join("complete"), identity()).map_err(|e| e.to_string())?;
        if !ready_at(stage.path()) {
            return Err("Conversation tool setup did not finish. Retry setup.".into());
        }
        // A broken installation is kept for diagnosis; never remove a live version.
        if target.exists() {
            fs::rename(
                &target,
                parent.join(format!(
                    "incomplete-{}",
                    chrono::Utc::now().timestamp_micros()
                )),
            )
            .map_err(|e| e.to_string())?;
        }
        fs::rename(stage.path(), target).map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
}

#[cfg(test)]
pub(crate) async fn fixture_runtime() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    ensure_in(temp.path().to_path_buf()).await.unwrap();
    let root = temp.path().join(identity());
    (temp, root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "Downloads pinned Node and npm packages into a disposable directory; no model or credentials"]
    async fn managed_runtime_cold_install_and_reuse() {
        let temp = tempfile::tempdir().unwrap();
        ensure_in(temp.path().to_path_buf()).await.unwrap();
        let root = temp.path().join(identity());
        assert!(ready_at(&root));
        let mut command = command_at(&root);
        command.arg("--version");
        let version = run(command, "Runtime probe").await.unwrap();
        assert_eq!(
            String::from_utf8(version).unwrap().trim(),
            format!("v{}", manifest().version)
        );
        let mut command = command_at(&root);
        command.args([
            "-e",
            "require('playwright'); require('@anthropic-ai/sandbox-runtime'); console.log('ready')",
        ]);
        assert_eq!(run(command, "Tool probe").await.unwrap(), b"ready\n");
        let before = fs::metadata(root.join("complete"))
            .unwrap()
            .modified()
            .unwrap();
        ensure_in(temp.path().to_path_buf()).await.unwrap();
        assert_eq!(
            before,
            fs::metadata(root.join("complete"))
                .unwrap()
                .modified()
                .unwrap()
        );
    }
    #[test]
    fn lockfile_pins_every_download_and_manifest_covers_supported_platforms() {
        let lock: serde_json::Value = serde_json::from_str(LOCK).unwrap();
        for (name, package) in lock["packages"].as_object().unwrap() {
            if name.is_empty() {
                continue;
            }
            assert!(package["resolved"]
                .as_str()
                .unwrap()
                .starts_with("https://registry.npmjs.org/"));
            assert!(package["integrity"]
                .as_str()
                .unwrap()
                .starts_with("sha512-"));
        }
        assert_eq!(manifest().artifacts.len(), 6);
        for artifact in manifest().artifacts {
            assert_eq!(artifact.sha256.len(), 64);
        }
    }
    #[test]
    fn partial_installations_are_never_ready() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("complete"), identity()).unwrap();
        assert!(!ready_at(temp.path()));
    }
}
