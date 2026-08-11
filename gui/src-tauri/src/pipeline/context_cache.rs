//! Run-local shared context for cache-aware provider calls.
//!
//! The profile setting is intentionally provider-neutral. This value owns the
//! stable paper/orientation prefix and a small registry used by providers that
//! need to warm a prompt prefix or create a base CLI session exactly once per
//! compatible model/configuration.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

type SharedSlot = Arc<Mutex<Option<String>>>;

pub struct PreparedContext {
    key: String,
    content: Arc<str>,
    workspace_dir: String,
    _workspace: tempfile::TempDir,
    slots: Mutex<HashMap<String, SharedSlot>>,
}

/// Deduplicates identical prepared contexts for the lifetime of one pipeline
/// run. Provider cache/session state lives on [`PreparedContext`], so merely
/// recomputing an identical digest for every parallel unit is not sufficient:
/// those units must share the same `Arc`.
#[derive(Debug, Default)]
pub struct PreparedContextPool {
    contexts: std::sync::Mutex<HashMap<String, Arc<PreparedContext>>>,
}

impl std::fmt::Debug for PreparedContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedContext")
            .field("key", &self.key)
            .field("bytes", &self.content.len())
            .finish_non_exhaustive()
    }
}

impl PreparedContext {
    pub fn new(paper_text: &str, orientation: &serde_json::Value) -> Result<Self, String> {
        let orientation_text = if orientation.is_null() {
            String::new()
        } else {
            serde_json::to_string_pretty(orientation)
                .map_err(|error| format!("Failed to prepare orientation cache: {error}"))?
        };
        let clean_paper_text = crate::safety::strip_span_tags(paper_text);
        let clean_orientation_text = crate::safety::strip_span_tags(&orientation_text);

        let mut content = String::new();
        push(
            &mut content,
            "SHARED INPUT CONTEXT\n\n\
             The material below is the stable reference for every task in this pipeline run. \
             Use it directly and do not re-read the extracted input or orientation files unless \
             a task specifically requires inspecting an original figure or another source asset.\n\n\
             <extracted_input>\n",
        )?;
        push(&mut content, clean_paper_text.as_ref())?;
        push(&mut content, "\n</extracted_input>")?;
        if !clean_orientation_text.is_empty() {
            push(&mut content, "\n\n<orientation_map>\n")?;
            push(&mut content, clean_orientation_text.as_ref())?;
            push(&mut content, "\n</orientation_map>")?;
        }
        push(
            &mut content,
            "\n\nA task-specific instruction follows separately. Answer that task using this shared context.",
        )?;

        let digest = format!("{:x}", Sha256::digest(content.as_bytes()));
        let workspace = tempfile::Builder::new()
            .prefix("pipeline_shared_context_")
            .tempdir()
            .map_err(|error| format!("Failed to create shared-context workspace: {error}"))?;
        let workspace_dir = workspace.path().to_string_lossy().replace('\\', "/");
        Ok(Self {
            key: format!("pipeline-{}", &digest[..32]),
            content: Arc::from(content),
            workspace_dir,
            _workspace: workspace,
            slots: Mutex::new(HashMap::new()),
        })
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn bytes(&self) -> usize {
        self.content.len()
    }

    /// A private, read-only working directory for warming CLI base sessions.
    /// Task-specific roots are applied only to the forked child session.
    pub fn workspace_dir(&self) -> &str {
        &self.workspace_dir
    }

    /// A stable compatibility key for a provider cache/session. The context
    /// digest is included automatically; callers add model, tools, system
    /// prompt, sandbox, and other provider-specific compatibility inputs.
    pub fn compatibility_key<'a>(
        &self,
        provider: &str,
        parts: impl IntoIterator<Item = &'a str>,
    ) -> String {
        let mut digest = Sha256::new();
        digest.update(self.key.as_bytes());
        digest.update([0]);
        digest.update(provider.as_bytes());
        for part in parts {
            digest.update([0]);
            digest.update(part.as_bytes());
        }
        let digest = format!("{:x}", digest.finalize());
        format!("{provider}:{}", &digest[..32])
    }

