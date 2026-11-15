use super::*;

#[test]
fn choices_name_each_saved_passage_and_series_instead_of_their_kind() {
    let f = fixture(false);
    let insert = |id: &str, kind: &str, body: Value| {
        f.store
            .connection()
            .unwrap()
            .execute(
                "INSERT INTO project_records VALUES(?1,?2,?3,1,?4,'2026-09-07')",
                params![id, f.ws, kind, body.to_string()],
            )
            .unwrap();
    };
    insert(
        "anchor_a",
        "anchor",
        json!({"selection":{"quote":"Wages rise with tenure."},"body":"note"}),
    );
    insert(
        "anchor_b",
        "anchor",
        json!({"selection":{"quote":""},"body":"Whole page table"}),
    );
    insert(
        "series_a",
        "series",
        json!({"series":{"variable":"output","resultId":"irf_1","sampleId":"full"}}),
    );
    insert("task_a", "task", json!({"objective":"Check the sample"}));
    insert("build_a", "build", json!({}));
    let choices = commands::choices(&f.store, &f.ws).unwrap();
    let labels = |record_kind: &str| {
        let mut found = choices["objects"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| o["recordKind"] == record_kind)
            .map(|o| o["label"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        found.sort();
        found
    };
    assert_eq!(
        labels("anchor"),
        ["Wages rise with tenure.", "Whole page table"]
    );
    assert_eq!(labels("series"), ["output \u{b7} irf_1 \u{b7} full"]);
    assert_eq!(labels("task"), ["Check the sample"]);
    // A record with nothing to show keeps its kind rather than an empty option.
    assert_eq!(labels("build"), ["build"]);
}
#[cfg(unix)] // Unix tools or journalled project writes
#[test]
fn figures_accept_a_python_script_profile_and_refuse_other_programs() {
    let f = fixture(true);
    let saved = |id: &str, adapter: &str, argv: &[&str]| {
        fs::write(f.root.join("analysis.py"), "# Fixture").unwrap();
        research::save_execution_profile(
            &f.store,
            research::SaveExecutionProfileRequest {
                profile_id: None,
                workspace_id: f.ws.clone(),
                name: id.into(),
                adapter: adapter.into(),
                argv: argv.iter().map(|a| a.to_string()).collect(),
                cwd: f.root.to_string_lossy().into_owned(),
                environment: json!({}),
                inputs: vec!["analysis.py".into()],
                timeout_seconds: 60,
                outputs: vec![],
                expected_revision: None,
                operation_id: format!("profile-{id}"),
            },
        )
        .unwrap()
    };
    let source = numeric(&f, "baseline", 1.5, "sample");
    let figure = |profile: &research::ExecutionProfile, operation: &str| {
        let mut request = spec(source.clone());
        request.kind = "coefficient".into();
        assets::figure_plan(
            &f.store,
            assets::FigureRequest {
                workspace_id: f.ws.clone(),
                spec: request,
                python_profile_id: profile.id.clone(),
                operation_id: operation.into(),
                supersedes: None,
            },
        )
    };
    let script = saved("script", "python", &[&python(), "analysis.py"]);
    let recipe = figure(&script, "figure-script").unwrap();
    let plan = desk::record(&f.store, &f.ws, recipe.body["planId"].as_str().unwrap()).unwrap();
    assert_eq!(plan.body["profile"]["argv"][1], "plot.py");

    // The plot runs in its own folder, where a relative interpreter means nothing.
    let venv = f.root.join(".venv/bin");
    fs::create_dir_all(&venv).unwrap();
    std::os::unix::fs::symlink(python(), venv.join("python")).unwrap();
    let relative = saved("venv", "python", &[".venv/bin/python", "analysis.py"]);
    let recipe = figure(&relative, "figure-venv").unwrap();
    let plan = desk::record(&f.store, &f.ws, recipe.body["planId"].as_str().unwrap()).unwrap();
    assert_eq!(
        plan.body["profile"]["argv"][0],
        Path::new(&relative.cwd)
            .join(".venv/bin/python")
            .to_string_lossy()
            .as_ref()
    );

    let other = saved("other", "r", &["Rscript", "analysis.R"]);
    assert!(figure(&other, "figure-other").is_err());
}
#[test]
fn rootless_kit_is_small_idempotent_and_never_grants_execution() {
    let f = fixture(false);
    let kit =
        delivery::install(&f.store, &f.ws, "theory", "Customized instructions", "kit").unwrap();
    assert_eq!(
        kit.id,
        delivery::install(&f.store, &f.ws, "theory", "Customized instructions", "kit")
            .unwrap()
            .id
    );
    assert!(delivery::install(&f.store, &f.ws, "theory", "Changed instructions", "kit").is_err());
    assert_eq!(project::records(&f.store, &f.ws, "task").unwrap().len(), 2);
    assert_eq!(
        research::list_notes(&f.store, &f.ws, false).unwrap().len(),
        1
    );
    assert!(research::list_execution_profiles(&f.store, &f.ws)
        .unwrap()
        .is_empty());
    assert!(f.store.workspace(&f.ws).unwrap().root.is_none());
}
#[test]
fn notation_scopes_and_superseding_definition_preserve_source() {
    let f = fixture(false);
    let s = theory::Symbol {
        notation: "x".into(),
        scope: "lemma 1".into(),
        definition: "consumption".into(),
        domain: "positive reals".into(),
        units: "goods".into(),
        aliases: vec![],
        sources: vec![],
    };
    let a = theory::save(&f.store, &f.ws, s.clone(), None, "a").unwrap();
    let mut other = s.clone();
    other.scope = "lemma 2".into();
    other.definition = "income".into();
    theory::save(&f.store, &f.ws, other, None, "b").unwrap();
    assert!(theory::collisions(&f.store, &f.ws).unwrap()["candidates"]
        .as_array()
        .unwrap()
        .is_empty());
    let mut global = s;
    global.scope = "global".into();
    global.definition = "quantity".into();
    theory::save(&f.store, &f.ws, global, Some(&a.id), "c").unwrap();
    assert_eq!(
        theory::collisions(&f.store, &f.ws).unwrap()["candidates"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        desk::record(&f.store, &f.ws, &a.id).unwrap().body["definition"],
        "consumption"
    );
}
#[test]
fn tables_preserve_values_and_reject_incompatible_samples_or_mutated_refs() {
    let f = fixture(true);
    let source = numeric(&f, "r1", 1.23456789, "baseline");
    let mut s = spec(source.clone());
    let r = assets::table(&f.store, &f.ws, s.clone(), None, "table").unwrap();
    let asset: assets::PublicationAsset = serde_json::from_value(r.body.clone()).unwrap();
    assert_eq!(asset.entries[0]["value"]["estimate"], 1.23456789);
    let csv =
        String::from_utf8(artifact_bytes(&f.store, &f.ws, &asset.artifacts[2].id, 10000).unwrap())
            .unwrap();
    assert!(csv.contains("1.23456789"));
    s.entries.push(assets::Entry {
        label: "Other".into(),
        source: numeric(&f, "r2", 2.0, "alternative"),
        manual: None,
    });
    assert!(assets::table(&f.store, &f.ws, s.clone(), None, "bad").is_err());
    s.sample_comparison_rationale = Some("Compare the explicitly different samples".into());
    assert!(assets::table(&f.store, &f.ws, s, None, "different").is_ok());
    let mut changed = source;
    changed.revision = "later".into();
    assert!(assets::table(&f.store, &f.ws, spec(changed), None, "bad-revision").is_err());
}
#[test]
fn manual_overrides_are_labeled_and_do_not_invent_uncertainty() {
    let f = fixture(true);
    let mut s = spec(numeric(&f, "r", 1.0, "sample"));
    s.entries[0].manual = Some(assets::ManualValue {
        value: 2.0,
        reason: "Correct a known transcription error, pending rerun".into(),
    });
    let r = assets::table(&f.store, &f.ws, s.clone(), None, "manual").unwrap();
    assert_eq!(r.body["entries"][0]["value"]["originalEstimate"], 1.0);
    assert!(r.body["entries"][0]["value"]["confidenceInterval"].is_null());
    s.entries[0].manual.as_mut().unwrap().reason.clear();
    assert!(assets::table(&f.store, &f.ws, s, None, "invalid").is_err());
}
#[test]
fn deliverable_regeneration_keeps_old_draft_and_exact_sources() {
    let f = fixture(false);
    let n = delivery::note(
        &f.store,
        &f.ws,
        "question",
        "What mechanism explains the result?",
        "note",
    )
    .unwrap();
    let source = ResearchObjectRef {
        kind: "note".into(),
        id: n.id,
        revision: n.revision.to_string(),
        start: None,
        end: None,
    };
    let mut outline = delivery::Outline {
        title: "Seminar".into(),
        template: "seminar".into(),
        sections: vec![delivery::Section {
            heading: "Question".into(),
            text: "Initial explanation.".into(),
            sources: vec![source],
            assets: vec![],
        }],
    };
    let a = delivery::assemble(&f.store, &f.ws, outline.clone(), None, "a").unwrap();
    outline.sections[0].text = "Revised explanation.".into();
    let b = delivery::assemble(&f.store, &f.ws, outline, Some(&a.id), "b").unwrap();
    assert_ne!(a.content_hash, b.content_hash);
    assert_eq!(
        desk::record(&f.store, &f.ws, &a.id).unwrap().body["outline"]["sections"][0]["text"],
        "Initial explanation."
    );
    assert_eq!(b.body["sources"].as_array().unwrap().len(), 1);
}
#[cfg(unix)] // Unix tools or journalled project writes
#[test]
fn checked_artifacts_and_task_staging_refuse_cross_project_and_external_changes() {
    let f = fixture(true);
    fs::write(f.root.join("paper.md"), "original").unwrap();
    let task = project::mutate(
        &f.store,
        project::ProjectMutation {
            workspace_id: f.ws.clone(),
            operation_id: "task".into(),
            action: project::ProjectAction::CreateTask {
                objective: "Stage publication".into(),
                anchor_id: None,
                expected_outputs: vec![],
                expected_checks: vec![],
            },
        },
    )
    .unwrap();
    let cp = project::mutate(
        &f.store,
        project::ProjectMutation {
            workspace_id: f.ws.clone(),
            operation_id: "copy".into(),
            action: project::ProjectAction::Checkpoint {
                task_id: task["id"].as_str().unwrap().into(),
                paths: vec!["paper.md".into()],
                backend: "copy".into(),
            },
        },
    )
    .unwrap();
    let r = assets::table(
        &f.store,
        &f.ws,
        spec(numeric(&f, "r", 1.0, "s")),
        None,
        "table",
    )
    .unwrap();
    let id = r.body["artifacts"][0]["id"].as_str().unwrap();
    let checkpoint = cp["id"].as_str().unwrap();
    assert!(assets::stage(
        &f.store,
        &f.ws,
        &r.id,
        id,
        checkpoint,
        "paper.md",
        Some("wrong")
    )
    .is_err());
    assets::stage(
        &f.store,
        &f.ws,
        &r.id,
        id,
        checkpoint,
        "paper.md",
        Some(&desk::hash(b"original")),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join("paper.md")).unwrap(),
        "original"
    );
    assert!(assets::stage(&f.store, &f.ws, &r.id, id, checkpoint, "../escaped", None).is_err());
    let other = fixture(false);
    assert!(read_artifact(&f.store, &other.ws, id).is_err());
}

#[test]
#[ignore = "Requires an explicitly selected Python with matplotlib and installed TeX; emits artifacts for visual inspection"]
fn qualified_scientific_figures_and_tex_table() {
    let output = std::path::PathBuf::from(
        std::env::var("PIPELINE_PROGRAM_QA_DIR").expect("Set an explicit QA output directory"),
    );
    fs::create_dir_all(&output).unwrap();
    assert!(std::env::var("PIPELINE_PROGRAM_PYTHON").is_ok());
    let f = fixture(true);
    let p = profile(&f, "# Selected Python environment", 120);
    let a = numeric(&f, "baseline", 1.23456789, "sample");
    let b = numeric(&f, "alternative", 0.75, "sample");
    let mut table = spec(a.clone());
    table.entries.push(assets::Entry {
        label: "Alternative specification with retained interval".into(),
        source: b.clone(),
        manual: None,
    });
    let t = assets::table(&f.store, &f.ws, table.clone(), None, "table").unwrap();
    let ta: assets::PublicationAsset = serde_json::from_value(t.body).unwrap();
    for a in &ta.artifacts {
        fs::write(
            output.join(format!("table.{}", a.extension)),
            artifact_bytes(&f.store, &f.ws, &a.id, 8 * 1024 * 1024).unwrap(),
        )
        .unwrap();
    }
    table.kind = "coefficient".into();
    table.title = "Coefficients across prespecified alternatives".into();
    table.notes="Points and retained confidence intervals. Values are shown in percentage points; neither statistical significance nor causal validity is inferred.".into();
    let recipe = assets::figure_plan(
        &f.store,
        assets::FigureRequest {
            workspace_id: f.ws.clone(),
            spec: table,
            python_profile_id: p.id.clone(),
            operation_id: "figure".into(),
            supersedes: None,
        },
    )
    .unwrap();
    let run_figure = |recipe: &DeskRecord, operation: &str, prefix: &str| {
        let plan = desk::record(&f.store, &f.ws, recipe.body["planId"].as_str().unwrap()).unwrap();
        research::execution_plan::authorize(&f.store, &f.ws, &plan.id, &plan.content_hash).unwrap();
        let e = research::run_execution(
            &f.store,
            research::RunExecutionRequest {
                plan_id: Some(plan.id),
                profile_id: plan.body["profile"]["id"].as_str().unwrap().into(),
                session_id: None,
                test_only: true,
                operation_id: operation.into(),
            },
        )
        .unwrap();
        assert_eq!(e.outcome, "completed", "{:?}", e.stderr);
        let asset = assets::adopt_figure(
            &f.store,
            &f.ws,
            &recipe.id,
            &e.id,
            &format!("{operation}-adopt"),
        )
        .unwrap();
        for a in asset.body["artifacts"].as_array().unwrap() {
            let a: Artifact = serde_json::from_value(a.clone()).unwrap();
            fs::write(
                output.join(format!("{prefix}.{}", a.extension)),
                artifact_bytes(&f.store, &f.ws, &a.id, 8 * 1024 * 1024).unwrap(),
            )
            .unwrap();
        }
        asset
    };
    let coefficient = run_figure(&recipe, "plot", "coefficient");
    assert_eq!(
        coefficient.body["entries"][0]["value"]["estimate"],
        1.23456789
    );
    let series = |id: &str, values: Value| {
        let series = json!({"resultId":id,"variable":"Output","units":"percent deviation","shockNormalization":"One percentage-point innovation","horizonUnit":"quarters","horizons":[0.0,1.0,2.0,3.0,4.0,5.0,6.0],"values":values,"specificationId":id,"sampleId":"model"});
        let artifact = blob(
            &f.store,
            &f.ws,
            &serde_json::to_vec(&json!({"schema":"research-results-v2","series":[series]}))
                .unwrap(),
            "json",
        )
        .unwrap();
        let mut manifest = research::get_execution(&f.store, "execution_baseline")
            .unwrap()
            .output_manifest;
        if !manifest["artifacts"].is_array() {
            manifest["artifacts"] = json!([]);
        }
        manifest["artifacts"].as_array_mut().unwrap().push(json!({"artifactId":artifact.id,"contentHash":artifact.hash,"path":format!("{id}.json")}));
        f.store.connection().unwrap().execute("UPDATE research_executions SET output_manifest_json=?1 WHERE id='execution_baseline'",[manifest.to_string()]).unwrap();
        let body = json!({"series":series,"executionId":"execution_baseline","artifactId":artifact.id,"artifactHash":artifact.hash,"locator":"series/0","provenance":"host_adopted_output"});
        f.store
            .connection()
            .unwrap()
            .execute(
                "INSERT INTO project_records VALUES(?1,?2,'series',1,?3,'2026-09-07')",
                params![id, f.ws, body.to_string()],
            )
            .unwrap();
        ResearchObjectRef {
            kind: "record".into(),
            id: id.into(),
            revision: "1".into(),
            start: None,
            end: None,
        }
    };
    let irf=assets::AssetSpec{kind:"irf".into(),title:"Output response to a normalized innovation".into(),entries:vec![assets::Entry{label:"Baseline: estimated adjustment".into(),source:series("irf1",json!([0.0,1.0,0.8,null,0.3,0.2,0.1])),manual:None},assets::Entry{label:"Alternative adjustment mechanism".into(),source:series("irf2",json!([0.0,0.6,0.55,0.45,0.35,0.25,0.15])),manual:None}],digits:3,uncertainty:"none".into(),notes:"One percentage-point innovation. Missing observations remain gaps; lines connect observed horizons only. No uncertainty bands were supplied.".into(),sample_comparison_rationale:None};
    let irf_recipe = assets::figure_plan(
        &f.store,
        assets::FigureRequest {
            workspace_id: f.ws.clone(),
            spec: irf,
            python_profile_id: p.id,
            operation_id: "irf".into(),
            supersedes: None,
        },
    )
    .unwrap();
    run_figure(&irf_recipe, "irf-run", "irf");
    fs::write(output.join("table-test.tex"),"\\documentclass{article}\n\\usepackage[T1]{fontenc}\n\\usepackage[margin=1in]{geometry}\n\\begin{document}\n\\input{table.tex}\n\\end{document}\n").unwrap();
    let result = std::process::Command::new("/Library/TeX/texbin/pdflatex")
        .args([
            "-no-shell-escape",
            "-interaction=nonstopmode",
            "-halt-on-error",
            "table-test.tex",
        ])
        .current_dir(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    let log = fs::read_to_string(output.join("table-test.log")).unwrap();
    assert!(!log.contains("Overfull"), "Table overflows its page: {log}");
    println!("Visual qualification artifacts: {}", output.display());
}

#[test]
fn publication_reuses_existing_adopted_artifact_identity() {
    let f = fixture(false);
    let a = blob(&f.store, &f.ws, b"retained content", "json").unwrap();
    f.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE artifacts SET id='adopted_output_identity',origin='execution' WHERE id=?1",
            [a.id],
        )
        .unwrap();
    let b = blob(&f.store, &f.ws, b"retained content", "json").unwrap();
    assert_eq!(b.id, "adopted_output_identity");
    assert_eq!(
        artifact_bytes(&f.store, &f.ws, &b.id, 100).unwrap(),
        b"retained content"
    );
}
#[test]
fn changed_result_execution_propagates_through_publication_into_deliverable() {
    let f = fixture(true);
    let source = numeric(&f, "impact_result", 1.0, "sample");
    let asset = assets::table(&f.store, &f.ws, spec(source), None, "impact-table").unwrap();
    let draft = delivery::assemble(
        &f.store,
        &f.ws,
        delivery::Outline {
            title: "Figure-backed seminar".into(),
            template: "seminar".into(),
            sections: vec![delivery::Section {
                heading: "Result".into(),
                text: "Inspect the retained result".into(),
                sources: vec![],
                assets: vec![asset.id.clone()],
            }],
        },
        None,
        "impact-draft",
    )
    .unwrap();
    f.store.connection().unwrap().execute("UPDATE research_executions SET outcome='outcome_unknown' WHERE id='execution_impact_result'",[]).unwrap();
    let report = project::relations::impact(&f.store, &f.ws).unwrap();
    let impact = report
        .impacts
        .iter()
        .find(|i| i.object.id == draft.id)
        .unwrap();
    assert_eq!(impact.status, "review_needed");
    assert!(impact.path.iter().any(|p| p.id == asset.id));
    assert_eq!(
        desk::record(&f.store, &f.ws, &draft.id)
            .unwrap()
            .content_hash,
        draft.content_hash
    );
}
