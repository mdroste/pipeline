use super::*;
static JOB_TEST_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
#[test]
fn bibtex_preserves_nested_fields_and_duplicate_keys() {
    let text =
        "@article{a, title={A {nested} title}, year=2020, custom={x,y}}\n@book{a,title=\"Second\"}";
    let entries = parse_bibtex(text).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].fields["custom"], "x,y");
    assert!(entries[1].warnings[0].contains("Duplicate"));
    assert_eq!(&text[entries[0].start..entries[0].end], entries[0].raw);
    assert!(parse_bibtex("@article{x,title={unterminated}").is_err());
}
struct Fixture {
    _temp: tempfile::TempDir,
    store: Store,
    ws: String,
    root: PathBuf,
}
fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let ws = store
        .create_workspace(crate::workbench::store::CreateWorkspaceRequest {
            name: "Studio".into(),
            root: Some(root.to_string_lossy().into_owned()),
            operation_id: "create".into(),
        })
        .unwrap()
        .record
        .id;
    Fixture {
        _temp: temp,
        store,
        ws,
        root,
    }
}
fn act(f: &Fixture, action: StudioAction) -> Value {
    studio_mutate(
        &f.store,
        StudioMutation {
            workspace_id: f.ws.clone(),
            operation_id: id("test").unwrap(),
            action,
        },
    )
    .unwrap()
}
fn paper(f: &Fixture, name: &str, text: &str) -> research::PaperWithRevision {
    fs::write(f.root.join(name), text).unwrap();
    research::import_paper(
        &f.store,
        research::ImportPaperRequest {
            workspace_id: f.ws.clone(),
            paper_id: None,
            title: name.into(),
            role: "other".into(),
            path: f.root.join(name).to_string_lossy().into_owned(),
            operation_id: id("paper").unwrap(),
        },
    )
    .unwrap()
}
#[cfg(unix)]
fn profile(
    f: &Fixture,
    command: &str,
    inputs: Vec<String>,
    outputs: Vec<String>,
) -> research::ExecutionProfile {
    let p = research::save_execution_profile(
        &f.store,
        research::SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: f.ws.clone(),
            name: "fixture".into(),
            adapter: "command".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), command.into()],
            cwd: f.root.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs,
            timeout_seconds: 10,
            outputs,
            expected_revision: None,
            operation_id: id("profile").unwrap(),
        },
    )
    .unwrap();
    let preview = research::preview_host_execution(&f.store, &p.id).unwrap();
    research::authorize_host_execution(&f.store, &p.id, &preview.fingerprint).unwrap();
    p
}
#[cfg(unix)]
fn exported_result(
    f: &Fixture,
    estimate: f64,
    units: &str,
) -> (research::ResearchExecution, ResultRef) {
    let payload = json!({"schema":"research-results-v1","results":[{"resultId":"coefficient","estimand":"slope","specificationId":"main","sampleId":"sample","estimate":estimate,"standardError":0.1,"confidenceInterval":[estimate-0.2,estimate+0.2],"n":100,"units":units,"transformation":null,"uncertaintyMethod":"OLS","sourceExecutionId":"forged-host-execution","artifactLocator":"forged.json"}]});
    fs::write(f.root.join("input.json"), payload.to_string()).unwrap();
    let p = profile(
        f,
        "cp input.json results.json",
        vec!["input.json".into()],
        vec!["results.json".into()],
    );
    let e = research::run_execution(
        &f.store,
        research::RunExecutionRequest {
            profile_id: p.id,
            session_id: None,
            test_only: true,
            operation_id: id("execute").unwrap(),
        },
    )
    .unwrap();
    assert_eq!(e.outcome, "completed");
    let artifact = e.output_manifest["artifacts"][0]["artifactId"]
        .as_str()
        .unwrap();
    import_results(&f.store, &f.ws, &e.id, artifact).unwrap();
    let r = ResultRef {
        execution_id: e.id.clone(),
        result_id: "coefficient".into(),
    };
    (e, r)
}
#[cfg(unix)]
#[test]
fn editor_saves_are_conflict_aware_reversible_and_task_scoped() {
    let f = fixture();
    fs::write(f.root.join("main.tex"), "original").unwrap();
    let opened = editor_file(&f.store, &f.ws, None, "main.tex").unwrap();
    fs::write(f.root.join("main.tex"), "external edit").unwrap();
    assert!(save_text(&f.store, &f.ws, None, "main.tex", &opened.hash, "draft").is_err());
    assert_eq!(
        fs::read_to_string(f.root.join("main.tex")).unwrap(),
        "external edit"
    );
    let opened = editor_file(&f.store, &f.ws, None, "main.tex").unwrap();
    let saved = act(
        &f,
        StudioAction::SaveText {
            checkpoint_id: None,
            path: "main.tex".into(),
            expected_hash: opened.hash,
            content: "accepted editor text".into(),
        },
    );
    assert_eq!(
        fs::read_to_string(f.root.join("main.tex")).unwrap(),
        "accepted editor text"
    );
    let app = saved["application"]["id"].as_str().unwrap();
    tasks::undo(&f.store, &f.ws, app).unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join("main.tex")).unwrap(),
        "external edit"
    );
    assert!(editor_file(&f.store, &f.ws, None, "../secret.tex").is_err());
}
#[test]
fn build_config_is_inert_and_retains_declared_root() {
    let f = fixture();
    fs::create_dir(f.root.join("paper")).unwrap();
    fs::write(f.root.join("paper/main.tex"), "\\documentclass{article}").unwrap();
    let config = BuildConfig {
        name: "Paper".into(),
        checkpoint_id: None,
        directory: "paper".into(),
        root_document: "main.tex".into(),
        expected_pdf: "main.pdf".into(),
        engine: "pdflatex".into(),
        timeout_seconds: 120,
        inputs: vec!["main.tex".into()],
        advanced_argv: None,
    };
    let saved = save_build(&f.store, &f.ws, None, 0, config.clone()).unwrap();
    let p: research::ExecutionProfile =
        serde_json::from_value(saved.body["profile"].clone()).unwrap();
    assert!(p.argv.contains(&"-no-shell-escape".to_string()));
    assert_eq!(
        p.cwd,
        f.root
            .join("paper")
            .canonicalize()
            .unwrap()
            .to_string_lossy()
    );
    assert!(research::list_executions(&f.store, &f.ws)
        .unwrap()
        .is_empty());
    assert!(!research::preview_host_execution(&f.store, &p.id)
        .map(|p| p.authorized)
        .unwrap_or(false));
    let mut invalid = config;
    invalid.advanced_argv = Some(vec![
        "pdflatex".into(),
        "--shell-escape".into(),
        "main.tex".into(),
    ]);
    assert!(save_build(&f.store, &f.ws, None, 0, invalid).is_err());
    let d=parse_diagnostics("./section.tex:12: Undefined control sequence\nLaTeX Warning: Citation `x' undefined\nOutput written on main.pdf");
    assert_eq!(d.len(), 2);
    assert_eq!(d[0].line, Some(12));
    assert_eq!(d[1].severity, "warning");
}
#[test]
fn report_import_is_idempotent_and_preserves_corrected_decisions() {
    let f = fixture();
    let p = paper(
        &f,
        "report.md",
        "The appendix lacks a derivation.\n\nAdd a robustness analysis.",
    );
    let revision = p.revision.unwrap().id;
    let mut comments = preview_report(&f.store, &f.ws, &revision).unwrap();
    comments[0].number = "R1.2".into();
    let imported = import_report(&f.store, &f.ws, &revision, comments.clone()).unwrap();
    let id = imported[0]["id"].as_str().unwrap();
    let old = record(&f.store, &f.ws, id, "response").unwrap();
    let mut body: ResponseRecord = decode(&old).unwrap();
    body.decision.disposition = "rejected".into();
    body.decision.rationale = "Derivation is present in Appendix B".into();
    body.decision.counterargument = "See equation B.4".into();
    body.decision.draft = "The derivation is already in Appendix B.".into();
    save_response(&f.store, &f.ws, id, old.revision, body.decision).unwrap();
    import_report(&f.store, &f.ws, &revision, comments).unwrap();
    assert_eq!(records(&f.store, &f.ws, "response").unwrap().len(), 2);
    assert_eq!(
        record(&f.store, &f.ws, id, "response").unwrap().body["decision"]["disposition"],
        "rejected"
    );
    assert_eq!(studio_history(&f.store, &f.ws, id).unwrap().len(), 1);
    let letter = export_responses(&f.store, &f.ws, &[id.into()], "latex").unwrap();
    assert!(letter["text"].as_str().unwrap().contains("Comment R1.2"));
    assert!(letter["text"].as_str().unwrap().contains("rejected"));
}
#[test]
fn workflow_occurrences_retain_source_versions_without_closing_by_agreement() {
    let f = fixture();
    let finding = crate::models::Finding {
        id: "issue-1".into(),
        title: "Appendix".into(),
        body: "Missing derivation".into(),
        ..Default::default()
    };
    let mut package = FindingPackage {
        version: 1,
        run_id: "run-1".into(),
        source_step_id: "final".into(),
        findings: vec![finding],
    };
    let imported = import_findings(&f.store, &f.ws, package.clone(), &["issue-1".into()]).unwrap();
    let id = imported[0]["id"].as_str().unwrap();
    let old = record(&f.store, &f.ws, id, "response").unwrap();
    let mut d: ResponseRecord = decode(&old).unwrap();
    d.decision.disposition = "deferred".into();
    d.decision.rationale = "Needs a separate experiment".into();
    save_response(&f.store, &f.ws, id, old.revision, d.decision).unwrap();
    package.findings[0].body = "All reviewers agree".into();
    import_findings(&f.store, &f.ws, package, &["issue-1".into()]).unwrap();
    let updated = record(&f.store, &f.ws, id, "response").unwrap();
    assert_eq!(updated.body["decision"]["disposition"], "deferred");
    assert_eq!(studio_history(&f.store, &f.ws, id).unwrap().len(), 2);
    assert_eq!(records(&f.store, &f.ws, "response").unwrap().len(), 1);
}
#[test]
fn added_analysis_statements_are_flagged_and_tex_export_escapes_source() {
    let f = fixture();
    let p = paper(&f, "report.md", "Check 10% & x_y.");
    let revision = p.revision.unwrap().id;
    let comments = preview_report(&f.store, &f.ws, &revision).unwrap();
    let imported = import_report(&f.store, &f.ws, &revision, comments).unwrap();
    let object_id = imported[0]["id"].as_str().unwrap();
    let old = record(&f.store, &f.ws, object_id, "response").unwrap();
    let mut d: ResponseRecord = decode(&old).unwrap();
    d.decision.draft = "We added a robustness analysis.".into();
    let r = save_response(&f.store, &f.ws, object_id, old.revision, d.decision).unwrap();
    assert_eq!(r.body["flags"].as_array().unwrap().len(), 3);
    let letter = export_responses(&f.store, &f.ws, &[object_id.into()], "latex").unwrap();
    assert_eq!(letter["draft"], true);
    assert!(letter["text"].as_str().unwrap().contains("10\\% \\& x\\_y"));
}
#[cfg(unix)]
#[test]
fn adopted_export_overrides_provenance_and_comparison_requires_real_conversion() {
    let f = fixture();
    let (e, left) = exported_result(&f, 2., "percent");
    let r = result(&f.store, &f.ws, &left).unwrap();
    assert_eq!(r.source_execution_id, e.id);
    assert_eq!(r.artifact_locator, "results.json#results/0");
    let artifact = e.output_manifest["artifacts"][0]["artifactId"]
        .as_str()
        .unwrap();
    import_results(&f.store, &f.ws, &e.id, artifact).unwrap();
    assert_eq!(
        research::list_structured_results(&f.store, &f.ws)
            .unwrap()
            .len(),
        1
    );
    let (_, right) = exported_result(&f, 0.03, "fraction");
    let compare = |conversion| CompareExperimentRequest {
        workspace_id: f.ws.clone(),
        left: left.clone(),
        right: right.clone(),
        rationale: "Same quantity".into(),
        conversion,
        relative_meaningful: true,
    };
    assert_eq!(
        compare_experiment(&f.store, compare(None)).unwrap()["comparable"],
        false
    );
    let compared = compare_experiment(
        &f.store,
        compare(Some(UnitConversion {
            from_units: "fraction".into(),
            to_units: "percent".into(),
            factor: 100.,
            offset: 0.,
            rationale: "Fraction to percentage points".into(),
        })),
    )
    .unwrap();
    assert_eq!(compared["signedChange"], 1.);
    assert_eq!(compared["relativeChange"], 0.5);
    assert_eq!(
        execution(&f.store, &f.ws, &e.id).unwrap().outcome,
        "completed"
    );
}
#[cfg(unix)]
#[test]
fn numeric_bindings_check_rounding_sign_units_and_only_their_dependencies() {
    let f = fixture();
    let (e, r) = exported_result(&f, 0.126, "log points");
    let p = paper(&f, "main.tex", "The effect is 0.13 log points.");
    let revision = p.revision.unwrap();
    let selection = DocumentSelection {
        revision_id: revision.id.clone(),
        revision_hash: revision.content_hash,
        start: Some(0),
        end: Some(30),
        page: None,
        region: None,
        quote: "The effect is 0.13 log points.".into(),
    };
    let mut selection = selection;
    selection.end = Some(selection.quote.len());
    let a = documents::annotate(&f.store, &f.ws, selection, "Headline".into()).unwrap();
    let b = NumericBinding {
        result: r,
        anchor_id: a.id,
        role: "prose".into(),
        component: "estimate".into(),
        printed: "0.13".into(),
        precision: 2,
        reported_units: "log points".into(),
        origin: "manual".into(),
        confirmed: true,
    };
    let saved = bind_number(&f.store, &f.ws, None, 0, b.clone()).unwrap();
    assert_eq!(
        binding_coverage(&f.store, &f.ws).unwrap()[0]["numericPassed"],
        true
    );
    fs::write(f.root.join("unrelated.txt"), "unrelated edit").unwrap();
    assert_eq!(
        binding_coverage(&f.store, &f.ws).unwrap()[0]["state"],
        "current"
    );
    fs::write(f.root.join("input.json"), "changed input").unwrap();
    let c = binding_coverage(&f.store, &f.ws).unwrap();
    assert_eq!(c[0]["state"], "stale");
    assert_eq!(c[0]["dependency"]["changedInputs"][0], "input.json");
    assert_eq!(c[0]["result"]["sourceExecutionId"], e.id);
    let mut b = b;
    b.reported_units = "percent".into();
    bind_number(&f.store, &f.ws, Some(&saved.id), saved.revision, b).unwrap();
    assert!(binding_coverage(&f.store, &f.ws).unwrap()[0]["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r.as_str().unwrap().contains("units")));
}
#[test]
fn macro_names_cannot_inject_tex_and_irf_axes_are_explicit() {
    let valid = ImpulseResponse {
        result_id: "irf".into(),
        variable: "output".into(),
        units: "percent".into(),
        shock_normalization: "one percentage point policy shock".into(),
        horizon_unit: "quarters".into(),
        horizons: vec![0., 1., 2.],
        values: vec![Some(0.), None, Some(0.1)],
        specification_id: "main".into(),
        sample_id: "calibration".into(),
    };
    assert!(validate_series(&valid).is_ok());
    let mut bad = valid.clone();
    bad.horizons = vec![0., 2., 1.];
    assert!(validate_series(&bad).is_err());
    bad = valid;
    bad.values[0] = Some(f64::NAN);
    assert!(validate_series(&bad).is_err());
}
#[test]
fn bibliography_reimport_keeps_keys_versions_and_project_notes() {
    let f = fixture();
    let p=paper(&f,"refs.bib","@article{key,title={Working paper},custom={unknown}}\n@article{key,title={Published version}}");
    let revision = p.revision.unwrap().id;
    let entries = parse_bibtex(&exact_text(&f.store, &f.ws, &revision).unwrap()).unwrap();
    let selected = entries.iter().map(|e| e.id.clone()).collect::<Vec<_>>();
    import_bibliography(&f.store, &f.ws, &revision, &selected).unwrap();
    let before = records(&f.store, &f.ws, "bibliography").unwrap();
    assert_eq!(before.len(), 2);
    let note = LiteratureNote {
        question: "Mechanism".into(),
        statement: "This source may be related".into(),
        citation_key: "key".into(),
        source_version_id: Some(
            before[0].body["source"]["versionId"]
                .as_str()
                .unwrap()
                .into(),
        ),
        start: None,
        end: None,
        quote: "".into(),
        identity_checked: true,
        support: "unsupported".into(),
        method: "manual".into(),
        related_version_ids: vec![],
    };
    save_literature(&f.store, &f.ws, None, 0, note.clone()).unwrap();
    let mut unsupported = note;
    unsupported.support = "supports".into();
    assert!(save_literature(&f.store, &f.ws, None, 0, unsupported).is_err());
    import_bibliography(&f.store, &f.ws, &revision, &selected).unwrap();
    assert_eq!(records(&f.store, &f.ws, "bibliography").unwrap().len(), 2);
    assert_eq!(records(&f.store, &f.ws, "literature").unwrap().len(), 1);
    assert_eq!(research::list_sources(&f.store, &f.ws).unwrap().len(), 2);
}
#[test]
fn source_support_keeps_abstract_access_and_exact_passage() {
    let f = fixture();
    fs::write(
        f.root.join("abstract.txt"),
        "We study a different mechanism.",
    )
    .unwrap();
    let s = research::import_source(
        &f.store,
        research::ImportSourceRequest {
            workspace_id: f.ws.clone(),
            title: "Paper".into(),
            citation_key: Some("key".into()),
            identifiers: json!({}),
            version_label: Some("abstract".into()),
            path: Some(f.root.join("abstract.txt").to_string_lossy().into_owned()),
            locator: None,
            access_state: "abstract".into(),
            acquired_via: "local_file".into(),
            operation_id: id("source").unwrap(),
        },
    )
    .unwrap()
    .source;
    let n = LiteratureNote {
        question: "Mechanism".into(),
        statement: "Source does not support our claimed mechanism".into(),
        citation_key: "key".into(),
        source_version_id: Some(s.version_id),
        start: Some(0),
        end: Some(31),
        quote: "We study a different mechanism.".into(),
        identity_checked: true,
        support: "contradicts".into(),
        method: "manual".into(),
        related_version_ids: vec![],
    };
    let mut n = n;
    n.end = Some(n.quote.len());
    let saved = save_literature(&f.store, &f.ws, None, 0, n.clone()).unwrap();
    assert_eq!(saved.body["checklist"]["access"], "abstract");
    assert_eq!(saved.body["checklist"]["passageRelevance"], "contradicts");
    n.quote = "A fabricated quote".into();
    assert!(save_literature(&f.store, &f.ws, None, 0, n).is_err());
}
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn durable_job_does_not_hold_database_workers_and_cancels_descendants() {
    let _gate = JOB_TEST_GATE.lock().await;
    let f = fixture();
    let p = profile(
        &f,
        "printf 'started\\n'; sleep 5; printf 'finished\\n'",
        vec![],
        vec![],
    );
    let request = research::RunExecutionRequest {
        profile_id: p.id,
        session_id: None,
        test_only: true,
        operation_id: id("job").unwrap(),
    };
    let queued = research::jobs::queue(&f.store, request.clone(), "detached", None).unwrap();
    assert_eq!(queued.outcome, "queued");
    let duplicate = research::jobs::queue(&f.store, request.clone(), "detached", None).unwrap();
    assert_eq!(duplicate.id, queued.id);
    let mut conflict = request;
    conflict.test_only = false;
    assert!(research::jobs::queue(&f.store, conflict, "detached", None).is_err());
    research::jobs::launch_pending();
    for _ in 0..120 {
        if research::jobs::log_page(&f.store, &f.ws, &queued.id, "stdout", 0)
            .unwrap()
            .text
            .contains("started")
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    for _ in 0..6 {
        let store = f.store.clone();
        let ws = f.ws.clone();
        tokio::time::timeout(
            Duration::from_secs(1),
            crate::workbench::commands::run_store_owned(store, move |store| store.workspace(&ws)),
        )
        .await
        .unwrap()
        .unwrap();
    }
    let log = research::jobs::log_page(&f.store, &f.ws, &queued.id, "stdout", 0).unwrap();
    assert!(log.text.contains("started"));
    research::cancel_execution(
        &f.store,
        research::CancelExecutionRequest {
            execution_id: queued.id.clone(),
        },
    )
    .unwrap();
    for _ in 0..100 {
        if !research::jobs::execution_active(&queued.id) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(
        execution(&f.store, &f.ws, &queued.id).unwrap().outcome,
        "interrupted"
    );
    assert!(!research::jobs::execution_active(&queued.id));
    let jobs = research::jobs::list_jobs(&f.store, &f.ws).unwrap();
    assert_eq!(jobs[0].state, "cancelled");
}
#[cfg(unix)]
#[test]
fn abandoned_job_retains_unknown_outcome_and_finalization_is_recoverable() {
    let f = fixture();
    let (e, _) = exported_result(&f, 1., "units");
    f.store.connection().unwrap().execute("INSERT INTO execution_jobs(execution_id,operation_id,request_hash,owner,process_instance,created_at,finalization_json) VALUES(?1,'operation','hash','detached','previous-instance','now',?2)",params![e.id,json!({"outcome":e.outcome,"endedAt":e.ended_at,"exitStatus":e.exit_status,"stdout":e.stdout,"stderr":e.stderr,"outputs":e.output_manifest,"validation":e.validation}).to_string()]).unwrap();
    f.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE research_executions SET outcome='running' WHERE id=?1",
            [&e.id],
        )
        .unwrap();
    let jobs = research::jobs::list_jobs(&f.store, &f.ws).unwrap();
    assert_eq!(jobs[0].state, "unknown");
    assert!(jobs[0].can_reconcile);
    let restored = research::jobs::reconcile_job(&f.store, &f.ws, &e.id).unwrap();
    assert_eq!(restored.outcome, "completed");
    assert_eq!(restored.output_manifest, e.output_manifest);
}
#[cfg(unix)]
#[test]
fn failed_build_cannot_adopt_an_older_pdf_as_success() {
    let f = fixture();
    fs::write(f.root.join("old.pdf"), b"%PDF-1.4 old").unwrap();
    let p = profile(&f, "exit 1", vec![], vec!["old.pdf".into()]);
    let e = research::run_execution(
        &f.store,
        research::RunExecutionRequest {
            profile_id: p.id,
            session_id: None,
            test_only: true,
            operation_id: id("failed").unwrap(),
        },
    )
    .unwrap();
    f.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE research_executions SET adapter='latex' WHERE id=?1",
            [&e.id],
        )
        .unwrap();
    let receipt = inspect_build(&f.store, &f.ws, &e.id).unwrap();
    assert_eq!(receipt["outcome"], "failed");
    assert!(receipt["pdf"].is_null());
    assert!(receipt["mappingArtifactId"].is_null());
}
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn changed_queued_inputs_do_not_launch_and_conflicting_jobs_are_rejected() {
    let _gate = JOB_TEST_GATE.lock().await;
    let f = fixture();
    fs::write(f.root.join("script.sh"), "printf 'launched' > marker").unwrap();
    let p = profile(&f, "sh script.sh", vec!["script.sh".into()], vec![]);
    let q = research::jobs::queue(
        &f.store,
        research::RunExecutionRequest {
            profile_id: p.id.clone(),
            session_id: None,
            test_only: true,
            operation_id: id("job").unwrap(),
        },
        "detached",
        None,
    )
    .unwrap();
    assert!(research::jobs::queue(
        &f.store,
        research::RunExecutionRequest {
            profile_id: p.id,
            session_id: None,
            test_only: true,
            operation_id: id("other").unwrap()
        },
        "detached",
        None
    )
    .is_err());
    fs::write(f.root.join("script.sh"), "printf 'CHANGED' > marker").unwrap();
    research::jobs::launch_pending();
    for _ in 0..100 {
        if !research::jobs::execution_active(&q.id) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(!f.root.join("marker").exists());
    let finished = execution(&f.store, &f.ws, &q.id).unwrap();
    assert_eq!(finished.outcome, "failed");
    assert_eq!(finished.validation["processStarted"], false);
}
#[cfg(target_os = "macos")]
#[test]
#[ignore = "Runs local latexmk and SyncTeX against a disposable multi-file manuscript"]
fn live_studio_build_and_synctex() {
    let f = fixture();
    let main="\\documentclass{article}\n\\begin{document}\n\\input{section}\nA citation: \\cite{example}.\n\\bibliographystyle{plain}\n\\bibliography{references}\n\\appendix\n\\section{Appendix}\nA retained appendix.\n\\end{document}\n";
    fs::write(f.root.join("main.tex"), main).unwrap();
    fs::write(
        f.root.join("section.tex"),
        "\\section{Result}\nThe coefficient is 0.13.\n\\input{table}\n",
    )
    .unwrap();
    fs::write(
        f.root.join("table.tex"),
        "\\begin{tabular}{lr}Estimate & 0.13\\\\\\end{tabular}\n",
    )
    .unwrap();
    fs::write(
        f.root.join("references.bib"),
        "@article{example,title={Example},author={Author, A.},journal={Journal},year={2020}}\n",
    )
    .unwrap();
    let config = BuildConfig {
        name: "Multi-file paper".into(),
        checkpoint_id: None,
        directory: ".".into(),
        root_document: "main.tex".into(),
        expected_pdf: "main.pdf".into(),
        engine: "latexmk".into(),
        timeout_seconds: 60,
        inputs: vec![
            "main.tex".into(),
            "section.tex".into(),
            "table.tex".into(),
            "references.bib".into(),
        ],
        advanced_argv: None,
    };
    let record = save_build(&f.store, &f.ws, None, 0, config).unwrap();
    let profile: research::ExecutionProfile =
        serde_json::from_value(record.body["profile"].clone()).unwrap();
    let preview = research::preview_host_execution(&f.store, &profile.id).unwrap();
    research::authorize_host_execution(&f.store, &profile.id, &preview.fingerprint).unwrap();
    let e = research::run_execution(
        &f.store,
        research::RunExecutionRequest {
            profile_id: profile.id,
            session_id: None,
            test_only: true,
            operation_id: id("build").unwrap(),
        },
    )
    .unwrap();
    assert_eq!(e.outcome, "completed", "{e:?}");
    let receipt = inspect_build(&f.store, &f.ws, &e.id).unwrap();
    assert!(receipt["pdf"]["revision"]["id"].is_string());
    assert!(receipt["mappingArtifactId"].is_string());
    let map = synchronize(
        &f.store,
        SyncRequest {
            workspace_id: f.ws.clone(),
            execution_id: e.id.clone(),
            source_path: Some("section.tex".into()),
            line: Some(2),
            page: None,
            x: None,
            y: None,
        },
    )
    .unwrap();
    assert!(map["result"].as_str().unwrap().contains("Page:1"), "{map}");
    assert_eq!(map["candidates"][0]["page"], 1, "{map}");
    let inspected = inspect_pages(&f.store, &f.ws, &e.id, vec![1], "Fixture page check").unwrap();
    assert_eq!(inspected["inspectedPages"], json!([1]));
    assert!(inspect_pages(&f.store, &f.ws, &e.id, vec![999], "Invalid page").is_err());
    assert_eq!(
        e.input_manifest["snapshots"]["artifacts"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let path = Path::new("/private/tmp/pipeline-pi04-09-qualification");
    fs::create_dir_all(path).unwrap();
    fs::write(
        path.join("latex-synctex.json"),
        serde_json::to_vec_pretty(&json!({"execution":e,"receipt":receipt,"sync":map})).unwrap(),
    )
    .unwrap();
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "Runs the checked-in Python and oldstata exporters through authorized host profiles"]
fn live_studio_result_exporters() {
    let mut receipts = Vec::new();
    for (filename, adapter, argv, outputs) in [
        (
            "export_results.py",
            "command",
            vec!["python3", "export_results.py"],
            vec!["results.json"],
        ),
        (
            "export_results.do",
            "stata",
            vec!["/bin/zsh", "-lic", "oldstata -q -b do export_results.do"],
            vec!["results.json", "export_results.log"],
        ),
    ] {
        let f = fixture();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/workbench/fixtures/research-studio")
                .join(filename),
            f.root.join(filename),
        )
        .unwrap();
        let p = research::save_execution_profile(
            &f.store,
            research::SaveExecutionProfileRequest {
                profile_id: None,
                workspace_id: f.ws.clone(),
                name: filename.into(),
                adapter: adapter.into(),
                argv: argv.into_iter().map(String::from).collect(),
                cwd: f.root.to_string_lossy().into_owned(),
                environment: json!({}),
                inputs: vec![filename.into()],
                outputs: outputs.into_iter().map(String::from).collect(),
                timeout_seconds: 60,
                expected_revision: None,
                operation_id: id("profile").unwrap(),
            },
        )
        .unwrap();
        let preview = research::preview_host_execution(&f.store, &p.id).unwrap();
        research::authorize_host_execution(&f.store, &p.id, &preview.fingerprint).unwrap();
        let e = research::run_execution(
            &f.store,
            research::RunExecutionRequest {
                profile_id: p.id,
                session_id: None,
                test_only: true,
                operation_id: id("export").unwrap(),
            },
        )
        .unwrap();
        assert_eq!(e.outcome, "completed", "{e:?}");
        let artifact = e.output_manifest["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["path"] == "results.json")
            .unwrap()["artifactId"]
            .as_str()
            .unwrap();
        let imported = import_results(&f.store, &f.ws, &e.id, artifact).unwrap();
        let results = research::list_structured_results(&f.store, &f.ws).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_execution_id, e.id);
        assert!(results[0].standard_error.is_some_and(|se| se > 0.));
        receipts.push(json!({"exporter":filename,"execution":e,"import":imported}));
    }
    let path = Path::new("/private/tmp/pipeline-pi04-09-qualification");
    fs::create_dir_all(path).unwrap();
    fs::write(
        path.join("result-exporters.json"),
        serde_json::to_vec_pretty(&receipts).unwrap(),
    )
    .unwrap();
}

fn theory_note(kind: &str, status: &str) -> TheoryNote {
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
    let context = project_context(&f.store, &f.ws).unwrap();
    assert!(context.contains(&format!("Abandoned approach {}", rejected.id)));
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
