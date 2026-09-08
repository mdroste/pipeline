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
    pub receipts: BTreeMap<String, Receipt>,
    #[serde(default = "object")]
    pub outputs: Value,
    #[serde(default)]
    pub decisions: BTreeMap<String, Value>,
    #[serde(default)]
    pub actions: u32,
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
}
#[derive(Debug)]
pub enum Advance {
    Ready(Vec<Ready>),
    Waiting,
    Finished,
    Attention(String),
}

pub fn advance(chain: &Chain, progress: &mut Progress, inputs: &Value) -> Advance {
    if progress.receipts.len() >= MAX_INSTANCES {
        return Advance::Attention("Task instance limit reached".into());
    }
    match walk(&chain.steps, "", progress, inputs) {
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
) -> Result<(bool, Vec<Ready>), String> {
    for step in steps {
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
                    p.decisions.insert(address.clone(), json!(choice));
                    choice
                };
                let result = walk(
                    if choice { then_steps } else { else_steps },
                    &format!("{address}/"),
                    p,
                    inputs,
                )?;
                if !result.0 {
                    return Ok(result);
                }
                p.decisions.insert(format!("{address}/done"), json!(true));
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
                    let result = walk(steps, &format!("{key}/"), p, inputs)?;
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
                    p.decisions.insert(key, json!(stop));
                    if stop {
                        break;
                    }
                    if i + 1 == *max_iterations {
                        p.limit_reached = true;
                    }
                }
                p.decisions.insert(format!("{address}/done"), json!(true));
            }
            Action::Parallel { branches } => {
                let mut complete = true;
                let mut ready = Vec::new();
                for (i, branch) in branches.iter().enumerate() {
                    let (done, mut pending) = walk(branch, &format!("{address}/{i}/"), p, inputs)?;
                    complete &= done;
                    ready.append(&mut pending);
                }
                if !complete {
                    return Ok((false, ready));
                }
                p.decisions.insert(format!("{address}/done"), json!(true));
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
                        p.decisions.insert(key, json!(enter));
                        enter
                    };
                    if !enter {
                        break;
                    }
                    let result = walk(steps, &format!("{address}/{i}/"), p, inputs)?;
                    if !result.0 {
                        return Ok(result);
                    }
                    if i + 1 == *max_iterations {
                        p.limit_reached = true;
                    }
                }
                p.decisions.insert(format!("{address}/done"), json!(true));
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
                    p.decisions.insert(address.clone(), value.clone());
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
                    let result = walk(steps, &format!("{address}/{i}/"), p, &local)?;
                    if !result.0 {
                        return Ok(result);
                    }
                }
                p.decisions.insert(format!("{address}/done"), json!(true));
            }
            Action::Chain { chain } => {
                let result = walk(&chain.steps, &format!("{address}/"), p, inputs)?;
                if !result.0 {
                    return Ok(result);
                }
                p.decisions.insert(format!("{address}/done"), json!(true));
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
                        }],
                    ))
                }
            },
        }
    }
    Ok((true, Vec::new()))
}
