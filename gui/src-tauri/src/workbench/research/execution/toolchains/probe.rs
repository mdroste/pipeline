//! Bounded version probes, install-metadata versions and executable hashes.

use super::super::file_identity::hash_resolved_file;
use super::locations::{Host, Platform};
use super::{Candidate, DetectedToolchain};
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_OUTPUT_LIMIT: usize = 64 * 1024;
const VERSION_TEXT_LIMIT: usize = 200;
const HASH_LIMIT: u64 = 64 * 1024 * 1024;

/// Arguments of the version probe. Licensed programs (Stata, MATLAB) are
/// never started during detection.
pub(super) fn version_args(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "r" | "python" | "julia" | "quarto" => Some(&["--version"]),
        "jupyter" => Some(&["nbconvert", "--version"]),
        _ => None,
    }
}

pub(super) fn version_command(kind: &str) -> &'static str {
    match kind {
        "r" => "Rscript --version",
        "python" => "python --version",
        "julia" => "julia --version",
        "quarto" => "quarto --version",
        "jupyter" => "jupyter nbconvert --version",
        _ => "install metadata",
    }
}

pub(super) fn adapter_for(kind: &str) -> Option<&'static str> {
    match kind {
        "stata" => Some("stata"),
        "r" => Some("r"),
        "python" => Some("python"),
        "julia" => Some("julia"),
        "quarto" => Some("quarto"),
        "jupyter" => Some("notebook"),
        _ => None,
    }
}

fn note_for(kind: &str) -> Option<&'static str> {
    match kind {
        "stata" => Some(
            "Detection does not start Stata. A profile test confirms the license and batch mode.",
        ),
        "matlab" => {
            Some("There is no MATLAB adapter yet. Use the command adapter with matlab -batch.")
        }
        _ => None,
    }
}

/// Placeholders that open an installer or store page instead of answering.
fn probe_blocked(path: &Path, platform: Platform) -> Option<&'static str> {
    let text = path.to_string_lossy();
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match platform {
        Platform::MacOs
            if text.starts_with("/usr/bin/python")
                && !Path::new("/Library/Developer/CommandLineTools/usr/bin/python3").is_file()
                && !Path::new("/Applications/Xcode.app/Contents/Developer/usr/bin/python3")
                    .is_file() =>
        {
            Some("This is the macOS placeholder for Python. Running it may ask to install the developer tools, so detection did not start it.")
        }
        Platform::Windows
            if text.to_ascii_lowercase().contains(r"\microsoft\windowsapps\")
                && name.starts_with("python") =>
        {
            Some("This is the Microsoft Store placeholder for Python. Install Python, then detect again.")
        }
        _ => None,
    }
}

/// First line of a bounded, non-interactive version probe.
pub(super) fn version_text(
    kind: &str,
    program: &Path,
    prefix: &[OsString],
    path: &OsStr,
) -> Option<String> {
    let args = version_args(kind)?;
    let mut command = Command::new(program);
    command
        .args(prefix)
        .args(args)
        .current_dir(std::env::temp_dir());
    if !path.is_empty() {
        command.env("PATH", path);
    }
    for secret in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GOOGLE_API_KEY",
        "AWS_SECRET_ACCESS_KEY",
        "GITHUB_TOKEN",
    ] {
        command.env_remove(secret);
    }
    let output =
        crate::process::run_bounded_independent(&mut command, PROBE_TIMEOUT, PROBE_OUTPUT_LIMIT)
            .ok()?;
    if !output.status.success() {
        return None;
    }
    first_line(&output.stdout).or_else(|| first_line(&output.stderr))
}

pub(super) fn first_line(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let line = line
        .chars()
        .filter(|value| !value.is_control())
        .take(VERSION_TEXT_LIMIT)
        .collect::<String>();
    (!line.is_empty()).then_some(line)
}

fn dotted_version() -> &'static regex::Regex {
    static VERSION: OnceLock<regex::Regex> = OnceLock::new();
    VERSION.get_or_init(|| {
        regex::Regex::new(r"[0-9]+(?:\.[0-9]+)+|[0-9]+").expect("static version regex")
    })
}

