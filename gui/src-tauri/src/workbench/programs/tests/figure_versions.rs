//! "Edit as new version" on a figure: the plan carries the figure it replaces,
//! and saving the finished run links the two.
use super::*;

/// Records a finished run of the recipe's plan with the outputs plot.py writes.
fn finish_plan(f: &Fixture, recipe: &DeskRecord, execution: &str) {
    let artifacts: Vec<Value> = [
        ("figure.svg", "svg"),
        ("figure.pdf", "pdf"),
        ("figure.png", "png"),
        ("figure-receipt.json", "json"),
    ]
    .iter()
    .map(|(path, extension)| {
        let content = format!("{execution} {path}");
        let artifact = blob(&f.store, &f.ws, content.as_bytes(), extension).unwrap();
        json!({"artifactId":artifact.id,"contentHash":artifact.hash,"path":path})
    })
    .collect();
    f.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE research_executions SET input_manifest_json=?1,output_manifest_json=?2 WHERE id=?3",
            params![
                json!({"planId":recipe.body["planId"]}).to_string(),
                json!({"artifacts":artifacts}).to_string(),
                execution
            ],
        )
        .unwrap();
}

#[test]
fn a_figure_saved_as_a_new_version_replaces_the_earlier_one() {
    let f = fixture(true);
    let p = profile(&f, "# Selected Python environment", 60);
    let source = numeric(&f, "baseline", 1.5, "sample");
    numeric(&f, "rerun", 1.5, "sample");
    let mut figure = spec(source);
    figure.kind = "coefficient".into();
    let plan = |supersedes: Option<&str>, operation: &str| {
        assets::figure_plan(
            &f.store,
            assets::FigureRequest {
                workspace_id: f.ws.clone(),
                spec: figure.clone(),
                python_profile_id: p.id.clone(),
                operation_id: operation.into(),
                supersedes: supersedes.map(String::from),
            },
        )
    };

    let first_recipe = plan(None, "first").unwrap();
    assert!(first_recipe.body.get("supersedes").is_none());
    finish_plan(&f, &first_recipe, "execution_baseline");
    let first = assets::adopt_figure(
        &f.store,
        &f.ws,
        &first_recipe.id,
        "execution_baseline",
        "adopt-first",
    )
    .unwrap();
    assert_eq!(first.supersedes, None);

    let next_recipe = plan(Some(&first.id), "next").unwrap();
    finish_plan(&f, &next_recipe, "execution_rerun");
    let next = assets::adopt_figure(
        &f.store,
        &f.ws,
        &next_recipe.id,
        "execution_rerun",
        "adopt-next",
    )
    .unwrap();
    assert_eq!(next.kind, "publication_asset");
    assert_eq!(next.supersedes.as_deref(), Some(first.id.as_str()));

    // The retry of a plan matches; the same request for another figure does not.
    assert_eq!(plan(Some(&first.id), "next").unwrap().id, next_recipe.id);
    assert!(plan(Some(&next.id), "next").is_err());
    assert!(plan(None, "next").is_err());
    assert_eq!(plan(None, "first").unwrap().id, first_recipe.id);

    // A plan is checked when it is made, before anything runs.
    assert!(plan(Some(&first_recipe.id), "wrong-kind").is_err());
    assert!(plan(Some("desk_missing"), "missing").is_err());
}
