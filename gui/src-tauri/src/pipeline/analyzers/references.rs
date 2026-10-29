//! `reference_audit` v1: find a manuscript's references and, when the step
//! enables network lookups, check each against Crossref.
//!
//! Without the network the check is local: undefined LaTeX citations,
//! duplicate entries, and malformed DOIs. With it, each reference is
//! `matched`, `other_version` (for example a working paper later published),
//! `metadata_mismatch`, `ambiguous`, `not_found`, or `lookup_failed`, and
//! matched DOIs are checked for retractions, withdrawals, expressions of
//! concern, and corrections, which Crossref records with Retraction Watch.
//! `not_found` means not found in Crossref, never that a work is fabricated.

mod bibliography;
mod bibtex;
mod crossref;

use crate::document_bundle::DocumentBundle;
use bibliography::Block;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const DEFAULT_MAX_REFERENCES: usize = 150;
const MAX_BIB_FILES: usize = 20;
const MAX_BIB_BYTES: u64 = 4 * 1024 * 1024;
const MAX_REFERENCE_FINDINGS: usize = 60;
const STRONG_TITLE: f64 = 0.85;
const WEAK_TITLE: f64 = 0.6;
const MAX_CONSECUTIVE_FAILURES: usize = 3;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Reference {
    pub index: usize,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub key: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub node_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source_file: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arxiv: Option<String>,
    /// `bibtex` or `bibliography`.
    pub origin: &'static str,
    pub status: &'static str,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched: Option<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<Value>,
}

#[derive(Debug, Default, Serialize)]
pub struct LocalChecks {
    pub undefined_citations: Vec<String>,
    pub uncited_entries: usize,
    pub duplicates: Vec<(usize, usize)>,
    pub malformed_dois: Vec<usize>,
}

fn bounded(text: &str, max: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(max).collect()
}

/// `.bib` files named by `\bibliography`/`\addbibresource`, or every `.bib`
/// within three levels of the source folder when none is named.
fn bib_files(source_path: &str, latex: &str) -> Vec<(String, String)> {
    let source = Path::new(source_path);
    let root = if source.is_dir() {
        source.to_path_buf()
    } else {
        source.parent().map(Path::to_path_buf).unwrap_or_default()
    };
    if root.as_os_str().is_empty() {
        return Vec::new();
    }
    let named = regex::Regex::new(r"\\(?:bibliography|addbibresource)\{([^}]*)\}")
        .expect("valid")
        .captures_iter(latex)
        .flat_map(|captures| {
            captures[1]
                .split(',')
                .map(|name| name.trim().to_string())
                .collect::<Vec<_>>()
        })
        .filter(|name| !name.is_empty() && !name.contains(".."))
        .map(|name| {
            if name.ends_with(".bib") {
                name
            } else {
                format!("{name}.bib")
            }
        })
        .collect::<Vec<_>>();
    let mut candidates: Vec<PathBuf> = named.iter().map(|name| root.join(name)).collect();
    if candidates.is_empty() {
        let mut stack = vec![(root.clone(), 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut entries: Vec<_> = read.flatten().collect();
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let name = entry.file_name().to_string_lossy().to_string();
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if name.starts_with('.') || kind.is_symlink() {
                    continue;
                }
                if kind.is_dir() && depth < 3 {
                    stack.push((entry.path(), depth + 1));
                } else if kind.is_file() && name.to_ascii_lowercase().ends_with(".bib") {
                    candidates.push(entry.path());
                }
            }
        }
        candidates.sort();
    }
    candidates
        .into_iter()
        .filter_map(|path| {
            let metadata = std::fs::symlink_metadata(&path).ok()?;
            if !metadata.is_file() || metadata.len() > MAX_BIB_BYTES {
                return None;
            }
            let canonical = path.canonicalize().ok()?;
            if !canonical.starts_with(root.canonicalize().ok()?) {
                return None;
            }
            let text = String::from_utf8(std::fs::read(&canonical).ok()?).ok()?;
            let relative = path
                .strip_prefix(&root)
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            Some((relative, text))
        })
        .take(MAX_BIB_FILES)
        .collect()
}

