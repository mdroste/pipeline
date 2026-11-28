//! Ordered harness resolution, prompt composition, and compatibility fingerprints.

use super::catalog::OWN_WRITING;
use super::config::{find_preset, load_config, merge_config, ABOUT_YOU_KEY};
use super::context::context_for_session;
use super::tool_catalog::dynamic_tools;
use super::{
    harness_modules, hash_bytes, open_connection, validate_id, EffectiveHarness, HarnessModule,
    InstructionSection, ModuleAvailability, DEFAULT_CONTEXT_BYTES, FIELD_CONVENTIONS_KEY,
    HARNESS_SCHEMA_VERSION, MAX_CONTEXT_BYTES, MAX_FIELD_CONVENTIONS_BYTES, MAX_INSTRUCTIONS_BYTES,
    WORKSPACE_PREAMBLE,
};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use serde_json::{json, Map, Value};

/// Encodes source text for the instructions, keeping as much of its start as
/// fits in `limit` bytes. Returns the encoded text and the source bytes kept.
pub(super) fn fit_encoded_context(context: &str, limit: usize) -> WorkbenchResult<(String, usize)> {
    let encode = |text: &str| {
        serde_json::to_string(text)
            .map(|encoded| encoded.replace('<', "\\u003c"))
            .map_err(|error| WorkbenchError::storage("Failed to encode source context", error))
    };
    let mut kept = context;
    let mut encoded = encode(kept)?;
    while encoded.len() > limit && !kept.is_empty() {
        let scaled = (kept.len() as u128 * limit as u128 / encoded.len() as u128) as usize;
        kept = crate::workbench::search::prefix(kept, scaled.min(kept.len() - 1));
        encoded = encode(kept)?;
    }
    Ok((encoded, kept.len()))
}

pub fn resolve_harness(store: &Store, session_id: &str) -> WorkbenchResult<EffectiveHarness> {
    resolve(store, session_id, None)
}

/// The fingerprint this conversation would have without one returned result,
/// which separates that delivery's change from any other.
pub fn fingerprint_without_result(
    store: &Store,
    session_id: &str,
    result_id: &str,
) -> WorkbenchResult<String> {
    Ok(resolve(store, session_id, Some(result_id))?.fingerprint)
}

