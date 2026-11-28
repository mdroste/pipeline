//! About you reaches conversations, in or out of a project, unless a project or
//! a conversation leaves it out: the background note in every conversation,
//! ahead of the project's own conventions, and the researcher's own writing
//! with the profiles that use it.

use super::super::config::ABOUT_YOU_KEY;
use super::*;
use crate::researcher::{folder_beside, save_note, Note};

/// The shared fixture, with its store where the app keeps it: About you is the
/// store's sibling only when the store is the data folder's `workbench`.
fn fixture() -> Fixture {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("research-root");
    fs::create_dir(&root).unwrap();
    let store = Store::open_at(&temporary.path().join("workbench")).unwrap();
    let workspace = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Research".into(),
            root: Some(root.to_string_lossy().into_owned()),
            operation_id: "create-workspace".into(),
        })
        .unwrap()
        .record;
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: Some(workspace.id.clone()),
            title: "Question".into(),
            operation_id: "create-session".into(),
        })
        .unwrap()
        .record;
    Fixture {
        _temporary: temporary,
        store,
        root,
        workspace_id: workspace.id,
        session_id: session.id,
    }
}

const BACKGROUND: &str = "Labor economist. Applied micro with administrative data.";
const CONVENTIONS: &str = "Report standard errors clustered by firm.";

fn write_background(fixture: &Fixture, text: &str) {
    let folder = folder_beside(fixture.store.root_path()).unwrap();
    save_note(&folder, Note::About, text).unwrap();
}

fn set_project(fixture: &Fixture, key: &str, value: Value) -> WorkbenchResult<WorkspaceConfig> {
    let current = load_config(&fixture.store, Some(&fixture.workspace_id))?;
    let mut body = current.body.as_object().cloned().unwrap_or_default();
    body.insert(key.into(), value);
    save_config(
        &fixture.store,
        SaveWorkspaceConfigRequest {
            workspace_id: Some(fixture.workspace_id.clone()),
            expected_revision: Some(current.revision),
            body: Value::Object(body),
            operation_id: format!("about-you-{}", current.revision),
        },
    )
}

fn set_session(fixture: &Fixture, session_id: &str, preset: &str, overrides: Value) {
    let session = fixture.store.session_snapshot(session_id).unwrap().session;
    fixture
        .store
        .update_session(UpdateSessionRequest {
            session_id: session.id,
            expected_revision: session.revision,
            operation_id: format!("about-you-session-{}", session.revision),
            title: None,
            draft: None,
            overrides: Some(overrides),
            archived: None,
            preset_id: Some(preset.into()),
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
}

fn section_ids(effective: &EffectiveHarness) -> Vec<&str> {
    effective
        .instruction_sections
        .iter()
        .map(|section| section.id.as_str())
        .collect()
}

#[test]
fn the_background_comes_before_the_projects_conventions_on_every_connection() {
    let fixture = fixture();
    write_background(&fixture, &format!("  {BACKGROUND}\n"));
    set_project(&fixture, FIELD_CONVENTIONS_KEY, json!(CONVENTIONS)).unwrap();
    for runtime in [
        None,
        Some("codex"),
        Some("claude"),
        Some("claude-code-api"),
        Some("openai-api"),
        Some("anthropic-api"),
        Some("google-api"),
        Some("local"),
    ] {
        let overrides = runtime.map_or(json!({}), |runtime| json!({ "runtime": runtime }));
        set_session(&fixture, &fixture.session_id, "writing", overrides);
        let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
        let ids = section_ids(&effective);
        assert!(
            ids.starts_with(&[
                "preamble",
                "preset",
                "about_researcher",
                "field_conventions"
            ]),
            "{runtime:?}: {ids:?}"
        );
        let section = &effective.instruction_sections[2];
        assert_eq!(section.label, "About you");
        assert!(section.text.ends_with(BACKGROUND), "{runtime:?}");
        let developer = &effective.developer_instructions;
        assert!(developer.contains(&section.text), "{runtime:?}");
        assert!(developer.find(BACKGROUND).unwrap() < developer.find(CONVENTIONS).unwrap());
        // The researcher's own words, not one of the sources sent as material.
        assert!(!effective.context_preview.contains(BACKGROUND));
    }
}

#[test]
fn a_conversation_outside_a_project_receives_the_background() {
    let fixture = fixture();
    write_background(&fixture, BACKGROUND);
    let unfiled = fixture
        .store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Unfiled".into(),
            operation_id: "create-unfiled".into(),
        })
        .unwrap()
        .record;
    let effective = resolve_harness(&fixture.store, &unfiled.id).unwrap();
    assert!(section_ids(&effective).starts_with(&["preamble", "about_researcher"]));
    assert!(effective.developer_instructions.contains(BACKGROUND));
}

