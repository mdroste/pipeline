//! Ordered harness resolution, prompt composition, and compatibility fingerprints.

use super::config::{find_preset, load_config, merge_config};
use super::context::context_for_session;
use super::tool_catalog::dynamic_tools;
use super::{
    harness_modules, hash_bytes, open_connection, validate_id, EffectiveHarness, HarnessModule,
    InstructionSection, ModuleAvailability, DEFAULT_CONTEXT_BYTES, HARNESS_SCHEMA_VERSION,
    MAX_CONTEXT_BYTES, MAX_INSTRUCTIONS_BYTES, WORKSPACE_PREAMBLE,
};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use serde_json::{json, Map, Value};

pub fn resolve_harness(store: &Store, session_id: &str) -> WorkbenchResult<EffectiveHarness> {
    validate_id("session id", session_id)?;
    let snapshot = store.session_snapshot(session_id)?;
    let global = load_config(store, None)?;
    let workspace = match snapshot.workspace.as_ref() {
        Some(value) => Some(load_config(store, Some(&value.id))?),
        None => None,
    };
    let mut values = Map::new();
    let mut sources = Map::new();
    for (key, value) in [
        ("mode", json!("inspect")),
        ("webSearch", json!(false)),
        ("commandNetwork", json!(false)),
        ("contextBudgetBytes", json!(DEFAULT_CONTEXT_BYTES)),
    ] {
        values.insert(key.to_string(), value);
        sources.insert(key.to_string(), json!("builtIn"));
    }
    merge_config(&mut values, &global.body, "global", &mut sources);
    if let Some(config) = workspace.as_ref() {
        merge_config(&mut values, &config.body, "workspace", &mut sources);
    }
    merge_config(
        &mut values,
        &snapshot.session.overrides,
        "conversation",
        &mut sources,
    );
    let preset_id = snapshot.session.preset_id.as_deref().unwrap_or("plain");
    let preset = find_preset(store, snapshot.session.workspace_id.as_deref(), preset_id)?;
    let mode = values
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("inspect");
    if !matches!(mode, "inspect" | "edit") {
        return Err(WorkbenchError::invalid(
            "Workspace mode must be inspect or edit",
        ));
    }
    let budget = values
        .get("contextBudgetBytes")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_CONTEXT_BYTES as u64) as usize;
    if !(4_096..=MAX_CONTEXT_BYTES).contains(&budget) {
        return Err(WorkbenchError::invalid(
            "Context budget must be between 4 KiB and 512 KiB",
        ));
    }
    let has_workspace = snapshot.workspace.is_some();
    let has_tested_execution_profile = snapshot.session.workspace_id.as_deref().map(|workspace_id| {
        open_connection(store)?.query_row(
            "SELECT EXISTS(SELECT 1 FROM execution_profiles WHERE workspace_id = ?1 AND test_status = 'passed')",
            [workspace_id],
            |row| row.get::<_, bool>(0),
        ).map_err(|error| WorkbenchError::storage("Failed to inspect execution capabilities", error))
    }).transpose()?.unwrap_or(false);
    let catalog = harness_modules();
    let in_task_copy = crate::workbench::discovery::is_role(store, session_id)?
        || snapshot
            .session
            .overrides
            .get("projectCheckpointId")
            .is_some();
    let module_availability = catalog
        .iter()
        .map(|spec| {
            let reasons = module_unavailability_reasons(
                spec,
                has_workspace,
                mode,
                has_tested_execution_profile,
                in_task_copy,
            );
            ModuleAvailability {
                id: spec.id.to_string(),
                available: reasons.is_empty(),
                reasons,
            }
        })
        .collect::<Vec<_>>();
    let mut enabled = Vec::new();
    let mut unavailable = Vec::new();
    for module in &preset.modules {
        let Some(availability) = module_availability
            .iter()
            .find(|candidate| candidate.id == *module)
        else {
            return Err(WorkbenchError::invalid(
                "Preset references an unavailable module",
            ));
        };
        if availability.available {
            enabled.push(module.clone());
        } else {
            unavailable.push(module.clone());
        }
    }
    let (mut context_preview, mut context_truncated) =
        if enabled.iter().any(|module| module == "paper_context") {
            context_for_session(
                store,
                snapshot.session.workspace_id.as_deref(),
                snapshot.session.paper_id.as_deref(),
                budget,
            )?
        } else {
            (String::new(), false)
        };
    let task_context = crate::workbench::tasks::returned_context(store, session_id)?;
    if !task_context.is_empty() {
        let remaining = budget.saturating_sub(task_context.len());
        context_truncated |= context_preview.len() > remaining;
        context_preview = format!(
            "{}\n{}",
            task_context,
            crate::workbench::search::prefix(&context_preview, remaining)
        );
    }
    let selected_context = crate::workbench::desk::context_preview(store, session_id)?;
    if !selected_context.is_empty() {
        // Explicit selections take priority within the same per-turn budget.
        context_truncated |= selected_context.len() > budget;
        let selected = crate::workbench::search::prefix(&selected_context, budget);
        let remaining = budget.saturating_sub(selected.len() + 1);
        context_truncated |= context_preview.len() > remaining;
        context_preview = if remaining == 0 {
            selected.to_string()
        } else {
            format!(
                "{}\n{}",
                selected,
                crate::workbench::search::prefix(&context_preview, remaining)
            )
        };
    }
    let mut developer = if preset.base_instructions.is_some() {
        WORKSPACE_PREAMBLE.replace("Preserve Codex base instructions. ", "")
    } else {
        WORKSPACE_PREAMBLE.to_string()
    };
    if snapshot
        .session
        .overrides
        .get("runtime")
        .and_then(Value::as_str)
        == Some("claude")
    {
        developer = developer.replace(
            "Preserve Codex base instructions.",
            "Preserve Claude Code base instructions.",
        );
    }
    let mut sections = vec![InstructionSection {
        id: "preamble".to_string(),
        label: "Pipeline instructions".to_string(),
        text: developer.clone(),
    }];
    if !preset.instructions.is_empty() {
        developer.push_str("\n\nResearch harness instructions:\n");
        developer.push_str(&preset.instructions);
        sections.push(InstructionSection {
            id: "preset".to_string(),
            label: format!("Preset instructions: {}", preset.name),
            text: preset.instructions.clone(),
        });
    }
    let active_recipe = snapshot
        .session
        .overrides
        .get("recipeId")
        .and_then(Value::as_str)
        .map(|recipe_id| {
            crate::workbench::release::find_recipe(
                store,
                snapshot.session.workspace_id.as_deref(),
                recipe_id,
            )
        })
        .transpose()?;
    if let Some(recipe) = active_recipe.as_ref() {
        let start = developer.len();
        developer.push_str("\n\nOptional research recipe (versioned harness module):\n");
        developer.push_str(&recipe.instructions);
        developer.push_str("\nExpected checks: ");
        developer.push_str(&recipe.expected_checks.join(", "));
        developer.push_str(". Report checks actually run, unresolved issues, and missing evidence. Ordinary replies need not use a JSON envelope.");
        sections.push(InstructionSection {
            id: "recipe".to_string(),
            label: format!("Recipe: {} (v{})", recipe.name, recipe.version),
            text: developer[start..].trim_start().to_string(),
        });
    }
    if !context_preview.is_empty() {
        let start = developer.len();
        developer.push_str("\n\n<workspace_context role=\"source_material\">\n");
        developer.push_str(
            &serde_json::to_string(&context_preview)
                .map_err(|error| WorkbenchError::storage("Failed to encode source context", error))?
                .replace('<', "\\u003c"),
        );
        developer.push_str("\n</workspace_context>");
        sections.push(InstructionSection {
            id: "context".to_string(),
            label: "Selected sources".to_string(),
            text: developer[start..].trim_start().to_string(),
        });
    }
    if developer.len() > MAX_INSTRUCTIONS_BYTES {
        return Err(WorkbenchError::invalid(
            "Effective harness instructions exceed 256 KiB",
        ));
    }
    if let Some(ws) = snapshot.session.workspace_id.as_deref() {
        let policy = crate::workbench::data::policy(store, ws)?;
        let text = format!("Curated dataset access policy: {}. Raw row reads require explicit assistantRows access; host execution retains its separately authorized operating-system access.",serde_json::to_string(&policy).map_err(|e|WorkbenchError::storage("Dataset access policy",e))?);
        developer.push_str("\n\n");
        developer.push_str(&text);
        sections.push(InstructionSection {
            id: "data_policy".into(),
            label: "Project dataset access policy".into(),
            text,
        });
    }
    let tools = dynamic_tools(&enabled);
    let requested_web_search = values
        .get("webSearch")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut diagnostics = Vec::new();
    if let Some(recipe) = active_recipe.as_ref() {
        let missing =
            crate::workbench::release::check_recipe_inputs(store, session_id, &recipe.id)?
                .into_iter()
                .filter(|check| !check.available)
                .map(|check| check.detail)
                .collect::<Vec<_>>();
        if !missing.is_empty() {
            diagnostics.push(format!(
                "Recipe '{}' is missing required inputs: {}. The recipe remains optional and plain conversation is available.",
                recipe.name,
                missing.join("; ")
            ));
        }
    }
    if requested_web_search {
        diagnostics.push("Native web-search control is unavailable in the pinned App Server protocol; it remains disabled.".to_string());
    }
    if preset
        .modules
        .iter()
        .any(|module| module == "research_execution")
        && !has_tested_execution_profile
    {
        diagnostics.push(
            "Research execution is disabled until an execution profile passes its test action."
                .to_string(),
        );
    }
    let command_network = values
        .get("commandNetwork")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let permission_profile =
        if let Some((writer, _)) = crate::workbench::discovery::role_policy(store, session_id)? {
            if command_network || mode != if writer { "edit" } else { "inspect" } {
                return Err(WorkbenchError::invalid(
                    "Self-discovery role access cannot be changed outside its saved scope",
                ));
            }
            if writer {
                "discovery-edit"
            } else {
                "discovery-inspect"
            }
        } else {
            match (mode, command_network) {
                ("edit", true) => "workbench-edit-network",
                ("edit", false) => "workbench-edit",
                ("inspect", true) => "workbench-inspect-network",
                _ => "workbench-inspect",
            }
        };
    let mut effective = EffectiveHarness {
        schema_version: HARNESS_SCHEMA_VERSION,
        session_id: session_id.to_string(),
        workspace_id: snapshot.session.workspace_id,
        preset,
        mode: mode.to_string(),
        web_search: false,
        command_network,
        permission_profile: permission_profile.to_string(),
        context_budget_bytes: budget,
        enabled_modules: enabled,
        unavailable_modules: unavailable,
        diagnostics,
        developer_instructions: developer,
        context_preview,
        context_truncated,
        dynamic_tools: tools,
        fingerprint: String::new(),
        value_sources: Value::Object(sources),
        module_availability: Vec::new(),
        instruction_sections: Vec::new(),
    };
    // The renamed default is a display change. Preserve existing native bindings
    // by retaining its legacy display metadata in the fingerprint only.
    let mut fingerprint_view = effective.clone();
    if fingerprint_view.preset.id == "plain" && fingerprint_view.preset.built_in {
        fingerprint_view.preset.name = "Plain conversation".into();
        fingerprint_view.preset.description =
            "No research instructions or automatic context.".into();
    }
    let fingerprint_body = serde_json::to_vec(&fingerprint_view).map_err(|error| {
        WorkbenchError::storage("Failed to fingerprint effective harness", error)
    })?;
    effective.fingerprint = hash_bytes(&fingerprint_body);
    // Derived views are attached only after hashing so that a harness whose
    // semantic inputs are unchanged keeps the fingerprint recorded on its
    // existing native binding.
    effective.module_availability = module_availability;
    effective.instruction_sections = sections;
    Ok(effective)
}

/// Why a catalog module cannot run in the current session, in user-facing
/// terms. An empty list means the module is available.
fn module_unavailability_reasons(
    spec: &HarnessModule,
    has_workspace: bool,
    mode: &str,
    has_tested_execution_profile: bool,
    in_task_copy: bool,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if spec.requires_workspace && !has_workspace {
        reasons
            .push("Available only inside a Workspace; this conversation is unfiled.".to_string());
    }
    if spec.capability == "execute" {
        if mode != "edit" {
            reasons.push("Needs Edit access mode.".to_string());
        }
        if !has_tested_execution_profile {
            reasons.push("Needs an execution profile that has passed its test.".to_string());
        }
        if in_task_copy {
            reasons.push("Unavailable while this conversation works in a task copy.".to_string());
        }
    }
    reasons
}
