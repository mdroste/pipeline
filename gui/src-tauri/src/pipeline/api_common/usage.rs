//! Token partitions, tool counts and hosted-search accounting.
use super::wire::{AnthropicUsage, GoogleCandidate};

/// Accumulated token usage across all round-trips in a tool loop.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub requests: u32,
    pub tool_calls: crate::models::ToolCallCounts,
}

impl Usage {
    pub fn add(&mut self, input: u64, output: u64, cached: u64, cache_write: u64) {
        self.input_tokens += input;
        self.output_tokens += output;
        self.cached_input_tokens += cached;
        self.cache_write_input_tokens += cache_write;
        self.requests += 1;
    }

    pub fn call_usage(&self) -> crate::pipeline::logging::CallUsage {
        crate::pipeline::logging::CallUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cached_input_tokens: self.cached_input_tokens,
            cache_write_input_tokens: self.cache_write_input_tokens,
            model_round_trips: u64::from(self.requests),
            tool_calls: self.tool_calls,
            ..Default::default()
        }
    }

    pub fn merge(&mut self, other: Usage) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cached_input_tokens = self
            .cached_input_tokens
            .saturating_add(other.cached_input_tokens);
        self.cache_write_input_tokens = self
            .cache_write_input_tokens
            .saturating_add(other.cache_write_input_tokens);
        self.requests = self.requests.saturating_add(other.requests);
        self.tool_calls.add_counts(other.tool_calls);
    }

    /// Format as a compact string for log lines.
    pub fn summary(&self) -> String {
        let total = self.input_tokens + self.output_tokens;
        if total == 0 {
            return String::new();
        }
        let cache = if self.cached_input_tokens > 0 || self.cache_write_input_tokens > 0 {
            format!(
                ", {} cached/{} cache-write",
                self.cached_input_tokens, self.cache_write_input_tokens
            )
        } else {
            String::new()
        };
        format!(
            ", {}+{} tokens{cache} ({} req)",
            self.input_tokens, self.output_tokens, self.requests
        )
    }
}

pub(super) fn direct_tool_kind(name: &str) -> crate::models::ToolCallKind {
    match name {
        "Read" | "ReadTextBatch" | "Write" => crate::models::ToolCallKind::TextFile,
        "ReadDocumentAsset" | "ReadDocumentAssetsBatch" => crate::models::ToolCallKind::Image,
        other => crate::pipeline::logging::classify_tool_name(other),
    }
}

pub(super) fn anthropic_hosted_search_count(usage: &AnthropicUsage) -> u64 {
    usage
        .server_tool_use
        .as_ref()
        .and_then(|server| server.web_search_requests)
        .unwrap_or(0)
}

pub(super) fn google_hosted_search_count(candidate: &GoogleCandidate) -> u64 {
    let mut queries = std::collections::HashSet::new();
    let mut record_queries = |values: Option<&Vec<serde_json::Value>>| {
        for query in values.into_iter().flatten() {
            if let Some(query) = query
                .as_str()
                .map(str::trim)
                .filter(|query| !query.is_empty())
            {
                queries.insert(query.to_string());
            }
        }
    };
    record_queries(
        candidate
            .grounding_metadata
            .as_ref()
            .and_then(|metadata| metadata.get("webSearchQueries"))
            .and_then(serde_json::Value::as_array),
    );
    for part in &candidate.content.parts {
        let Some(tool_call) = part.get("toolCall") else {
            continue;
        };
        let is_google_search = tool_call
            .get("toolType")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|kind| kind.starts_with("GOOGLE_SEARCH"));
        if is_google_search {
            record_queries(
                tool_call
                    .get("args")
                    .and_then(|args| args.get("queries"))
                    .and_then(serde_json::Value::as_array),
            );
        }
    }
    queries.len() as u64
}