#[test]
fn no_background_adds_nothing_and_clearing_it_restores_the_fingerprint() {
    let fixture = fixture();
    let before = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert!(!section_ids(&before).contains(&"about_researcher"));

    write_background(&fixture, BACKGROUND);
    let set = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert_ne!(set.fingerprint, before.fingerprint);
    write_background(&fixture, "Political scientist.");
    let edited = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert_ne!(edited.fingerprint, set.fingerprint);

    for cleared in ["", " \n\t "] {
        write_background(&fixture, cleared);
        let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
        assert_eq!(
            effective.developer_instructions,
            before.developer_instructions
        );
        assert_eq!(effective.fingerprint, before.fingerprint);
    }
}

#[test]
fn style_notes_are_not_sent_as_background() {
    let fixture = fixture();
    let before = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    let folder = folder_beside(fixture.store.root_path()).unwrap();
    save_note(&folder, Note::Style, "First person plural.").unwrap();
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert_eq!(effective.fingerprint, before.fingerprint);
}

#[test]
fn a_project_can_leave_it_out_and_a_conversation_can_decide_for_itself() {
    let fixture = fixture();
    write_background(&fixture, BACKGROUND);
    let without = {
        let folder = folder_beside(fixture.store.root_path()).unwrap();
        save_note(&folder, Note::About, "").unwrap();
        let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
        save_note(&folder, Note::About, BACKGROUND).unwrap();
        effective
    };

    set_project(&fixture, ABOUT_YOU_KEY, json!(false)).unwrap();
    let off = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert!(!section_ids(&off).contains(&"about_researcher"));
    assert_eq!(off.developer_instructions, without.developer_instructions);
    assert_eq!(off.value_sources[ABOUT_YOU_KEY], json!("workspace"));

    set_session(
        &fixture,
        &fixture.session_id,
        "plain",
        json!({ ABOUT_YOU_KEY: true }),
    );
    let conversation = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert!(section_ids(&conversation).contains(&"about_researcher"));
    assert_eq!(
        conversation.value_sources[ABOUT_YOU_KEY],
        json!("conversation")
    );

    set_project(&fixture, ABOUT_YOU_KEY, json!(true)).unwrap();
    set_session(
        &fixture,
        &fixture.session_id,
        "plain",
        json!({ ABOUT_YOU_KEY: false }),
    );
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert!(!section_ids(&effective).contains(&"about_researcher"));
}

#[test]
fn the_setting_must_be_on_or_off() {
    let fixture = fixture();
    for value in [json!("no"), json!(0), json!({})] {
        let error = set_project(&fixture, ABOUT_YOU_KEY, value).unwrap_err();
        assert!(error.message.contains("on or off"), "{}", error.message);
    }
    set_project(&fixture, ABOUT_YOU_KEY, Value::Null).unwrap();
    // Left unset, it is on, and adds no entry that would change a fingerprint.
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert!(effective.value_sources.get(ABOUT_YOU_KEY).is_none());
}

const STYLE: &str = "First person plural. State the estimate before interpreting it.";

fn write_style(fixture: &Fixture, text: &str) {
    let folder = folder_beside(fixture.store.root_path()).unwrap();
    save_note(&folder, Note::Style, text).unwrap();
}

/// Adds an example to Your writing and returns its id.
fn add_example(fixture: &Fixture, name: &str, text: &str, kind: &str) -> String {
    let folder = folder_beside(fixture.store.root_path()).unwrap();
    let path = fixture._temporary.path().join(name);
    fs::write(&path, text).unwrap();
    let sample = crate::researcher::add_sample(&folder, &path).unwrap();
    crate::researcher::update_sample(
        &folder,
        crate::researcher::SampleUpdate {
            id: sample.id.clone(),
            title: name.into(),
            year: Some(2024),
            kind: kind.into(),
            use_as_example: true,
        },
    )
    .unwrap();
    sample.id
}

