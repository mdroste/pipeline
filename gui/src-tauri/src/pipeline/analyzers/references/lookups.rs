//! Crossref lookups for the reference check: picking the candidate that fits a
//! reference, classifying the match, applying notices only to a record
//! confirmed as the cited work, and recording what went unchecked.

use super::{crossref, LocalChecks, Reference};
use serde_json::{json, Value};

const DEFAULT_MAX_REFERENCES: usize = 150;
const STRONG_TITLE: f64 = 0.85;
const WEAK_TITLE: f64 = 0.6;
const MAX_CONSECUTIVE_FAILURES: usize = 3;

fn work_json(work: &crossref::Work) -> Value {
    json!({
        "doi": work.doi,
        "title": work.title,
        "authors": work.families.iter().take(5).collect::<Vec<_>>(),
        "year": work.year,
        "container": work.container,
        "type": work.kind,
    })
}

/// Classifies a reference against its best Crossref candidate.
pub(super) fn classify(reference: &mut Reference, work: &crossref::Work, by_doi: bool) {
    let coverage = crossref::title_coverage(&work.title, &reference.text);
    let author = crossref::author_present(work, &reference.text);
    let year_gap = match (reference.year, work.year) {
        (Some(cited), Some(found)) => cited.abs_diff(found),
        _ => 0,
    };
    reference.matched = Some(work_json(work));
    let detailed = super::super::fold::tokens(&reference.text).len() >= 6;
    let (status, reason) = if by_doi && detailed && coverage < 0.5 {
        (
            "metadata_mismatch",
            "The DOI belongs to a work with a different title.",
        )
    } else if !by_doi && coverage < WEAK_TITLE {
        (
            "not_found",
            "No Crossref record closely matched this reference.",
        )
    } else if !by_doi && coverage < STRONG_TITLE {
        (
            "ambiguous",
            "The closest Crossref record matched only part of the title.",
        )
    } else if !author {
        (
            "metadata_mismatch",
            "The matching Crossref record lists different authors.",
        )
    } else if year_gap > 1
        || (crossref::looks_unpublished(&reference.text) && work.kind == "journal-article")
    {
        ("other_version", "Crossref lists this work with a different year or as a published article; the cited version may be a working paper or an earlier draft.")
    } else {
        ("matched", "")
    };
    if status == "not_found" {
        reference.matched = None;
    }
    reference.status = status;
    reference.reason = reason.to_string();
}

pub struct Options {
    pub network: bool,
    pub check_retractions: bool,
    pub max_references: usize,
}

impl Options {
    pub fn from_value(options: &Value) -> Self {
        Self {
            network: options
                .get("network")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            check_retractions: options
                .get("check_retractions")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            max_references: options
                .get("max_references")
                .and_then(Value::as_u64)
                .map(|count| count as usize)
                .unwrap_or(DEFAULT_MAX_REFERENCES),
        }
    }
}

pub(super) fn notice_flag(notices: &[crossref::Notice]) -> Option<&'static str> {
    let has = |kinds: &[&str]| {
        notices
            .iter()
            .any(|notice| kinds.contains(&notice.kind.as_str()))
    };
    if has(&["retraction", "withdrawal", "removal"]) {
        Some("retracted")
    } else if has(&["expression_of_concern"]) {
        Some("expression_of_concern")
    } else if has(&[
        "correction",
        "erratum",
        "corrigendum",
        "addendum",
        "clarification",
    ]) {
        Some("corrected")
    } else {
        None
    }
}

/// The closest of Crossref's candidates: title coverage first, then the named
/// author, the nearest year, and the kind of record the reference describes.
/// Crossref's own order settles anything still equal.
pub(super) fn best_candidate<'a>(
    works: &'a [crossref::Work],
    reference: &Reference,
) -> Option<&'a crossref::Work> {
    let unpublished = crossref::looks_unpublished(&reference.text);
    let rank = |work: &crossref::Work| {
        let year_gap = match (reference.year, work.year) {
            (None, _) => 0,
            (Some(cited), Some(found)) => cited.abs_diff(found),
            (Some(_), None) => u16::MAX,
        };
        (
            crossref::author_present(work, &reference.text),
            std::cmp::Reverse(year_gap),
            (work.kind == "journal-article") != unpublished,
        )
    };
    works.iter().rev().max_by(|left, right| {
        crossref::title_coverage(&left.title, &reference.text)
            .total_cmp(&crossref::title_coverage(&right.title, &reference.text))
            .then_with(|| rank(left).cmp(&rank(right)))
    })
}

/// Whether a classified record is the cited work itself, so that a notice on
/// the record is a notice on the citation. A search result must name authors
/// and have a title long enough to identify a work; another version found by
/// search may not share its notices.
fn confirmed_work(reference: &Reference, work: &crossref::Work, by_doi: bool) -> bool {
    match reference.status {
        "matched" => {
            by_doi || (!work.families.is_empty() && crossref::title_words(&work.title) >= 3)
        }
        "other_version" => by_doi,
        _ => false,
    }
}

