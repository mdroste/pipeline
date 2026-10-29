//! Environment files and virtual environments in a project folder.

use super::locations::{launchable, Platform};
use super::probe::extract_version;
use super::{Candidate, EnvironmentFile, ProjectEnvironment, PythonEnvironment};
use crate::workbench::research::papers::hash_resolved_file;
use std::io::Read;
use std::path::Path;

const FILE_HASH_LIMIT: u64 = 16 * 1024 * 1024;

/// Files that identify a project's software environment, by ecosystem.
const ENVIRONMENT_FILES: [(&str, &str); 16] = [
    ("renv.lock", "R"),
    (".Rprofile", "R"),
    ("DESCRIPTION", "R"),
    ("uv.lock", "Python"),
    ("requirements.txt", "Python"),
    ("environment.yml", "Conda"),
    ("environment.yaml", "Conda"),
    ("pyproject.toml", "Python"),
    ("poetry.lock", "Python"),
    ("Pipfile.lock", "Python"),
    (".python-version", "Python"),
    ("Project.toml", "Julia"),
    ("Manifest.toml", "Julia"),
    ("_quarto.yml", "Quarto"),
    ("_quarto.yaml", "Quarto"),
    ("profile.do", "Stata"),
];

const VIRTUAL_ENVIRONMENTS: [&str; 3] = [".venv", "venv", "env"];

pub(super) fn inspect(root: &Path, platform: Platform) -> ProjectEnvironment {
    let mut files = Vec::new();
    for (name, ecosystem) in ENVIRONMENT_FILES {
        let path = root.join(name);
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let sha256 = (metadata.len() <= FILE_HASH_LIMIT)
            .then(|| hash_resolved_file(&path, "environment file").ok())
            .flatten()
            .map(|(_, hash, _)| hash);
        files.push(EnvironmentFile {
            path: name.into(),
            ecosystem: ecosystem.into(),
            sha256,
            size_bytes: metadata.len(),
        });
    }
    let python_environments = VIRTUAL_ENVIRONMENTS
        .iter()
        .filter_map(|name| python_environment(root, name, platform))
        .collect();
    ProjectEnvironment {
        root: root.to_string_lossy().into_owned(),
        files,
        python_environments,
        stata_ado_folder: root.join("ado").is_dir(),
    }
}

fn interpreter_path(folder: &Path, platform: Platform) -> std::path::PathBuf {
    if platform.windows() {
        folder.join("Scripts").join("python.exe")
    } else {
        folder.join("bin").join("python")
    }
}

fn python_environment(root: &Path, name: &str, platform: Platform) -> Option<PythonEnvironment> {
    let folder = root.join(name);
    let configuration = folder.join("pyvenv.cfg");
    if !configuration.is_file() {
        return None;
    }
    let interpreter = interpreter_path(&folder, platform);
    Some(PythonEnvironment {
        path: name.into(),
        interpreter: launchable(&interpreter, platform)
            .then(|| interpreter.to_string_lossy().into_owned()),
        version: pyvenv_version(&configuration),
        configuration_sha256: hash_resolved_file(&configuration, "Python environment")
            .ok()
            .map(|(_, hash, _)| hash),
    })
}

/// The `version` (venv) or `version_info` (uv) value of `pyvenv.cfg`.
pub(super) fn pyvenv_version(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut text = String::new();
    file.take(64 * 1024).read_to_string(&mut text).ok()?;
    text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        matches!(key.trim(), "version" | "version_info")
            .then(|| extract_version(value.trim()))
            .flatten()
    })
}

/// Interpreters and Jupyter inside the project's virtual environments.
pub(super) fn environment_tools(
    environment: &ProjectEnvironment,
    platform: Platform,
) -> Vec<Candidate> {
    let mut tools = Vec::new();
    for python in &environment.python_environments {
        let Some(interpreter) = &python.interpreter else {
            continue;
        };
        let interpreter = std::path::PathBuf::from(interpreter);
        let label = format!("{} (this project)", python.path);
        let jupyter = interpreter.with_file_name(if platform.windows() {
            "jupyter.exe"
        } else {
            "jupyter"
        });
        tools.push(Candidate {
            kind: "python",
            path: interpreter,
            source: "projectEnvironment",
            label: label.clone(),
            version_hint: python
                .version
                .clone()
                .map(|version| (version, "pyvenv.cfg")),
        });
        if launchable(&jupyter, platform) {
            tools.push(Candidate {
                kind: "jupyter",
                path: jupyter,
                source: "projectEnvironment",
                label: format!("Jupyter in {label}"),
                version_hint: None,
            });
        }
    }
    tools
}
