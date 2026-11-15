use super::definition::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    #[serde(default)]
    pub sequence: u32,
    pub address: String,
    pub step_id: String,
    pub label: String,
    pub state: String,
    pub operation: String,
    /// Absent on receipts written before the dispatch-marker contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_kind: Option<super::adapters::contract::Kind>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub wake_at: Option<i64>,
    pub output: Option<Value>,
    pub error: Option<String>,
    #[serde(default)]
    pub child: Option<Value>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// Workspace conversation created exclusively for this scheduled run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence_session: Option<String>,
    /// Researcher-supplied corrections to an unchanged snapshot, scoped to its iteration.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub corrected_snapshots: BTreeMap<String, Value>,
    pub receipts: BTreeMap<String, Receipt>,
    #[serde(default = "object")]
    pub outputs: Value,
    #[serde(default)]
    pub decisions: BTreeMap<String, Value>,
    #[serde(default)]
    pub actions: u32,
    /// Absolute cutoffs for open groups, keyed by the actual iteration/branch address.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub group_deadlines: BTreeMap<String, i64>,
    #[serde(default)]
    pub limit_reached: bool,
}
fn object() -> Value {
    json!({})
}
#[derive(Debug, Clone)]
pub struct Ready {
    pub address: String,
    pub step: Step,
    pub inputs: Value,
    /// Enclosing group instance prefixes and their individual action ceilings.
    pub groups: Vec<(String, u32)>,
    pub timeout_secs: u32,
}
impl Ready {
    pub(crate) fn cutoff(&self, progress: &Progress, deadline: i64, start: i64) -> i64 {
        self.groups
            .iter()
            .filter_map(|(prefix, _)| progress.group_deadlines.get(prefix.trim_end_matches('/')))
            .copied()
            .fold(
                deadline.min(start.saturating_add(i64::from(self.timeout_secs))),
                i64::min,
            )
    }
}

#[derive(Debug)]
pub enum Advance {
    Ready(Vec<Ready>),
    Waiting,
    Finished,
    Attention(String),
}

pub fn advance(chain: &Chain, progress: &mut Progress, inputs: &Value) -> Advance {
    advance_at(chain, progress, inputs, super::store::now())
}

