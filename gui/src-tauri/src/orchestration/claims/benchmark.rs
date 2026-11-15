//! Fixed scientific examples and answer key. The empirical cases recompute the
//! sample statistic from retained rows. A synthetic terminal receipt represents
//! the approved check, not a claim that a provider conducted this experiment.
use super::*;
use crate::orchestration::{
    adapters,
    store::{Scope, Store},
};
pub(crate) struct Case {
    pub claim: String,
    pub scope: String,
    pub grade: String,
    pub issue: String,
    pub input: Submission,
    pub retained: BTreeMap<String, Value>,
}
pub(crate) fn case(key: &str, store: &Store) -> Case {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/automation-claims/cases.json"
    ))
    .unwrap();
    let key_record = &cases[key];
    let packets: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/automation-claims/packets.json"
    ))
    .unwrap();
    let x = &packets[key_record["packetId"].as_str().unwrap()];
    assert!(x["task"].as_str().is_some());

    let get = |s: &str| x[s].as_str().unwrap_or("").to_owned();
    let (claim, scope, grade, issue) = (
        get("claim"),
        get("scope"),
        key_record["grade"].as_str().unwrap().to_string(),
        key_record["issue"].as_str().unwrap_or("").to_string(),
    );
    let mut retained = BTreeMap::new();
    let capture = |text: String, name: &str| {
        adapters::snapshot(store, &Scope::default(), json!(text), name).unwrap()
    };
    let cite = |role, id: &str, location: &str, observed: &str| Inspection {
        role,
        evidence_id: id.into(),
        location: location.into(),
        observed: observed.into(),
    };
    let mut input = Submission {
        goal_id: None,
        subject: 0,
        claim: claim.clone(),
        scope: scope.clone(),
        kind: Kind::Theory,
        verdict: Verdict::Supported,
        inspections: vec![],
        objections: vec![],
        comparison: None,
    };
    if key == "missingAssumption" {
        retained.insert("r1-file1".into(), capture(get("artifact"), "proof.md"));
        input.inspections.push(cite(
            Role::Derivation,
            "r1-file1",
            "Proposition 1, inverse step",
            &issue,
        ));
        input.objections.push(issue.clone());
    } else if key == "infeasible" {
        input.verdict = Verdict::Infeasible;
        input.objections.push(issue.clone());
    } else {
        // Both designs are centered: sum(x*y)/sum(x*x) is the OLS slope with an intercept.
        let rows: Vec<[f64; 2]> = serde_json::from_value(x["observations"].clone()).unwrap();
        assert_eq!(rows.iter().map(|r| r[0]).sum::<f64>(), 0.0);
        let b = rows.iter().map(|r| r[0] * r[1]).sum::<f64>()
            / rows.iter().map(|r| r[0] * r[0]).sum::<f64>();
        let reported = x["reported"].as_f64().unwrap();
        let file = capture(serde_json::to_string(&rows).unwrap(), "observations.json");
        let code = capture("sum(x*y) / sum(x*x); x is centered".into(), "procedure.txt");
        retained.insert("r1-execution".into(),json!({"kind":"executionReceipt","result":{"passed":true,"receipt":{"id":"fixture-ols","outcome":"completed","inputManifest":{"complete":true,"files":[{"path":"observations.json","contentHash":file["hash"]},{"path":"procedure.txt","contentHash":code["hash"]}]},"outputManifest":{"slope":b}}}}));
        retained.insert("r1-file1".into(), file);
        retained.insert("r1-file2".into(), code);
        retained.insert(
            "r1-file3".into(),
            capture(format!("Table 1. Slope: {reported:.2}"), "table.md"),
        );
        input.kind = Kind::Empirical;
        input.inspections = vec![
            cite(
                Role::Data,
                "r1-file1",
                "all three rows",
                "Checked the frozen sample.",
            ),
            cite(
                Role::Procedure,
                "r1-file2",
                "slope formula",
                "Checked centered OLS with intercept.",
            ),
            cite(
                Role::Output,
                "r1-execution",
                "outputManifest.slope",
                &b.to_string(),
            ),
            cite(
                Role::Table,
                "r1-file3",
                "Table 1, slope",
                &reported.to_string(),
            ),
        ];
        input.comparison = Some(Comparison {
            reported,
            reproduced: b,
            decimal_places: 2,
        });
    }
    Case {
        claim,
        scope,
        grade,
        issue,
        input,
        retained,
    }
}
