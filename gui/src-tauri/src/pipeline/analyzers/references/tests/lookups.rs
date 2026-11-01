//! The Crossref path through a local HTTP server, so that lookups, notices,
//! and the report go through the real client.

use super::super::*;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;

/// Serves a JSON body chosen from each request's path and query.
fn fake_crossref(answer: impl Fn(&str) -> (u16, Value) + Send + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut buffer = [0u8; 8192];
            let read = stream.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let target = request.split_whitespace().nth(1).unwrap_or_default();
            let (status, body) = answer(target);
            let body = body.to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    format!("http://{address}")
}

fn record(doi: &str, title: &str, family: &str, year: u16, kind: &str) -> Value {
    json!({
        "DOI": doi,
        "title": [title],
        "author": [{"family": family}],
        "issued": {"date-parts": [[year]]},
        "container-title": ["Journal"],
        "type": kind,
    })
}

fn retraction_of(doi: &str) -> Value {
    json!({"message": {"items": [{
        "DOI": "10.1/notice",
        "update-to": [{"DOI": doi, "type": "retraction", "source": "retraction-watch"}]
    }]}})
}

fn listed(text: &str, doi: Option<&str>) -> Reference {
    Reference {
        index: 1,
        text: text.into(),
        year: bibliography::year(text),
        doi: doi.map(str::to_string),
        origin: "bibliography",
        status: "not_checked",
        ..Default::default()
    }
}

fn options(max_references: usize) -> Options {
    Options {
        network: true,
        check_retractions: true,
        max_references,
    }
}