fn from_bibtex(
    files: &[(String, String)],
    cited: &[String],
    checks: &mut LocalChecks,
) -> Vec<Reference> {
    let mut references = Vec::new();
    let mut known = std::collections::HashSet::new();
    for (file, text) in files {
        for entry in bibtex::parse(text) {
            if !known.insert(entry.key.clone()) {
                continue;
            }
            if !cited.is_empty() && !cited.contains(&entry.key) {
                checks.uncited_entries += 1;
                continue;
            }
            let field = |name: &str| {
                entry
                    .fields
                    .get(name)
                    .map(|value| bibtex::plain(value))
                    .unwrap_or_default()
            };
            let authors = entry
                .fields
                .get("author")
                .map(|value| bibtex::family_names(value))
                .unwrap_or_default();
            let venue = [
                field("journal"),
                field("booktitle"),
                field("institution"),
                field("publisher"),
            ]
            .into_iter()
            .find(|value| !value.is_empty())
            .unwrap_or_default();
            let year = field("year");
            let text = bounded(
                &format!(
                    "{}. {}. {}. {}",
                    authors.join(", "),
                    year,
                    field("title"),
                    venue
                ),
                600,
            );
            let raw_doi = field("doi");
            let doi = bibliography::doi(&raw_doi).or_else(|| bibliography::doi(&field("url")));
            if !raw_doi.is_empty() && doi.is_none() {
                checks.malformed_dois.push(references.len() + 1);
            }
            references.push(Reference {
                index: references.len() + 1,
                key: entry.key.clone(),
                source_file: file.clone(),
                first_author: authors.first().cloned(),
                year: bibliography::year(&year),
                doi,
                arxiv: bibliography::arxiv(&format!("{} {}", field("eprint"), field("journal")))
                    .or_else(|| {
                        (field("archiveprefix").eq_ignore_ascii_case("arxiv"))
                            .then(|| field("eprint"))
                            .filter(|eprint| !eprint.is_empty())
                    }),
                text,
                origin: "bibtex",
                status: "not_checked",
                ..Default::default()
            });
        }
    }
    checks.undefined_citations = cited
        .iter()
        .filter(|key| !known.contains(*key))
        .take(50)
        .cloned()
        .collect();
    references
}

fn from_document(blocks: &[Block<'_>]) -> Vec<Reference> {
    bibliography::entries(blocks)
        .into_iter()
        .enumerate()
        .map(|(index, entry)| Reference {
            index: index + 1,
            node_id: entry.node_id,
            first_author: bibliography::first_family(&entry.text),
            year: bibliography::year(&entry.text),
            doi: bibliography::doi(&entry.text),
            arxiv: bibliography::arxiv(&entry.text),
            text: entry.text,
            origin: "bibliography",
            status: "not_checked",
            ..Default::default()
        })
        .collect()
}

fn find_duplicates(references: &[Reference]) -> Vec<(usize, usize)> {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut duplicates = Vec::new();
    for reference in references {
        let identity = reference.doi.clone().unwrap_or_else(|| {
            super::fold::fold(&reference.text, super::fold::Fold::Latex)
                .text
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect()
        });
        if identity.chars().count() < 12 {
            continue;
        }
        if let Some(first) = seen.get(&identity) {
            duplicates.push((*first, reference.index));
        } else {
            seen.insert(identity, reference.index);
        }
    }
    duplicates
}

/// References from `.bib` files for LaTeX inputs, otherwise from the
/// extracted bibliography.
pub fn collect(
    bundle: Option<&DocumentBundle>,
    text: &str,
    source_path: &str,
) -> (Vec<Reference>, LocalChecks) {
    let mut checks = LocalChecks::default();
    let latex = bundle.is_some_and(|bundle| bundle.source_kind == "latex")
        || source_path.to_ascii_lowercase().ends_with(".tex");
    let joined;
    let latex_text = match bundle {
        Some(bundle) if latex => {
            joined = bundle
                .nodes
                .iter()
                .map(|node| node.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            joined.as_str()
        }
        _ => text,
    };
    let mut references = Vec::new();
    if latex || Path::new(source_path).is_dir() {
        let files = bib_files(source_path, latex_text);
        let cited = bibtex::cited_keys(latex_text);
        references = from_bibtex(&files, &cited, &mut checks);
    }
    if references.is_empty() {
        let blocks: Vec<Block<'_>> = match bundle {
            Some(bundle) if !bundle.nodes.is_empty() => {
                let mut nodes: Vec<_> = bundle.nodes.iter().collect();
                nodes.sort_by_key(|node| node.order);
                nodes
                    .into_iter()
                    .map(|node| Block {
                        node_id: node.id.as_str(),
                        text: node.text.as_str(),
                    })
                    .collect()
            }
            _ => bibliography::text_blocks(text),
        };
        references = from_document(&blocks);
    }
    checks.duplicates = find_duplicates(&references);
    (references, checks)
}

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
fn classify(reference: &mut Reference, work: &crossref::Work, by_doi: bool) {
    let coverage = crossref::title_coverage(&work.title, &reference.text);
    let author = crossref::author_present(work, &reference.text);
    let year_gap = match (reference.year, work.year) {
        (Some(cited), Some(found)) => cited.abs_diff(found),
        _ => 0,
    };
    reference.matched = Some(work_json(work));
    let detailed = super::fold::tokens(&reference.text).len() >= 6;
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

fn notice_flag(notices: &[crossref::Notice]) -> Option<&'static str> {
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

/// Resolves references against Crossref. Stops after repeated failures and
/// marks the rest `not_checked`.
async fn resolve(references: &mut [Reference], options: &Options) -> Result<u32, String> {
    let mut client = crossref::Client::new()?;
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
        let best = works.iter().max_by(|left, right| {
            crossref::title_coverage(&left.title, &reference.text)
                .total_cmp(&crossref::title_coverage(&right.title, &reference.text))
        });
        match best {
            Some(work) => classify(reference, work, by_doi),
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
                Ok(notices) => {
                    reference.notices = notices
                        .iter()
                        .map(|notice| json!({"type": notice.kind, "doi": notice.notice_doi, "source": notice.source}))
                        .collect();
                    if let Some(flag) = notice_flag(&notices) {
                        reference.status = flag;
                    }
                }
                Err(crossref::LookupError::Cancelled) => return Err("Pipeline cancelled".into()),
                Err(crossref::LookupError::Failed(error)) => {
                    reference.reason =
                        format!("{} Retraction check failed: {error}", reference.reason)
                            .trim()
                            .to_string();
                }
            }
        }
    }
    Ok(client.requests)
}

