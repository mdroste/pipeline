//! A results export reaches a figure through the real import. Python's `range`
//! and numpy integer arrays write whole-number horizons, and R's jsonlite writes
//! whole doubles the same way, so a series is stored as `0.0` where its retained
//! export says `0`.
use super::*;

fn imported_series(f: &Fixture, horizons: &str, values: &str) -> ResearchObjectRef {
    let script = format!(
        "import json\nseries={{'resultId':'irf','variable':'Output','units':'percent deviation','shockNormalization':'One percentage-point innovation','horizonUnit':'quarters','horizons':{horizons},'values':{values},'specificationId':'main','sampleId':'model'}}\njson.dump({{'schema':'research-results-v2','series':[series]}},open('results.json','w'))\n"
    );
    let p = profile(f, &script, 30);
    let preview = research::preview_host_execution(&f.store, &p.id).unwrap();
    research::authorize_host_execution(&f.store, &p.id, &preview.fingerprint).unwrap();
    let e = research::run_execution(
        &f.store,
        research::RunExecutionRequest {
            plan_id: None,
            profile_id: p.id,
            session_id: None,
            test_only: true,
            operation_id: "series-run".into(),
        },
    )
    .unwrap();
    assert_eq!(e.outcome, "completed", "{e:?}");
    project::studio_mutate(
        &f.store,
        project::StudioMutation {
            workspace_id: f.ws.clone(),
            operation_id: "series-import".into(),
            action: project::StudioAction::ImportResults {
                execution_id: e.id.clone(),
                artifact_id: e.output_manifest["artifacts"][0]["artifactId"]
                    .as_str()
                    .unwrap()
                    .into(),
            },
        },
    )
    .unwrap();
    let record = project::records(&f.store, &f.ws, "series")
        .unwrap()
        .remove(0);
    ResearchObjectRef {
        kind: "record".into(),
        id: record.id,
        revision: record.revision.to_string(),
        start: None,
        end: None,
    }
}
fn figure(source: ResearchObjectRef) -> assets::AssetSpec {
    assets::AssetSpec {
        kind: "irf".into(),
        title: "Output response".into(),
        entries: vec![assets::Entry {
            label: "Baseline".into(),
            source,
            manual: None,
        }],
        digits: 3,
        uncertainty: "none".into(),
        notes: String::new(),
        sample_comparison_rationale: None,
    }
}

#[test]
fn whole_number_horizons_and_values_make_a_figure() {
    let f = fixture(true);
    let source = imported_series(&f, "list(range(4))", "[1, 2, None, 0.5]");
    let resolved = assets::resolve(&f.store, &f.ws, &figure(source)).unwrap();
    assert_eq!(
        resolved[0]["value"]["series"]["horizons"],
        json!([0.0, 1.0, 2.0, 3.0])
    );
}

#[test]
fn a_changed_stored_number_is_still_refused() {
    let f = fixture(true);
    let source = imported_series(&f, "list(range(4))", "[1, 2, None, 0.5]");
    let c = f.store.connection().unwrap();
    c.execute("UPDATE project_records SET body_json=json_set(body_json,'$.series.values[1]',2.5) WHERE id=?1",[&source.id]).unwrap();
    drop(c);
    let refused = assets::resolve(&f.store, &f.ws, &figure(source)).unwrap_err();
    assert!(refused.message.contains("differ"), "{refused:?}");
}