fn finding_ids(output: &Value) -> Vec<String> {
    output["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect()
}

fn whole_report(output: &Value) -> String {
    output["findings"].to_string()
}

const TITLE: &str = "Wage Effects of the Regional Minimum Standard";
const CITED: &str =
    "Rivera, Ana. 2013. Wage Effects of the Regional Minimum Standard. American Economic Review.";

#[tokio::test]
async fn a_confirmed_match_reports_its_retraction() {
    let api = fake_crossref(|target| {
        if target.contains("filter=updates") {
            (200, retraction_of("10.1/real"))
        } else {
            (
                200,
                json!({"message": {"items": [record("10.1/real", TITLE, "Rivera", 2013, "journal-article")]}}),
            )
        }
    });
    let output = report(
        vec![listed(CITED, None)],
        LocalChecks::default(),
        &options(10),
        &api,
    )
    .await
    .unwrap();
    assert_eq!(output["references"][0]["status"], "retracted");
    assert!(finding_ids(&output).contains(&"reference-1-retracted".to_string()));
    assert_eq!(output["references"][0]["notices"][0]["type"], "retraction");
}

#[tokio::test]
async fn a_retraction_on_a_different_authors_record_never_reaches_the_citation() {
    let api = fake_crossref(|target| {
        if target.contains("filter=updates") {
            (200, retraction_of("10.1/other"))
        } else {
            (
                200,
                json!({"message": {"items": [record("10.1/other", TITLE, "Stranger", 2013, "journal-article")]}}),
            )
        }
    });
    let output = report(
        vec![listed(CITED, None)],
        LocalChecks::default(),
        &options(10),
        &api,
    )
    .await
    .unwrap();
    let reference = &output["references"][0];
    assert_eq!(reference["status"], "metadata_mismatch");
    assert!(reference.get("notices").is_none());
    let ids = finding_ids(&output);
    assert!(!ids.iter().any(|id| id.ends_with("-retracted")), "{ids:?}");
    let text = whole_report(&output);
    assert!(text.contains("could not be confirmed"), "{text}");
    assert!(!text.contains("has been retracted"), "{text}");
}

#[tokio::test]
async fn a_retraction_on_a_partly_matching_record_keeps_the_reference_ambiguous() {
    let partial = "Wage Effects of the Regional Minimum Standard in Towns";
    let api = fake_crossref(move |target| {
        if target.contains("filter=updates") {
            (200, retraction_of("10.1/near"))
        } else {
            (
                200,
                json!({"message": {"items": [record("10.1/near", partial, "Rivera", 2013, "journal-article")]}}),
            )
        }
    });
    let output = report(
        vec![listed(CITED, None)],
        LocalChecks::default(),
        &options(10),
        &api,
    )
    .await
    .unwrap();
    assert_eq!(output["references"][0]["status"], "ambiguous");
    let ids = finding_ids(&output);
    assert_eq!(ids, vec!["reference-1-ambiguous".to_string()]);
    let text = whole_report(&output);
    assert!(text.contains("could not be confirmed"), "{text}");
    assert!(!text.contains("has been retracted"), "{text}");
}

#[tokio::test]
async fn a_record_too_thin_to_identify_the_work_never_retracts_it() {
    for (title, family) in [(TITLE, ""), ("Regional Standard", "Rivera")] {
        let api = fake_crossref(move |target| {
            if target.contains("filter=updates") {
                return (200, retraction_of("10.1/thin"));
            }
            let mut work = record("10.1/thin", title, family, 2013, "journal-article");
            if family.is_empty() {
                work["author"] = json!([]);
            }
            (200, json!({"message": {"items": [work]}}))
        });
        let output = report(
            vec![listed(CITED, None)],
            LocalChecks::default(),
            &options(10),
            &api,
        )
        .await
        .unwrap();
        assert_eq!(output["references"][0]["status"], "ambiguous");
        let text = whole_report(&output);
        assert!(text.contains("could not be confirmed"), "{text}");
        assert!(!text.contains("has been retracted"), "{text}");
    }
}

#[tokio::test]
async fn a_published_version_found_by_search_does_not_retract_the_cited_working_paper() {
    let api = fake_crossref(|target| {
        if target.contains("filter=updates") {
            (200, retraction_of("10.1/journal"))
        } else {
            (
                200,
                json!({"message": {"items": [record("10.1/journal", TITLE, "Rivera", 2016, "journal-article")]}}),
            )
        }
    });
    let cited = CITED.replace("American Economic Review", "NBER Working Paper 1234");
    let output = report(
        vec![listed(&cited, None)],
        LocalChecks::default(),
        &options(10),
        &api,
    )
    .await
    .unwrap();
    assert_eq!(output["references"][0]["status"], "other_version");
    let text = whole_report(&output);
    assert!(text.contains("could not be confirmed"), "{text}");
    assert!(!text.contains("has been retracted"), "{text}");
}

#[tokio::test]
async fn a_cited_doi_with_a_different_year_still_reports_its_retraction() {
    let api = fake_crossref(|target| {
        if target.contains("filter=updates") {
            (200, retraction_of("10.1/real"))
        } else {
            (
                200,
                json!({"message": record("10.1/real", TITLE, "Rivera", 2016, "journal-article")}),
            )
        }
    });
    let output = report(
        vec![listed(CITED, Some("10.1/real"))],
        LocalChecks::default(),
        &options(10),
        &api,
    )
    .await
    .unwrap();
    assert_eq!(output["references"][0]["status"], "retracted");
}

#[tokio::test]
async fn a_journal_article_beats_the_same_titled_working_paper_ranked_below_it() {
    let api = fake_crossref(|target| {
        if target.contains("filter=updates") {
            (200, retraction_of("10.1/journal"))
        } else {
            (
                200,
                json!({"message": {"items": [
                    record("10.1/journal", TITLE, "Rivera", 2013, "journal-article"),
                    record("10.3386/w1", TITLE, "Rivera", 2010, "report"),
                ]}}),
            )
        }
    });
    let output = report(
        vec![listed(CITED, None)],
        LocalChecks::default(),
        &options(10),
        &api,
    )
    .await
    .unwrap();
    assert_eq!(output["references"][0]["matched"]["doi"], "10.1/journal");
    assert_eq!(
        output["references"][0]["status"], "retracted",
        "the retraction lookup follows the selected record"
    );
}

fn candidate(doi: &str, year: u16, kind: &str) -> crossref::Work {
    crossref::Work {
        doi: doi.into(),
        title: TITLE.into(),
        families: vec!["Rivera".into()],
        year: Some(year),
        kind: kind.into(),
        ..Default::default()
    }
}

#[test]
fn candidate_ties_break_on_year_kind_and_then_crossrefs_order() {
    let journal = candidate("10.1/journal", 2013, "journal-article");
    let report_record = candidate("10.3386/w1", 2010, "report");

    let works = [report_record.clone(), journal.clone()];
    let picked = best_candidate(&works, &listed(CITED, None)).unwrap();
    assert_eq!(picked.doi, "10.1/journal", "the cited year decides");

    let undated = CITED.replace("2013. ", "");
    let picked = best_candidate(&works, &listed(&undated, None)).unwrap();
    assert_eq!(picked.doi, "10.1/journal", "published work by default");

    let working = undated.replace("American Economic Review", "NBER Working Paper 1234");
    let picked = best_candidate(&works, &listed(&working, None)).unwrap();
    assert_eq!(
        picked.doi, "10.3386/w1",
        "a cited working paper keeps its kind"
    );

    let same = [
        journal.clone(),
        candidate("10.1/second", 2013, "journal-article"),
    ];
    let picked = best_candidate(&same, &listed(CITED, None)).unwrap();
    assert_eq!(picked.doi, "10.1/journal", "Crossref's ranking stands");
}

fn finding_with_id<'a>(output: &'a Value, id: &str) -> &'a Value {
    output["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == id)
        .unwrap_or_else(|| panic!("no finding {id}: {}", output["findings"]))
}

