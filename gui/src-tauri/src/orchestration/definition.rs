use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
mod availability;

pub const VERSION: u32 = 1;
pub const MAX_STEPS: usize = 128;
pub const MAX_INSTANCES: usize = 2048;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Chain {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub limits: Limits,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_actions: u32,
    pub deadline_hours: u32,
    pub action_timeout_secs: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_actions: 64,
            deadline_hours: 168,
            action_timeout_secs: 7200,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub id: String,
    pub label: String,
    #[serde(flatten)]
    pub action: Action,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Action {
    Workspace {
        prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        effort: Option<String>,
    },
    Review {
        profile_id: String,
        input: Binding,
        #[serde(default = "document")]
        interpretation: String,
        #[serde(default)]
        variables: std::collections::BTreeMap<String, String>,
    },
    Snapshot {
        input: Binding,
        #[serde(default = "paper_name")]
        filename: String,
        #[serde(default)]
        require_change: bool,
    },
    Check {
        profile_id: String,
    },
    CapturedCheck {
        plan_id: String,
    },
    Deliver {
        input: Binding,
    },
    Delay {
        seconds: u64,
    },
    Until {
        at: i64,
    },
    Input {
        prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_seconds: Option<u64>,
    },
    If {
        condition: Condition,
        then_steps: Vec<Step>,
        #[serde(default)]
        else_steps: Vec<Step>,
    },
    Repeat {
        max_iterations: u32,
        until: Condition,
        steps: Vec<Step>,
    },
    While {
        max_iterations: u32,
        condition: Condition,
        steps: Vec<Step>,
    },
    Parallel {
        branches: Vec<Vec<Step>>,
    },
    ForEach {
        input: Binding,
        max_items: u32,
        steps: Vec<Step>,
    },
    Chain {
        chain: Box<Chain>,
    },
}
fn document() -> String {
    "document".into()
}
fn paper_name() -> String {
    "paper.md".into()
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Binding {
    Literal {
        value: Value,
    },
    Output {
        step: String,
        #[serde(default)]
        pointer: String,
    },
    Input {
        key: String,
    },
    FirstAvailable {
        values: Vec<Binding>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub enum Condition {
    Equals { left: Binding, right: Value },
    LessThan { left: Binding, right: f64 },
    Exists { value: Binding },
    All { conditions: Vec<Condition> },
    Any { conditions: Vec<Condition> },
    Not { condition: Box<Condition> },
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Truth {
    True,
    False,
    Unknown,
}
impl Binding {
    pub fn resolve(&self, outputs: &Value, inputs: &Value) -> Option<Value> {
        match self {
            Self::Literal { value } => Some(value.clone()),
            Self::Output { step, pointer } => outputs.get(step)?.pointer(pointer).cloned(),
            Self::Input { key } => inputs.get(key).cloned(),
            Self::FirstAvailable { values } => {
                values.iter().find_map(|v| v.resolve(outputs, inputs))
            }
        }
    }
}
impl Condition {
    pub fn evaluate(&self, outputs: &Value, inputs: &Value) -> Truth {
        use Truth::*;
        let truth = |v| if v { True } else { False };
        match self {
            Self::Equals { left, right } => left
                .resolve(outputs, inputs)
                .map(|v| truth(v == *right))
                .unwrap_or(Unknown),
            Self::LessThan { left, right } => left
                .resolve(outputs, inputs)
                .and_then(|v| v.as_f64())
                .map(|v| truth(v < *right))
                .unwrap_or(Unknown),
            Self::Exists { value } => {
                truth(value.resolve(outputs, inputs).is_some_and(|v| !v.is_null()))
            }
            Self::Not { condition } => match condition.evaluate(outputs, inputs) {
                True => False,
                False => True,
                Unknown => Unknown,
            },
            Self::All { conditions } => {
                let values: Vec<_> = conditions
                    .iter()
                    .map(|c| c.evaluate(outputs, inputs))
                    .collect();
                if values.contains(&False) {
                    False
                } else if values.contains(&Unknown) {
                    Unknown
                } else {
                    True
                }
            }
            Self::Any { conditions } => {
                let values: Vec<_> = conditions
                    .iter()
                    .map(|c| c.evaluate(outputs, inputs))
                    .collect();
                if values.contains(&True) {
                    True
                } else if values.contains(&Unknown) {
                    Unknown
                } else {
                    False
                }
            }
        }
    }
}
pub fn validate(chain: &Chain) -> Result<(), String> {
    if chain.schema_version != VERSION {
        return Err("Unsupported chain schema version".into());
    }
    if chain.name.trim().is_empty() || chain.name.len() > 200 || chain.description.len() > 8000 {
        return Err("Name or description is invalid".into());
    }
    if chain.steps.is_empty()
        || chain.limits.max_actions == 0
        || chain.limits.max_actions > 1024
        || chain.limits.deadline_hours == 0
        || chain.limits.deadline_hours > 8760
        || !(30..=28800).contains(&chain.limits.action_timeout_secs)
    {
        return Err("Task limits are outside the supported range".into());
    }
    if serde_json::to_vec(chain).map_err(|e| e.to_string())?.len() > 256 * 1024 {
        return Err("Chain exceeds 256 KiB".into());
    }
    let mut ids = BTreeSet::new();
    validate_steps(&chain.steps, 0, &mut ids)?;
    validate_references(&chain.steps, &ids)?;
    availability::validate(&chain.steps, BTreeSet::new(), &BTreeSet::new())?;
    Ok(())
}
fn validate_steps(steps: &[Step], depth: usize, ids: &mut BTreeSet<String>) -> Result<(), String> {
    if depth > 8 {
        return Err("Chain nesting exceeds 8 levels".into());
    }
    for step in steps {
        if step.id.is_empty()
            || step.id.len() > 64
            || !step
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || !ids.insert(step.id.clone())
        {
            return Err("Step IDs must be unique identifiers of at most 64 characters".into());
        }
        if ids.len() > MAX_STEPS || step.label.len() > 200 {
            return Err("Chain has too many steps or an invalid label".into());
        }
        match &step.action {
            Action::Workspace { prompt, .. } if prompt.is_empty() || prompt.len() > 65536 => {
                return Err("Workspace prompt must contain 1–65536 bytes".into())
            }
            Action::Review { interpretation, .. }
                if !["document", "latex_project", "source_tree", "none"]
                    .contains(&interpretation.as_str()) =>
            {
                return Err("Unknown input interpretation".into())
            }
            Action::Snapshot { filename, .. }
                if filename.is_empty()
                    || filename.len() > 100
                    || filename.contains(['/', '\\'])
                    || filename == "."
                    || filename == ".." =>
            {
                return Err("Snapshot requires a plain filename".into())
            }
            Action::Delay { seconds } if *seconds > 31_536_000 => {
                return Err("Delay exceeds one year".into())
            }
            Action::Input {
                prompt,
                timeout_seconds,
            } if prompt.is_empty()
                || prompt.len() > 8000
                || timeout_seconds.is_some_and(|v| v == 0 || v > 31_536_000) =>
            {
                return Err("Invalid input wait".into())
            }
            Action::If {
                then_steps,
                else_steps,
                ..
            } => {
                validate_steps(then_steps, depth + 1, ids)?;
                validate_steps(else_steps, depth + 1, ids)?;
            }
            Action::Repeat {
                max_iterations,
                steps,
                ..
            }
            | Action::While {
                max_iterations,
                steps,
                ..
            } => {
                if !(1..=32).contains(max_iterations) || steps.is_empty() {
                    return Err("Loops require 1–32 iterations and a body".into());
                }
                validate_steps(steps, depth + 1, ids)?;
            }
            Action::Parallel { branches } => {
                if branches.is_empty() || branches.len() > 8 {
                    return Err("Parallel requires 1–8 branches".into());
                }
                for branch in branches {
                    validate_steps(branch, depth + 1, ids)?;
                }
            }
            Action::ForEach {
                max_items, steps, ..
            } => {
                if !(1..=64).contains(max_items) {
                    return Err("For each is limited to 64 items".into());
                }
                validate_steps(steps, depth + 1, ids)?;
            }
            Action::Chain { chain } => {
                if chain.schema_version != VERSION {
                    return Err("Unsupported subchain version".into());
                }
                validate_steps(&chain.steps, depth + 1, ids)?;
            }
            _ => {}
        }
    }
    Ok(())
}
/// Substitute only explicit output/input references. Never interpret arbitrary code.
const PROMPT_REFERENCE: &str = r"\{\{(output|input):([a-zA-Z0-9_-]+)(#[^}]*)?\}\}";
pub fn render_prompt(template: &str, outputs: &Value, inputs: &Value) -> Result<String, String> {
    let regex = regex::Regex::new(PROMPT_REFERENCE).map_err(|e| e.to_string())?;
    let mut missing = None;
    let result = regex
        .replace_all(template, |c: &regex::Captures<'_>| {
            let root = if &c[1] == "output" { outputs } else { inputs };
            let pointer = c.get(3).map(|v| &v.as_str()[1..]).unwrap_or("");
            match root.get(&c[2]).and_then(|v| v.pointer(pointer)) {
                Some(Value::String(v)) => v.clone(),
                Some(v) if !v.is_null() => v.to_string(),
                _ => {
                    missing = Some(c[0].to_string());
                    String::new()
                }
            }
        })
        .into_owned();
    if let Some(key) = missing {
        return Err(format!("Missing task input: {key}"));
    }
    if result.len() > 256 * 1024 {
        return Err("Task context exceeds 256 KiB; select a smaller output".into());
    }
    Ok(result)
}

pub fn catalog() -> Result<Value, String> {
    Ok(serde_json::json!({
        "schemaVersion":1,
        "profiles":crate::pipeline_config::list_profiles()?,
        "schema":serde_json::from_str::<Value>(include_str!("schema.json")).map_err(|e|e.to_string())?,
        "contextBindings":"Workspace prompts use {{input:key}} or {{output:stepId#/json/pointer}}. Bindings are explicit literal, input, output, or firstAvailable. Missing values never silently satisfy a condition.",
        "reviewOutput":{"complete":"boolean: complete review with structured findings","highPriorityCount":"integer","unknownPriorityCount":"integer","paperText":"extracted text of the exact reviewed artifact","report":"bounded Markdown report","reviewedArtifact":"immutable artifact","runId":"Review history identity"},
        "rules":["Give every step a globally unique ID. Conditions never execute code.","Snapshot Workspace text or a file inside the conversation folder before passing it to Review.","Review the latest revision before finishing; place revision before Review in subsequent loop iterations.","Use maxIterations and maxActions. A proposal is not permission to run."]
    }))
}

fn validate_binding(value: &Binding, ids: &BTreeSet<String>, depth: usize) -> Result<(), String> {
    if depth > 16 {
        return Err("Binding nesting exceeds 16 levels".into());
    }
    match value {
        Binding::Output { step, pointer } => {
            if !ids.contains(step) {
                return Err(format!("Unknown output step: {step}"));
            }
            if !pointer.is_empty() && !pointer.starts_with('/') {
                return Err("Output pointers must be empty or start with /".into());
            }
        }
        Binding::FirstAvailable { values } => {
            if values.is_empty() || values.len() > 16 {
                return Err("First available requires 1–16 bindings".into());
            }
            for v in values {
                validate_binding(v, ids, depth + 1)?;
            }
        }
        Binding::Input { key } if key.is_empty() || key.len() > 128 => {
            return Err("Invalid input key".into())
        }
        _ => {}
    }
    Ok(())
}
fn validate_condition(
    value: &Condition,
    ids: &BTreeSet<String>,
    depth: usize,
) -> Result<(), String> {
    if depth > 16 {
        return Err("Condition nesting exceeds 16 levels".into());
    }
    match value {
        Condition::Equals { left, .. } | Condition::LessThan { left, .. } => {
            validate_binding(left, ids, 0)?
        }
        Condition::Exists { value } => validate_binding(value, ids, 0)?,
        Condition::Not { condition } => validate_condition(condition, ids, depth + 1)?,
        Condition::All { conditions } | Condition::Any { conditions } => {
            if conditions.is_empty() || conditions.len() > 32 {
                return Err("Boolean conditions require 1–32 operands".into());
            }
            for c in conditions {
                validate_condition(c, ids, depth + 1)?;
            }
        }
    }
    Ok(())
}
fn validate_references(steps: &[Step], ids: &BTreeSet<String>) -> Result<(), String> {
    for s in steps {
        match &s.action {
            Action::Workspace { prompt, .. } => {
                for binding in prompt_bindings(prompt)? {
                    validate_binding(&binding, ids, 0)?;
                }
            }
            Action::Review { input, .. }
            | Action::Snapshot { input, .. }
            | Action::Deliver { input } => validate_binding(input, ids, 0)?,
            Action::If {
                condition,
                then_steps,
                else_steps,
            } => {
                validate_condition(condition, ids, 0)?;
                validate_references(then_steps, ids)?;
                validate_references(else_steps, ids)?;
            }
            Action::Repeat { until, steps, .. }
            | Action::While {
                condition: until,
                steps,
                ..
            } => {
                validate_condition(until, ids, 0)?;
                validate_references(steps, ids)?;
            }
            Action::ForEach { input, steps, .. } => {
                validate_binding(input, ids, 0)?;
                validate_references(steps, ids)?;
            }
            Action::Parallel { branches } => {
                for b in branches {
                    validate_references(b, ids)?;
                }
            }
            Action::Chain { chain } => validate_references(&chain.steps, ids)?,
            _ => {}
        }
    }
    Ok(())
}

fn prompt_bindings(prompt: &str) -> Result<Vec<Binding>, String> {
    Ok(regex::Regex::new(PROMPT_REFERENCE)
        .map_err(|e| e.to_string())?
        .captures_iter(prompt)
        .filter(|c| &c[1] == "output")
        .map(|c| Binding::Output {
            step: c[2].to_owned(),
            pointer: c
                .get(3)
                .map(|v| v.as_str()[1..].to_owned())
                .unwrap_or_default(),
        })
        .collect())
}
