//! CLI output and path helpers for pipeline-cli (kept outside src/bin so the
//! Tauri bundler does not treat the folder as another binary).
use std::collections::HashMap;
use std::io::Write as _;
use std::path::Path;

/// Write one line of report or JSON output. The Rust runtime ignores SIGPIPE,
/// so `println!` panics (exit 101) when the reader of a pipe has gone away;
/// stop printing quietly instead and let the exit code reflect the run.
static STDOUT_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(super) fn output_exit_code(code: i32) -> i32 {
    if code == 0 && STDOUT_FAILED.load(std::sync::atomic::Ordering::Relaxed) {
        1
    } else {
        code
    }
}
pub(super) fn emit_stdout(text: &str) {
    emit_to(&mut std::io::stdout().lock(), text);
}

pub(super) fn emit_to(out: &mut impl std::io::Write, text: &str) -> bool {
    match out
        .write_all(text.as_bytes())
        .and_then(|()| out.write_all(b"\n"))
        .and_then(|()| out.flush())
    {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => false,
        Err(error) => {
            STDOUT_FAILED.store(true, std::sync::atomic::Ordering::Relaxed);
            eprintln!("Error: cannot write to stdout: {error}");
            false
        }
    }
}

/// Run manifests and automations treat input paths as absolute; resolve a
/// relative CLI path against the current directory once, up front.
pub(super) fn absolutize(path: &str) -> Result<String, String> {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Ok(path.to_string());
    }
    let cwd = std::env::current_dir()
        .map_err(|error| format!("Cannot resolve the current directory: {error}"))?;
    Ok(cwd.join(candidate).to_string_lossy().into_owned())
}

pub(super) fn absolutize_inputs(
    inputs: HashMap<String, String>,
) -> Result<HashMap<String, String>, String> {
    inputs
        .into_iter()
        .map(|(key, value)| absolutize(&value).map(|value| (key, value)))
        .collect()
}

pub(super) fn finish_single_run(value: &serde_json::Value, out: Option<&str>, force: bool) -> i32 {
    let markdown = match report_markdown(value) {
        Ok(markdown) => markdown,
        Err(error) => {
            eprintln!("Error: {error}");
            return 1;
        }
    };
    match out {
        Some("-") => emit_stdout(markdown),
        Some(path) => {
            if let Err(error) = write_report_file(Path::new(path), markdown, force) {
                eprintln!("Error: {error}");
                return 1;
            }
            eprintln!("Wrote report to {path}");
        }
        None => emit_stdout(markdown),
    }
    if let Some(run_id) = value.get("run_id").and_then(|id| id.as_str()) {
        eprintln!("Run: {run_id}");
    }
    let failed = failed_step_count(value);
    if failed > 0 {
        eprintln!("{failed} step(s) failed");
        1
    } else {
        output_exit_code(0)
    }
}

pub(super) fn report_markdown(value: &serde_json::Value) -> Result<&str, String> {
    value
        .get("markdown")
        .and_then(|markdown| markdown.as_str())
        .filter(|markdown| !markdown.is_empty())
        .ok_or_else(|| "Pipeline returned no Markdown report".to_string())
}

pub(super) fn failed_step_count(value: &serde_json::Value) -> usize {
    value
        .get("report")
        .and_then(|report| report.get("failed_steps"))
        .and_then(|failed| failed.as_array())
        .map(Vec::len)
        .unwrap_or(0)
}

pub(super) fn validate_output_path(path: &Path, force: bool) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "{} is a symbolic link; choose a regular output path",
            path.display()
        )),
        Ok(metadata) if !metadata.is_file() => {
            Err(format!("{} is not a regular file", path.display()))
        }
        Ok(_) if !force => Err(format!(
            "{} already exists; pass --force to replace it",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                if !parent.is_dir() {
                    return Err(format!(
                        "output directory {} does not exist or is not a directory",
                        parent.display()
                    ));
                }
            }
            Ok(())
        }
        Err(error) => Err(format!(
            "Could not inspect output path {}: {error}",
            path.display()
        )),
    }
}

pub(super) fn write_report_file(path: &Path, markdown: &str, force: bool) -> Result<(), String> {
    if force {
        return std::fs::write(path, markdown)
            .map_err(|error| format!("Failed to write {}: {error}", path.display()));
    }
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                format!(
                    "{} already exists; pass --force to replace it",
                    path.display()
                )
            } else {
                format!("Failed to create {}: {error}", path.display())
            }
        })?;
    output
        .write_all(markdown.as_bytes())
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))
}
