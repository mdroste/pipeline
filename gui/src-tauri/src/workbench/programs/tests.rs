use super::*;
use crate::workbench::{
    project, research,
    store::{CreateSessionRequest, CreateWorkspaceRequest},
};
#[cfg(unix)]
mod capsule_unix;
struct Fixture {
    _temp: tempfile::TempDir,
    store: Store,
    ws: String,
    root: std::path::PathBuf,
    session: String,
}
fn fixture(rooted: bool) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let ws = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Research programs".into(),
            root: rooted.then(|| root.to_string_lossy().into_owned()),
            operation_id: "workspace".into(),
        })
        .unwrap()
        .record
        .id;
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: Some(ws.clone()),
            title: "Research conversation".into(),
            operation_id: "session".into(),
        })
        .unwrap()
        .record
        .id;
    Fixture {
        _temp: temp,
        store,
        ws,
        root,
        session,
    }
}
fn python() -> String {
    std::env::var("PIPELINE_PROGRAM_PYTHON").unwrap_or_else(|_| "/usr/bin/python3".into())
}
fn profile(f: &Fixture, script: &str, timeout: u64) -> research::ExecutionProfile {
    fs::write(f.root.join("analysis.py"), script).unwrap();
    research::save_execution_profile(
        &f.store,
        research::SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: f.ws.clone(),
            name: "Fixture calculation".into(),
            adapter: "command".into(),
            argv: vec![python(), "analysis.py".into()],
            cwd: f.root.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs: vec!["analysis.py".into()],
            timeout_seconds: timeout,
            outputs: vec!["results.json".into()],
            expected_revision: None,
            operation_id: "profile".into(),
        },
    )
    .unwrap()
}
fn capture(f: &Fixture, p: &research::ExecutionProfile) -> DeskRecord {
    research::execution_plan::capture(
        &f.store,
        research::execution_plan::CapturePlanRequest {
            research_inputs: vec![],
            profile_id: p.id.clone(),
            parameters: json!({}),
            random_seed: Some("7".into()),
            toolchain_version: "Fixture Python; executable captured".into(),
            operation_id: "capture".into(),
        },
    )
    .unwrap()
}
fn numeric(f: &Fixture, id: &str, value: f64, sample: &str) -> ResearchObjectRef {
    let e = format!("execution_{id}");
    let result = json!({"resultId":id,"estimand":"coefficient","specificationId":id,"sampleId":sample,"estimate":value,"standardError":0.125,"confidenceInterval":[value-0.25,value+0.25],"n":100,"units":"percentage points","transformation":null,"uncertaintyMethod":"fixture interval","sourceExecutionId":e,"artifactLocator":"results.json#/results/0"});
    f.store.connection().unwrap().execute("INSERT INTO research_executions(id,workspace_id,adapter,command_json,cwd,input_manifest_json,dependency_hash,outcome,output_manifest_json,validation_json,snapshot_consistency,created_at) VALUES(?1,?2,'command','[]',?3,'{}','fixture','completed','{}','{}','uncertain','2026-09-07')",params![e,f.ws,f.root.to_string_lossy()]).unwrap();
    f.store
        .connection()
        .unwrap()
        .execute(
            "INSERT INTO structured_results VALUES(?1,?2,?3,?4,'2026-09-07')",
            params![id, e, id, result.to_string()],
        )
        .unwrap();
    ResearchObjectRef {
        kind: "result".into(),
        id: id.into(),
        revision: "2026-09-07".into(),
        start: None,
        end: None,
    }
}
fn spec(source: ResearchObjectRef) -> assets::AssetSpec {
    assets::AssetSpec {
        kind: "table".into(),
        title: "Calibration & results".into(),
        entries: vec![assets::Entry {
            label: "Long coefficient α".into(),
            source,
            manual: None,
        }],
        digits: 3,
        uncertainty: "ci".into(),
        notes: "Exact unrounded values retained in the companion JSON.".into(),
        sample_comparison_rationale: None,
    }
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
fn grid_request(f: &Fixture, plan: &DeskRecord) -> experiments::Prepare {
    experiments::Prepare {
        workspace_id: f.ws.clone(),
        title: "Complete fixture grid".into(),
        question: "Compare all declared modes".into(),
        base_plan_id: plan.id.clone(),
        factors: vec![experiments::Factor {
            name: "mode".into(),
            values: vec![
                json!("success"),
                json!("fail"),
                json!("timeout"),
                json!("excluded"),
            ],
        }],
        exclusions: vec![experiments::Exclusion {
            matches: BTreeMap::from([("mode".into(), json!("excluded"))]),
            reason: "Declared domain exclusion".into(),
        }],
        interpretation: "Retain successes and failures without changing the baseline".into(),
        max_runs: 4,
        max_seconds: 30,
        operation_id: "grid".into(),
    }
}
#[cfg(unix)] // Unix tools or journalled project writes
#[test]
fn grid_budget_is_checked_before_creating_any_derived_plan() {
    let f = fixture(true);
    let p = profile(&f, "print('fixture')", 10);
    let plan = capture(&f, &p);
    let mut request = grid_request(&f, &plan);
    request.max_seconds = 1;
    assert!(experiments::prepare(&f.store, request).is_err());
    assert_eq!(
        desk::records(&f.store, &f.ws, "execution_plan")
            .unwrap()
            .len(),
        1
    );
}
#[cfg(unix)] // Unix tools or journalled project writes
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grid_producer_retains_success_failure_timeout_exclusion_and_does_not_repeat() {
    let f = fixture(true);
    let p=profile(&f,"import json,os,sys,time\nm=json.load(open(os.environ['PIPELINE_PARAMETERS_FILE']))['mode']\nif m=='fail': sys.exit(7)\nif m=='timeout': time.sleep(4)\njson.dump({'estimate':1.25},open('results.json','w'))\n",1);
    let base = capture(&f, &p);
    let grid = experiments::prepare(&f.store, grid_request(&f, &base)).unwrap();
    experiments::control(&f.store, &f.ws, &grid.id, "start", &grid.content_hash).unwrap();
    for _ in 0..400 {
        experiments::tick(&f.store, &f.ws, &grid.id).unwrap();
        research::jobs::launch_pending();
        if experiments::status(&f.store, &f.ws, &grid.id).unwrap()["run"]["state"] == "finished" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let status = experiments::status(&f.store, &f.ws, &grid.id).unwrap();
    assert_eq!(status["run"]["state"], "finished", "{status}");
    let states = status["attempts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["state"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(states, vec!["completed", "failed", "timed_out", "excluded"]);
    for _ in 0..3 {
        experiments::tick(&f.store, &f.ws, &grid.id).unwrap();
    }
    assert_eq!(research::list_executions(&f.store, &f.ws).unwrap().len(), 3);
}
#[test]
fn followup_queue_is_durable_scoped_reorderable_and_does_not_send() {
    let f = fixture(false);
    let a = followups::enqueue(&f.store, &f.session, "First question", None, None, "a").unwrap();
    followups::enqueue(&f.store, &f.session, "Second question", None, None, "b").unwrap();
    let rows = followups::list(&f.store, &f.session).unwrap();
    let second = &rows[1];
    followups::control(
        &f.store,
        &f.session,
        second["id"].as_str().unwrap(),
        1,
        "up",
    )
    .unwrap();
    let rows = followups::list(&f.store, &f.session).unwrap();
    assert_eq!(rows[0]["request"]["text"], "Second question");
    assert!(followups::control(
        &f.store,
        &f.session,
        a[0]["id"].as_str().unwrap(),
        1,
        "cancel"
    )
    .is_err());
    assert!(f
        .store
        .conversation_snapshot(&f.session)
        .unwrap()
        .turns
        .is_empty());
    assert!(
        followups::enqueue(&f.store, &f.session, "Different content", None, None, "a").is_err()
    );
}
#[test]
fn followup_unknown_acknowledgement_cannot_be_dispatched_again() {
    let f = fixture(false);
    let rows = followups::enqueue(&f.store, &f.session, "One request", None, None, "a").unwrap();
    let id = rows[0]["id"].as_str().unwrap();
    let fp = rows[0]["fingerprint"].as_str().unwrap();
    followups::prepare_dispatch(&f.store, &f.session, id, fp).unwrap();
    let rows = followups::reconcile(&f.store, &f.session, id, None).unwrap();
    assert_eq!(rows[0]["state"], "attention");
    assert!(followups::prepare_dispatch(&f.store, &f.session, id, fp).is_err());
}
#[test]
fn local_monitor_deduplicates_backoff_and_coalesces_missed_checks() {
    let f = fixture(true);
    fs::write(f.root.join("paper.md"), "v1").unwrap();
    let m = monitors::Monitor {
        kind: "accepted_file".into(),
        target: "paper.md".into(),
        interval_seconds: 60,
        notify: true,
        network_consent: false,
    };
    let r = monitors::save(&f.store, &f.ws, "Paper changes", m.clone(), "monitor").unwrap();
    let v1 = monitors::local(&f.store, &f.ws, &m).unwrap();
    assert!(!monitors::record_outcome(&f.store, &f.ws, &r.id, 1, Ok(v1.clone()), 1000).unwrap());
    fs::write(f.root.join("paper.md"), "v2").unwrap();
    let v2 = monitors::local(&f.store, &f.ws, &m).unwrap();
    assert!(monitors::record_outcome(&f.store, &f.ws, &r.id, 2, Ok(v2.clone()), 2000).unwrap());
    assert!(!monitors::record_outcome(&f.store, &f.ws, &r.id, 3, Ok(v2), 3000).unwrap());
    assert!(monitors::record_outcome(&f.store, &f.ws, &r.id, 4, Ok(v1), 4000).unwrap()); // returning to the initial baseline is still a distinct changed state
    assert_eq!(
        monitors::list(&f.store, &f.ws).unwrap()["checks"][0]["nextDueAt"],
        4060
    );
    for (revision, t) in [(5, 5000), (6, 6000), (7, 7000)] {
        monitors::record_outcome(
            &f.store,
            &f.ws,
            &r.id,
            revision,
            Err(WorkbenchError::invalid("Network unavailable")),
            t,
        )
        .unwrap();
    }
    let list = monitors::list(&f.store, &f.ws).unwrap();
    assert_eq!(list["checks"][0]["nextDueAt"], 7480);
    assert_eq!(monitors::due(&f.store, 100000).unwrap().len(), 1);
}
#[test]
fn monitor_repeated_transitions_and_failure_episodes_have_distinct_occurrences() {
    let f = fixture(true);
    fs::write(f.root.join("paper.md"), "v1").unwrap();
    let r = monitors::save(
        &f.store,
        &f.ws,
        "Paper changes",
        monitors::Monitor {
            kind: "accepted_file".into(),
            target: "paper.md".into(),
            interval_seconds: 60,
            notify: true,
            network_consent: false,
        },
        "monitor",
    )
    .unwrap();
    for (revision, value, notify) in [
        (1, "A", false),
        (2, "B", true),
        (3, "A", true),
        (4, "B", true),
        (5, "B", false),
    ] {
        assert_eq!(
            monitors::record_outcome(
                &f.store,
                &f.ws,
                &r.id,
                revision,
                Ok(json!(value)),
                revision * 1000
            )
            .unwrap(),
            notify
        );
        // Retrying even with a different completion time must not consume another occurrence.
        assert!(!monitors::record_outcome(
            &f.store,
            &f.ws,
            &r.id,
            revision,
            Ok(json!(value)),
            revision * 1000 + 1
        )
        .unwrap());
        if revision == 2 {
            let attention = monitors::list(&f.store, &f.ws).unwrap();
            monitors::control(
                &f.store,
                &f.ws,
                attention["attention"][0]["id"].as_str().unwrap(),
                0,
                "acknowledge",
            )
            .unwrap();
        }
    }
    for revision in 6..=8 {
        assert_eq!(
            monitors::record_outcome(
                &f.store,
                &f.ws,
                &r.id,
                revision,
                Err(WorkbenchError::invalid("Unavailable")),
                revision * 1000
            )
            .unwrap(),
            revision == 8
        );
        assert!(!monitors::record_outcome(
            &f.store,
            &f.ws,
            &r.id,
            revision,
            Err(WorkbenchError::invalid("Unavailable")),
            revision * 1000 + 1
        )
        .unwrap());
    }
    assert!(!monitors::record_outcome(&f.store, &f.ws, &r.id, 9, Ok(json!("B")), 9000).unwrap());
    for revision in 10..=12 {
        assert_eq!(
            monitors::record_outcome(
                &f.store,
                &f.ws,
                &r.id,
                revision,
                Err(WorkbenchError::invalid("Unavailable")),
                revision * 1000
            )
            .unwrap(),
            revision == 12
        );
    }
    let list = monitors::list(&f.store, &f.ws).unwrap();
    assert_eq!(list["attention"].as_array().unwrap().len(), 5);
    assert_eq!(list["checks"][0]["revision"], 13);
    let (_, captured_revision) = &monitors::due(&f.store, 100000).unwrap()[0];
    assert_eq!(*captured_revision, 13);
    monitors::control(&f.store, &f.ws, &r.id, 13, "pause").unwrap();
    monitors::control(&f.store, &f.ws, &r.id, 14, "resume").unwrap();
    assert!(!monitors::record_outcome(
        &f.store,
        &f.ws,
        &r.id,
        *captured_revision,
        Ok(json!("C")),
        100000
    )
    .unwrap());
    assert_eq!(
        monitors::list(&f.store, &f.ws).unwrap()["checks"][0]["revision"],
        15
    );
}
#[test]
fn remote_monitor_requires_both_explicit_consents() {
    let f = fixture(false);
    let m = monitors::Monitor {
        kind: "metadata_query".into(),
        target: "macroeconomic mechanisms".into(),
        interval_seconds: 3600,
        notify: true,
        network_consent: true,
    };
    assert!(monitors::save(&f.store, &f.ws, "Query", m.clone(), "monitor").is_err());
    crate::workbench::acquisition::set_network(&f.store, &f.ws, true).unwrap();
    assert!(monitors::save(&f.store, &f.ws, "Query", m, "monitor").is_ok());
}
fn capsule_request(f: &Fixture, plan: &DeskRecord, include: bool) -> capsule::Export {
    capsule::Export {
        workspace_id: f.ws.clone(),
        title: "Fixture replication".into(),
        path: f
            ._temp
            .path()
            .join("fixture.pwrc")
            .to_string_lossy()
            .into_owned(),
        selections: vec![capsule::Selection {
            plan_id: plan.id.clone(),
            included_paths: if include {
                vec!["analysis.py".into()]
            } else {
                vec![]
            },
            external_requirements: if include {
                BTreeMap::new()
            } else {
                BTreeMap::from([(
                    "analysis.py".into(),
                    "Obtain the licensed script from its author".into(),
                )])
            },
            expected: vec![capsule::Expected {
                output: "results.json".into(),
                pointer: "/estimate".into(),
                value: 1.25,
                absolute_tolerance: 1e-9,
                relative_tolerance: 1e-8,
            }],
        }],
    }
}
#[test]
fn capsule_archive_rejects_traversal_before_import() {
    use std::io::Write;
    let f = fixture(false);
    let path = f._temp.path().join("bad.pwrc");
    let mut archive = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    archive
        .start_file("../outside", zip::write::SimpleFileOptions::default())
        .unwrap();
    archive.write_all(b"bad").unwrap();
    archive.finish().unwrap();
    assert!(capsule::inspect(path.to_str().unwrap()).is_err());
    assert!(!f._temp.path().join("outside").exists());
}
#[test]
fn assumption_branch_is_open_and_does_not_change_the_original_argument() {
    let f = fixture(false);
    let note = project::TheoryNote {
        kind: "proposition".into(),
        title: "Original proposition".into(),
        statement: "An equilibrium exists.".into(),
        body: "Argument under the stated assumptions.".into(),
        assumptions: vec!["Compact feasible set".into()],
        assumption_ids: vec![],
        anchor_ids: vec![],
        related_ids: vec![],
        unresolved_steps: vec![],
        status: "open".into(),
        rejection_reason: String::new(),
        origin: "manual".into(),
        promotions: vec![],
    };
    let made = project::studio_mutate(
        &f.store,
        project::StudioMutation {
            workspace_id: f.ws.clone(),
            operation_id: "original".into(),
            action: project::StudioAction::SaveTheory {
                id: None,
                expected_revision: 0,
                note,
            },
        },
    )
    .unwrap();
    let id = made["id"].as_str().unwrap();
    let source = ResearchObjectRef {
        kind: "record".into(),
        id: id.into(),
        revision: "1".into(),
        start: None,
        end: None,
    };
    let branch = theory::branch(
        &f.store,
        theory::Branch {
            workspace_id: f.ws.clone(),
            source,
            title: "Remove compactness".into(),
            assumptions: vec!["Closed but unbounded feasible set".into()],
            question: "Does existence survive without compactness?".into(),
            operation_id: "branch".into(),
        },
    )
    .unwrap();
    assert_eq!(branch.body["note"]["body"]["status"], "open");
    assert_eq!(
        project::record(&f.store, &f.ws, id, "theory").unwrap().body["assumptions"],
        json!(["Compact feasible set"])
    );
    assert_eq!(
        project::records(&f.store, &f.ws, "theory").unwrap().len(),
        2
    );
}
#[test]
fn campaigns_keep_two_rounds_and_flag_unsubstantiated_analysis_claims() {
    let f = fixture(true);
    fs::write(f.root.join("report.md"), "Comment one: test robustness.\n").unwrap();
    let paper = research::import_paper(
        &f.store,
        research::ImportPaperRequest {
            workspace_id: f.ws.clone(),
            paper_id: None,
            title: "Round one manuscript".into(),
            role: "manuscript".into(),
            path: f.root.join("report.md").to_string_lossy().into_owned(),
            operation_id: "paper".into(),
        },
    )
    .unwrap();
    let revision = paper.revision.unwrap();
    let responses = project::studio_mutate(
        &f.store,
        project::StudioMutation {
            workspace_id: f.ws.clone(),
            operation_id: "report".into(),
            action: project::StudioAction::ImportReport {
                revision_id: revision.id.clone(),
                comments: vec![project::ReportComment {
                    number: "2".into(),
                    start: 0,
                    end: "Comment one: test robustness.".len(),
                    text: "Comment one: test robustness.".into(),
                }],
            },
        },
    )
    .unwrap();
    let response = responses[0]["id"].as_str().unwrap();
    let d = project::ResponseDecision {
        number: "2".into(),
        category: "missing_robustness".into(),
        severity: "high".into(),
        disposition: "addressed".into(),
        draft: "We added a robustness analysis.".into(),
        reports_analysis_added: true,
        ..Default::default()
    };
    project::studio_mutate(
        &f.store,
        project::StudioMutation {
            workspace_id: f.ws.clone(),
            operation_id: "response".into(),
            action: project::StudioAction::SaveResponse {
                id: response.into(),
                expected_revision: 1,
                response: d,
            },
        },
    )
    .unwrap();
    let mut c = campaigns::Campaign {
        round: 1,
        manuscript: ResearchObjectRef {
            kind: "paper".into(),
            id: revision.id,
            revision: revision.content_hash,
            start: None,
            end: None,
        },
        comments: vec![campaigns::Comment {
            response: ResearchObjectRef {
                kind: "record".into(),
                id: response.into(),
                revision: "2".into(),
                start: None,
                end: None,
            },
            required_outputs: vec![],
            researcher_addressed: true,
            judgment: "Researcher assertion, pending linked evidence".into(),
        }],
        change_summary: "Claimed robustness addition".into(),
        review_scope: "Selected response and unchanged manuscript".into(),
        broad_change: false,
        previous_round: None,
    };
    let first = campaigns::save(&f.store, &f.ws, "First round", c.clone(), None, "first").unwrap();
    let status = campaigns::completion(&f.store, &f.ws, &first.id).unwrap();
    assert_eq!(status["comments"][0]["researcherAddressed"], true);
    assert_eq!(status["comments"][0]["linkChecksPass"], false);
    assert!(status["comments"][0]["flags"].as_array().unwrap().len() >= 3);
    c.round = 2;
    c.previous_round = Some(first.id.clone());
    let second = campaigns::save(&f.store, &f.ws, "Second round", c, None, "second").unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(
        desk::record(&f.store, &f.ws, &first.id).unwrap().body["round"],
        1
    );
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

#[test]
fn followup_history_pages_never_hide_the_active_queue() {
    let f = fixture(false);
    for i in 0..105 {
        let rows = followups::enqueue(
            &f.store,
            &f.session,
            &format!("History {i}"),
            None,
            None,
            &format!("history-{i}"),
        )
        .unwrap();
        let row = &rows[0];
        followups::control(
            &f.store,
            &f.session,
            row["id"].as_str().unwrap(),
            row["revision"].as_i64().unwrap(),
            "cancel",
        )
        .unwrap();
    }
    let rows =
        followups::enqueue(&f.store, &f.session, "Still visible", None, None, "pending").unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 101);
    assert_eq!(rows[0]["request"]["text"], "Still visible");
    assert_eq!(rows[1]["request"]["text"], "History 104");
    let older = followups::list_history(&f.store, &f.session, 100).unwrap();
    assert_eq!(older.as_array().unwrap().len(), 6);
    assert_eq!(older[0]["request"]["text"], "Still visible");
    assert_eq!(older[5]["request"]["text"], "History 0");
    followups::control(
        &f.store,
        &f.session,
        older[0]["id"].as_str().unwrap(),
        older[0]["revision"].as_i64().unwrap(),
        "cancel",
    )
    .unwrap();
    let next =
        followups::enqueue(&f.store, &f.session, "Runnable", None, None, "runnable").unwrap();
    let request = followups::prepare_dispatch(
        &f.store,
        &f.session,
        next[0]["id"].as_str().unwrap(),
        next[0]["fingerprint"].as_str().unwrap(),
    )
    .unwrap()
    .0;
    assert_eq!(request.text, "Runnable");
}

#[test]
fn followup_refresh_reviews_new_turns_and_preserves_the_saved_request() {
    let f = fixture(false);
    let first =
        followups::enqueue(&f.store, &f.session, "First", None, None, "first").unwrap()[0].clone();
    let second = followups::enqueue(
        &f.store,
        &f.session,
        "Keep this wording",
        Some("saved-model".into()),
        Some("high".into()),
        "second",
    )
    .unwrap()[1]
        .clone();
    let id = second["id"].as_str().unwrap();
    let old_fingerprint = second["fingerprint"].as_str().unwrap();
    let (sent, _) = followups::prepare_dispatch(
        &f.store,
        &f.session,
        first["id"].as_str().unwrap(),
        first["fingerprint"].as_str().unwrap(),
    )
    .unwrap();
    let c = f.store.connection().unwrap();
    c.execute("INSERT INTO session_bindings(id,session_id,runtime_namespace,provider_thread_id,incarnation,created_at) VALUES('b',?1,'test','thread',1,'t')", [&f.session]).unwrap();
    c.execute("INSERT INTO turns(id,binding_id,client_submission_id,state,created_at,updated_at,terminal_at) VALUES('first-turn','b',?1,'completed','t','t','t')", [sent.client_submission_id]).unwrap();
    followups::reconcile(&f.store, &f.session, first["id"].as_str().unwrap(), None).unwrap();
    assert!(followups::prepare_dispatch(&f.store, &f.session, id, old_fingerprint).is_err());
    let review = followups::review_context(&f.store, &f.session, id, 1).unwrap();
    assert!(review.conversation_changed);
    assert_eq!(review.request.text, "Keep this wording");
    assert_eq!(review.request.model.as_deref(), Some("saved-model"));
    assert_eq!(review.request.effort.as_deref(), Some("high"));
    assert!(followups::refresh_context(&f.store, &f.session, id, 1, "unreviewed").is_err());
    // A selection change after preview must invalidate the confirmation.
    desk::save_context(&f.store, &f.session, 0, vec![]).unwrap();
    assert!(followups::refresh_context(&f.store, &f.session, id, 1, &review.fingerprint).is_err());
    let review = followups::review_context(&f.store, &f.session, id, 1).unwrap();
    research::save_config(
        &f.store,
        research::SaveWorkspaceConfigRequest {
            workspace_id: Some(f.ws.clone()),
            expected_revision: Some(0),
            body: json!({"contextBudgetBytes":16384,"commandNetwork":true}),
            operation_id: "changed-settings".into(),
        },
    )
    .unwrap();
    assert!(followups::refresh_context(&f.store, &f.session, id, 1, &review.fingerprint).is_err());
    let review = followups::review_context(&f.store, &f.session, id, 1).unwrap();
    assert!(review.settings_changed);
    let updated =
        followups::refresh_context(&f.store, &f.session, id, 1, &review.fingerprint).unwrap();
    let row = updated
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap();
    assert_eq!(row["state"], "queued");
    assert_eq!(row["revision"], 2);
    assert!(followups::prepare_dispatch(&f.store, &f.session, id, old_fingerprint).is_err());
    let (request, _) = followups::prepare_dispatch(
        &f.store,
        &f.session,
        id,
        row["fingerprint"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(request.text, "Keep this wording");
    assert_eq!(request.model.as_deref(), Some("saved-model"));
    assert!(followups::prepare_dispatch(
        &f.store,
        &f.session,
        id,
        row["fingerprint"].as_str().unwrap()
    )
    .is_err());
    assert!(followups::review_context(&f.store, &f.session, id, 2).is_err());
}

#[test]
fn followup_refresh_rejects_changed_roots_and_stale_queue_revisions() {
    let f = fixture(true);
    let rows = followups::enqueue(&f.store, &f.session, "Question", None, None, "q").unwrap();
    let id = rows[0]["id"].as_str().unwrap();
    let review = followups::review_context(&f.store, &f.session, id, 1).unwrap();
    fs::rename(&f.root, f.root.with_file_name("old-project")).unwrap();
    fs::create_dir(&f.root).unwrap();
    assert!(followups::refresh_context(&f.store, &f.session, id, 1, &review.fingerprint).is_err());
    assert!(followups::review_context(&f.store, &f.session, id, 1).is_err());
    followups::control(&f.store, &f.session, id, 1, "cancel").unwrap();
    assert!(followups::refresh_context(&f.store, &f.session, id, 1, &review.fingerprint).is_err());
}
