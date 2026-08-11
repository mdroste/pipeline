//! Durable, host-owned capture of terminal model responses.
//!
//! These files are evidence, not pipeline inputs. They live beside (rather
//! than inside) the model-writable per-step directories so a provider cannot
//! forge or overwrite them. A response is captured before envelope/schema
//! validation, then atomically promoted to a status-bearing Markdown artifact.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const MAX_CAPTURE_BYTES: usize = 8 * 1024 * 1024;
const MAX_JOURNAL_BYTES: usize = 128 * 1024 * 1024;
static JOURNAL_BYTES: OnceLock<Mutex<HashMap<PathBuf, usize>>> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
pub enum AttemptStatus {
    Accepted,
    RejectedEnvelope,
    RejectedSchema,
    Ignored,
}

impl AttemptStatus {
    fn slug(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::RejectedEnvelope => "rejected-envelope",
            Self::RejectedSchema => "rejected-schema",
            Self::Ignored => "ignored",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Accepted => "Accepted",
            Self::RejectedEnvelope => "Rejected: invalid report boundaries",
            Self::RejectedSchema => "Rejected: output schema mismatch",
            Self::Ignored => "Not selected",
        }
    }
}

#[derive(Debug)]
pub struct CapturedAttempt {
    pending_path: PathBuf,
    final_path: PathBuf,
    producer: String,
    source: String,
    attempt: u32,
    truncated: bool,
}

fn safe_component(value: &str) -> String {
    let mut readable = String::new();
    let mut previous_dash = false;
    for character in value.chars() {
        let normalized = if character.is_ascii_alphanumeric() {
            character.to_ascii_lowercase()
        } else {
            '-'
        };
        if normalized == '-' {
            if previous_dash {
                continue;
            }
            previous_dash = true;
        } else {
            previous_dash = false;
        }
        readable.push(normalized);
        if readable.len() >= 48 {
            break;
        }
    }
    let readable = readable.trim_matches('-');
    let readable = if readable.is_empty() {
        "response"
    } else {
        readable
    };
    let digest = format!("{:x}", Sha256::digest(value.as_bytes()));
    format!("{readable}--{}", &digest[..12])
}

fn bounded_text(text: &str) -> (&str, bool) {
    if text.len() <= MAX_CAPTURE_BYTES {
        return (text, false);
    }
    let mut end = MAX_CAPTURE_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    let directory = path
        .parent()
        .ok_or("Response journal path has no parent directory")?;
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("Failed to create response journal: {error}"))?;
    let mut temp = tempfile::NamedTempFile::new_in(directory)
        .map_err(|error| format!("Failed to create response journal temp file: {error}"))?;
    temp.write_all(bytes)
        .map_err(|error| format!("Failed to write response journal: {error}"))?;
    temp.flush()
        .map_err(|error| format!("Failed to flush response journal: {error}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| format!("Failed to sync response journal: {error}"))?;
    temp.persist(path)
        .map_err(|error| format!("Failed to publish response journal: {}", error.error))?;
    #[cfg(unix)]
    std::fs::File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("Failed to sync response journal directory: {error}"))?;
    Ok(())
}

fn reserve_journal_bytes(directory: &Path, bytes: usize) -> Result<(), String> {
    let budgets = JOURNAL_BYTES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut budgets = budgets.lock().unwrap_or_else(|error| error.into_inner());
    let current = budgets.entry(directory.to_path_buf()).or_insert_with(|| {
        std::fs::read_dir(directory)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let file_type = entry.file_type().ok()?;
                if !file_type.is_file() || file_type.is_symlink() {
                    return None;
                }
                usize::try_from(entry.metadata().ok()?.len()).ok()
            })
            .fold(0usize, usize::saturating_add)
    });
    if bytes > MAX_JOURNAL_BYTES.saturating_sub(*current) {
        return Err(format!(
            "Response journal exceeds its {} MiB per-run safety limit",
            MAX_JOURNAL_BYTES / 1024 / 1024,
        ));
    }
    *current = current.saturating_add(bytes);
    Ok(())
}

fn release_journal_bytes(directory: &Path, bytes: usize) {
    let Some(budgets) = JOURNAL_BYTES.get() else {
        return;
    };
    let mut budgets = budgets.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(current) = budgets.get_mut(directory) {
        *current = current.saturating_sub(bytes);
    }
}