/// The first dotted version number in a tool's version line.
pub(super) fn extract_version(text: &str) -> Option<String> {
    let dotted = dotted_version();
    dotted
        .find_iter(text)
        .find(|found| found.as_str().contains('.'))
        .or_else(|| dotted.find(text))
        .map(|found| found.as_str().to_owned())
}

/// Version digits in an install folder name (`Stata18`, `R-4.4.1`).
pub(super) fn folder_version(name: &str) -> Option<String> {
    extract_version(name)
}

/// MATLAB release names such as `R2024b`.
pub(super) fn matlab_release(name: &str) -> Option<String> {
    static RELEASE: OnceLock<regex::Regex> = OnceLock::new();
    RELEASE
        .get_or_init(|| regex::Regex::new(r"R20[0-9]{2}[ab]").expect("static MATLAB regex"))
        .find(name)
        .map(|found| found.as_str().to_owned())
}

/// `CFBundleShortVersionString` from an XML property list.
pub(super) fn plist_version(path: &Path) -> Option<String> {
    static KEY: OnceLock<regex::Regex> = OnceLock::new();
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(256 * 1024).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    KEY.get_or_init(|| {
        regex::Regex::new(r"<key>CFBundleShortVersionString</key>\s*<string>([^<]{1,40})</string>")
            .expect("static plist regex")
    })
    .captures(&text)
    .map(|captures| captures[1].trim().to_owned())
    .filter(|version| !version.is_empty())
}

/// Stata's version from the install folder of a resolved executable.
pub(super) fn stata_install_version(executable: &Path) -> Option<(String, &'static str)> {
    for ancestor in executable.ancestors().skip(1) {
        let name = ancestor.file_name()?.to_string_lossy();
        if name.to_ascii_lowercase().ends_with(".app") {
            if let Some(version) = plist_version(&ancestor.join("Contents").join("Info.plist")) {
                return Some((version, "install folder (Info.plist)"));
            }
            continue;
        }
        if name.to_ascii_lowercase().starts_with("stata") {
            if let Some(version) = folder_version(&name) {
                return Some((version, "install folder name"));
            }
        }
    }
    None
}

/// SHA-256 of the resolved executable when it is no larger than 64 MiB.
fn hash_capped(path: &Path) -> (Option<String>, Option<u64>) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return (None, None);
    };
    if metadata.len() > HASH_LIMIT {
        return (None, Some(metadata.len()));
    }
    match hash_resolved_file(path, "toolchain executable") {
        Ok((_, hash, size)) => (Some(hash), Some(size)),
        Err(_) => (None, Some(metadata.len())),
    }
}

/// Version, hash and guidance for each candidate. Probes run concurrently,
/// each bounded by its own timeout.
pub(super) fn describe(host: &Host, candidates: Vec<Candidate>) -> Vec<DetectedToolchain> {
    std::thread::scope(|scope| {
        let handles = candidates
            .into_iter()
            .map(|candidate| scope.spawn(move || describe_one(host, candidate)))
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .collect()
    })
}

fn describe_one(host: &Host, candidate: Candidate) -> DetectedToolchain {
    let blocked = probe_blocked(&candidate.path, host.platform);
    let probed = (host.probe_versions && blocked.is_none())
        .then(|| version_text(candidate.kind, &candidate.path, &[], &host.path))
        .flatten();
    let (version, version_text, version_source) = match (probed, candidate.version_hint) {
        (Some(text), _) => (
            extract_version(&text),
            Some(text),
            version_command(candidate.kind).to_owned(),
        ),
        (None, Some((version, source))) => (Some(version), None, source.to_owned()),
        (None, None) => {
            let reason = if version_args(candidate.kind).is_none() {
                "not checked: detection does not start licensed programs"
            } else if blocked.is_some() || !host.probe_versions {
                "not checked"
            } else {
                "the version check did not answer"
            };
            (None, None, reason.to_owned())
        }
    };
    let (sha256, size_bytes) = hash_capped(&candidate.path);
    DetectedToolchain {
        kind: candidate.kind.into(),
        adapter: adapter_for(candidate.kind).map(Into::into),
        label: candidate.label,
        path: candidate.path.to_string_lossy().into_owned(),
        source: candidate.source.into(),
        version,
        version_text,
        version_source,
        sha256,
        size_bytes,
        note: blocked.or_else(|| note_for(candidate.kind)).map(Into::into),
    }
}