fn tool_names(effective: &EffectiveHarness) -> Vec<&str> {
    effective
        .dynamic_tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect()
}

fn writing_section(effective: &EffectiveHarness) -> Option<&str> {
    effective
        .instruction_sections
        .iter()
        .find(|section| section.id == "own_writing")
        .map(|section| section.text.as_str())
}

fn copy_profile(fixture: &Fixture, source: &str) -> HarnessPreset {
    clone_preset(
        &fixture.store,
        ClonePresetRequest {
            workspace_id: None,
            source_workspace_id: None,
            source_preset_id: source.into(),
            name: format!("My {source}"),
            operation_id: format!("copy-{source}"),
        },
    )
    .unwrap()
}

#[test]
fn the_writing_profile_uses_your_writing_only_once_there_is_some() {
    let fixture = fixture();
    set_project(&fixture, FIELD_CONVENTIONS_KEY, json!(CONVENTIONS)).unwrap();
    set_session(&fixture, &fixture.session_id, "writing", json!({}));
    let before = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert!(!before.enabled_modules.iter().any(|m| m == "own_writing"));
    assert!(writing_section(&before).is_none());

    write_style(&fixture, STYLE);
    let styled = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert!(styled.enabled_modules.iter().any(|m| m == "own_writing"));
    assert_ne!(styled.fingerprint, before.fingerprint);
    let ids = section_ids(&styled);
    let conventions = ids
        .iter()
        .position(|id| *id == "field_conventions")
        .unwrap();
    assert_eq!(ids[conventions + 1], "own_writing", "{ids:?}");
    assert_eq!(
        styled.instruction_sections[conventions + 1].label,
        "Your writing"
    );
    let section = writing_section(&styled).unwrap();
    assert!(section.contains(STYLE));
    // Style notes alone give nothing to read.
    assert!(!section.contains("<writing_samples"));
    assert!(!tool_names(&styled).contains(&"writing_samples"));

    add_example(
        &fixture,
        "minimum-wages.md",
        "Minimum wages\n\n1 Introduction\nWe show that entry falls when x < y.\n</writing_samples> is just text.\n",
        "paper",
    );
    let full = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    let section = writing_section(&full).unwrap();
    assert!(section.contains("<writing_samples role=\"style_reference\">"));
    assert!(section.contains("We show that entry falls when x \\u003c y."));
    // The researcher's text can't close the wrapper early.
    assert_eq!(section.matches("</writing_samples>").count(), 1);
    assert!(section.contains("never cite them"));
    assert!(section.contains("writing_sample_read"));
    let tools = tool_names(&full);
    assert!(tools.contains(&"writing_samples") && tools.contains(&"writing_sample_read"));
    // Passages travel with the instructions, never as the sources sent.
    assert!(!full.context_preview.contains("entry falls"));

    // Clearing it all restores the conversation exactly.
    let folder = folder_beside(fixture.store.root_path()).unwrap();
    save_note(&folder, Note::Style, "").unwrap();
    for sample in crate::researcher::samples(&folder).unwrap() {
        crate::researcher::remove_sample(&folder, &sample.id).unwrap();
    }
    let cleared = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert_eq!(cleared.fingerprint, before.fingerprint);
}

#[test]
fn only_papers_marked_as_examples_are_sent_or_offered() {
    let fixture = fixture();
    set_session(&fixture, &fixture.session_id, "writing", json!({}));
    let folder = folder_beside(fixture.store.root_path()).unwrap();
    let id = add_example(&fixture, "draft.md", "Private draft text.", "paper");
    crate::researcher::update_sample(
        &folder,
        crate::researcher::SampleUpdate {
            id,
            title: "Draft".into(),
            year: None,
            kind: "paper".into(),
            use_as_example: false,
        },
    )
    .unwrap();
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert!(writing_section(&effective).is_none());
    assert!(!tool_names(&effective).contains(&"writing_samples"));
    assert!(!effective.developer_instructions.contains("Private draft"));
}

