use super::*;

pub(super) fn theory_note(kind: &str, status: &str) -> TheoryNote {
    TheoryNote {
        kind: kind.into(),
        title: "Existence under log utility".into(),
        statement: "An equilibrium exists when utility is logarithmic.".into(),
        body: "Fixed-point argument over the price simplex.\nStep 2 uses compactness.".into(),
        assumptions: vec!["Preferences are strictly convex".into()],
        assumption_ids: vec![],
        anchor_ids: vec![],
        related_ids: vec![],
        unresolved_steps: vec![],
        status: status.into(),
        rejection_reason: String::new(),
        origin: "manual".into(),
        promotions: vec![],
        imported_from: None,
        imported_status: None,
    }
}
#[test]
fn theory_notes_keep_unresolved_steps_and_abandoned_reasons_discoverable() {
    let f = fixture();
    let mut sketch = theory_note("proof_sketch", "supported");
    sketch.unresolved_steps = vec!["Continuity of the excess-demand map".into()];
    assert!(save_theory(&f.store, &f.ws, None, 0, sketch.clone()).is_err());
    sketch.status = "open".into();
    let saved = save_theory(&f.store, &f.ws, None, 0, sketch.clone()).unwrap();
    assert_eq!(
        saved.body["unresolvedSteps"][0],
        "Continuity of the excess-demand map"
    );
    let overview = theory_overview(&f.store, &f.ws).unwrap();
    assert!(overview.evidence[&saved.id]
        .label
        .contains("not a complete proof"));

    let mut proposed = theory_note("conjecture", "supported");
    proposed.origin = "proposed".into();
    assert!(save_theory(&f.store, &f.ws, None, 0, proposed).is_err());

    let mut rejected = theory_note("rejected_approach", "abandoned");
    assert!(save_theory(&f.store, &f.ws, None, 0, rejected.clone()).is_err());
    rejected.rejection_reason =
        "The contraction constant exceeds one for plausible discount factors.".into();
    let mut open_rejected = rejected.clone();
    open_rejected.status = "open".into();
    assert!(save_theory(&f.store, &f.ws, None, 0, open_rejected).is_err());
    let rejected = save_theory(&f.store, &f.ws, None, 0, rejected).unwrap();
    // The assistant reads abandoned approaches through the notes they are
    // copied into, once and only once.
    assert!(!project_context(&f.store, &f.ws)
        .unwrap()
        .contains("contraction constant exceeds one"));
    let convert = crate::workbench::project::earlier_records::convert_abandoned_approaches;
    assert_eq!(convert(&f.store, &f.ws).unwrap(), 1);
    assert_eq!(convert(&f.store, &f.ws).unwrap(), 0);
    let notes = research::list_notes(&f.store, &f.ws, true).unwrap();
    let copied = notes
        .iter()
        .find(|n| n.origin == format!("theory:{}", rejected.id))
        .unwrap();
    assert_eq!(
        (copied.kind.as_str(), copied.state.as_str()),
        ("decision", "accepted")
    );
    let context = project_context(&f.store, &f.ws).unwrap();
    assert!(context.contains("Abandoned approach: Existence under log utility"));
    assert!(context.contains("contraction constant exceeds one"));
    assert!(context.contains("Preferences are strictly convex"));
    assert!(!context.contains("Fixed-point argument over the price simplex"));

    let mut linked = theory_note("derivation", "open");
    linked.assumption_ids = vec![saved.id.clone()];
    assert!(save_theory(&f.store, &f.ws, None, 0, linked.clone()).is_err());
    let assumption =
        save_theory(&f.store, &f.ws, None, 0, theory_note("assumption", "open")).unwrap();
    linked.assumption_ids = vec![assumption.id.clone()];
    linked.promotions = vec![Promotion {
        checkpoint_id: "fake".into(),
        path: "x.tex".into(),
        note_revision: 9,
        promoted_at: "now".into(),
    }];
    let linked = save_theory(&f.store, &f.ws, None, 0, linked).unwrap();
    assert_eq!(linked.body["promotions"], json!([]));
    assert!(studio_records(&f.store, &f.ws, "theory").unwrap().len() >= 4);
    assert_eq!(studio_history(&f.store, &f.ws, &saved.id).unwrap().len(), 0);
}
#[test]
fn numerical_checks_never_become_general_proofs() {
    let f = fixture();
    let note = save_theory(&f.store, &f.ws, None, 0, theory_note("proposition", "open")).unwrap();
    let check = |method: &str, outcome: &str, domain: &str, general: bool| TheoryCheck {
        theory_id: note.id.clone(),
        method: method.into(),
        outcome: outcome.into(),
        summary: "Grid over discount factors in (0.5, 0.99).".into(),
        domain: domain.into(),
        tolerance: Some(1e-8),
        precision: Some(10),
        execution_id: None,
        recipe_run_id: None,
        anchor_ids: vec![],
        claims_generality: general,
        origin: "manual".into(),
    };
    assert!(save_check(
        &f.store,
        &f.ws,
        None,
        0,
        check(
            "numerical_verification",
            "passed",
            "beta in (0.5,0.99)",
            true
        )
    )
    .is_err());
    assert!(save_check(
        &f.store,
        &f.ws,
        None,
        0,
        check("numerical_verification", "passed", "", false)
    )
    .is_err());
    assert!(save_check(
        &f.store,
        &f.ws,
        None,
        0,
        check("model_assessment", "passed", "", true)
    )
    .is_err());
    let mut bad_tolerance = check(
        "numerical_verification",
        "passed",
        "beta in (0.5,0.99)",
        false,
    );
    bad_tolerance.tolerance = Some(f64::NAN);
    assert!(save_check(&f.store, &f.ws, None, 0, bad_tolerance).is_err());
    let mut unknown_execution = check(
        "numerical_verification",
        "passed",
        "beta in (0.5,0.99)",
        false,
    );
    unknown_execution.execution_id = Some("exec_missing".into());
    assert!(save_check(&f.store, &f.ws, None, 0, unknown_execution).is_err());

    let numerical = save_check(
        &f.store,
        &f.ws,
        None,
        0,
        check(
            "numerical_verification",
            "passed",
            "beta in (0.5,0.99)",
            false,
        ),
    )
    .unwrap();
    assert_eq!(numerical.body["scope"], "instances_only");
    let overview = theory_overview(&f.store, &f.ws).unwrap();
    assert!(overview.evidence[&note.id]
        .label
        .contains("tested instances only"));

    let analytical = save_check(
        &f.store,
        &f.ws,
        None,
        0,
        check("analytical_argument", "passed", "", true),
    )
    .unwrap();
    assert_eq!(analytical.body["scope"], "general");
    assert!(analytical.body["label"]
        .as_str()
        .unwrap()
        .contains("not machine-verified"));
    let overview = theory_overview(&f.store, &f.ws).unwrap();
    assert_eq!(overview.evidence[&note.id].analytical, 1);
    assert!(overview.evidence[&note.id]
        .label
        .contains("Analytical argument"));

    let counter = save_check(
        &f.store,
        &f.ws,
        None,
        0,
        check("numerical_counterexample", "passed", "beta = 0.51", false),
    )
    .unwrap();
    assert_eq!(counter.body["scope"], "refuting_instance");
    let overview = theory_overview(&f.store, &f.ws).unwrap();
    assert!(overview.evidence[&note.id].label.contains("counterexample"));
    assert_eq!(overview.checks.len(), 3);
    assert!(save_check(
        &f.store,
        &f.ws,
        Some(&counter.id),
        0,
        check("heuristic", "inconclusive", "", false)
    )
    .is_err());
}
#[test]
fn directions_convert_into_tasks_without_scores() {
    let f = fixture();
    assert!(serde_json::from_value::<ResearchDirection>(
        json!({"question":"q","status":"idea","noveltyScore":0.9})
    )
    .is_err());
    let direction = ResearchDirection {
        question: "Does search friction explain the wage gap?".into(),
        mechanism: "Directed search with heterogeneous vacancies".into(),
        closest_known_work: "Menzio and Shi".into(),
        minimal_model_or_data: "Two-type block-recursive model".into(),
        first_discriminating_test: String::new(),
        likely_failure_mode: "Gap vanishes with symmetric vacancies".into(),
        next_action: "Write the two-type model".into(),
        status: "idea".into(),
        task_id: None,
        theory_ids: vec![],
        drop_reason: String::new(),
    };
    let mut converted = direction.clone();
    converted.status = "converted".into();
    assert!(save_direction(&f.store, &f.ws, None, 0, converted).is_err());
    let mut dropped = direction.clone();
    dropped.status = "dropped".into();
    assert!(save_direction(&f.store, &f.ws, None, 0, dropped).is_err());
    let saved = save_direction(&f.store, &f.ws, None, 0, direction.clone()).unwrap();
    assert!(convert_direction(&f.store, &f.ws, &saved.id, saved.revision).is_err());
    let mut with_test = direction;
    with_test.first_discriminating_test =
        "Compare gap under symmetric and asymmetric vacancy costs".into();
    let saved =
        save_direction(&f.store, &f.ws, Some(&saved.id), saved.revision, with_test).unwrap();
    let result = convert_direction(&f.store, &f.ws, &saved.id, saved.revision).unwrap();
    assert_eq!(result["direction"]["body"]["status"], "converted");
    let task_id = result["task"]["id"].as_str().unwrap();
    assert_eq!(result["direction"]["body"]["taskId"], task_id);
    let task = record(&f.store, &f.ws, task_id, "task").unwrap();
    assert!(task.body["objective"]
        .as_str()
        .unwrap()
        .contains("First discriminating test"));
    assert_eq!(
        task.body["expectedChecks"][0],
        "Compare gap under symmetric and asymmetric vacancy costs"
    );
    let revision = result["direction"]["revision"].as_i64().unwrap();
    assert!(convert_direction(&f.store, &f.ws, &saved.id, revision).is_err());
    assert_eq!(
        theory_overview(&f.store, &f.ws).unwrap().directions.len(),
        1
    );
}
#[cfg(unix)]
#[test]
fn promotion_stages_derivation_prose_without_changing_assumptions() {
    let f = fixture();
    fs::write(
        f.root.join("main.tex"),
        "\\section{Model}\nText.\n\\section{Proofs}\n",
    )
    .unwrap();
    let task = tasks::create_task(
        &f.store,
        &f.ws,
        "Promote derivation".into(),
        None,
        vec![],
        vec![],
    )
    .unwrap();
    let cp = tasks::checkpoint(&f.store, &f.ws, &task.id, vec!["main.tex".into()], "copy").unwrap();
    let note = save_theory(&f.store, &f.ws, None, 0, theory_note("derivation", "open")).unwrap();
    assert!(promote_theory(&f.store, &f.ws, &cp.id, "analysis.py", &note.id, None).is_err());
    assert!(promote_theory(&f.store, &f.ws, &cp.id, "main.tex", &note.id, Some(99)).is_err());
    let mut rejected = theory_note("rejected_approach", "abandoned");
    rejected.rejection_reason = "Fails without compactness".into();
    let rejected = save_theory(&f.store, &f.ws, None, 0, rejected).unwrap();
    assert!(promote_theory(&f.store, &f.ws, &cp.id, "main.tex", &rejected.id, None).is_err());

    let result = promote_theory(&f.store, &f.ws, &cp.id, "main.tex", &note.id, Some(3)).unwrap();
    let staged = fs::read_to_string(
        session_task_root(&f.store, &f.ws, &cp.id)
            .unwrap()
            .join("main.tex"),
    )
    .unwrap();
    assert!(
        staged.starts_with("\\section{Model}\nText.\n\\section{Proofs}\n% Pipeline theory note")
    );
    assert!(staged.contains(
        "Assumptions referenced, unchanged by this promotion: Preferences are strictly convex"
    ));
    assert!(staged.contains("Fixed-point argument over the price simplex."));
    assert!(staged
        .trim_end()
        .ends_with(&format!("% End theory note {}", note.id)));
    assert_eq!(
        fs::read_to_string(f.root.join("main.tex")).unwrap(),
        "\\section{Model}\nText.\n\\section{Proofs}\n"
    );
    assert_eq!(result["checkpoint"]["body"]["state"], "review");
    assert_eq!(
        result["note"]["body"]["promotions"][0]["checkpointId"],
        cp.id
    );
    assert_eq!(
        result["note"]["body"]["assumptions"],
        json!(["Preferences are strictly convex"])
    );
    let appended = promote_theory(&f.store, &f.ws, &cp.id, "notes.md", &note.id, None).unwrap();
    let md = fs::read_to_string(
        session_task_root(&f.store, &f.ws, &cp.id)
            .unwrap()
            .join("notes.md"),
    )
    .unwrap();
    assert!(md.starts_with("<!-- Pipeline theory note"));
    assert_eq!(
        appended["note"]["body"]["promotions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