pub(crate) fn advance_at(
    chain: &Chain,
    progress: &mut Progress,
    inputs: &Value,
    time: i64,
) -> Advance {
    if progress.receipts.len() >= MAX_INSTANCES {
        return Advance::Attention("Task instance limit reached".into());
    }
    let mut budget = WalkBudget::new(progress, time, chain.limits.action_timeout_secs);
    match walk(&chain.steps, "", progress, inputs, &mut budget) {
        Ok((true, _)) => Advance::Finished,
        Ok((false, ready)) if ready.is_empty() => Advance::Waiting,
        Ok((false, ready)) => Advance::Ready(ready),
        Err(e) => Advance::Attention(e),
    }
}
fn walk(
    steps: &[Step],
    prefix: &str,
    p: &mut Progress,
    inputs: &Value,
    budget: &mut WalkBudget,
) -> Result<(bool, Vec<Ready>), String> {
    for step in steps {
        budget.tick()?;
        let address = format!("{prefix}{}", step.id);
        if p.decisions.contains_key(&format!("{address}/done")) {
            continue;
        }
        match &step.action {
            Action::If {
                condition,
                then_steps,
                else_steps,
            } => {
                let choice = if let Some(v) = p.decisions.get(&address) {
                    v.as_bool().unwrap_or(false)
                } else {
                    let choice = match condition.evaluate(&p.outputs, inputs) {
                        Truth::True => true,
                        Truth::False => false,
                        Truth::Unknown => {
                            return Err(format!(
                                "{}: condition has missing or unknown inputs",
                                step.label
                            ))
                        }
                    };
                    budget.record(p, address.clone(), json!(choice))?;
                    choice
                };
                let result = walk(
                    if choice { then_steps } else { else_steps },
                    &format!("{address}/"),
                    p,
                    inputs,
                    budget,
                )?;
                if !result.0 {
                    return Ok(result);
                }
                budget.record(p, format!("{address}/done"), json!(true))?;
            }
            Action::Repeat {
                max_iterations,
                until,
                steps,
            } => {
                for i in 0..*max_iterations {
                    let key = format!("{address}/{i}");
                    if let Some(value) = p.decisions.get(&key) {
                        if value == &json!(true) {
                            break;
                        } else {
                            continue;
                        }
                    }
                    let result = walk(steps, &format!("{key}/"), p, inputs, budget)?;
                    if !result.0 {
                        return Ok(result);
                    }
                    let stop = match until.evaluate(&p.outputs, inputs) {
                        Truth::True => true,
                        Truth::False => false,
                        Truth::Unknown => {
                            return Err(format!("{}: stop condition is unknown", step.label))
                        }
                    };
                    budget.record(p, key, json!(stop))?;
                    if stop {
                        break;
                    }
                    if i + 1 == *max_iterations {
                        p.limit_reached = true;
                    }
                }
                budget.record(p, format!("{address}/done"), json!(true))?;
            }
            Action::Parallel { branches } => {
                let mut complete = true;
                let mut ready = Vec::new();
                for (i, branch) in branches.iter().enumerate() {
                    let (done, mut pending) =
                        walk(branch, &format!("{address}/{i}/"), p, inputs, budget)?;
                    complete &= done;
                    ready.append(&mut pending);
                }
                if !complete {
                    return Ok((false, ready));
                }
                budget.record(p, format!("{address}/done"), json!(true))?;
            }
            Action::While {
                max_iterations,
                condition,
                steps,
            } => {
                for i in 0..*max_iterations {
                    let key = format!("{address}/{i}/enter");
                    let enter = if let Some(value) = p.decisions.get(&key) {
                        value == &json!(true)
                    } else {
                        let enter = match condition.evaluate(&p.outputs, inputs) {
                            Truth::True => true,
                            Truth::False => false,
                            Truth::Unknown => {
                                return Err(format!("{}: while condition is unknown", step.label))
                            }
                        };
                        budget.record(p, key, json!(enter))?;
                        enter
                    };
                    if !enter {
                        break;
                    }
                    let result = walk(steps, &format!("{address}/{i}/"), p, inputs, budget)?;
                    if !result.0 {
                        return Ok(result);
                    }
                    // The condition reads saved outputs, so asking once more
                    // costs no model call. Only a condition that is plainly
                    // false shows the loop finished on this pass.
                    if i + 1 == *max_iterations
                        && condition.evaluate(&p.outputs, inputs) != Truth::False
                    {
                        p.limit_reached = true;
                    }
                }
                budget.record(p, format!("{address}/done"), json!(true))?;
            }
            Action::ForEach {
                input,
                max_items,
                steps,
            } => {
                let items = if let Some(v) = p.decisions.get(&address) {
                    v.clone()
                } else {
                    let value = input
                        .resolve(&p.outputs, inputs)
                        .ok_or("For each input is unavailable")?;
                    let items = value.as_array().ok_or("For each requires an array")?;
                    if items.len() > *max_items as usize {
                        return Err("For each input exceeds its item limit".into());
                    }
                    budget.record(p, address.clone(), value.clone())?;
                    value
                };
                for (i, item) in items
                    .as_array()
                    .ok_or("Invalid saved iteration input")?
                    .iter()
                    .enumerate()
                {
                    let mut local = inputs.clone();
                    local["item"] = item.clone();
                    local["index"] = json!(i);
                    let result = walk(steps, &format!("{address}/{i}/"), p, &local, budget)?;
                    if !result.0 {
                        return Ok(result);
                    }
                }
                budget.record(p, format!("{address}/done"), json!(true))?;
            }
            Action::Chain { chain } => {
                let prefix = format!("{address}/");
                let deadline = *p.group_deadlines.entry(address.clone()).or_insert_with(|| {
                    // Older receipts have no saved group clock. Their first action proves entry.
                    let start = p
                        .receipts
                        .values()
                        .filter(|r| r.address.starts_with(&prefix))
                        .map(|r| r.started_at)
                        .min()
                        .unwrap_or(budget.time);
                    start.saturating_add(i64::from(chain.limits.deadline_hours) * 3600)
                });
                if deadline <= budget.time {
                    return Err(format!("Group {address} reached its elapsed deadline. Start a new run to give it more time."));
                }
                let (finished, mut ready) = walk(&chain.steps, &prefix, p, inputs, budget)?;
                if !finished {
                    for action in &mut ready {
                        action.timeout_secs =
                            action.timeout_secs.min(chain.limits.action_timeout_secs);
                        action
                            .groups
                            .push((prefix.clone(), chain.limits.max_actions));
                    }
                    return Ok((false, ready));
                }
                p.group_deadlines.remove(&address);
                budget.record(p, format!("{address}/done"), json!(true))?;
            }
            _ => match p.receipts.get(&address) {
                Some(r) if r.state == "completed" => {}
                Some(r) if matches!(r.state.as_str(), "failed" | "unknown") => {
                    return Err(r
                        .error
                        .clone()
                        .unwrap_or_else(|| "Action needs attention".into()))
                }
                Some(_) => return Ok((false, Vec::new())),
                None => {
                    return Ok((
                        false,
                        vec![Ready {
                            address,
                            step: step.clone(),
                            inputs: inputs.clone(),
                            groups: Vec::new(),
                            timeout_secs: budget.timeout,
                        }],
                    ))
                }
            },
        }
    }
    Ok((true, Vec::new()))
}

