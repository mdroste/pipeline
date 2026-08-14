//! `run_if` condition evaluation.
//!
//! Conditions are deterministic and evaluated in Rust — never by an LLM — so a
//! profile stays inspectable. Two checks cover the common needs: gate on the
//! survey (orientation) JSON, or gate on an earlier step's output text.

use crate::models::StepOutput;
use crate::pipeline_config::RunCondition;
use regex::Regex;

/// Whether a step's `run_if` condition is satisfied (i.e. the step should run).
///
/// Missing referents fail closed to a sensible default: an `OutputMatches` on
/// an absent/empty step does not match (so the step is skipped unless the check
/// is negated); a `SurveyPath` treats an absent pointer as "does not exist".
pub fn condition_met(
    cond: &RunCondition,
    orientation: &serde_json::Value,
    prior: &[StepOutput],
) -> bool {
    match cond {
        RunCondition::OutputMatches {
            step,
            pattern,
            negate,
        } => {
            let text = resolve_step_text(step, prior);
            let matched = match Regex::new(pattern) {
                Ok(re) => text.as_deref().map(|t| re.is_match(t)).unwrap_or(false),
                // Profiles are validated before execution. If malformed data
                // bypasses that boundary, fail closed even when negated.
                Err(_) => return false,
            };
            matched ^ *negate
        }
        RunCondition::SurveyPath {
            pointer,
            equals,
            exists,
            contains,
        } => {
            let found = orientation.pointer(pointer);
            let mut ok = true;
            if let Some(want_exists) = exists {
                ok &= found.is_some() == *want_exists;
            }
            if let Some(want) = equals {
                ok &= found.map(|v| v == want).unwrap_or(false);
            }
            if let Some(want) = contains {
                ok &= found
                    .and_then(serde_json::Value::as_array)
                    .map(|items| items.iter().any(|item| item == want))
                    .unwrap_or(false);
            }
            // Neither constraint set → default to a presence check.
            if exists.is_none() && equals.is_none() && contains.is_none() {
                ok = found.is_some();
            }
            ok
        }
    }
}

/// Text of a prior step by id: exact composite id, or the base id (joining all
/// agents' outputs for a multi-agent step).
fn resolve_step_text(step: &str, prior: &[StepOutput]) -> Option<String> {
    if let Some(o) = prior.iter().find(|o| o.step_id == step) {
        return Some(o.raw_text.clone());
    }
    let joined: Vec<&str> = prior
        .iter()
        .filter(|o| o.step_id.split('/').next() == Some(step))
        .map(|o| o.raw_text.as_str())
        .collect();
    if joined.is_empty() {
        None
    } else {
        Some(joined.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(id: &str, text: &str) -> StepOutput {
        StepOutput {
            step_id: id.into(),
            step_label: id.into(),
            phase: "parallel".into(),
            raw_text: text.into(),
            ..Default::default()
        }
    }

    fn survey() -> serde_json::Value {
        serde_json::json!({
            "metadata": { "paper_type": "empirical" },
            "flags": { "has_code": true }
        })
    }

    #[test]
    fn output_matches_hits_and_misses() {
        let prior = vec![out("triage", "SEVERITY: high, needs deep review")];
        let c = RunCondition::OutputMatches {
            step: "triage".into(),
            pattern: "SEVERITY: high".into(),
            negate: false,
        };
        assert!(condition_met(&c, &serde_json::Value::Null, &prior));

        let c2 = RunCondition::OutputMatches {
            step: "triage".into(),
            pattern: "SEVERITY: low".into(),
            negate: false,
        };
        assert!(!condition_met(&c2, &serde_json::Value::Null, &prior));
    }

    #[test]
    fn output_matches_negate_and_missing_step() {
        // Negated: runs when the pattern does NOT match.
        let prior = vec![out("triage", "all clear")];
        let c = RunCondition::OutputMatches {
            step: "triage".into(),
            pattern: "SEVERITY: high".into(),
            negate: true,
        };
        assert!(condition_met(&c, &serde_json::Value::Null, &prior));

        // Missing step → no match → false (unless negated).
        let c2 = RunCondition::OutputMatches {
            step: "absent".into(),
            pattern: "x".into(),
            negate: false,
        };
        assert!(!condition_met(&c2, &serde_json::Value::Null, &[]));
    }

    #[test]
    fn output_matches_joins_multi_agent() {
        let prior = vec![
            out("triage/claude", "clean"),
            out("triage/antigravity", "SEVERITY: high"),
        ];
        let c = RunCondition::OutputMatches {
            step: "triage".into(),
            pattern: "SEVERITY: high".into(),
            negate: false,
        };
        assert!(condition_met(&c, &serde_json::Value::Null, &prior));
    }

    #[test]
    fn invalid_regex_fails_closed_even_when_negated() {
        let condition = RunCondition::OutputMatches {
            step: "triage".into(),
            pattern: "(".into(),
            negate: true,
        };
        assert!(!condition_met(
            &condition,
            &serde_json::Value::Null,
            &[out("triage", "anything")]
        ));
    }

    #[test]
    fn survey_path_equals() {
        let c = RunCondition::SurveyPath {
            pointer: "/metadata/paper_type".into(),
            equals: Some(serde_json::json!("empirical")),
            exists: None,
            contains: None,
        };
        assert!(condition_met(&c, &survey(), &[]));

        let c2 = RunCondition::SurveyPath {
            pointer: "/metadata/paper_type".into(),
            equals: Some(serde_json::json!("theory")),
            exists: None,
            contains: None,
        };
        assert!(!condition_met(&c2, &survey(), &[]));
    }

    #[test]
    fn survey_path_exists() {
        let present = RunCondition::SurveyPath {
            pointer: "/flags/has_code".into(),
            equals: None,
            exists: Some(true),
            contains: None,
        };
        assert!(condition_met(&present, &survey(), &[]));

        let absent = RunCondition::SurveyPath {
            pointer: "/flags/missing".into(),
            equals: None,
            exists: Some(false),
            contains: None,
        };
        assert!(condition_met(&absent, &survey(), &[]));

        // Default (neither set) = presence check.
        let default_presence = RunCondition::SurveyPath {
            pointer: "/metadata/paper_type".into(),
            equals: None,
            exists: None,
            contains: None,
        };
        assert!(condition_met(&default_presence, &survey(), &[]));
    }

    #[test]
    fn survey_path_array_contains() {
        let survey = serde_json::json!({
            "review_plan": {
                "specialist_ids": ["formal_proofs", "mathematics"]
            }
        });
        let selected = RunCondition::SurveyPath {
            pointer: "/review_plan/specialist_ids".into(),
            equals: None,
            exists: None,
            contains: Some(serde_json::json!("formal_proofs")),
        };
        assert!(condition_met(&selected, &survey, &[]));

        let absent = RunCondition::SurveyPath {
            pointer: "/review_plan/specialist_ids".into(),
            equals: None,
            exists: None,
            contains: Some(serde_json::json!("causal_identification")),
        };
        assert!(!condition_met(&absent, &survey, &[]));
    }
}