/// A bibliography with a duplicate, a malformed DOI, and an undefined key.
fn flawed_bibliography(dir: &std::path::Path) -> (Vec<Reference>, LocalChecks) {
    std::fs::write(
        dir.join("refs.bib"),
        "@article{a, author={Card, David}, title={Minimum Wages}, year={1994}, doi={10.1257/aer.84.4.772}}\n@article{b, author={Neumark, David}, title={Comment}, year={2000}, doi={not a doi}}\n@article{c, author={Card, David}, title={Minimum Wages}, year={1994}, doi={10.1257/AER.84.4.772}}",
    )
    .unwrap();
    let tex = dir.join("paper.tex");
    std::fs::write(&tex, "x").unwrap();
    collect(
        None,
        r"\cite{a,b,c} \citep{missing} \bibliography{refs}",
        tex.to_str().unwrap(),
    )
}

fn assert_local_results_and_the_outage_are_reported(output: &Value, retractions: bool) {
    assert_eq!(output["summary"]["matched"], Value::Null);
    let outage = finding_with_id(output, "references-crossref-unavailable");
    assert_eq!(outage["severity"], "medium");
    assert_eq!(output["findings"][0]["id"], outage["id"]);
    let problem = outage["problem"].as_str().unwrap();
    assert!(problem.contains("could not be reached"), "{problem}");
    assert!(
        problem.contains("whether each work can be found"),
        "{problem}"
    );
    assert_eq!(problem.contains("retractions"), retractions, "{problem}");
    assert_eq!(
        finding_with_id(output, "references-undefined-citations")["problem"]
            .as_str()
            .unwrap(),
        "These keys are cited but missing from the bibliography files: missing."
    );
    finding_with_id(output, "references-duplicate-1-3");
    finding_with_id(output, "references-malformed-dois");
    assert!(crate::findings::parse_value(output, "refs", "References").is_some());
}

#[tokio::test]
async fn an_unreachable_crossref_keeps_local_results_and_says_what_did_not_run() {
    // A port with no listener: every connection is refused.
    let closed = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    };
    let dir = tempfile::tempdir().unwrap();
    let (references, checks) = flawed_bibliography(dir.path());
    let output = report(
        references,
        checks,
        &options(10),
        &format!("http://{closed}"),
    )
    .await
    .unwrap();
    assert_local_results_and_the_outage_are_reported(&output, true);
}

#[tokio::test]
async fn crossref_errors_on_every_lookup_count_as_an_outage_without_retraction_checks() {
    let api = fake_crossref(|_| (400, json!({})));
    let dir = tempfile::tempdir().unwrap();
    let (references, checks) = flawed_bibliography(dir.path());
    let without_retractions = Options {
        check_retractions: false,
        ..options(10)
    };
    let output = report(references, checks, &without_retractions, &api)
        .await
        .unwrap();
    assert_local_results_and_the_outage_are_reported(&output, false);
}

