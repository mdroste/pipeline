//! Capsule and environment tests that run a real Python interpreter.
use super::*;

#[test]
fn replication_capsule_two_stores_rebinds_inert_inputs_and_checks_real_output() {
    let f = fixture(true);
    use std::os::unix::fs::PermissionsExt;
    let p = profile(
        &f,
        "import subprocess\nsubprocess.run(['./helper.py'],check=True)\n",
        30,
    );
    fs::write(
        f.root.join("helper.py"),
        format!(
            "#!{}\nimport json\njson.dump({{'estimate':1.25}},open('results.json','w'))\n",
            python()
        ),
    )
    .unwrap();
    fs::set_permissions(f.root.join("helper.py"), fs::Permissions::from_mode(0o700)).unwrap();
    let p = research::save_execution_profile(
        &f.store,
        research::SaveExecutionProfileRequest {
            profile_id: Some(p.id),
            workspace_id: f.ws.clone(),
            name: p.name,
            adapter: p.adapter,
            argv: p.argv,
            cwd: p.cwd,
            environment: p.environment,
            inputs: vec!["analysis.py".into(), "helper.py".into()],
            outputs: p.outputs,
            timeout_seconds: 30,
            expected_revision: Some(p.revision),
            operation_id: "helper-profile".into(),
        },
    )
    .unwrap();
    let plan = capture(&f, &p);
    let mut r = capsule_request(&f, &plan, true);
    r.selections[0].included_paths.push("helper.py".into());
    let preview = capsule::preview(&f.store, &r).unwrap();
    let path = r.path.clone();
    capsule::export(&f.store, r, preview["fingerprint"].as_str().unwrap()).unwrap();
    let recipient = fixture(true);
    let local = profile(&recipient, "# Local toolchain only", 30);
    let inspection = capsule::inspect(&path).unwrap();
    let imported = capsule::import(
        &recipient.store,
        &recipient.ws,
        &path,
        inspection["fingerprint"].as_str().unwrap(),
        "import",
    )
    .unwrap();
    assert!(research::list_executions(&recipient.store, &recipient.ws)
        .unwrap()
        .is_empty());
    let bind_request = capsule::Bind {
        workspace_id: recipient.ws.clone(),
        capsule_id: imported.id,
        ordinal: 0,
        local_profile_id: local.id,
        external_inputs: BTreeMap::new(),
        operation_id: "bind".into(),
    };
    let binding = capsule::bind(&recipient.store, bind_request.clone()).unwrap();
    let again = capsule::bind(&recipient.store, bind_request).unwrap();
    assert_eq!(binding["binding"]["id"], again["binding"]["id"]);
    assert_eq!(
        research::list_execution_profiles(&recipient.store, &recipient.ws)
            .unwrap()
            .len(),
        2
    );

    let planid = binding["plan"]["id"].as_str().unwrap();
    let status = research::execution_plan::status(&recipient.store, &recipient.ws, planid).unwrap();
    assert!(!status.authorized);
    assert!(status.record.body["capturedFiles"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["path"] == "helper.py" && f["executable"] == true));
    research::execution_plan::authorize(
        &recipient.store,
        &recipient.ws,
        planid,
        &status.record.content_hash,
    )
    .unwrap();
    let profile_id = status.record.body["profile"]["id"].as_str().unwrap();
    let e = research::run_execution(
        &recipient.store,
        research::RunExecutionRequest {
            plan_id: Some(planid.into()),
            profile_id: profile_id.into(),
            session_id: None,
            test_only: true,
            operation_id: "verify".into(),
        },
    )
    .unwrap();
    assert_eq!(e.outcome, "completed", "{:?}", e.stderr);
    let report = capsule::verify(
        &recipient.store,
        &recipient.ws,
        binding["binding"]["id"].as_str().unwrap(),
        &e.id,
    )
    .unwrap();
    assert_eq!(report["status"], "checked");
    assert_eq!(report["numericChecks"][0]["actual"], 1.25);
    assert_eq!(report["scientificValidity"], "Not assessed");
    assert!(!fs::read_to_string(recipient.root.join("analysis.py"))
        .unwrap()
        .contains("json.dump"));
}
#[test]
fn missing_capsule_input_is_incomplete_and_creates_no_profile() {
    let f = fixture(true);
    let p = profile(&f, "print('external')", 10);
    let plan = capture(&f, &p);
    let r = capsule_request(&f, &plan, false);
    let preview = capsule::preview(&f.store, &r).unwrap();
    let path = r.path.clone();
    capsule::export(&f.store, r, preview["fingerprint"].as_str().unwrap()).unwrap();
    let recipient = fixture(true);
    let local = profile(&recipient, "# Local toolchain", 10);
    let imported = capsule::import(
        &recipient.store,
        &recipient.ws,
        &path,
        preview["fingerprint"].as_str().unwrap(),
        "import",
    )
    .unwrap();
    let result = capsule::bind(
        &recipient.store,
        capsule::Bind {
            workspace_id: recipient.ws.clone(),
            capsule_id: imported.id,
            ordinal: 0,
            local_profile_id: local.id,
            external_inputs: BTreeMap::new(),
            operation_id: "bind".into(),
        },
    )
    .unwrap();
    assert_eq!(result["status"], "incomplete");
    assert_eq!(result["missingInputs"], json!(["analysis.py"]));
    assert_eq!(result["uncheckedOutputs"], json!(["results.json"]));
    assert_eq!(
        research::list_execution_profiles(&recipient.store, &recipient.ws)
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn capsule_reports_environment_mismatch_before_creating_execution_material() {
    let f = fixture(true);
    let p = profile(&f, "print('environment')", 10);
    let plan = capture(&f, &p);
    let mut body = plan.body;
    body["profile"]["environment"]["OMP_NUM_THREADS"] = json!("2");
    body["profile"]["environment"]["MODEL_DATA_ROOT"] = json!("/exporter/private/data");
    let plan = desk::insert(
        &f.store,
        &f.ws,
        "execution_plan",
        "Environment requirement",
        body,
        None,
        "environment-plan",
    )
    .unwrap();
    let r = capsule_request(&f, &plan, true);
    let path = r.path.clone();
    let preview = capsule::preview(&f.store, &r).unwrap();
    assert_eq!(
        preview["manifest"]["plans"][0]["environment"]["variables"]["OMP_NUM_THREADS"],
        "2"
    );
    assert!(!preview["manifest"]
        .to_string()
        .contains("/exporter/private"));
    capsule::export(&f.store, r, preview["fingerprint"].as_str().unwrap()).unwrap();
    let imported = capsule::import(
        &f.store,
        &f.ws,
        &path,
        preview["fingerprint"].as_str().unwrap(),
        "inert",
    )
    .unwrap();
    let result = capsule::bind(
        &f.store,
        capsule::Bind {
            workspace_id: f.ws.clone(),
            capsule_id: imported.id,
            ordinal: 0,
            local_profile_id: p.id,
            external_inputs: BTreeMap::new(),
            operation_id: "bind-env".into(),
        },
    )
    .unwrap();
    assert_eq!(result["status"], "incomplete");
    assert_eq!(result["environmentMismatches"].as_array().unwrap().len(), 2);
    assert_eq!(
        research::list_execution_profiles(&f.store, &f.ws)
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn changed_python_environment_requires_recapture() {
    use std::os::unix::fs::symlink;
    let f = fixture(true);
    let mut p = profile(&f, "print('test')", 10);
    let env = f._temp.path().join("env");
    fs::create_dir_all(env.join("bin")).unwrap();
    symlink(python(), env.join("bin/python3")).unwrap();
    fs::write(env.join("pyvenv.cfg"), "home = /fixture/one\n").unwrap();
    p.argv[0] = env.join("bin/python3").to_string_lossy().into_owned();
    f.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE execution_profiles SET argv_json=?1 WHERE id=?2",
            params![json!(p.argv).to_string(), p.id],
        )
        .unwrap();
    let plan = capture(&f, &p);
    assert_eq!(plan.body["launch"]["invocationPath"], p.argv[0]);
    fs::write(env.join("pyvenv.cfg"), "home = /fixture/two\n").unwrap();
    assert!(
        research::execution_plan::authorize(&f.store, &f.ws, &plan.id, &plan.content_hash).is_err()
    );
}

#[test]
fn captured_plan_recovery_certifies_plan_and_not_live_profile() {
    let f = fixture(true);
    let p = profile(
        &f,
        "import json\njson.dump({'estimate':1.25},open('results.json','w'))\n",
        30,
    );
    let plan = capture(&f, &p);
    research::execution_plan::authorize(&f.store, &f.ws, &plan.id, &plan.content_hash).unwrap();
    let e = research::run_execution(
        &f.store,
        research::RunExecutionRequest {
            plan_id: Some(plan.id.clone()),
            profile_id: p.id.clone(),
            session_id: None,
            test_only: true,
            operation_id: "recover-test".into(),
        },
    )
    .unwrap();
    assert_eq!(e.outcome, "completed");
    let journal = json!({"outcome":e.outcome,"endedAt":e.ended_at,"exitStatus":e.exit_status,"stdout":e.stdout,"stderr":e.stderr,"outputs":e.output_manifest,"validation":e.validation,"testOnly":true,"profileId":p.id,"profileRevision":p.revision});
    let c = f.store.connection().unwrap();
    c.execute("INSERT INTO execution_jobs(execution_id,operation_id,request_hash,owner,process_instance,created_at,finalization_json) VALUES(?1,'recover-test','fixture','detached','past','now',?2)",params![e.id,journal.to_string()]).unwrap();
    c.execute(
        "UPDATE research_executions SET outcome='outcome_unknown' WHERE id=?1",
        [&e.id],
    )
    .unwrap();
    c.execute(
        "UPDATE execution_plan_state SET test_status=NULL WHERE plan_id=?1",
        [&plan.id],
    )
    .unwrap();
    research::jobs::reconcile_job(&f.store, &f.ws, &e.id).unwrap();
    assert_eq!(
        research::execution_plan::status(&f.store, &f.ws, &plan.id)
            .unwrap()
            .test_status
            .as_deref(),
        Some("passed")
    );
    let live = research::list_execution_profiles(&f.store, &f.ws).unwrap();
    assert_eq!(
        live.iter().find(|x| x.id == p.id).unwrap().test_status,
        None
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_family_cancel_never_launches_a_later_attempt() {
    let f = fixture(true);
    let p = profile(&f, "raise RuntimeError('must not launch')", 10);
    let base = capture(&f, &p);
    let grid = experiments::prepare(&f.store, grid_request(&f, &base)).unwrap();
    experiments::control(&f.store, &f.ws, &grid.id, "start", &grid.content_hash).unwrap();
    let worker = {
        let store = f.store.clone();
        let ws = f.ws.clone();
        let id = grid.id.clone();
        std::thread::spawn(move || experiments::tick(&store, &ws, &id))
    };
    experiments::control(&f.store, &f.ws, &grid.id, "cancel", &grid.content_hash).unwrap();
    worker.join().unwrap().unwrap();
    research::jobs::launch_pending();
    for _ in 0..100 {
        if !research::jobs::workspace_active(&f.ws) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    experiments::tick(&f.store, &f.ws, &grid.id).unwrap();
    assert_eq!(
        experiments::status(&f.store, &f.ws, &grid.id).unwrap()["run"]["state"],
        "cancelled"
    );
    for e in research::list_executions(&f.store, &f.ws).unwrap() {
        assert_eq!(e.validation["processStarted"], false);
    }
}

#[test]
fn older_capsule_inputs_default_to_nonexecutable() {
    let input: capsule::Input = serde_json::from_value(
        json!({"path":"analysis.py","hash":"fixture","included":true,"externalRequirement":null}),
    )
    .unwrap();
    assert!(!input.executable);
}
