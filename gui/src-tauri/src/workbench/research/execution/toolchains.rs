//! Read-only discovery of research toolchains and environment identity.
//!
//! Detection inspects well-known install folders and `PATH`. It never
//! installs software and never starts licensed programs (Stata, MATLAB): their
//! versions come from install metadata. Other tools answer a bounded
//! `--version` probe with no input and a short timeout.

mod locations;
mod probe;
mod project;
#[cfg(test)]
mod tests;

use super::adapters::{stata_mode, StataMode};
use super::types::ExecutionProfile;
use crate::workbench::research::papers::hash_resolved_file;
use crate::workbench::store::WorkbenchResult;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use locations::Host;

/// Tools found on this computer and environment files in the project folder.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainReport {
    pub platform: String,
    pub tools: Vec<DetectedToolchain>,
    pub project: Option<ProjectEnvironment>,
    pub boundary: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DetectedToolchain {
    /// `stata`, `r`, `python`, `julia`, `quarto`, `jupyter` or `matlab`.
    pub kind: String,
    /// The execution adapter that runs this tool, when one exists.
    pub adapter: Option<String>,
    pub label: String,
    /// Path to enter as the profile's program.
    pub path: String,
    /// `standardLocation`, `path` or `projectEnvironment`.
    pub source: String,
    pub version: Option<String>,
    pub version_text: Option<String>,
    /// How the version was obtained, in plain language.
    pub version_source: String,
    pub sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEnvironment {
    pub root: String,
    pub files: Vec<EnvironmentFile>,
    pub python_environments: Vec<PythonEnvironment>,
    /// A project `ado` folder for Stata packages (AEA convention).
    pub stata_ado_folder: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentFile {
    pub path: String,
    pub ecosystem: String,
    pub sha256: Option<String>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PythonEnvironment {
    pub path: String,
    pub interpreter: Option<String>,
    pub version: Option<String>,
    pub configuration_sha256: Option<String>,
}

const BOUNDARY: &str = "Detection looked only in standard install folders, your PATH and this project's folder. It installed nothing and did not start Stata or MATLAB. A profile test is still required before a tool is used for research.";

/// Detect toolchains on this computer, plus environment files in `project_root`.
pub fn detect_toolchains(project_root: Option<&Path>) -> ToolchainReport {
    detect_with(&Host::current(), project_root)
}

pub(super) fn detect_with(host: &Host, project_root: Option<&Path>) -> ToolchainReport {
    let project = project_root
        .and_then(|root| std::fs::canonicalize(root).ok())
        .filter(|root| root.is_dir())
        .map(|root| project::inspect(&root, host.platform));
    let mut candidates = Vec::new();
    if let Some(environment) = &project {
        candidates.extend(project::environment_tools(environment, host.platform));
    }
    candidates.extend(locations::candidates(host));
    let tools = probe::describe(host, dedupe(candidates));
    ToolchainReport {
        platform: host.platform.name().into(),
        tools,
        project,
        boundary: BOUNDARY.into(),
    }
}

/// One discovered executable before version and hash inspection.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Candidate {
    pub kind: &'static str,
    pub path: PathBuf,
    pub source: &'static str,
    pub label: String,
    /// Version read from install metadata, with its plain-language source.
    pub version_hint: Option<(String, &'static str)>,
}

const MAX_PER_KIND: usize = 8;

/// Keep project environments first, then drop later entries that resolve to
/// the same file (PATH symlinks into standard installs) and cap each kind.
fn dedupe(candidates: Vec<Candidate>) -> Vec<Candidate> {
    let mut seen = std::collections::HashSet::new();
    let mut counts = std::collections::HashMap::<&str, usize>::new();
    let mut kept = Vec::new();
    for candidate in candidates {
        let count = counts.entry(candidate.kind).or_default();
        if *count >= MAX_PER_KIND {
            continue;
        }
        let identity = std::fs::canonicalize(&candidate.path).unwrap_or(candidate.path.clone());
        // A virtual-environment interpreter resolves to its base interpreter,
        // yet its environment differs, so project entries are always kept.
        if candidate.source != "projectEnvironment" && !seen.insert((candidate.kind, identity)) {
            continue;
        }
        *count += 1;
        kept.push(candidate);
    }
    kept
}

/// Hashes of the environment files present in a profile's working folder.
/// Part of the authorized launch identity for structured adapters.
pub(super) fn environment_identity(cwd: &Path, names: &[&str]) -> WorkbenchResult<Value> {
    let mut files = Vec::new();
    for name in names {
        let path = cwd.join(name);
        if path.is_file() {
            let (_, hash, size) = hash_resolved_file(&path, "environment file")?;
            files.push(json!({"path":name,"contentHash":hash,"sizeBytes":size}));
        }
    }
    Ok(
        json!({"files":files,"checked":names,"coverage":"lockfiles and startup files found in the working folder; installed packages are not inspected"}),
    )
}

/// The runtime version observed for a structured adapter's receipt. Probes the
/// authorized program with `--version`; Stata's comes from install metadata.
pub(super) fn runtime_identity(profile: &ExecutionProfile, launch: &Value) -> Value {
    let kind = match profile.adapter.as_str() {
        "r" => "r",
        "python" => "python",
        "julia" => "julia",
        "quarto" => "quarto",
        "notebook" => "jupyter",
        "stata" if stata_mode(&profile.argv) == StataMode::Direct => "stata",
        _ => return Value::Null,
    };
    let Some(program) = launch["invocationPath"].as_str().map(PathBuf::from) else {
        return Value::Null;
    };
    let (version, text, source) = if kind == "stata" {
        let executable = launch["executable"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| program.clone());
        match probe::stata_install_version(&executable) {
            Some((version, source)) => (Some(version), None, source.to_string()),
            None => (None, None, "not available from the install folder".into()),
        }
    } else {
        let prefix = launch["prefixArgs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>();
        let path = launch["path"].as_str().unwrap_or_default();
        match probe::version_text(kind, &program, &prefix, std::ffi::OsStr::new(path)) {
            Some(text) => (
                probe::extract_version(&text),
                Some(text),
                format!("{} after the run", probe::version_command(kind)),
            ),
            None => (None, None, "the version check did not answer".into()),
        }
    };
    json!({"kind":kind,"program":program,"executableHash":launch["hash"],"version":version,"versionText":text,"versionSource":source})
}
