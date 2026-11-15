//! Read-only reviewer discovery over the build-validated module catalog.
//!
//! Routing cards contain scope, never executable prompts. Retrieval is local,
//! deterministic, and independent for subjects and methods. A run keeps the
//! cards actually supplied to its planner; final selection must come from that
//! set. Batch expansion cannot install roles or change their permissions.

mod search;
#[cfg(test)]
mod tests;

use super::{catalog_revision, MethodLevel, SubjectLevel, METHODS, SUBJECTS};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::LazyLock;

pub const MAX_BATCH_CALLS: usize = 4;
const MAX_RESULTS: usize = 12;
const MAX_SUPPLIED_CARDS: usize = 80;
const MAX_QUERY_BYTES: usize = 2_000;
pub const REVISION: &str = "reviewer-registry-v1";

#[derive(Debug, Clone, Serialize)]
pub struct RoutingCard {
    pub id: &'static str,
    pub kind: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub group_label: &'static str,
    pub fallback: bool,
    pub description: &'static str,
    pub exclusions: &'static str,
}

pub fn cards() -> &'static [RoutingCard] {
    static CARDS: LazyLock<Vec<RoutingCard>> = LazyLock::new(|| {
        SUBJECTS
            .iter()
            .map(|role| RoutingCard {
                id: role.id,
                kind: "subject",
                label: role.label,
                group: role.discipline_id,
                group_label: role.discipline_label,
                fallback: role.level == SubjectLevel::Discipline,
                description: role.routing_description,
                exclusions: role.routing_exclusions,
            })
            .chain(METHODS.iter().map(|role| RoutingCard {
                id: role.id,
                kind: "method",
                label: role.label,
                group: role.family_id,
                group_label: role.family_label,
                fallback: role.level == MethodLevel::Family,
                description: role.routing_description,
                exclusions: role.routing_exclusions,
            }))
            .collect()
    });
    &CARDS
}