fn evidence(reference: &Reference) -> Vec<Value> {
    let mut items = Vec::new();
    if reference.origin == "bibtex" {
        items.push(json!({
            "evidence_type": "source",
            "verification_status": "unverified",
            "source_path": reference.source_file,
            "description": format!("Bibliography entry {}", reference.key),
            "quote": reference.key,
        }));
    } else {
        items.push(json!({
            "evidence_type": "document",
            "verification_status": "unverified",
            "node_id": reference.node_id,
            "description": format!("Reference {}", reference.index),
            "quote": reference.text.chars().take(300).collect::<String>(),
        }));
    }
    if let Some(doi) = reference
        .matched
        .as_ref()
        .and_then(|work| work.get("doi"))
        .and_then(Value::as_str)
        .filter(|doi| !doi.is_empty())
    {
        items.push(json!({
            "evidence_type": "external",
            "verification_status": "unverified",
            "doi": doi,
            "url": format!("https://doi.org/{doi}"),
            "publisher": "Crossref",
            "accessed_at": chrono::Utc::now().date_naive().to_string(),
            "description": "Crossref record",
        }));
    }
    items
}

fn label(reference: &Reference) -> String {
    if reference.key.is_empty() {
        format!("Reference {}", reference.index)
    } else {
        format!("Reference {} ({})", reference.index, reference.key)
    }
}

fn finding(
    id: String,
    title: String,
    severity: &str,
    confidence: &str,
    problem: String,
    action: &str,
    evidence: Vec<Value>,
) -> Value {
    json!({
        "id": id,
        "title": title,
        "category": "References",
        "severity": severity,
        "confidence": confidence,
        "verification_status": "unverified",
        "problem": problem,
        "consequence": "Readers and editors may be unable to trace or rely on the cited work.",
        "recommended_action": action,
        "body": problem,
        "evidence": evidence,
    })
}

