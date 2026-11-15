use super::*;

fn get(ids: &[&str]) -> ToolCall {
    ToolCall {
        tool: "GetReviewers".into(),
        queries: vec![],
        ids: ids.iter().map(|s| s.to_string()).collect(),
        kind: String::new(),
        group: String::new(),
    }
}

#[test]
fn prefetch_is_small_deterministic_and_covers_subject_and_method() {
    let text="# Abstract\nLabor economics: wages, employment and labor supply.\n# Identification\nWe use a difference-in-differences design with parallel trends, treatment timing, and control groups.";
    let first = RegistrySession::prefetch(text);
    let second = RegistrySession::prefetch(text);
    assert_eq!(first.prompt(), second.prompt());
    assert!(first.supplied.contains("subject_economics_labor"));
    assert!(first.supplied.contains("difference_in_differences"));
    assert!(first.supplied_cards().len() < 40);
    assert!(first.prompt().len() < 35_000);
    assert!(!first.prompt().contains("## Output"));
    assert!(directory().to_string().contains("Chemistry"));
}

#[test]
fn a_batch_can_discover_methods_outside_the_prefetched_subject() {
    let mut registry = RegistrySession::prefetch("Labor economics wages and employment.");
    let target = "crystallography_structure";
    assert!(!registry.supplied.contains(target));
    let result = registry
        .expand(&[
            ToolCall {
                tool: "SearchReviewers".into(),
                queries: vec!["X-ray crystallography structure diffraction refinement".into()],
                ids: vec![],
                kind: "method".into(),
                group: String::new(),
            },
            get(&["subject_economics_labor"]),
        ])
        .unwrap();
    assert_eq!(result["results"].as_array().unwrap().len(), 2);
    assert!(registry.supplied.contains(target));
    let mut schema =
        crate::auto_review::resolve_schema_catalogs(&crate::auto_review::orientation_schema())
            .unwrap();
    registry.restrict_schema(&mut schema);
    assert!(
        schema["properties"]["review_plan"]["properties"]["method_specialist_ids"]["items"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!(target))
    );
}

#[test]
fn invalid_batch_is_atomic_and_unseen_roles_cannot_be_selected() {
    let mut registry = RegistrySession::prefetch("Labor economics wages employment.");
    let before = registry.supplied.clone();
    assert!(registry
        .expand(&[
            get(&["crystallography_structure"]),
            get(&["uninstalled-role"])
        ])
        .is_err());
    assert_eq!(registry.supplied, before);
    assert!(registry
        .validate_selection(
            &json!({"review_plan":{"method_specialist_ids":["crystallography_structure"]}})
        )
        .is_err());
    assert!(registry
        .expand(&[ToolCall {
            tool: "SearchReviewers".into(),
            queries: vec!["x".repeat(2_001)],
            ids: vec![],
            kind: String::new(),
            group: String::new()
        }])
        .is_err());
    assert!(registry.expand(&[get(&["../../outside"])]).is_err());
    assert!(registry
        .expand(&[ToolCall {
            tool: "LaunchReviewer".into(),
            queries: vec![],
            ids: vec![],
            kind: String::new(),
            group: String::new()
        }])
        .is_err());
}

#[test]
fn snapshot_pins_only_selected_executable_modules() {
    let mut registry = RegistrySession::prefetch("Labor economics");
    registry
        .expand(&[get(&["subject_economics_labor", "formal_proofs"])])
        .unwrap();
    let orientation = json!({"review_plan":{"subject_specialist_ids":["subject_economics_labor"],"method_specialist_ids":["formal_proofs"]}});
    let snapshot = registry.snapshot(&orientation);
    assert_eq!(snapshot["catalog_revision"], catalog_revision());
    assert_eq!(snapshot["selected_modules"].as_array().unwrap().len(), 2);
    assert_eq!(
        snapshot["selected_modules"][1]["prompt"],
        crate::auto_review::copyable_specialist_step("formal_proofs")
            .unwrap()
            .prompt
    );
}
