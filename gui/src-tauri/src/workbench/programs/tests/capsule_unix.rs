//! Capsule and environment tests that run a real Python interpreter.
use super::*;

#[test]
fn replication_capsule_two_stores_rebinds_inert_inputs_and_checks_real_output() {
    let f = fixture(true);
    let p = profile(
        &f,
        "import json\njson.dump({'estimate':1.25},open('results.json','w'))\n",
        30,
    );
    let plan = capture(&f, &p);
    let r = capsule_request(&f, &plan, true);
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