pub fn directory() -> Value {
    let mut seen = BTreeSet::new();
    Value::Array(
        cards()
            .iter()
            .filter(|card| seen.insert((card.kind, card.group)))
            .map(|card| {
                json!({
                    "kind":card.kind, "group":card.group, "label":card.group_label
                })
            })
            .collect(),
    )
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCall {
    pub tool: String,
    #[serde(default)]
    pub queries: Vec<String>,
    #[serde(default)]
    pub ids: Vec<String>,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub group: String,
}

pub fn tool_schema() -> Value {
    json!({
        "type":"array", "maxItems":MAX_BATCH_CALLS,
        "description":"Batch independent reviewer lookups. SearchReviewers searches queries together; GetReviewers reads exact IDs. Unused fields are empty arrays or strings.",
        "items":{
            "type":"object", "required":["tool","queries","ids","kind","group"],
            "properties":{
                "tool":{"type":"string","enum":["SearchReviewers","GetReviewers"]},
                "queries":{"type":"array","maxItems":4,"items":{"type":"string"}},
                "ids":{"type":"array","maxItems":MAX_RESULTS,"uniqueItems":true,"items":{"type":"string"}},
                "kind":{"type":"string","enum":["","subject","method"]},
                "group":{"type":"string","description":"Optional exact group ID from the directory. Empty searches all groups."}
            }
        }
    })
}

/// A planner's private discovery ledger. Only host-returned cards enter it.
pub struct RegistrySession {
    supplied: BTreeSet<&'static str>,
    prefetched: Vec<&'static str>,
    calls: Vec<Value>,
}

impl RegistrySession {
    pub fn prefetch(text: &str) -> Self {
        let mut session = Self {
            supplied: BTreeSet::new(),
            prefetched: Vec::new(),
            calls: Vec::new(),
        };
        let queries = search::document_queries(text);
        for (kind, limit) in [("subject", 8), ("method", 12)] {
            let selected = search::rank(&queries, kind, "", limit);
            for card in selected {
                session.supplied.insert(card.id);
            }
        }
        // Broad alternatives remain available without making a second read
        // mandatory; their exclusions still govern the final panel.
        let groups = session
            .supplied
            .iter()
            .filter_map(|id| cards().iter().find(|c| c.id == *id))
            .map(|c| (c.kind, c.group))
            .collect::<BTreeSet<_>>();
        for card in cards()
            .iter()
            .filter(|c| c.fallback && groups.contains(&(c.kind, c.group)))
        {
            session.supplied.insert(card.id);
        }
        session.prefetched = session.supplied.iter().copied().collect();
        session
    }

    pub fn supplied_cards(&self) -> Vec<&RoutingCard> {
        cards()
            .iter()
            .filter(|card| self.supplied.contains(card.id))
            .collect()
    }

    pub fn prompt(&self) -> String {
        format!("REVIEWER REGISTRY\n{}\n\nSUPPLIED ROUTING CARDS\n{}\n\nCandidates are suggestions, not assignments. Search across groups when needed, including methods outside the subject discipline. Select only roles whose cards have been supplied. Full reviewer instructions are loaded by the host after selection.\n",
            directory(), serde_json::to_string(&self.supplied_cards()).expect("static routing cards"))
    }

    pub fn restrict_schema(&self, schema: &mut Value) {
        for (field, kind) in [
            ("subject_specialist_ids", "subject"),
            ("method_specialist_ids", "method"),
        ] {
            let values = self
                .supplied_cards()
                .into_iter()
                .filter(|card| card.kind == kind)
                .map(|card| json!(card.id))
                .collect::<Vec<_>>();
            schema["properties"]["review_plan"]["properties"][field]["items"]["enum"] =
                json!(values);
        }
    }

    pub fn validate_selection(&self, orientation: &Value) -> Result<(), String> {
        for field in ["subject_specialist_ids", "method_specialist_ids"] {
            for id in orientation["review_plan"][field]
                .as_array()
                .into_iter()
                .flatten()
            {
                if !id.as_str().is_some_and(|id| self.supplied.contains(id)) {
                    return Err(format!("Reviewer {id} has not been supplied. Use GetReviewers or SearchReviewers first."));
                }
            }
        }
        Ok(())
    }

    /// Validate the whole batch before publishing any cards. Failed requests
    /// cannot authorize a final selection via a partially changed ledger.
    pub fn expand(&mut self, calls: &[ToolCall]) -> Result<Value, String> {
        if calls.is_empty() || calls.len() > MAX_BATCH_CALLS {
            return Err(format!(
                "Request 1 to {MAX_BATCH_CALLS} catalog operations per batch."
            ));
        }
        let mut supplied = self.supplied.clone();
        let mut results = Vec::new();
        for call in calls {
            if !["", "subject", "method"].contains(&call.kind.as_str()) {
                return Err("Reviewer kind must be subject, method, or empty.".into());
            }
            if !call.group.is_empty()
                && !cards()
                    .iter()
                    .any(|c| c.group == call.group && (call.kind.is_empty() || c.kind == call.kind))
            {
                return Err(format!("Unknown reviewer group: {}", call.group));
            }
            let found: Vec<&RoutingCard> = match call.tool.as_str() {
                "SearchReviewers" => {
                    if !call.ids.is_empty()
                        || call.queries.is_empty()
                        || call.queries.len() > 4
                        || call
                            .queries
                            .iter()
                            .any(|q| q.trim().is_empty() || q.len() > MAX_QUERY_BYTES)
                    {
                        return Err("SearchReviewers needs 1 to 4 nonempty queries of at most 2000 bytes and no IDs.".into());
                    }
                    search::rank(&call.queries, &call.kind, &call.group, MAX_RESULTS)
                }
                "GetReviewers" => {
                    if !call.queries.is_empty()
                        || call.ids.is_empty()
                        || call.ids.len() > MAX_RESULTS
                        || !call.kind.is_empty()
                        || !call.group.is_empty()
                    {
                        return Err(
                            "GetReviewers needs 1 to 12 IDs and no queries or filters.".into()
                        );
                    }
                    call.ids
                        .iter()
                        .map(|id| {
                            cards()
                                .iter()
                                .find(|c| c.id == id)
                                .ok_or_else(|| format!("Unknown reviewer: {id}"))
                        })
                        .collect::<Result<_, _>>()?
                }
                _ => return Err(format!("Unknown catalog operation: {}", call.tool)),
            };
            supplied.extend(found.iter().map(|card| card.id));
            if supplied.len() > MAX_SUPPLIED_CARDS {
                return Err(
                    "Reviewer discovery reached its 80-card limit; use the supplied cards.".into(),
                );
            }
            results.push(json!({"request":call,"cards":found}));
        }
        self.supplied = supplied;
        self.calls.extend(results.clone());
        Ok(json!({"catalog_revision":catalog_revision(),"results":results}))
    }

    pub fn snapshot(&self, orientation: &Value) -> Value {
        let selected = ["subject_specialist_ids", "method_specialist_ids"]
            .into_iter()
            .flat_map(|field| {
                orientation["review_plan"][field]
                    .as_array()
                    .into_iter()
                    .flatten()
            })
            .filter_map(Value::as_str)
            .filter_map(super::copyable_specialist_step)
            .collect::<Vec<_>>();
        json!({"version":REVISION,"catalog_revision":catalog_revision(),
            "prefetched_ids":self.prefetched,"supplied_cards":self.supplied_cards(),
            "expansions":self.calls,"selected_modules":selected})
    }
}