fn findings(references: &[Reference], checks: &LocalChecks) -> Vec<Value> {
    let mut items = Vec::new();
    let order = [
        ("retracted", "high", "high", "has been retracted or withdrawn", "Remove the citation or explain why the retracted work is cited."),
        ("expression_of_concern", "medium", "high", "has an expression of concern", "Check the notice and qualify any claim that relies on this work."),
        ("metadata_mismatch", "medium", "medium", "does not match its Crossref record", "Correct the authors, title, or DOI."),
        ("not_found", "medium", "low", "could not be found in Crossref", "Check that the work exists and that its details are correct. Books, reports, and data are often not in Crossref."),
        ("other_version", "low", "medium", "may cite an earlier version", "Cite the published version if one exists, or confirm that the earlier version is intended."),
        ("corrected", "low", "high", "has a published correction", "Check whether the correction affects the cited result."),
    ];
    let mut emitted = 0usize;
    for (status, severity, confidence, phrase, action) in order {
        for reference in references
            .iter()
            .filter(|reference| reference.status == status)
        {
            if emitted >= MAX_REFERENCE_FINDINGS {
                break;
            }
            emitted += 1;
            let mut problem = format!("{} {phrase}.", label(reference));
            if !reference.reason.is_empty() {
                problem.push(' ');
                problem.push_str(&reference.reason);
            }
            if let Some(work) = &reference.matched {
                problem.push_str(&format!(
                    " Crossref: \"{}\" ({}).",
                    work.get("title")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    work.get("year")
                        .and_then(Value::as_u64)
                        .map(|year| year.to_string())
                        .unwrap_or_else(|| "no year".into())
                ));
            }
            items.push(finding(
                format!("reference-{}-{status}", reference.index),
                format!("{} {phrase}", label(reference)),
                severity,
                confidence,
                problem,
                action,
                evidence(reference),
            ));
        }
    }
    let overflow = references
        .iter()
        .filter(|reference| {
            !matches!(
                reference.status,
                "matched" | "not_checked" | "lookup_failed" | "ambiguous"
            )
        })
        .count()
        .saturating_sub(emitted);
    if overflow > 0 {
        items.push(finding(
            "references-more".into(),
            format!("{overflow} more reference issues"),
            "low",
            "medium",
            format!("{overflow} further references have issues; see the reference check output for the full list."),
            "Review the full reference list in the check output.",
            Vec::new(),
        ));
    }
    if !checks.undefined_citations.is_empty() {
        items.push(finding(
            "references-undefined-citations".into(),
            format!(
                "{} citation keys have no bibliography entry",
                checks.undefined_citations.len()
            ),
            "medium",
            "high",
            format!(
                "These keys are cited but missing from the bibliography files: {}.",
                checks.undefined_citations.join(", ")
            ),
            "Add the missing entries or correct the keys.",
            Vec::new(),
        ));
    }
    for (first, second) in checks.duplicates.iter().take(10) {
        items.push(finding(
            format!("references-duplicate-{first}-{second}"),
            format!("References {first} and {second} appear to be the same work"),
            "low",
            "medium",
            format!("References {first} and {second} share a DOI or identical text."),
            "Merge the duplicate entries.",
            references.get(second - 1).map(evidence).unwrap_or_default(),
        ));
    }
    items
}

pub async fn run(
    bundle: Option<&DocumentBundle>,
    text: &str,
    source_path: &str,
    options: &Value,
) -> Result<Value, String> {
    let options = Options::from_value(options);
    let (mut references, checks) = collect(bundle, text, source_path);
    let requests = if options.network && !references.is_empty() {
        resolve(&mut references, &options).await?
    } else {
        0
    };
    let mut summary = std::collections::BTreeMap::<&str, usize>::new();
    for reference in &references {
        *summary.entry(reference.status).or_default() += 1;
    }
    Ok(json!({
        "analyzer": super::registry::REFERENCE_AUDIT,
        "version": 1,
        "network": options.network,
        "services": if options.network { vec!["Crossref"] } else { Vec::new() },
        "requests": requests,
        "checked_at": chrono::Utc::now().to_rfc3339(),
        "reference_count": references.len(),
        "checked_limit": options.max_references,
        "summary": summary,
        "local_checks": checks,
        "meaning": "Matches are against Crossref records. A reference that is not found may be a book, report, or dataset outside Crossref, or have errors in its details; it is not evidence that the work does not exist.",
        "references": references,
        "findings": findings(&references, &checks),
    }))
}

#[cfg(test)]
#[path = "references/tests.rs"]
mod tests;
