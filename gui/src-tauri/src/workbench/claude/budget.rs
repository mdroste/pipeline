//! What a turn's output budget counts. Claude Code repeats each tool result in
//! its `user` frames, and a page image Pipeline returned isn't output the model
//! wrote, so it can't be what ends a turn. Only those results are let off, and
//! only up to the bytes Pipeline actually sent, so a process that claims more
//! still meets the limit. Frames aren't kept: one is in memory at a time.

use serde_json::Value;
use std::collections::HashSet;

const OUTPUT_LIMIT: usize = 64 * 1024 * 1024;
/// A result may be carried twice in its echo, once in `message` and once in
/// `tool_use_result`. If the CLI repeats it once, the rest goes unused.
const ECHO_COPIES: usize = 2;
const ECHO_ENVELOPE: usize = 4096;
const PIPELINE_TOOLS: &str = "mcp__pipeline__";
/// More than a turn's control request budget allows.
const TRACKED_CALLS: usize = 256;

pub(super) struct OutputBudget {
    limit: usize,
    counted: usize,
    echo_allowance: usize,
    pipeline_calls: HashSet<String>,
}

impl Default for OutputBudget {
    fn default() -> Self {
        Self::new(OUTPUT_LIMIT)
    }
}

impl OutputBudget {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit,
            counted: 0,
            echo_allowance: 0,
            pipeline_calls: HashSet::new(),
        }
    }

    /// Records `size` bytes of tool reply written to Claude for this request.
    pub(super) fn supplied(&mut self, request: &Value, size: usize) {
        let inner = &request["request"];
        if inner["subtype"] == "mcp_message" && inner["message"]["method"] == "tools/call" {
            self.echo_allowance += ECHO_COPIES * size + ECHO_ENVELOPE;
        }
    }

    /// Accounts for one frame read from Claude.
    pub(super) fn frame(&mut self, value: &Value, size: usize) -> Result<(), String> {
        let echoed = if self.echoes_a_pipeline_tool(value) {
            size.min(self.echo_allowance)
        } else {
            0
        };
        self.echo_allowance -= echoed;
        self.counted += size - echoed;
        self.note_pipeline_calls(value);
        if self.counted > self.limit {
            return Err("Claude turn exceeded its output budget".into());
        }
        Ok(())
    }

    fn echoes_a_pipeline_tool(&self, value: &Value) -> bool {
        value["type"] == "user"
            && parts(value).any(|part| {
                part["type"] == "tool_result"
                    && part["tool_use_id"]
                        .as_str()
                        .is_some_and(|id| self.pipeline_calls.contains(id))
            })
    }

    fn note_pipeline_calls(&mut self, value: &Value) {
        if value["type"] != "assistant" {
            return;
        }
        for part in parts(value) {
            let named = part["name"]
                .as_str()
                .is_some_and(|name| name.starts_with(PIPELINE_TOOLS));
            if let (true, "tool_use", Some(id)) = (
                named,
                part["type"].as_str().unwrap_or_default(),
                part["id"].as_str().filter(|id| id.len() <= 512),
            ) {
                if self.pipeline_calls.len() < TRACKED_CALLS {
                    self.pipeline_calls.insert(id.to_owned());
                }
            }
        }
    }
}

fn parts(value: &Value) -> impl Iterator<Item = &Value> {
    value["message"]["content"].as_array().into_iter().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MIB: usize = 1024 * 1024;

    fn tool_call() -> Value {
        json!({"request":{"subtype":"mcp_message","message":{"method":"tools/call"}}})
    }
    fn tool_use(id: &str, name: &str) -> Value {
        json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":id,"name":name}]}})
    }
    fn tool_result(id: &str) -> Value {
        json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":id}]}})
    }

    /// One page: Claude asks, Pipeline answers, and Claude repeats the answer.
    fn read_page(budget: &mut OutputBudget, id: &str) -> Result<(), String> {
        budget.frame(&tool_use(id, "mcp__pipeline__workbench_paper_page"), 400)?;
        budget.supplied(&tool_call(), 7 * MIB);
        budget.frame(&tool_result(id), 2 * 7 * MIB)
    }

    #[test]
    fn a_scanned_paper_does_not_spend_the_output_budget() {
        let mut budget = OutputBudget::default();
        for page in 0..100 {
            read_page(&mut budget, &format!("page-{page}")).unwrap();
        }
        assert!(budget.counted < MIB);
    }

    #[test]
    fn an_echo_cannot_claim_more_than_pipeline_sent() {
        let mut budget = OutputBudget::new(10 * MIB);
        read_page(&mut budget, "page").unwrap();
        budget.frame(&tool_result("page"), 4 * MIB).unwrap();
        budget.frame(&tool_result("page"), 4 * MIB).unwrap();
        let error = budget.frame(&tool_result("page"), 4 * MIB).unwrap_err();
        assert!(error.contains("output budget"));
    }

    #[test]
    fn with_no_tool_reply_an_echo_counts_in_full() {
        let mut budget = OutputBudget::new(10 * MIB);
        budget
            .frame(&tool_use("page", "mcp__pipeline__x"), 0)
            .unwrap();
        assert!(budget.frame(&tool_result("page"), 11 * MIB).is_err());
    }

    #[test]
    fn claude_s_own_tools_and_writing_are_counted() {
        let mut budget = OutputBudget::new(10 * MIB);
        read_page(&mut budget, "page").unwrap();
        budget.frame(&tool_use("bash", "Bash"), 0).unwrap();
        let native = budget.frame(&tool_result("bash"), 11 * MIB);
        assert!(native.is_err(), "a native command's output is not ours");

        let mut budget = OutputBudget::new(10 * MIB);
        read_page(&mut budget, "page").unwrap();
        let written = json!({"type":"assistant","message":{"content":[{"type":"text"}]}});
        assert!(budget.frame(&written, 11 * MIB).is_err());
    }

    #[test]
    fn only_a_tool_reply_earns_an_allowance() {
        let mut budget = OutputBudget::new(MIB);
        budget
            .frame(&tool_use("page", "mcp__pipeline__x"), 0)
            .unwrap();
        let listing =
            json!({"request":{"subtype":"mcp_message","message":{"method":"tools/list"}}});
        budget.supplied(&listing, 8 * MIB);
        assert!(budget.frame(&tool_result("page"), 2 * MIB).is_err());
    }

    #[test]
    fn a_process_cannot_grow_the_call_list_without_limit() {
        let mut budget = OutputBudget::default();
        for call in 0..2 * TRACKED_CALLS {
            let frame = tool_use(&format!("call-{call}"), "mcp__pipeline__x");
            budget.frame(&frame, 10).unwrap();
        }
        assert_eq!(budget.pipeline_calls.len(), TRACKED_CALLS);
    }
}
