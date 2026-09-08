use super::*;
use crate::workbench::{
    data, project, research, search,
    store::{CreateSessionRequest, CreateWorkspaceRequest, UpdateSessionRequest},
};
use std::fs;
struct Fixture {
    _temp: tempfile::TempDir,
    store: Store,
    root: std::path::PathBuf,
    ws: String,
    session: String,
}
fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let ws = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Desk".into(),
            root: Some(root.to_string_lossy().into_owned()),
            operation_id: "workspace".into(),
        })
        .unwrap()
        .record
        .id;
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: Some(ws.clone()),
            title: "Conversation".into(),
            operation_id: "session".into(),
        })
        .unwrap()
        .record
        .id;
    Fixture {
        _temp: temp,
        store,
        root,
        ws,
        session,
    }
}
fn finish_index(f: &Fixture) {
    for _ in 0..500 {
        if search::advance(&f.store, &f.ws).unwrap().complete {
            return;
        }
    }
    panic!("Index did not finish");
}
fn find(f: &Fixture, q: &str) -> search::SearchPage {
    search::search(
        &f.store,
        search::SearchRequest {
            workspace_id: f.ws.clone(),
            query: q.into(),
            kind: None,
            cursor: None,
            limit: Some(50),
        },
    )
    .unwrap()
}
fn provenance(v: &str) -> data::DataAcquisition {
    data::DataAcquisition {
        provider: "fixture".into(),
        source: "declared local fixture".into(),
        retrieved_at: now(),
        requested_vintage: Some(v.into()),
        returned_vintage: None,
        series_ids: vec![],
        units: Some("index".into()),
        frequency: None,
        transformation: None,
    }
}
#[test]
fn migration_fts_two_hundred_documents_and_exact_unicode_links() {
    let f = fixture();
    let path = f.root.join("note.md");
    let mut first = None;
    for i in 0..200 {
        fs::write(
            &path,
            format!("# Repeated title\nMarkup αβγ equation {i}; unique_number_{i}\n"),
        )
        .unwrap();
        let p = research::import_paper(
            &f.store,
            research::ImportPaperRequest {
                workspace_id: f.ws.clone(),
                paper_id: None,
                title: "Repeated title".into(),
                role: "other".into(),
                path: path.to_string_lossy().into_owned(),
                operation_id: format!("paper-{i}"),
            },
        )
        .unwrap();
        if i == 0 {
            first = Some(p);
        }
    }
    finish_index(&f);
    let page = find(&f, "αβγ");
    assert_eq!(page.hits.len(), 50);
    assert!(page.next_cursor.is_some());
    let link = page.hits[0].object.clone();
    assert!(search::read_object(&f.store, &f.ws, &link, 1000)
        .unwrap()
        .text
        .contains("αβγ"));
    let paper = first.unwrap();
    fs::write(&path, "A revised paper without that equation").unwrap();
    research::import_paper(
        &f.store,
        research::ImportPaperRequest {
            workspace_id: f.ws.clone(),
            paper_id: Some(paper.paper.id),
            title: "Repeated title".into(),
            role: "other".into(),
            path: path.to_string_lossy().into_owned(),
            operation_id: "later".into(),
        },
    )
    .unwrap();
    assert!(!find(&f, "αβγ").index.complete);
    assert!(find(&f, "αβγ").hits.is_empty());
    finish_index(&f);
    assert_eq!(find(&f, "αβγ").hits.len(), 50);
    search::read_object(&f.store, &f.ws, &link, 1000).unwrap();
    search::rebuild(&f.store, &f.ws).unwrap();
    finish_index(&f);
    assert_eq!(find(&f, "αβγ").hits.len(), 50);
}
#[test]
fn context_is_exact_scoped_and_changes_harness_fingerprint() {
    let f = fixture();
    let d = insert(
        &f.store,
        &f.ws,
        "collection",
        "Explicit source",
        json!({"objects":[],"rationale":"Exact choice"}),
        None,
        "collection",
    )
    .unwrap();
    let snapshot = f.store.session_snapshot(&f.session).unwrap().session;
    f.store
        .update_session(UpdateSessionRequest {
            session_id: f.session.clone(),
            expected_revision: snapshot.revision,
            operation_id: "preset".into(),
            title: None,
            draft: None,
            overrides: None,
            archived: None,
            preset_id: Some("research_assistant".into()),
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
    let before = research::resolve_harness(&f.store, &f.session)
        .unwrap()
        .fingerprint;
    let selected = ContextItem {
        role: "main".into(),
        object: d.reference(),
    };
    save_context(&f.store, &f.session, 0, vec![selected.clone()]).unwrap();
    let after = research::resolve_harness(&f.store, &f.session).unwrap();
    assert_ne!(before, after.fingerprint);
    data::save_policy(
        &f.store,
        &f.ws,
        data::DataPolicy {
            assistant_rows: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_ne!(
        after.fingerprint,
        research::resolve_harness(&f.store, &f.session)
            .unwrap()
            .fingerprint
    );
    assert!(after.context_preview.contains(&d.content_hash));
    assert!(save_context(&f.store, &f.session, 0, vec![]).is_err());
    let other = f
        .store
        .create_workspace(CreateWorkspaceRequest {
            name: "Other".into(),
            root: None,
            operation_id: "other".into(),
        })
        .unwrap()
        .record;
    assert!(search::read_object(&f.store, &other.id, &d.reference(), 10).is_err());
    let mut fake = selected;
    fake.object.revision = "latest".into();
    assert!(save_context(&f.store, &f.session, 1, vec![fake]).is_err());
}
#[test]
fn dataset_vintages_samples_and_exclusions_preserve_identity_and_policy() {
    let f = fixture();
    let a = data::import_bytes(
        &f.store,
        &f.ws,
        "Vintage one",
        b"unit,x\na,\nb,0\n",
        "csv",
        provenance("2020-01-01"),
        None,
        "data-a",
    )
    .unwrap();
    let b = data::import_bytes(
        &f.store,
        &f.ws,
        "Vintage two",
        b"unit,x\na,1\nb,0\n",
        "csv",
        provenance("2021-01-01"),
        Some(&a.id),
        "data-b",
    )
    .unwrap();
    assert_ne!(a.content_hash, b.content_hash);
    assert_eq!(a.body["columns"][1]["missing"], 1);
    let sample = data::SampleDefinition {
        datasets: vec![a.reference()],
        inclusion_rules: "All units".into(),
        filters: vec![],
        weights: None,
        date_range: None,
        unit_of_observation: "unit".into(),
        membership_hash: None,
        origin: "declared".into(),
    };
    let old =
        data::save_sample(&f.store, &f.ws, "Sample", sample.clone(), None, "sample-a").unwrap();
    let mut filtered = sample;
    filtered.inclusion_rules = "Exclude unit a".into();
    let new = data::save_sample(
        &f.store,
        &f.ws,
        "Sample",
        filtered,
        Some(&old.id),
        "sample-b",
    )
    .unwrap();
    assert_ne!(old.content_hash, new.content_hash);
    let output = insert(
        &f.store,
        &f.ws,
        "collection",
        "Dependent table",
        json!({"objects":[]}),
        None,
        "table",
    )
    .unwrap();
    let unrelated = insert(
        &f.store,
        &f.ws,
        "collection",
        "Unrelated table",
        json!({"objects":[]}),
        None,
        "unrelated",
    )
    .unwrap();
    project::relations::save_relation(
        &f.store,
        &f.ws,
        project::relations::Relation {
            input: old.reference(),
            dependent: output.reference(),
            origin: "user".into(),
            accepted: true,
            reason: "Table uses the old sample".into(),
        },
        "relation",
    )
    .unwrap();
    let report = project::relations::impact(&f.store, &f.ws).unwrap();
    assert!(report.impacts.iter().any(|i| i.object.id == output.id));
    assert!(!report.impacts.iter().any(|i| i.object.id == unrelated.id));
    let read = search::read_object(&f.store, &f.ws, &a.reference(), 64000).unwrap();
    assert!(!read.text.contains("\"missing\""));
    assert!(!read.text.contains("\"a\""));
    data::save_policy(
        &f.store,
        &f.ws,
        data::DataPolicy {
            dictionary: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(search::read_object(&f.store, &f.ws, &a.reference(), 10).is_err());
    finish_index(&f);
    assert!(find(&f, "Vintage").hits.is_empty());
}
#[test]
fn excluded_note_disappears_and_rejected_history_remains_exact() {
    let f = fixture();
    let n = research::create_note(
        &f.store,
        research::CreateNoteRequest {
            workspace_id: f.ws.clone(),
            paper_id: None,
            kind: "decision".into(),
            body: "Rejected approach because assumptions contradict measurement".into(),
            state: Some("rejected".into()),
            origin: "user".into(),
            pinned: false,
            operation_id: "note".into(),
        },
        false,
    )
    .unwrap();
    finish_index(&f);
    let old = find(&f, "measurement").hits[0].object.clone();
    research::update_note(
        &f.store,
        research::UpdateNoteRequest {
            note_id: n.id.clone(),
            expected_revision: 1,
            body: Some("New rationale".into()),
            state: None,
            pinned: None,
            operation_id: "update".into(),
        },
    )
    .unwrap();
    assert!(search::read_object(&f.store, &f.ws, &old, 1000)
        .unwrap()
        .text
        .contains("measurement"));
    let home = project::home(&f.store, &f.ws).unwrap();
    let mut settings: project::ProjectHomeSettings =
        serde_json::from_value(home.settings.body).unwrap();
    settings.excluded_note_ids.push(n.id);
    project::mutate(
        &f.store,
        project::ProjectMutation {
            workspace_id: f.ws.clone(),
            operation_id: "exclude".into(),
            action: project::ProjectAction::SaveHome {
                expected_revision: home.settings.revision,
                settings,
            },
        },
    )
    .unwrap();
    finish_index(&f);
    assert!(find(&f, "measurement").hits.is_empty());
    assert!(search::read_object(&f.store, &f.ws, &old, 10).is_err());
}
#[cfg(unix)]
#[test]
fn captured_python_plan_ignores_live_edits_and_replays() {
    let f = fixture();
    let script = f.root.join("analysis.py");
    fs::write(
        &script,
        "import json\nopen('result.json','w').write(json.dumps({'mean':sum([1,2,3])/3}))\n",
    )
    .unwrap();
    let p = research::save_execution_profile(
        &f.store,
        research::SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: f.ws.clone(),
            name: "Python capture".into(),
            adapter: "command".into(),
            argv: vec!["/usr/bin/python3".into(), "analysis.py".into()],
            cwd: f.root.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs: vec!["analysis.py".into()],
            timeout_seconds: 30,
            outputs: vec!["result.json".into()],
            expected_revision: None,
            operation_id: "profile".into(),
        },
    )
    .unwrap();
    let plan = research::execution_plan::capture(
        &f.store,
        research::execution_plan::CapturePlanRequest {
            research_inputs: vec![],
            profile_id: p.id.clone(),
            parameters: json!({}),
            random_seed: Some("42".into()),
            toolchain_version: "system Python, captured executable hash".into(),
            operation_id: "plan".into(),
        },
    )
    .unwrap();
    research::execution_plan::authorize(&f.store, &f.ws, &plan.id, &plan.content_hash).unwrap();
    fs::write(
        &script,
        "raise RuntimeError('Live edited script must not run')",
    )
    .unwrap();
    let run = |op: &str, test_only| {
        research::run_execution(
            &f.store,
            research::RunExecutionRequest {
                plan_id: Some(plan.id.clone()),
                profile_id: p.id.clone(),
                session_id: None,
                test_only,
                operation_id: op.into(),
            },
        )
        .unwrap()
    };
    let a = run("test-plan", true);
    assert_eq!(a.outcome, "completed", "{:?}", a);
    let b = run("replay-plan", false);
    assert_eq!(b.outcome, "completed");
    assert_ne!(a.id, b.id);
    assert_ne!(a.cwd, b.cwd);
    assert_eq!(
        a.input_manifest["snapshotConsistency"],
        "captured_inputs_verified"
    );
    let value: Value = serde_json::from_slice(
        &fs::read(std::path::Path::new(&b.cwd).join("result.json")).unwrap(),
    )
    .unwrap();
    assert!((value["mean"].as_f64().unwrap() - 2.).abs() < 1e-12);
    assert!(a.validation["executionContainment"] == "host_access");
}
#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires the separately installed TeX toolchain; run explicitly for release qualification"]
fn captured_real_tex_build() {
    let f = fixture();
    fs::write(f.root.join("paper.tex"),"\\documentclass{article}\n\\begin{document}Captured equation $\\alpha=2$.\\end{document}\n").unwrap();
    let p = research::save_execution_profile(
        &f.store,
        research::SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: f.ws.clone(),
            name: "TeX capture".into(),
            adapter: "latex".into(),
            argv: vec![
                "/Library/TeX/texbin/pdflatex".into(),
                "-no-shell-escape".into(),
                "-interaction=nonstopmode".into(),
                "-halt-on-error".into(),
                "-recorder".into(),
                "paper.tex".into(),
            ],
            cwd: f.root.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs: vec!["paper.tex".into()],
            timeout_seconds: 30,
            outputs: vec!["paper.pdf".into(), "paper.log".into(), "paper.fls".into()],
            expected_revision: None,
            operation_id: "tex-profile".into(),
        },
    )
    .unwrap();
    let plan = research::execution_plan::capture(
        &f.store,
        research::execution_plan::CapturePlanRequest {
            research_inputs: vec![],
            profile_id: p.id.clone(),
            parameters: json!({}),
            random_seed: None,
            toolchain_version: "Installed TeX, executable hash captured".into(),
            operation_id: "tex-plan".into(),
        },
    )
    .unwrap();
    research::execution_plan::authorize(&f.store, &f.ws, &plan.id, &plan.content_hash).unwrap();
    fs::write(
        f.root.join("paper.tex"),
        "This live edit must not be compiled",
    )
    .unwrap();
    for (operation, test_only) in [("tex-test", true), ("tex-replay", false)] {
        let r = research::run_execution(
            &f.store,
            research::RunExecutionRequest {
                plan_id: Some(plan.id.clone()),
                profile_id: p.id.clone(),
                session_id: None,
                test_only,
                operation_id: operation.into(),
            },
        )
        .unwrap();
        assert_eq!(r.outcome, "completed", "{:?}", r);
        let pdf = fs::read(std::path::Path::new(&r.cwd).join("paper.pdf")).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert_eq!(
            crate::workbench::acquisition::validate_pdf_pages(&pdf).unwrap(),
            1
        );
        assert!(
            fs::read_to_string(std::path::Path::new(&r.cwd).join("paper.fls"))
                .unwrap()
                .lines()
                .any(|line| line.starts_with("INPUT ") && line.ends_with("paper.tex"))
        );
    }
}
#[test]
fn acquisition_keeps_metadata_honest_and_individual_imports_distinct() {
    use crate::workbench::acquisition::{self, Candidate};
    let f = fixture();
    let receipt = acquisition::record_lookup(
        &f.store,
        &f.ws,
        "sample query",
        Ok(vec![Candidate {
            title: "Metadata paper".into(),
            doi: Some("10.1000/example".into()),
            authors: vec!["Researcher".into()],
            year: Some(2020),
            url: Some("https://example.org/paper".into()),
            abstract_text: Some("A bounded abstract about markup measurement".into()),
            metadata: json!({}),
        }]),
        "lookup",
    )
    .unwrap();
    let a = acquisition::import_candidate(
        &f.store,
        &f.ws,
        &receipt.id,
        0,
        Some("duplicate-key".into()),
        "first-import",
    )
    .unwrap();
    let retry = acquisition::import_candidate(
        &f.store,
        &f.ws,
        &receipt.id,
        0,
        Some("duplicate-key".into()),
        "first-import",
    )
    .unwrap();
    assert_eq!(a.id, retry.id);
    let b = acquisition::import_candidate(
        &f.store,
        &f.ws,
        &receipt.id,
        0,
        Some("duplicate-key".into()),
        "second-import",
    )
    .unwrap();
    assert_ne!(a.body["sourceVersionId"], b.body["sourceVersionId"]);
    let source =
        research::source_by_id(&f.store, &f.ws, a.body["sourceId"].as_str().unwrap()).unwrap();
    assert_eq!(source.access_state, "abstract");
    let r = ResearchObjectRef {
        kind: "source".into(),
        id: source.version_id.clone(),
        revision: source.content_hash.unwrap(),
        start: None,
        end: None,
    };
    assert!(search::read_object(&f.store, &f.ws, &r, 1000)
        .unwrap()
        .text
        .contains("markup"));
    finish_index(&f);
    assert!(!find(&f, "duplicate-key").hits.is_empty());
    // Identifier metadata remains searchable when the source also has an abstract.
    assert!(!find(&f, "10.1000/example").hits.is_empty());
    acquisition::inbox_state(&f.store, &f.ws, &a.id, "excluded", "exclude-source").unwrap();
    assert!(search::read_object(&f.store, &f.ws, &r, 1000).is_err());
    let metadata = research::import_source(
        &f.store,
        research::ImportSourceRequest {
            workspace_id: f.ws.clone(),
            title: "No bytes".into(),
            citation_key: None,
            identifiers: json!({}),
            version_label: None,
            path: None,
            locator: None,
            access_state: "full".into(),
            acquired_via: "manual".into(),
            operation_id: "honest-metadata".into(),
        },
    )
    .unwrap();
    assert_eq!(metadata.source.access_state, "metadata");
}
#[test]
fn accepted_decision_retry_and_supersession_keep_note_history() {
    let f = fixture();
    let d = project::relations::Decision {
        statement: "Use observed firms".into(),
        rationale: "The sampling frame covers these firms".into(),
        alternatives: vec!["Include imputed firms".into()],
        assumptions: vec![],
        state: "accepted".into(),
        note_id: None,
    };
    let a = project::relations::save_decision(
        &f.store,
        &f.ws,
        "Sample choice",
        d.clone(),
        None,
        "decision-a",
    )
    .unwrap();
    let retry = project::relations::save_decision(
        &f.store,
        &f.ws,
        "Sample choice",
        d.clone(),
        None,
        "decision-a",
    )
    .unwrap();
    assert_eq!(a.id, retry.id);
    let mut changed = d;
    changed.statement = "Include verified imputations".into();
    project::relations::save_decision(
        &f.store,
        &f.ws,
        "Revised sample choice",
        changed,
        Some(&a.id),
        "decision-b",
    )
    .unwrap();
    assert_eq!(
        research::get_note(&f.store, a.body["noteId"].as_str().unwrap())
            .unwrap()
            .state,
        "retired"
    );
    finish_index(&f);
    assert!(!find(&f, "sampling").hits.is_empty());
}
#[test]
fn row_preview_requires_its_own_model_policy() {
    let f = fixture();
    let d = data::import_bytes(
        &f.store,
        &f.ws,
        "Private rows",
        b"x,y\n,0\n2,3\n",
        "csv",
        provenance("2020"),
        None,
        "rows",
    )
    .unwrap();
    assert!(data::preview_rows(&f.store, &f.ws, &d.id, 0, true).is_err());
    save_context(
        &f.store,
        &f.session,
        0,
        vec![ContextItem {
            role: "data_dictionary".into(),
            object: d.reference(),
        }],
    )
    .unwrap();
    data::save_policy(
        &f.store,
        &f.ws,
        data::DataPolicy {
            dictionary: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(context_preview(&f.store, &f.session)
        .unwrap()
        .contains("omitted"));
    let local = data::preview_rows(&f.store, &f.ws, &d.id, 0, false).unwrap();
    assert_eq!(local.rows[0], vec![None, Some("0".into())]);
    data::save_policy(
        &f.store,
        &f.ws,
        data::DataPolicy {
            assistant_rows: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        data::preview_rows(&f.store, &f.ws, &d.id, 0, true)
            .unwrap()
            .rows
            .len(),
        2
    );
}

#[test]
fn lexical_abbreviations_reopen_the_original_phrase() {
    let f = fixture();
    let note = research::create_note(
        &f.store,
        research::CreateNoteRequest {
            workspace_id: f.ws.clone(),
            paper_id: None,
            kind: "decision".into(),
            body: "Plot the impulse response in units of percent.".into(),
            state: Some("accepted".into()),
            origin: "user".into(),
            pinned: false,
            operation_id: "irf-note".into(),
        },
        false,
    )
    .unwrap();
    finish_index(&f);
    let page = find(&f, "IRF");
    assert_eq!(page.hits[0].object.id, note.id);
    assert!(
        search::read_object(&f.store, &f.ws, &page.hits[0].object, 1000)
            .unwrap()
            .text
            .contains("impulse response")
    );
}
