//! Device-local selection of the research data directory. The active location
//! is fixed for the lifetime of the process; saving a selection never switches
//! an open store or moves files. Credentials, settings, and engines stay local.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static STORAGE: OnceLock<Result<Storage, String>> = OnceLock::new();

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[serde(default)]
    directory: Option<PathBuf>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageSettings {
    pub active_directory: Option<String>,
    pub configured_directory: String,
    pub default_directory: String,
    pub restart_required: bool,
    pub error: Option<String>,
}

struct Storage {
    local: PathBuf,
    active: Result<PathBuf, String>,
}

pub fn local_root() -> Result<PathBuf, String> {
    Ok(dirs::home_dir()
        .ok_or("Cannot determine home directory")?
        .join(".pipeline"))
}

fn storage() -> Result<&'static Storage, String> {
    STORAGE
        .get_or_init(|| Ok(Storage::new(local_root()?)))
        .as_ref()
        .map_err(Clone::clone)
}

/// Resolve the active location without silently falling back when a custom
/// drive is missing. In particular, do not recreate a missing mount point.
pub fn data_root() -> Result<PathBuf, String> {
    storage()?.data_root()
}

pub fn settings() -> Result<StorageSettings, String> {
    storage()?.settings()
}

pub fn set_directory(directory: &str) -> Result<StorageSettings, String> {
    let storage = storage()?; // Pin the old location before persisting a change.
    storage.save(Path::new(directory))?;
    storage.settings()
}

impl Storage {
    fn new(local: PathBuf) -> Self {
        let active =
            read_config(&local).map(|config| config.directory.unwrap_or_else(|| local.clone()));
        Self { local, active }
    }

    fn data_root(&self) -> Result<PathBuf, String> {
        let root = self.active.as_ref().map_err(Clone::clone)?;
        if root == &self.local {
            std::fs::create_dir_all(root)
                .map_err(|e| format!("Cannot create Pipeline data folder: {e}"))?;
        }
        if !root.is_absolute() || !root.is_dir() {
            return Err(format!(
                "Research data folder is unavailable: {}. Reconnect it or choose a folder in Settings → General, then restart Pipeline.",
                root.display()
            ));
        }
        if root != &self.local {
            let resolved = validate_directory(root, &self.local, None)?;
            if &resolved != root {
                return Err("Research data folder now resolves to a different location. Select it again in Settings and restart Pipeline.".into());
            }
        }
        Ok(root.clone())
    }

    fn settings(&self) -> Result<StorageSettings, String> {
        let config = read_config(&self.local);
        let error = config
            .as_ref()
            .err()
            .cloned()
            .or_else(|| self.data_root().err());
        let configured = config
            .ok()
            .and_then(|c| c.directory)
            .unwrap_or_else(|| self.local.clone());
        Ok(StorageSettings {
            active_directory: self.active.as_ref().ok().map(|p| p.display().to_string()),
            configured_directory: configured.display().to_string(),
            default_directory: self.local.display().to_string(),
            restart_required: self.active.as_ref().ok() != Some(&configured),
            error,
        })
    }

    fn save(&self, directory: &Path) -> Result<(), String> {
        std::fs::create_dir_all(&self.local)
            .map_err(|e| format!("Cannot create local settings folder: {e}"))?;
        let directory = validate_directory(directory, &self.local, self.active.as_ref().ok())?;
        let probe = tempfile::NamedTempFile::new_in(&directory)
            .map_err(|e| format!("Research data folder is not writable: {e}"))?;
        probe
            .close()
            .map_err(|e| format!("Cannot finish checking data folder: {e}"))?;
        let local = self.local.canonicalize().map_err(|e| e.to_string())?;
        let config = Config {
            directory: (directory != local).then_some(directory),
        };
        let mut file = tempfile::NamedTempFile::new_in(&self.local)
            .map_err(|e| format!("Cannot save data folder setting: {e}"))?;
        serde_json::to_writer_pretty(&mut file, &config).map_err(|e| e.to_string())?;
        file.write_all(b"\n").map_err(|e| e.to_string())?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        file.persist(self.local.join("storage.json"))
            .map_err(|e| format!("Cannot save data folder setting: {e}"))?;
        #[cfg(unix)]
        std::fs::File::open(&self.local)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| format!("Cannot sync data folder setting: {e}"))?;
        Ok(())
    }
}

fn read_config(local: &Path) -> Result<Config, String> {
    let path = local.join("storage.json");
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(format!("Cannot read data folder setting: {e}")),
        Ok(_) => {}
    }
    let file = crate::safety::open_regular_file(&path)?;
    let mut bytes = Vec::new();
    file.take(16 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 {
        return Err("Data folder setting exceeds the size limit".into());
    }
    let config: Config = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Cannot read data folder setting: {e}"))?;
    if config.directory.as_ref().is_some_and(|p| !p.is_absolute()) {
        return Err("Saved research data folder must be an absolute path".into());
    }
    Ok(config)
}

fn validate_directory(
    directory: &Path,
    local: &Path,
    active: Option<&PathBuf>,
) -> Result<PathBuf, String> {
    if !directory.is_absolute() {
        return Err("Choose an absolute folder path".into());
    }
    let directory = directory
        .canonicalize()
        .map_err(|e| format!("Research data folder is unavailable: {e}"))?;
    if !directory.is_dir() {
        return Err("Research data location must be a folder".into());
    }
    let local = local.canonicalize().map_err(|e| e.to_string())?;
    if directory == local {
        return Ok(directory);
    }
    if directory.parent().is_none()
        || local.starts_with(&directory)
        || directory.starts_with(&local)
    {
        return Err(
            "Choose a dedicated data folder outside Pipeline's local settings folder".into(),
        );
    }
    if let Some(active) = active.and_then(|p| p.canonicalize().ok()) {
        if directory != active && (directory.starts_with(&active) || active.starts_with(&directory))
        {
            return Err(
                "Choose a folder that does not contain, or sit inside, the current data folder"
                    .into(),
            );
        }
    }
    Ok(directory)
}

#[cfg(test)]
mod tests;
