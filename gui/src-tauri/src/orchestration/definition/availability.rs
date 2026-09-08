//! Validate order without introducing implicit dependencies between branches.
use super::*;
type Outputs = BTreeSet<String>;

fn binding(value: &Binding, available: &Outputs, previous: &Outputs) -> Result<(), String> {
    match value {
        Binding::Output { step, .. } if !available.contains(step) => Err(format!(
            "Output step {step} is unavailable here. Put dependent steps in sequence; parallel branches must be independent."
        )),
        Binding::FirstAvailable { values } => {
            // A prior-iteration value is optional, so it needs a usable fallback.
            let optional = available.union(previous).cloned().collect();
            for value in values {
                binding(value, &optional, &Outputs::new())?;
            }
            if values.iter().any(|v| has_fallback(v, available)) {
                Ok(())
            } else {
                Err("First available requires a fallback available before this step".into())
            }
        }
        _ => Ok(()),
    }
}

fn has_fallback(value: &Binding, available: &Outputs) -> bool {
    match value {
        Binding::Output { step, .. } => available.contains(step),
        Binding::FirstAvailable { values } => values.iter().any(|v| has_fallback(v, available)),
        _ => true,
    }
}

fn condition(value: &Condition, available: &Outputs, previous: &Outputs) -> Result<(), String> {
    match value {
        Condition::Equals { left, .. } | Condition::LessThan { left, .. } => {
            binding(left, available, previous)
        }
        Condition::Exists { value } => binding(
            value,
            &available.union(previous).cloned().collect(),
            &Outputs::new(),
        ),
        Condition::Not { condition: inner } => condition(inner, available, previous),
        Condition::All { conditions } | Condition::Any { conditions } => {
            for inner in conditions {
                condition(inner, available, previous)?;
            }
            Ok(())
        }
    }
}

// Only an existence guard makes a prior-iteration output available to its branch.
fn guarded(value: &Condition, truth: bool) -> Outputs {
    match value {
        Condition::Exists {
            value: Binding::Output { step, .. },
        } if truth => Outputs::from([step.clone()]),
        Condition::Not { condition } => guarded(condition, !truth),
        Condition::All { conditions } if truth => {
            conditions.iter().flat_map(|c| guarded(c, true)).collect()
        }
        Condition::Any { conditions } if !truth => {
            conditions.iter().flat_map(|c| guarded(c, false)).collect()
        }
        _ => Outputs::new(),
    }
}

fn produced(steps: &[Step]) -> Outputs {
    let mut result = Outputs::new();
    for step in steps {
        match &step.action {
            Action::If {
                then_steps,
                else_steps,
                ..
            } => {
                result.extend(produced(then_steps));
                result.extend(produced(else_steps));
            }
            Action::Repeat { steps, .. }
            | Action::While { steps, .. }
            | Action::ForEach { steps, .. } => result.extend(produced(steps)),
            Action::Parallel { branches } => {
                for branch in branches {
                    result.extend(produced(branch));
                }
            }
            Action::Chain { chain } => result.extend(produced(&chain.steps)),
            _ => {
                result.insert(step.id.clone());
            }
        }
    }
    result
}

pub(super) fn validate(
    steps: &[Step],
    mut available: Outputs,
    previous: &Outputs,
) -> Result<Outputs, String> {
    for step in steps {
        match &step.action {
            Action::Workspace { prompt, .. } => {
                for reference in prompt_bindings(prompt)? {
                    binding(&reference, &available, previous)?;
                }
            }
            Action::Review { input, .. }
            | Action::Snapshot { input, .. }
            | Action::Deliver { input } => binding(input, &available, previous)?,
            Action::If {
                condition: test,
                then_steps,
                else_steps,
            } => {
                condition(test, &available, previous)?;
                let mut yes = available.clone();
                yes.extend(guarded(test, true));
                let mut no = available.clone();
                no.extend(guarded(test, false));
                validate(then_steps, yes, previous)?;
                validate(else_steps, no, previous)?;
            }
            Action::Repeat { until, steps, .. } => {
                let prior = previous.union(&produced(steps)).cloned().collect();
                let after = validate(steps, available.clone(), &prior)?;
                condition(until, &after, previous)?;
            }
            Action::While {
                condition: test,
                steps,
                ..
            } => {
                let prior = previous.union(&produced(steps)).cloned().collect();
                condition(test, &available, &prior)?;
                let mut entered = available.clone();
                entered.extend(guarded(test, true));
                validate(steps, entered, &prior)?;
            }
            Action::ForEach { input, steps, .. } => {
                binding(input, &available, previous)?;
                let prior = previous.union(&produced(steps)).cloned().collect();
                validate(steps, available.clone(), &prior)?;
            }
            Action::Parallel { branches } => {
                let all = produced(std::slice::from_ref(step));
                for branch in branches {
                    let siblings: Outputs = all.difference(&produced(branch)).cloned().collect();
                    let prior = previous.difference(&siblings).cloned().collect();
                    let prefix = available.difference(&siblings).cloned().collect();
                    validate(branch, prefix, &prior)?;
                }
            }
            Action::Chain { chain } => {
                validate(&chain.steps, available.clone(), previous)?;
            }
            _ => {}
        }
        // These outputs may be absent if a conditional/empty loop skipped them;
        // normal binding/condition evaluation still checks their actual values.
        available.extend(produced(std::slice::from_ref(step)));
    }
    Ok(available)
}