#[tokio::test]
async fn an_outage_part_way_through_reports_every_unchecked_reference() {
    let served = std::sync::atomic::AtomicUsize::new(0);
    let api = fake_crossref(move |target| {
        if target.contains("filter=updates") {
            return (200, json!({"message": {"items": []}}));
        }
        match served.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
            0 => (
                200,
                json!({"message": record("10.1/real", TITLE, "Rivera", 2013, "journal-article")}),
            ),
            _ => (400, json!({})),
        }
    });
    let references: Vec<Reference> = (0..6)
        .map(|index| {
            let mut item = listed(CITED, Some("10.1/real"));
            item.index = index + 1;
            item
        })
        .collect();
    let output = report(references, LocalChecks::default(), &options(10), &api)
        .await
        .unwrap();
    assert_eq!(output["summary"]["matched"], 1);
    assert_eq!(output["summary"]["lookup_failed"], 3);
    assert_eq!(output["summary"]["not_checked"], 2);
    let finding = output["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "references-not-checked-1")
        .unwrap();
    assert!(finding["problem"]
        .as_str()
        .unwrap()
        .starts_with("5 of 6 references were not checked"));
}

#[tokio::test]
async fn references_past_the_limit_are_reported() {
    let api = fake_crossref(|target| {
        if target.contains("filter=updates") {
            (200, json!({"message": {"items": []}}))
        } else {
            (
                200,
                json!({"message": record("10.1/real", TITLE, "Rivera", 2013, "journal-article")}),
            )
        }
    });
    let references: Vec<Reference> = (0..3)
        .map(|index| {
            let mut item = listed(CITED, Some("10.1/real"));
            item.index = index + 1;
            item
        })
        .collect();
    let output = report(references, LocalChecks::default(), &options(2), &api)
        .await
        .unwrap();
    assert_eq!(output["summary"]["not_checked"], 1);
    let text = whole_report(&output);
    assert!(
        text.contains("Only the first 2 of 3 references were checked"),
        "{text}"
    );
}

#[tokio::test]
async fn a_failed_retraction_lookup_is_reported() {
    let api = fake_crossref(|target| {
        if target.contains("filter=updates") {
            (400, json!({}))
        } else {
            (
                200,
                json!({"message": record("10.1/real", TITLE, "Rivera", 2013, "journal-article")}),
            )
        }
    });
    let output = report(
        vec![listed(CITED, Some("10.1/real"))],
        LocalChecks::default(),
        &options(10),
        &api,
    )
    .await
    .unwrap();
    assert_eq!(output["references"][0]["status"], "matched");
    let text = whole_report(&output);
    assert!(text.contains("The retraction check failed for 1"), "{text}");
}

#[tokio::test]
async fn offline_runs_report_malformed_dois_in_a_document_bibliography() {
    let text = "Intro.\n\nReferences\n[1] Card, D. 1994. Minimum wages. AER. doi:10.1257.\n[2] Neumark, D. 2000. A comment on wages. AER. https://doi.org/not-a-doi\n[3] Autor, D. 2013. The China syndrome. AER. doi:10.1257/aer.103.6.2121";
    let output = run(None, text, "", &json!({})).await.unwrap();
    assert_eq!(output["local_checks"]["malformed_dois"], json!([1, 2]));
    let finding = output["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "references-malformed-dois")
        .unwrap();
    let problem = finding["problem"].as_str().unwrap();
    assert!(problem.contains("Reference 1, Reference 2"), "{problem}");
    assert!(crate::findings::parse_value(&output, "refs", "References").is_some());
}

#[tokio::test]
async fn offline_runs_report_malformed_dois_in_a_bib_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("refs.bib"),
        "@article{a, author={Card, David}, title={Minimum Wages}, year={1994}, doi={not a doi}}",
    )
    .unwrap();
    let tex = dir.path().join("paper.tex");
    std::fs::write(&tex, "x").unwrap();
    let output = run(
        None,
        r"\cite{a} \bibliography{refs}",
        tex.to_str().unwrap(),
        &json!({}),
    )
    .await
    .unwrap();
    assert!(whole_report(&output).contains("Reference 1 (a)"));
}
