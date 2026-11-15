use super::{stable_snapshot_fingerprint, RunSnapshot};
use crate::pipeline_config::PipelineConfig;

/// What a manifest keeps in place of a secret variable's value.
const REDACTED_SECRET: &str = "[redacted secret; re-enter to rerun]";

pub(in crate::commands) fn bind_runtime_snapshot(
    mut snapshot: RunSnapshot,
    variables: &std::collections::HashMap<String, String>,
    extra_inputs: &std::collections::HashMap<String, String>,
) -> Result<RunSnapshot, String> {
    validate_runtime_bindings(&snapshot.config, variables, extra_inputs, false)?;
    let variables = variables
        .iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    let extra_inputs = extra_inputs
        .iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    // Chain from the current fingerprint, not config_fingerprint: the
    // one-run agent override has already been folded in by
    // bind_model_overrides, and rebasing on the config fingerprint would
    // silently drop it from the plan/launch mismatch check.
    snapshot.fingerprint =
        stable_snapshot_fingerprint(&(snapshot.fingerprint.as_str(), variables, extra_inputs))
            .map_err(|error| format!("Could not fingerprint runtime options: {error}"))?;
    Ok(snapshot)
}

pub(in crate::commands) fn validate_runtime_bindings(
    config: &PipelineConfig,
    variables: &std::collections::HashMap<String, String>,
    extra_inputs: &std::collections::HashMap<String, String>,
    require_required: bool,
) -> Result<(), String> {
    let declared_variables = config
        .variables
        .iter()
        .map(|spec| (spec.key.as_str(), spec))
        .collect::<std::collections::HashMap<_, _>>();
    for key in variables.keys() {
        if !declared_variables.contains_key(key.as_str()) {
            return Err(format!("Run supplied undeclared variable '{key}'"));
        }
    }
    for spec in &config.variables {
        let value = variables
            .get(&spec.key)
            .map(String::as_str)
            .unwrap_or(&spec.default);
        let label = if spec.label.trim().is_empty() {
            spec.key.as_str()
        } else {
            spec.label.as_str()
        };
        if require_required && spec.required && value.trim().is_empty() {
            return Err(format!("Missing required variable '{label}'"));
        }
        if require_required && spec.secret && value == REDACTED_SECRET {
            return Err(format!(
                "Secret variable '{label}' must be entered again before this run"
            ));
        }
        if value.is_empty() {
            continue;
        }
        let length = value.chars().count() as u32;
        if spec
            .validation
            .min_length
            .is_some_and(|minimum| length < minimum)
        {
            return Err(format!(
                "Variable '{label}' must contain at least {} characters",
                spec.validation.min_length.unwrap_or_default()
            ));
        }
        if spec
            .validation
            .max_length
            .is_some_and(|maximum| length > maximum)
        {
            return Err(format!(
                "Variable '{label}' must contain at most {} characters",
                spec.validation.max_length.unwrap_or_default()
            ));
        }
        if !spec.validation.pattern.is_empty()
            && !regex::Regex::new(&spec.validation.pattern)
                .map_err(|error| format!("Variable '{label}' validation is invalid: {error}"))?
                .is_match(value)
        {
            return Err(format!(
                "Variable '{label}' does not match its required format"
            ));
        }
        if spec.kind == "choice" && !spec.choices.iter().any(|choice| choice == value) {
            return Err(format!(
                "Variable '{label}' is not one of its allowed choices"
            ));
        }
    }

    let declared_inputs = config
        .extraction
        .extra_inputs
        .iter()
        .map(|slot| slot.key.as_str())
        .collect::<std::collections::HashSet<_>>();
    for key in extra_inputs.keys() {
        if !declared_inputs.contains(key.as_str()) {
            return Err(format!("Run supplied undeclared named input '{key}'"));
        }
    }
    Ok(())
}

/// Values safe to retain in manifests. Secret values are deliberately not
/// recoverable; a re-run must ask for them again.
pub(in crate::commands) fn durable_variables(
    config: &PipelineConfig,
    variables: &std::collections::HashMap<String, String>,
) -> std::collections::HashMap<String, String> {
    variables
        .iter()
        .map(|(key, value)| {
            let secret = config
                .variables
                .iter()
                .any(|spec| spec.key == *key && spec.secret);
            (
                key.clone(),
                if secret {
                    REDACTED_SECRET.to_string()
                } else {
                    value.clone()
                },
            )
        })
        .collect()
}

/// A re-run's variables are built like a fresh run's: declared defaults, then
/// the parent's retained values, then secrets entered again for this re-run.
pub(in crate::commands) fn rerun_variables(
    config: &PipelineConfig,
    parent_variables: &std::collections::HashMap<String, String>,
    entered_secrets: std::collections::HashMap<String, String>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let mut variables: std::collections::HashMap<String, String> = config
        .variables
        .iter()
        .map(|spec| (spec.key.clone(), spec.default.clone()))
        .collect();
    variables.extend(parent_variables.clone());
    for (key, value) in entered_secrets {
        if !config
            .variables
            .iter()
            .any(|spec| spec.key == key && spec.secret)
        {
            return Err(format!(
                "Re-run supplied '{key}', which is not a secret variable of this review"
            ));
        }
        variables.insert(key, value);
    }
    Ok(variables)
}

/// Secret variables a re-run cannot fill from what the run retained.
pub(in crate::commands) fn secrets_to_reenter<'a>(
    config: &'a PipelineConfig,
    parent_variables: &std::collections::HashMap<String, String>,
) -> Vec<&'a crate::pipeline_config::VarSpec> {
    config
        .variables
        .iter()
        .filter(|spec| spec.secret)
        .filter(|spec| match parent_variables.get(&spec.key) {
            Some(value) => value == REDACTED_SECRET,
            None => spec.required && spec.default.trim().is_empty(),
        })
        .collect()
}