/// What to say about a notice on a record that may not be the cited work. It
/// names the record, never the citation.
fn unconfirmed_notice(flag: &str) -> &'static str {
    match flag {
        "retracted" => "Crossref has a retraction or withdrawal notice for this record, but the record could not be confirmed as the cited work, so whether the cited work was retracted could not be confirmed.",
        "expression_of_concern" => "Crossref has an expression of concern for this record, but the record could not be confirmed as the cited work, so whether it applies to the cited work could not be confirmed.",
        _ => "Crossref has a correction for this record, but the record could not be confirmed as the cited work, so whether it applies to the cited work could not be confirmed.",
    }
}

/// Resolves references against Crossref. Stops after repeated failures and
/// marks the rest `not_checked`.
pub(super) async fn resolve(
    client: &mut crossref::Client,
    references: &mut [Reference],
    options: &Options,
) -> Result<(), String> {
    let mut failures = 0usize;
    for reference in references.iter_mut().take(options.max_references) {
        if failures >= MAX_CONSECUTIVE_FAILURES {
            reference.reason = "Stopped after repeated Crossref failures.".into();
            continue;
        }
        let lookup = match reference.doi.clone() {
            Some(doi) => client
                .work(&doi)
                .await
                .map(|work| (work.into_iter().collect::<Vec<_>>(), true)),
            None => client
                .search(&reference.text)
                .await
                .map(|works| (works, false)),
        };
        let (works, by_doi) = match lookup {
            Ok(found) => {
                failures = 0;
                found
            }
            Err(crossref::LookupError::Cancelled) => return Err("Pipeline cancelled".into()),
            Err(crossref::LookupError::Failed(error)) => {
                failures += 1;
                reference.status = "lookup_failed";
                reference.reason = error;
                continue;
            }
        };
        let mut confirmed = false;
        match best_candidate(&works, reference) {
            Some(work) => {
                classify(reference, work, by_doi);
                confirmed = confirmed_work(reference, work, by_doi);
            }
            None if by_doi => {
                reference.status = "not_found";
                reference.reason = "This DOI is not registered with Crossref. It may belong to another registration agency such as DataCite; check it manually.".into();
            }
            None => {
                reference.status = "not_found";
                reference.reason = "No Crossref record matched this reference.".into();
            }
        }
        let matched_doi = reference
            .matched
            .as_ref()
            .and_then(|work| work.get("doi"))
            .and_then(Value::as_str)
            .filter(|doi| !doi.is_empty())
            .map(str::to_string);
        if let (true, Some(doi)) = (options.check_retractions, matched_doi) {
            match client.notices(&doi).await {
                Ok(notices) if confirmed => {
                    reference.notices = notices
                        .iter()
                        .map(|notice| json!({"type": notice.kind, "doi": notice.notice_doi, "source": notice.source}))
                        .collect();
                    if let Some(flag) = notice_flag(&notices) {
                        reference.status = flag;
                    }
                }
                Ok(notices) => {
                    if let Some(flag) = notice_flag(&notices) {
                        reference.reason =
                            format!("{} {}", reference.reason, unconfirmed_notice(flag))
                                .trim()
                                .to_string();
                        // A clean status would keep this out of the report.
                        if reference.status == "matched" {
                            reference.status = "ambiguous";
                        }
                    }
                }
                Err(crossref::LookupError::Cancelled) => return Err("Pipeline cancelled".into()),
                Err(crossref::LookupError::Failed(error)) => {
                    reference.notices_unchecked = true;
                    reference.reason =
                        format!("{} Retraction check failed: {error}", reference.reason)
                            .trim()
                            .to_string();
                }
            }
        }
    }
    Ok(())
}

/// Says what the lookups left unchecked. When none succeeded the local results
/// still stand, and a finding says which online checks did not run, because
/// without one the report would read as though the references were fine.
pub(super) fn record_lookup_gaps(
    references: &[Reference],
    options: &Options,
    checks: &mut LocalChecks,
) {
    let total = references.len();
    let failed: Vec<&Reference> = references
        .iter()
        .filter(|reference| reference.status == "lookup_failed")
        .collect();
    let not_checked = |stopped: bool| {
        references
            .iter()
            .filter(|reference| {
                reference.status == "not_checked" && reference.reason.is_empty() != stopped
            })
            .count()
    };
    let stopped = not_checked(true);
    let over_limit = not_checked(false);
    if let Some(first) = failed.first() {
        if failed.len() + stopped + over_limit == total {
            let notices = if options.check_retractions {
                ", and retractions, expressions of concern, and corrections"
            } else {
                ""
            };
            checks.crossref_outage = Some(format!(
                "Crossref could not be reached or did not answer, so none of the {total} references was checked against it. These online checks did not run: whether each work can be found, whether its details match, other versions{notices}. The checks that need no network are still in this report. Last error: {}",
                first.reason
            ));
            return;
        }
        checks.lookup_limitations.push(format!(
            "{} of {total} references were not checked against Crossref, because lookups failed: {}",
            failed.len() + stopped,
            first.reason
        ));
    }
    if over_limit > 0 {
        checks.lookup_limitations.push(format!(
            "Only the first {} of {total} references were checked against Crossref. The other {over_limit} were not.",
            options.max_references
        ));
    }
    let unchecked = references
        .iter()
        .filter(|reference| reference.notices_unchecked)
        .count();
    if unchecked > 0 {
        checks.lookup_limitations.push(format!(
            "The retraction check failed for {unchecked} references, so a retraction or correction of these works would not appear in this report."
        ));
    }
}