    /// Return the per-key initialization slot. Providers lock only their own
    /// slot while creating a base session/cache, so unrelated models can warm
    /// concurrently.
    pub async fn slot(&self, key: String) -> SharedSlot {
        let mut slots = self.slots.lock().await;
        slots
            .entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone()
    }

    /// Safe fallback for a provider that cannot initialize its native cache.
    pub fn prefixed_prompt(&self, task_prompt: &str) -> Result<String, String> {
        let mut prompt = String::new();
        crate::safety::push_str_limited(
            &mut prompt,
            self.content(),
            crate::safety::MAX_EXPANDED_PROMPT_BYTES,
            "Shared-context prompt",
        )?;
        crate::safety::push_str_limited(
            &mut prompt,
            "\n\n<TASK>\n",
            crate::safety::MAX_EXPANDED_PROMPT_BYTES,
            "Shared-context prompt",
        )?;
        crate::safety::push_str_limited(
            &mut prompt,
            task_prompt,
            crate::safety::MAX_EXPANDED_PROMPT_BYTES,
            "Shared-context prompt",
        )?;
        Ok(prompt)
    }
}

impl PreparedContextPool {
    pub fn prepare(
        &self,
        paper_text: &str,
        orientation: &serde_json::Value,
    ) -> Result<Arc<PreparedContext>, String> {
        let candidate = Arc::new(PreparedContext::new(paper_text, orientation)?);
        let mut contexts = self
            .contexts
            .lock()
            .map_err(|_| "Shared-context pool lock was poisoned".to_string())?;
        Ok(contexts
            .entry(candidate.key().to_string())
            .or_insert_with(|| candidate)
            .clone())
    }
}

fn push(target: &mut String, value: &str) -> Result<(), String> {
    crate::safety::push_str_limited(
        target,
        value,
        crate::safety::MAX_RUNTIME_CONTEXT_BYTES,
        "Shared input context",
    )
}

/// Generate an RFC 4122 version-4 UUID without adding another dependency.
pub fn new_session_id() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("Failed to create cache session id: {error}"))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_is_stable_and_places_shared_material_before_tasks() {
        let orientation = serde_json::json!({"sections": ["intro", "model"]});
        let first = PreparedContext::new("paper body", &orientation).unwrap();
        let second = PreparedContext::new("paper body", &orientation).unwrap();
        assert_eq!(first.key(), second.key());
        assert!(first.content().contains("<extracted_input>\npaper body"));
        assert!(first.content().contains("<orientation_map>"));
        let prompt = first.prefixed_prompt("Review the model.").unwrap();
        assert!(prompt.find("paper body") < prompt.find("Review the model."));
    }

    #[test]
    fn shared_context_excludes_presentational_span_markup() {
        let orientation = serde_json::json!({"title": "<span id=\"title\">Paper title</span>"});
        let context =
            PreparedContext::new("<span class='page-anchor'>Paper body</span>", &orientation)
                .unwrap();

        assert!(!context.content().to_ascii_lowercase().contains("<span"));
        assert!(context.content().contains("Paper body"));
        assert!(context.content().contains("Paper title"));
    }

    #[test]
    fn pool_returns_the_same_context_and_slot_registry_for_identical_material() {
        let pool = PreparedContextPool::default();
        let orientation = serde_json::json!({"sections": ["intro"]});
        let first = pool.prepare("paper body", &orientation).unwrap();
        let second = pool.prepare("paper body", &orientation).unwrap();
        let different = pool.prepare("different paper body", &orientation).unwrap();

        assert!(Arc::ptr_eq(&first, &second));
        assert!(!Arc::ptr_eq(&first, &different));
    }
}