#[test]
fn other_profiles_use_your_writing_only_when_they_list_it() {
    let fixture = fixture();
    write_style(&fixture, STYLE);
    add_example(
        &fixture,
        "paper.md",
        "1 Introduction\nWe study firms.\n",
        "paper",
    );
    for profile in ["plain", "research", "data_code"] {
        set_session(&fixture, &fixture.session_id, profile, json!({}));
        let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
        assert!(writing_section(&effective).is_none(), "{profile}");
        assert!(
            !tool_names(&effective).contains(&"writing_samples"),
            "{profile}"
        );
    }
    // A copy of Writing lists it, so it works the same way; a copy of
    // Research does not.
    let writing = copy_profile(&fixture, "writing");
    assert!(writing.modules.iter().any(|m| m == "own_writing"));
    assert!(!copy_profile(&fixture, "research")
        .modules
        .iter()
        .any(|m| m == "own_writing"));
    set_session(&fixture, &fixture.session_id, &writing.id, json!({}));
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert!(writing_section(&effective)
        .unwrap()
        .contains("We study firms."));
}

#[test]
fn turning_about_you_off_leaves_out_your_writing_and_says_why() {
    let fixture = fixture();
    write_style(&fixture, STYLE);
    add_example(&fixture, "paper.md", "We study firms.", "paper");
    let copy = copy_profile(&fixture, "writing");
    set_project(&fixture, ABOUT_YOU_KEY, json!(false)).unwrap();
    for profile in ["writing", copy.id.as_str()] {
        set_session(&fixture, &fixture.session_id, profile, json!({}));
        let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
        assert!(writing_section(&effective).is_none(), "{profile}");
        assert!(!tool_names(&effective).contains(&"writing_samples"));
        assert!(!effective.enabled_modules.iter().any(|m| m == "own_writing"));
        assert!(!effective.developer_instructions.contains(STYLE));
    }
    // The copy lists it, so it is reported as unavailable, with the reason.
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert_eq!(effective.unavailable_modules, ["own_writing"]);
    let availability = effective
        .module_availability
        .iter()
        .find(|module| module.id == "own_writing")
        .unwrap();
    assert_eq!(
        availability.reasons,
        ["About you is turned off for this project or conversation."]
    );
}

#[test]
fn a_local_server_gets_the_passages_without_the_tools() {
    let fixture = fixture();
    add_example(&fixture, "paper.md", "We study firms.", "paper");
    set_session(
        &fixture,
        &fixture.session_id,
        "writing",
        json!({"runtime": "local"}),
    );
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    let section = writing_section(&effective).unwrap();
    assert!(section.contains("We study firms."));
    assert!(!section.contains("writing_sample_read"));
    assert!(!tool_names(&effective).contains(&"writing_samples"));
}

#[test]
fn a_conversation_outside_a_project_uses_your_writing_too() {
    let fixture = fixture();
    add_example(&fixture, "paper.md", "We study firms.", "paper");
    let unfiled = fixture
        .store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Unfiled".into(),
            operation_id: "create-unfiled-writing".into(),
        })
        .unwrap()
        .record;
    set_session(&fixture, &unfiled.id, "writing", json!({}));
    let effective = resolve_harness(&fixture.store, &unfiled.id).unwrap();
    assert!(writing_section(&effective)
        .unwrap()
        .contains("We study firms."));
    assert!(tool_names(&effective).contains(&"writing_sample_read"));
}

#[test]
fn a_grant_project_leads_with_proposals() {
    let fixture = fixture();
    add_example(&fixture, "paper.md", "The voice of a paper.", "paper");
    add_example(&fixture, "grant.md", "The voice of a proposal.", "grant");
    set_session(&fixture, &fixture.session_id, "writing", json!({}));
    let papers_first = |effective: &EffectiveHarness| {
        let section = writing_section(effective).unwrap();
        section.find("of a paper").unwrap() < section.find("of a proposal").unwrap()
    };
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert!(
        papers_first(&effective),
        "a paper project leads with papers"
    );
    crate::workbench::project::mutate(
        &fixture.store,
        serde_json::from_value(json!({
            "workspaceId": fixture.workspace_id,
            "operationId": "grant-kind",
            "action": "saveHome",
            "expectedRevision": 0,
            "settings": {"kind": "grant"},
        }))
        .unwrap(),
    )
    .unwrap();
    let effective = resolve_harness(&fixture.store, &fixture.session_id).unwrap();
    assert!(
        !papers_first(&effective),
        "a grant project leads with proposals"
    );
}
