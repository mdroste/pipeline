use super::*;

fn extraction() -> crate::models::ExtractionResult {
    crate::models::ExtractionResult {text:"<!-- PAGE 1 -->\nLabor economics: wages and labor supply.\n<!-- PAGE 2 -->\nTheorem 1. A formal equilibrium proof with nontrivial derivations.".into(),method:"fixture".into(),source_path:"paper.pdf".into(),paper_hash:"fixture".into(),quality_notes:vec!["A captured diagnostic.".into()]}
}

fn answer() -> Value {
    json!({"orientation":{
        "metadata":{"title":"Wages","authors":[],"date":"","paper_type":"theory","page_count":2,"has_appendix":false,"has_online_appendix":false},
        "stated_contribution":"A wage equilibrium theorem.",
        "review_plan":{"primary_domain":"economics","subject":"labor","paper_forms":["formal_theory"],"subject_specialist_ids":["subject_economics_labor"],"method_specialist_ids":["formal_proofs"],"genre":"research_article","routing_uncertainty":[],"selection_notes":[
            {"id":"subject_economics_labor","reason":"Labor supply and wages.","references":["page:1"]},
            {"id":"formal_proofs","reason":"Theorem and proof.","references":["page:2"]}
        ]}
    }})
}

#[test]
fn planner_omits_stock_inventories_and_full_catalog_but_preserves_saved_shape() {
    let extraction = extraction();
    let schema = auto_review::orientation_schema();
    let mut session = PlanningSession::new(&extraction, None, None, &schema, None, &[]).unwrap();
    let provider = session.schema().unwrap();
    assert!(provider["properties"]["orientation"]["properties"]
        .get("notation")
        .is_none());
    assert!(provider["properties"]["orientation"]["properties"]
        .get("formal_results")
        .is_none());
    assert!(
        serde_json::to_vec(&structured::provider_schema(&provider).unwrap())
            .unwrap()
            .len()
            < 14_000
    );
    assert!(session.prompt(None).unwrap().len() < 50_000);
    let Outcome::Complete(result) = session.accept(answer()).unwrap() else {
        panic!("expected plan")
    };
    structured::validate(
        &auto_review::resolve_schema_catalogs(&schema).unwrap(),
        &result.orientation,
    )
    .unwrap();
    assert_eq!(result.orientation["notation"], json!([]));
    assert_eq!(
        result.orientation["extraction_quality_notes"][0]["description"],
        "A captured diagnostic."
    );
    assert!(result.registry.unwrap().contains("selected_modules"));
}

#[test]
fn catalog_and_supporting_document_reads_share_one_expansion() {
    let extraction = extraction();
    let schema = auto_review::orientation_schema();
    let inputs = [SupportingInput {
        key: "appendix".into(),
        text: "<!-- PAGE 9 -->\nA crystallography experiment with diffraction.".into(),
    }];
    let mut session =
        PlanningSession::new(&extraction, None, None, &schema, None, &inputs).unwrap();
    let request = json!({"tool_calls":[{"tool":"GetReviewers","queries":[],"ids":["crystallography_structure"],"kind":"","group":""}],"evidence_requests":[{"locator":"input:appendix:page:9","question":"Which experiment supports the result?"}]});
    let Outcome::Expanded(counts) = session.accept(request).unwrap() else {
        panic!("expected one expansion batch")
    };
    assert_eq!(counts.shell_or_other, 1);
    assert_eq!(counts.text_file, 1);
    assert_eq!(session.expansions, 1);
    let prompt = session.prompt(None).unwrap();
    assert!(prompt.contains("A crystallography experiment"));
    assert!(prompt.contains("crystallography_structure"));
    let mut answer = answer();
    answer["orientation"]["review_plan"]["method_specialist_ids"] =
        json!(["crystallography_structure"]);
    answer["orientation"]["review_plan"]["selection_notes"][1] = json!({"id":"crystallography_structure","reason":"The appendix reports diffraction.","references":["input:appendix:page:9"]});
    assert!(matches!(
        session.accept(answer).unwrap(),
        Outcome::Complete(_)
    ));
}

