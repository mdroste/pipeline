#![cfg(unix)]

use super::*;

#[cfg(unix)]
pub(super) fn grid_request(f: &Fixture, plan: &DeskRecord) -> experiments::Prepare {
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
    let listed = research::list_executions(&f.store, &f.ws).unwrap();
    assert_eq!(listed.len(), 3);
    // Lists leave output out, and only the run's own project can read it.
    assert!(listed
        .iter()
        .all(|e| e.stdout.is_none() && e.stderr.is_none()));
    let saved = research::get_execution(&f.store, &listed[0].id).unwrap();
    let output = research::jobs::execution_output(&f.store, &f.ws, &listed[0].id).unwrap();
    assert_eq!((output.stdout, output.stderr), (saved.stdout, saved.stderr));
    assert!(research::jobs::execution_output(&f.store, "other", &listed[0].id).is_err());
}
