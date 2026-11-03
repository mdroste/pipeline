//! A conversation keeps working wherever it is filed, or it isn't moved. Agent
//! profiles and recipes made for one project resolve only there.

use super::*;
use crate::workbench::release::{clone_recipe, CloneRecipeRequest};
use crate::workbench::store::{MoveSessionRequest, WorkbenchSession};

fn other_project(fixture: &Fixture) -> String {
    let root = fixture.root.parent().unwrap().join("other-root");
    fs::create_dir(&root).unwrap();
    fixture
        .store
        .create_workspace(CreateWorkspaceRequest {
            name: "Other".into(),
            root: Some(root.to_string_lossy().into_owned()),
            operation_id: "create-other".into(),
        })
        .unwrap()
        .record
        .id
}
fn session(fixture: &Fixture) -> WorkbenchSession {
    fixture
        .store
        .session_snapshot(&fixture.session_id)
        .unwrap()
        .session
}
fn move_to(fixture: &Fixture, target: Option<&str>) -> WorkbenchResult<WorkbenchSession> {
    let current = session(fixture);
    fixture
        .store
        .move_session(MoveSessionRequest {
            session_id: current.id,
            expected_revision: current.revision,
            operation_id: new_id("move").unwrap(),
            workspace_id: target.map(str::to_string),
        })
        .map(|moved| moved.record)
}
fn profile(fixture: &Fixture, workspace_id: Option<&str>, name: &str) -> String {
    clone_preset(
        &fixture.store,
        ClonePresetRequest {
            source_workspace_id: None,
            workspace_id: workspace_id.map(str::to_string),
            source_preset_id: "writing".into(),
            name: name.into(),
            operation_id: new_id("clone").unwrap(),
        },
    )
    .unwrap()
    .id
}
fn use_recipe(fixture: &Fixture, recipe: &str) {
    let current = session(fixture);
    fixture
        .store
        .update_session(UpdateSessionRequest {
            session_id: current.id,
            expected_revision: current.revision,
            operation_id: new_id("recipe").unwrap(),
            title: None,
            draft: None,
            overrides: Some(json!({"recipeId":recipe})),
            archived: None,
            preset_id: None,
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
}

#[test]
fn a_project_profile_blocks_the_move_by_name_and_nothing_changes() {
    let fixture = fixture();
    let other = other_project(&fixture);
    let local = profile(&fixture, Some(&fixture.workspace_id), "Referee letters");
    select_preset(&fixture, &local, "inspect");
    let before = session(&fixture);

    for target in [Some(other.as_str()), None] {
        let refused = move_to(&fixture, target).unwrap_err();
        assert_eq!(refused.code, "invalid_input");
        assert!(refused.message.contains("Referee letters"), "{refused:?}");
    }
    let after = session(&fixture);
    assert_eq!(after.workspace_id, before.workspace_id);
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.preset_id.as_deref(), Some(local.as_str()));
    resolve_harness(&fixture.store, &fixture.session_id).unwrap();
}

#[test]
fn a_project_recipe_blocks_the_move_by_name() {
    let fixture = fixture();
    let other = other_project(&fixture);
    let recipe = clone_recipe(
        &fixture.store,
        CloneRecipeRequest {
            workspace_id: fixture.workspace_id.clone(),
            source_recipe_id: "manuscript_consistency".into(),
            name: "Journal checklist".into(),
        },
    )
    .unwrap();
    use_recipe(&fixture, &recipe.id);

    let refused = move_to(&fixture, Some(&other)).unwrap_err();
    assert!(refused.message.contains("Journal checklist"), "{refused:?}");
    assert!(
        refused.message.contains("Disable the recipe"),
        "{refused:?}"
    );
    resolve_harness(&fixture.store, &fixture.session_id).unwrap();
}

#[test]
fn built_in_and_shared_settings_move_and_keep_resolving() {
    let fixture = fixture();
    let other = other_project(&fixture);
    let shared = profile(&fixture, None, "Shared writer");
    select_preset(&fixture, &shared, "inspect");
    use_recipe(&fixture, "manuscript_consistency");

    let moved = move_to(&fixture, Some(&other)).unwrap();
    assert_eq!(moved.workspace_id.as_deref(), Some(other.as_str()));
    assert_eq!(moved.preset_id.as_deref(), Some(shared.as_str()));
    resolve_harness(&fixture.store, &fixture.session_id).unwrap();

    select_preset(&fixture, "writing", "inspect");
    assert!(move_to(&fixture, None).unwrap().workspace_id.is_none());
    resolve_harness(&fixture.store, &fixture.session_id).unwrap();
}