fn resolve(
    store: &Store,
    session_id: &str,
    left_out: Option<&str>,
) -> WorkbenchResult<EffectiveHarness> {
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
        ("fileMode", json!("review")),
        ("webSearch", json!(false)),
        ("commandNetwork", json!(false)),
        ("commandDomains", json!("")),
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
    let file_mode = values
        .get("fileMode")
        .and_then(Value::as_str)
        .unwrap_or("review");
    if !matches!(file_mode, "review" | "direct") {
        return Err(WorkbenchError::invalid(
            "File changes must use Review copy or Direct edits",
        ));
    }
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
    // Everything else runs in a project. This folder holds nothing of the
    // researcher's, so its writes do not depend on the project access mode.
    let private_folder = !in_task_copy
        && snapshot
            .workspace
            .as_ref()
            .is_none_or(|workspace| workspace.root.is_none());
    // A research automation's checks run through its coordinator, only as
    // approved, so its roles never gain the run tool, whatever passes a test.
    let automation_role = crate::workbench::missions::marks_role(&snapshot.session.overrides);
    // About you is the researcher's own, in every project, so a project or a
    // conversation can leave all of it out. Its files are read each time:
    // another computer may have changed them.
    let about_you = values
        .get(ABOUT_YOU_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let researcher_folder = about_you
        .then(|| crate::researcher::folder_beside(store.root_path()))
        .flatten();
    let module_availability = catalog
        .iter()
        .map(|spec| {
            let reasons = module_unavailability_reasons(
                spec,
                has_workspace,
                mode,
                has_tested_execution_profile,
                in_task_copy,
                automation_role,
                about_you,
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
    // Reading the project's papers follows the conversation, not the profile:
    // automatic modules are on wherever they can run, and an attached paper
    // brings its context. Neither is reported missing when it cannot run.
    let attached_paper = snapshot.session.paper_id.is_some();
    for (spec, availability) in catalog.iter().zip(&module_availability) {
        let implied = spec.automatic || (spec.id == "paper_context" && attached_paper);
        if implied && availability.available && !enabled.iter().any(|module| module == spec.id) {
            enabled.push(spec.id.to_string());
        }
    }
    // The built-in Writing profile uses the researcher's own writing whenever
    // there is some, so a researcher who has added none sees no change. Other
    // profiles choose it under Tools & context.
    let builtin_writing = preset.built_in && preset.id == "writing";
    let own_writing = researcher_folder
        .as_deref()
        .filter(|_| builtin_writing || preset.modules.iter().any(|m| m == OWN_WRITING))
        .map(|folder| (folder, crate::researcher::writing(folder)))
        .filter(|(_, writing)| !writing.is_empty());
    if builtin_writing && own_writing.is_some() && !enabled.iter().any(|m| m == OWN_WRITING) {
        enabled.push(OWN_WRITING.to_string());
    }
    let own_writing = own_writing.filter(|_| enabled.iter().any(|m| m == OWN_WRITING));
    // A local server may reject function calling, so it gets the passages
    // without the tools to read further.
    let writing_tools = own_writing
        .as_ref()
        .is_some_and(|(_, writing)| !writing.examples.is_empty())
        && !matches!(
            crate::workbench::api::selected(&snapshot.session.overrides)?,
            Some(crate::connections::ApiProvider::Local)
        );
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
    let task_context = crate::workbench::tasks::returned_context(store, session_id, left_out)?;
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
    match snapshot
        .session
        .overrides
        .get("runtime")
        .and_then(Value::as_str)
    {
        Some("claude" | "claude-code-api") => {
            developer = developer.replace(
                "Preserve Codex base instructions.",
                "Preserve Claude Code base instructions.",
            )
        }
        Some("openai-api" | "anthropic-api" | "google-api" | "local") => {
            developer = developer.replace("Preserve Codex base instructions. ", "")
        }
        _ => {}
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
    // What the researcher says about themself holds in every project, so it
    // comes before the project's own conventions, which are the more specific.
    let background = researcher_folder
        .as_deref()
        .and_then(crate::researcher::background);
    if let Some(text) = background {
        let start = developer.len();
        developer.push_str(
            "\n\nAbout the researcher, in their own words. Use it to choose conventions and pitch explanations. Where this project's own field and conventions differ, follow the project's:\n",
        );
        developer.push_str(&text);
        sections.push(InstructionSection {
            id: "about_researcher".into(),
            label: "About you".into(),
            text: developer[start..].trim_start().to_string(),
        });
    }
    // The researcher's field and conventions hold for the whole project,
    // whatever the profile. Only the project's own setting applies, and it is
    // the researcher's instruction rather than source material.
    let field_conventions = workspace
        .as_ref()
        .and_then(|config| config.body.get(FIELD_CONVENTIONS_KEY))
        .and_then(Value::as_str)
        .map(|text| crate::workbench::search::prefix(text.trim(), MAX_FIELD_CONVENTIONS_BYTES))
        .filter(|text| !text.is_empty());
    if let Some(text) = field_conventions {
        let start = developer.len();
        developer.push_str(
            "\n\nThe researcher's field and conventions for this project. Follow them unless the researcher asks otherwise:\n",
        );
        developer.push_str(text);
        sections.push(InstructionSection {
            id: "field_conventions".into(),
            label: "Project field and conventions".into(),
            text: developer[start..].trim_start().to_string(),
        });
    }
    // The researcher's own writing comes after the project's conventions, which
    // still govern notation and reporting. Passages are wrapped and encoded like
    // sources: they are the researcher's prose, but never instructions or
    // evidence for this work.
    if let Some((folder, writing)) = own_writing.as_ref() {
        let start = developer.len();
        developer.push_str(
            "\n\nThe researcher's own writing. Use it when you draft or revise their prose, unless they ask otherwise.",
        );
        if let Some(style) = writing.style.as_deref() {
            developer.push_str("\n\nHow they describe their style:\n");
            developer.push_str(style);
        }
        // A grant project's voice is closest to the researcher's proposals.
        let project_kind = snapshot
            .session
            .workspace_id
            .as_deref()
            .and_then(|id| crate::workbench::project::home_record(store, id).ok())
            .and_then(|record| {
                record
                    .body
                    .get("kind")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            });
        let preferred = if project_kind.as_deref() == Some("grant") {
            "grant"
        } else {
            "paper"
        };
        let passages = crate::researcher::passages(folder, &writing.examples, preferred);
        if !passages.is_empty() {
            let encoded = serde_json::to_string(&passages)
                .map_err(|error| {
                    WorkbenchError::storage("Failed to encode writing samples", error)
                })?
                .replace('<', "\\u003c");
            developer.push_str(
                "\n\nPassages from their earlier work, for voice, sentence structure, and conventions only. They are not sources for this work: never cite them, quote them as evidence, or take facts, results, or references from them.\n<writing_samples role=\"style_reference\">\n",
            );
            developer.push_str(&encoded);
            developer.push_str("\n</writing_samples>");
        }
        if writing_tools {
            developer.push_str(
                "\n\nwriting_samples lists the earlier papers they marked as examples, and writing_sample_read reads more of one when a task calls for it.",
            );
        }
        sections.push(InstructionSection {
            id: "own_writing".into(),
            label: "Your writing".into(),
            text: developer[start..].trim_start().to_string(),
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
    const CONTEXT_OPEN: &str = "\n\n<workspace_context role=\"source_material\">\n";
    const CONTEXT_CLOSE: &str = "\n</workspace_context>";
    // The source limit can exceed what the instructions may hold, and encoding
    // lengthens the text, so sources give way rather than the conversation.
    let available = MAX_INSTRUCTIONS_BYTES
        .saturating_sub(developer.len() + CONTEXT_OPEN.len() + CONTEXT_CLOSE.len());
    let (encoded_context, kept) = fit_encoded_context(&context_preview, available)?;
    if kept < context_preview.len() {
        context_preview.truncate(kept);
        context_truncated = true;
    }
    if !context_preview.is_empty() {
        let start = developer.len();
        developer.push_str(CONTEXT_OPEN);
        developer.push_str(&encoded_context);
        developer.push_str(CONTEXT_CLOSE);
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
    if has_workspace {
        let text = if mode == "edit" && in_task_copy {
            "Work in the isolated project working copy. Shell writes stay in this copy; Pipeline captures changes for review and acceptance. Use project_create and project_edit for recoverable file operations."
        } else if mode == "edit" {
            "Use project_read, project_edit and project_create for project file changes. They preserve recovery and detect concurrent edits. Shell commands can read the project; shell writes require an isolated working copy. Use authorized research execution tools for tested local jobs."
        } else {
            "Project access is Read only: read files and explain findings. If a request needs file changes, tell the researcher to choose Allow edits, either for the whole project under Assistants in Project settings or for this conversation under Access mode in Assistant settings (the gear beside the model picker)."
        };
        developer.push_str("\n\n");
        developer.push_str(text);
        sections.push(InstructionSection {
            id: "project_access".into(),
            label: "Project access".into(),
            text: text.into(),
        });
    }
    if private_folder {
        let text = "When a command tool is offered, it runs in this conversation's private working folder. Commands can create and change scripts, data, and other files there, and those files stay available in later messages. Save work there rather than in temporary folders, and refer to saved files by their relative paths so the researcher can open them.";
        developer.push_str("\n\n");
        developer.push_str(text);
        sections.push(InstructionSection {
            id: "working_folder".into(),
            label: "Working folder".into(),
            text: text.into(),
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
    let mut capabilities = crate::workbench::capabilities::ConversationCapabilities::resolve(
        &values,
        private_folder || (mode == "edit" && in_task_copy),
    )?;
    capabilities.private_folder = private_folder;
    let permission_profile =
        if let Some((writer, _)) = crate::workbench::discovery::role_policy(store, session_id)? {
            if command_network
                || requested_web_search
                || mode != if writer { "edit" } else { "inspect" }
            {
                return Err(WorkbenchError::invalid(
                    "Research discovery access cannot be changed outside its saved scope",
                ));
            }
            if writer {
                "discovery-edit"
            } else {
                "discovery-inspect"
            }
        } else {
            match (capabilities.shell_write, command_network) {
                (true, true) => "workbench-edit-network",
                (true, false) => "workbench-edit",
                (false, true) => "workbench-inspect-network",
                _ => "workbench-inspect",
            }
        };
    let mut effective = EffectiveHarness {
        schema_version: HARNESS_SCHEMA_VERSION,
        session_id: session_id.to_string(),
        workspace_id: snapshot.session.workspace_id,
        preset,
        mode: mode.to_string(),
        file_mode: file_mode.to_string(),
        web_search: requested_web_search,
        command_network,
        capabilities,
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
        project_checkpoint_id: None,
    };
    let conversation_tools = crate::workbench::conversation_tools::catalog(&effective);
    effective.dynamic_tools.extend(conversation_tools);
    if writing_tools {
        effective
            .dynamic_tools
            .extend(crate::workbench::conversation_tools::writing_catalog());
    }
    let provider = crate::workbench::api::selected(&snapshot.session.overrides)?;
    // A local server may reject function calling, so a local conversation
    // with nothing else enabled stays plain chat.
    let folder_commands =
        private_folder && !matches!(provider, Some(crate::connections::ApiProvider::Local));
    if provider.is_some()
        && crate::workbench::shell::available()
        && (has_workspace
            || mode == "edit"
            || folder_commands
            || !effective.dynamic_tools.is_empty())
    {
        effective
            .dynamic_tools
            .push(crate::workbench::shell::catalog(private_folder));
    }
    // The renamed default is a display change. Preserve existing native bindings
    // by retaining its legacy display metadata in the fingerprint only.
    let mut fingerprint_view = effective.clone();
    if fingerprint_view.preset.id == "plain" && fingerprint_view.preset.built_in {
        fingerprint_view.preset.name = "Plain conversation".into();
        fingerprint_view.preset.description =
            "No research instructions or automatic context.".into();
    }
    let mut fingerprint_body = serde_json::to_vec(&fingerprint_view).map_err(|error| {
        WorkbenchError::storage("Failed to fingerprint effective harness", error)
    })?;
    // API provider changes retire their binding and invalidate pinned automations;
    // retain the subscription fingerprint format for existing conversations.
    if let Some(provider) = crate::workbench::api::selected(&snapshot.session.overrides)? {
        fingerprint_body.extend_from_slice(provider.runtime().as_bytes());
    }
    // A fresh copy must start a successor native thread with transferred history,
    // even when the file permissions and other instructions have not changed.
    if let Some(checkpoint) = snapshot.session.overrides["projectCheckpointId"].as_str() {
        fingerprint_body.extend_from_slice(checkpoint.as_bytes());
    }
    effective.fingerprint = hash_bytes(&fingerprint_body);
    // Derived views are attached only after hashing so that a harness whose
    // semantic inputs are unchanged keeps the fingerprint recorded on its
    // existing native binding.
    effective.module_availability = module_availability;
    effective.instruction_sections = sections;
    effective.project_checkpoint_id = snapshot.session.overrides["projectCheckpointId"]
        .as_str()
        .map(str::to_owned);
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
    automation_role: bool,
    about_you: bool,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if spec.id == OWN_WRITING && !about_you {
        reasons.push("About you is turned off for this project or conversation.".to_string());
    }
    if spec.requires_workspace && !has_workspace {
        reasons
            .push("Available only inside a Workspace; this conversation is unfiled.".to_string());
    }
    if spec.capability == "execute" {
        if mode != "edit" {
            reasons.push("Needs Allow edits.".to_string());
        }
        if !has_tested_execution_profile {
            reasons.push("Needs an execution profile that has passed its test.".to_string());
        }
        if in_task_copy {
            reasons.push("Unavailable while this conversation works in a task copy.".to_string());
        }
        if automation_role {
            reasons.push(
                "Unavailable in a research automation, which runs only the checks you approved."
                    .to_string(),
            );
        }
    }
    reasons
}