// Bound synchronous control evaluation independently of dispatched actions. This
// also applies to all-control chains, including nested empty branches.
const MAX_CONTROL_DECISIONS: usize = 4096;
pub(super) const MAX_PROGRESS_BYTES: usize = 8 * 1024 * 1024;
struct WalkBudget {
    time: i64,
    timeout: u32,
    remaining: usize,
    bytes: usize,
}
impl WalkBudget {
    fn new(progress: &Progress, time: i64, timeout: u32) -> Self {
        Self {
            time,
            timeout,
            remaining: 32768,
            bytes: serde_json::to_vec(progress).map_or(usize::MAX, |v| v.len()),
        }
    }
    fn tick(&mut self) -> Result<(), String> {
        if self.remaining == 0 || self.bytes >= MAX_PROGRESS_BYTES {
            return Err("Task control evaluation or context budget reached".into());
        }
        self.remaining -= 1;
        Ok(())
    }
    fn record(&mut self, p: &mut Progress, key: String, value: Value) -> Result<(), String> {
        self.tick()?;
        let bytes = serde_json::to_vec(&(&key, &value))
            .map_err(|e| e.to_string())?
            .len();
        if p.decisions.len() >= MAX_CONTROL_DECISIONS
            || bytes >= MAX_PROGRESS_BYTES.saturating_sub(self.bytes)
        {
            return Err("Task control decision or context budget reached".into());
        }
        self.bytes += bytes;
        p.decisions.insert(key, value);
        Ok(())
    }
}

impl Progress {
    pub fn expired_group(&self, time: i64) -> Option<&str> {
        self.group_deadlines
            .iter()
            .find(|(_, until)| **until <= time)
            .map(|(group, _)| group.as_str())
    }

    pub fn next_wake(&self, due: Option<i64>) -> Option<i64> {
        self.group_deadlines.values().copied().chain(due).min()
    }
}

pub(super) fn context_within_budget(progress: &Progress) -> bool {
    serde_json::to_vec(progress).is_ok_and(|bytes| bytes.len() < MAX_PROGRESS_BYTES)
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    #[test]
    fn empty_nested_controls_are_bounded_without_dispatch() {
        let mut step = json!({"id":"skip","label":"skip","kind":"if","condition":{"op":"equals","left":{"kind":"literal","value":true},"right":false},"thenSteps":[],"elseSteps":[]});
        // Use the normal deserializer for a real portable control chain.
        let false_condition = step["condition"].clone();
        for n in 0..4 {
            step = json!({"id":format!("repeat{n}"),"label":"repeat","kind":"repeat","maxIterations":20,"until":false_condition,"steps":[step]});
        }
        let chain: Chain =
            serde_json::from_value(json!({"schemaVersion":1,"name":"bounded","steps":[step]}))
                .unwrap();
        let mut p = Progress::default();
        assert!(matches!(
            advance(&chain, &mut p, &json!({})),
            Advance::Attention(_)
        ));
        assert!(p.decisions.len() <= MAX_CONTROL_DECISIONS);
        assert_eq!(p.actions, 0);
        assert!(serde_json::to_vec(&p).unwrap().len() < MAX_PROGRESS_BYTES);
    }
}
