use super::*;
use crate::workbench::{
    project, research,
    store::{CreateSessionRequest, CreateWorkspaceRequest},
};
#[cfg(unix)]
mod capsule_unix;
mod context_branch;
#[cfg(unix)]
mod figure_versions;
#[cfg(unix)]
mod grid_recovery;
#[cfg(unix)]
mod grid_ticks;
#[cfg(unix)]
mod irf_series;
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
#[cfg(unix)]
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

mod argument_and_campaign;
mod followup_and_monitor;
mod grid;
mod publication;
mod replication;
#[cfg(unix)]
use grid::grid_request;
#[cfg(unix)]
use replication::capsule_request;