#[test]
fn invalid_locators_conflicting_outcomes_and_repeated_expansion_cannot_launch() {
    let extraction = extraction();
    let schema = auto_review::orientation_schema();
    let mut session = PlanningSession::new(&extraction, None, None, &schema, None, &[]).unwrap();
    let mut invalid = answer();
    invalid["orientation"]["review_plan"]["selection_notes"][1]["references"] = json!(["page:900"]);
    assert!(session.accept(invalid).is_err());
    let request = json!({"evidence_requests":[{"locator":"page:2","question":"Check the proof."}]});
    let mut mixed = answer();
    mixed["evidence_requests"] = request["evidence_requests"].clone();
    assert!(session.accept(mixed).is_err());
    assert!(session
        .accept(json!({"evidence_requests":[{"locator":"page:900","question":"Missing page"}]}))
        .is_err());
    assert_eq!(session.expansions, 0);
    assert!(matches!(
        session.accept(request.clone()).unwrap(),
        Outcome::Expanded(_)
    ));
    assert!(matches!(
        session.accept(request.clone()).unwrap(),
        Outcome::Expanded(_)
    ));
    assert!(session.accept(request).is_err());
    assert!(session.schema().unwrap()["properties"]
        .get("tool_calls")
        .is_none());
}

#[test]
fn custom_inventory_schema_and_slide_genre_remain_authoritative() {
    let extraction = extraction();
    let mut schema = auto_review::slide_orientation_schema();
    schema["properties"]["notation"]["maxItems"] = json!(3);
    let session = PlanningSession::new(&extraction, None, None, &schema, None, &[]).unwrap();
    assert_eq!(
        session.schema().unwrap()["properties"]["orientation"]["properties"]["notation"]
            ["maxItems"],
        3
    );
    assert_eq!(
        session.schema().unwrap()["properties"]["orientation"]["properties"]["review_plan"]
            ["properties"]["genre"]["enum"],
        json!([auto_review::SLIDE_DECK_GENRE])
    );
    assert!(session.index.is_some());
}

#[test]
fn pagination_is_validated_per_input() {
    let schema = auto_review::orientation_schema();
    for primary_paginated in [true, false] {
        let mut extraction = extraction();
        if !primary_paginated {
            extraction.text =
                "Labor economics wages and labor supply. Theorem 1. Formal proof.".into();
        }
        let inputs = [SupportingInput {
            key: "appendix".into(),
            text: if primary_paginated {
                "A formal equilibrium proof with nontrivial derivations."
            } else {
                "<!-- PAGE 9 -->\nA formal equilibrium proof with nontrivial derivations."
            }
            .into(),
        }];
        let mut session =
            PlanningSession::new(&extraction, None, None, &schema, None, &inputs).unwrap();
        let mut answer = answer();
        answer["orientation"]["review_plan"]["selection_notes"][0]["references"] =
            json!([if primary_paginated {
                "page:1"
            } else {
                "text:1"
            }]);
        answer["orientation"]["review_plan"]["selection_notes"][1]["references"] =
            json!([if primary_paginated {
                "input:appendix:text:1"
            } else {
                "input:appendix:page:9"
            }]);
        assert!(matches!(
            session.accept(answer).unwrap(),
            Outcome::Complete(_)
        ));
    }
}

#[test]
fn preparation_is_durable_before_planning_and_remains_independent() {
    let extraction = extraction();
    let schema = auto_review::orientation_schema();
    let root = tempfile::tempdir().unwrap();
    let mut writer = crate::runs::RunWriter::create_in(root.path(), "preparation-fixture").unwrap();
    let bundle = crate::document_bundle::build(&extraction, writer.dir())
        .unwrap()
        .bundle;
    let original = bundle.to_json_pretty().unwrap();
    let preparation =
        super::super::document_preparation_snapshot(&extraction, Some(&bundle), Some(&schema), &[])
            .unwrap()
            .unwrap();
    writer
        .add_text(
            super::super::PREPARATION_PATH,
            "Document preparation",
            "context",
            &preparation,
        )
        .unwrap();
    assert!(!writer.dir().join(super::super::REVIEW_PLAN_PATH).exists());
    let mut session =
        PlanningSession::new(&extraction, Some(&bundle), None, &schema, None, &[]).unwrap();
    let Outcome::Complete(result) = session.accept(answer()).unwrap() else {
        panic!("expected plan")
    };
    assert_eq!(result.preparation.as_ref().unwrap(), &preparation);
    result.persist(&mut writer).unwrap();
    assert_eq!(bundle.to_json_pretty().unwrap(), original);
    let plan: Value = serde_json::from_str(
        &std::fs::read_to_string(writer.dir().join(super::super::REVIEW_PLAN_PATH)).unwrap(),
    )
    .unwrap();
    assert_eq!(plan, result.orientation["review_plan"]);
    let dir = writer.dir().to_path_buf();
    drop(writer);
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["rel_path"] == super::super::PREPARATION_PATH)
            .count(),
        1
    );
}