/// Save one provider-returned response before the caller attempts to validate
/// it. `run_artifact_dir` is the run's host-owned `artifacts/` root, not the
/// narrower directory granted to the model.
pub async fn capture(
    run_artifact_dir: Option<&str>,
    producer: &str,
    attempt: u32,
    source: &str,
    text: &str,
) -> Result<Option<CapturedAttempt>, String> {
    let Some(root) = run_artifact_dir else {
        return Ok(None);
    };
    let directory = Path::new(root).join("agent-responses");
    let producer_slug = safe_component(producer);
    let source_slug = safe_component(source)
        .split_once("--")
        .map(|(readable, _)| readable.to_string())
        .unwrap_or_else(|| "response".to_string());
    let stem = format!("{producer_slug}--attempt-{attempt:02}-{source_slug}");
    let pending_path = directory.join(format!("{stem}-captured.md"));
    let final_path = directory.join(stem);
    let (bounded, truncated) = bounded_text(text);
    let bytes = bounded.as_bytes().to_vec();
    let write_path = pending_path.clone();
    tokio::task::spawn_blocking(move || {
        reserve_journal_bytes(&directory, bytes.len())?;
        if let Err(error) = write_atomic(&write_path, &bytes) {
            release_journal_bytes(&directory, bytes.len());
            return Err(error);
        }
        Ok(())
    })
    .await
    .map_err(|error| format!("Response journal task failed: {error}"))??;
    Ok(Some(CapturedAttempt {
        pending_path,
        final_path,
        producer: producer.to_string(),
        source: source.to_string(),
        attempt,
        truncated,
    }))
}

impl CapturedAttempt {
    /// Add host-authored status metadata while retaining the captured response
    /// verbatim below the divider. If this promotion is interrupted, the
    /// original `captured` file remains independently inspectable.
    pub async fn finish(self, status: AttemptStatus, reason: &str) -> Result<(), String> {
        let final_path = self.final_path.with_file_name(format!(
            "{}-{}.md",
            self.final_path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("Response journal filename is invalid")?,
            status.slug(),
        ));
        let pending_path = self.pending_path.clone();
        let producer = self.producer;
        let source = self.source;
        let attempt = self.attempt;
        let truncated = self.truncated;
        let clean_reason = crate::safety::strip_span_tags(reason)
            .replace(['\r', '\n'], " ")
            .chars()
            .take(1_000)
            .collect::<String>();
        tokio::task::spawn_blocking(move || {
            let raw = std::fs::read(&pending_path)
                .map_err(|error| format!("Failed to reopen captured response: {error}"))?;
            let mut header = format!(
                "> **{}** · Producer: `{}` · Source: `{}` · Attempt: {}\n",
                status.label(),
                producer,
                source,
                attempt,
            );
            if !clean_reason.is_empty() {
                header.push_str(&format!(">\n> {}\n", clean_reason));
            }
            if truncated {
                header.push_str(&format!(
                    ">\n> Capture truncated at {} MiB by the response-journal safety limit.\n",
                    MAX_CAPTURE_BYTES / 1024 / 1024,
                ));
            }
            header.push_str("\n---\n\n");
            let mut content = header.into_bytes();
            content.extend_from_slice(&raw);
            write_atomic(&final_path, &content)?;
            std::fs::remove_file(&pending_path)
                .map_err(|error| format!("Failed to retire pending response capture: {error}"))?;
            Ok(())
        })
        .await
        .map_err(|error| format!("Response journal task failed: {error}"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_response_remains_readable_with_status() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let run = tempfile::tempdir().unwrap();
                let root = run.path().join("artifacts");
                let capture = capture(
                    Some(root.to_str().unwrap()),
                    "technical/codex",
                    1,
                    "terminal",
                    "# Useful report without markers\n\nFinding.",
                )
                .await
                .unwrap()
                .unwrap();

                capture
                    .finish(
                        AttemptStatus::RejectedEnvelope,
                        "expected exactly two report boundary markers",
                    )
                    .await
                    .unwrap();

                let entries = std::fs::read_dir(root.join("agent-responses"))
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .collect::<Vec<_>>();
                assert_eq!(entries.len(), 1);
                assert!(entries[0]
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains("rejected-envelope"));
                let saved = std::fs::read_to_string(&entries[0]).unwrap();
                assert!(saved.contains("Rejected: invalid report boundaries"));
                assert!(saved.contains("# Useful report without markers"));
            });
    }

    #[test]
    fn capture_is_a_noop_without_persistent_run_storage() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                assert!(capture(None, "step", 1, "terminal", "report")
                    .await
                    .unwrap()
                    .is_none());
            });
    }
}
