//! `reference_audit` v1: find a manuscript's references and, when the step
//! enables network lookups, check each against Crossref.
//!
//! Without the network the check is local: undefined LaTeX citations,
//! duplicate entries, and malformed DOIs. A key counts as undefined only when
//! a complete bibliography was read. Otherwise, and when no bibliography was
//! found, the check says what it could not cover. With the network, each
//! reference is
//! `matched`, `other_version` (for example a working paper later published),
//! `metadata_mismatch`, `ambiguous`, `not_found`, or `lookup_failed`, and
//! matched DOIs are checked for retractions, withdrawals, expressions of
//! concern, and corrections, which Crossref records with Retraction Watch.
//! A notice changes a reference's status only when the record is confirmed to
//! be the cited work; on any other record the report says so and never calls
//! the cited work retracted. `not_found` means not found in Crossref, never
//! that a work is fabricated. References that went unchecked are always
//! reported. If no lookup succeeded the local results stand, with a finding
//! that says which online checks did not run.

mod bibliography;
mod bibtex;
mod crossref;
mod lookups;

use crate::document_bundle::DocumentBundle;
use bibliography::Block;
#[cfg(test)]
use lookups::{best_candidate, classify, notice_flag};
use lookups::{record_lookup_gaps, resolve, Options};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const MAX_BIB_FILES: usize = 20;
const MAX_BIB_BYTES: u64 = 4 * 1024 * 1024;
/// Keeps the step's output bounded when a source cites nothing, so that every
/// entry of every file is listed.
const MAX_LISTED_REFERENCES: usize = MAX_BIB_FILES * 2_000;
const MAX_REFERENCE_FINDINGS: usize = 60;
const MAX_LISTED_MALFORMED: usize = 40;

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
    /// The notice lookup failed, so a clean status says nothing about notices.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub notices_unchecked: bool,
}

/// A bibliography file that was not read in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkippedBibFile {
    pub file: String,
    pub reason: String,
}

#[derive(Debug, Default, Serialize)]
pub struct LocalChecks {
    pub undefined_citations: Vec<String>,
    pub uncited_entries: usize,
    pub duplicates: Vec<(usize, usize)>,
    pub malformed_dois: Vec<usize>,
    pub skipped_bib_files: Vec<SkippedBibFile>,
    /// Keys defined by `\bibitem` or a compiled `.bbl` file.
    pub compiled_entries: usize,
    /// What the check could not cover, written for the researcher.
    pub limitations: Vec<String>,
    /// What the Crossref lookups could not cover, written for the researcher.
    pub lookup_limitations: Vec<String>,
    /// Set when no lookup succeeded, saying which online checks did not run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crossref_outage: Option<String>,
}

fn bounded(text: &str, max: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(max).collect()
}

/// What a LaTeX source's bibliography files gave up. Each `(file, text)` pair
/// names the file relative to the source folder.
#[derive(Default)]
struct BibliographySources {
    bib: Vec<(String, String)>,
    bbl: Vec<(String, String)>,
    skipped: Vec<SkippedBibFile>,
}

/// Every `.bib` and `.bbl` within three levels of the source folder.
fn project_bibliography_files(root: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut bib = Vec::new();
    let mut bbl = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = read.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if name.starts_with('.') || kind.is_symlink() {
                continue;
            }
            if kind.is_dir() && depth < 3 {
                stack.push((entry.path(), depth + 1));
            } else if kind.is_file() && name.ends_with(".bib") {
                bib.push(entry.path());
            } else if kind.is_file() && name.ends_with(".bbl") {
                bbl.push(entry.path());
            }
        }
    }
    bib.sort();
    bbl.sort();
    (bib, bbl)
}

/// UTF-8, or Latin-1 for the older files that are not. Citation keys are ASCII
/// either way, and each Latin-1 byte maps to a character, so nothing is dropped.
fn decode_bibliography(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap_or_else(|error| {
        error
            .as_bytes()
            .iter()
            .map(|byte| char::from(*byte))
            .collect()
    })
}

fn read_bibliography_file(path: &Path, root: &Path) -> Result<String, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| "not found".to_string())?;
    if !metadata.is_file() {
        return Err("not a regular file".into());
    }
    if metadata.len() > MAX_BIB_BYTES {
        return Err(format!(
            "larger than the {} MB limit",
            MAX_BIB_BYTES / (1024 * 1024)
        ));
    }
    let canonical = path
        .canonicalize()
        .ok()
        .filter(|canonical| canonical.starts_with(root))
        .ok_or("outside the project folder")?;
    std::fs::read(&canonical)
        .map(decode_bibliography)
        .map_err(|_| "could not be read".to_string())
}

fn read_bibliography_files(
    paths: Vec<PathBuf>,
    root: &Path,
    skipped: &mut Vec<SkippedBibFile>,
) -> Vec<(String, String)> {
    let mut files = Vec::new();
    for path in paths {
        let file = path
            .strip_prefix(root)
            .map(|relative| relative.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let read = if files.len() == MAX_BIB_FILES {
            Err(format!("beyond the limit of {MAX_BIB_FILES} files"))
        } else {
            read_bibliography_file(&path, root)
        };
        match read {
            Ok(text) => files.push((file, text)),
            Err(reason) => skipped.push(SkippedBibFile { file, reason }),
        }
    }
    files
}

/// `.bib` files named by `\bibliography`/`\addbibresource`, or every `.bib`
/// within three levels of the source folder when none is named, plus any
/// compiled `.bbl`, which no command names. Everything that could not be read
/// is recorded, because a citation may be defined in it.
fn bib_files(source_path: &str, latex: &str) -> BibliographySources {
    let source = Path::new(source_path);
    let root = if source.is_dir() {
        source.to_path_buf()
    } else {
        source.parent().map(Path::to_path_buf).unwrap_or_default()
    };
    if root.as_os_str().is_empty() {
        return BibliographySources::default();
    }
    let Ok(root) = root.canonicalize() else {
        return BibliographySources::default();
    };
    let mut sources = BibliographySources::default();
    let visible = crate::pipeline::extract::latex_without_comments(latex);
    let mut candidates = Vec::new();
    for capture in regex::Regex::new(r"\\(?:bibliography|addbibresource)\{([^}]*)\}")
        .expect("valid")
        .captures_iter(&visible)
    {
        for name in capture[1].split(',').map(str::trim) {
            if name.is_empty() || name.contains('#') {
                continue;
            }
            let file = if name.ends_with(".bib") {
                name.to_string()
            } else {
                format!("{name}.bib")
            };
            if !name.contains("..") {
                candidates.push(root.join(file));
                continue;
            }
            // A captured project keeps a nearby file under its own name.
            match crate::pipeline::extract::captured_reference(&root, &root, name) {
                Some(Some(captured)) => candidates.push(captured),
                _ => sources.skipped.push(SkippedBibFile {
                    file,
                    reason: "outside the project folder".into(),
                }),
            }
        }
    }
    let (found_bib, found_bbl) = project_bibliography_files(&root);
    if candidates.is_empty() {
        candidates = found_bib;
    }
    sources.bib = read_bibliography_files(candidates, &root, &mut sources.skipped);
    sources.bbl = read_bibliography_files(found_bbl, &root, &mut sources.skipped);
    sources
}

/// What to say when `\cite` keys could not be compared with a whole
/// bibliography. A key missing from a partial read may be in the unread part.
fn unread_bibliography_note(any_read: bool, skipped: &[SkippedBibFile]) -> String {
    let unread = skipped
        .iter()
        .map(|file| format!("{} ({})", file.file, file.reason))
        .collect::<Vec<_>>()
        .join("; ");
    if any_read {
        format!(
            "Part of the bibliography could not be read: {unread}. A citation missing from the part that was read may be in the rest, so undefined citations are not reported."
        )
    } else if unread.is_empty() {
        "No bibliography file was found for this LaTeX source, so its citations could not be compared with one. Undefined citations are not reported.".to_string()
    } else {
        format!(
            "No bibliography could be read for this LaTeX source: {unread}. Its citations could not be compared with one, so undefined citations are not reported."
        )
    }
}

fn from_bibtex(
    sources: BibliographySources,
    latex: &str,
    cited: &[String],
    checks: &mut LocalChecks,
) -> Vec<Reference> {
    let mut references = Vec::new();
    let mut known = std::collections::HashSet::new();
    let mut unlisted = 0usize;
    let cited_keys: std::collections::HashSet<&str> = cited.iter().map(String::as_str).collect();
    checks.skipped_bib_files = sources.skipped;
    for (file, text) in &sources.bib {
        let parsed = bibtex::parse(text);
        let unfinished = parsed.stopped.or_else(|| {
            (parsed.entries.is_empty() && !text.trim().is_empty())
                .then(|| "no entries could be read".to_string())
        });
        if let Some(reason) = unfinished {
            checks.skipped_bib_files.push(SkippedBibFile {
                file: file.clone(),
                reason,
            });
        }
        for entry in parsed.entries {
            if !known.insert(entry.key.clone()) {
                continue;
            }
            if !cited.is_empty() && !cited_keys.contains(entry.key.as_str()) {
                checks.uncited_entries += 1;
                continue;
            }
            if references.len() == MAX_LISTED_REFERENCES {
                unlisted += 1;
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
    if unlisted > 0 {
        checks.limitations.push(format!(
            "Only the first {MAX_LISTED_REFERENCES} bibliography entries were listed. The other {unlisted} were not checked."
        ));
    }
    // A compiled or written-out list holds every cited key as it stands, so
    // it stands in for a `.bib` that is missing, as it is in an arXiv source.
    let mut compiled: std::collections::HashSet<String> =
        bibtex::bibitem_keys(latex).into_iter().collect();
    for (_, text) in &sources.bbl {
        compiled.extend(bibtex::compiled_keys(text));
    }
    checks.compiled_entries = compiled.len();
    let any_read = !known.is_empty() || !compiled.is_empty();
    let complete = !compiled.is_empty() || (any_read && checks.skipped_bib_files.is_empty());
    known.extend(compiled);
    if complete {
        checks.undefined_citations = cited
            .iter()
            .filter(|key| !known.contains(*key))
            .take(50)
            .cloned()
            .collect();
    } else if !cited.is_empty() {
        checks.limitations.push(unread_bibliography_note(
            any_read,
            &checks.skipped_bib_files,
        ));
    }
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

/// References under the document's bibliography heading. Only paragraph-level
/// nodes separate that heading from its entries: other extractors keep a whole
/// page in one node, so those read the extracted text instead.
fn document_references(bundle: Option<&DocumentBundle>, text: &str) -> Vec<Reference> {
    let paragraphs = bundle.filter(|bundle| bundle.nodes.iter().any(|n| n.kind == "text_block"));
    let mut references = Vec::new();
    if let Some(bundle) = paragraphs {
        let mut nodes: Vec<_> = bundle.nodes.iter().collect();
        nodes.sort_by_key(|node| node.order);
        let blocks: Vec<Block<'_>> = nodes
            .into_iter()
            .map(|node| Block {
                node_id: node.id.as_str(),
                text: node.text.as_str(),
            })
            .collect();
        references = from_document(&blocks);
    }
    if references.is_empty() {
        references = from_document(&bibliography::text_blocks(text));
    }
    references
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
        let sources = bib_files(source_path, latex_text);
        let cited = bibtex::cited_keys(&bibtex::outside_bibliography(latex_text));
        references = from_bibtex(sources, latex_text, &cited, &mut checks);
    }
    if references.is_empty() {
        references = document_references(bundle, text);
        checks.malformed_dois.extend(
            references
                .iter()
                .filter(|item| item.doi.is_none() && bibliography::names_doi(&item.text))
                .map(|item| item.index),
        );
    }
    if references.is_empty() && checks.limitations.is_empty() && checks.uncited_entries == 0 {
        checks.limitations.push(if checks.compiled_entries > 0 {
            "This source's bibliography was used only to confirm its citation keys. Its entries could not be read one by one, so they were not checked for duplicates, DOIs, or Crossref records.".to_string()
        } else {
            "No bibliography was found, so no references were checked. The check looks for a heading such as References or Bibliography followed by at least three entries.".to_string()
        });
    }
    checks.duplicates = find_duplicates(&references);
    (references, checks)
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

fn limitation_finding(index: usize, limitation: &str) -> Value {
    let mut item = finding(
        format!("references-limitation-{}", index + 1),
        "The reference check was limited".into(),
        "info",
        "high",
        limitation.to_string(),
        "Check the bibliography by hand, or supply a complete copy and run the Review again.",
        Vec::new(),
    );
    item["consequence"] =
        "Problems in references that were not read do not appear in this report.".into();
    item
}

fn lookup_limitation_finding(index: usize, limitation: &str) -> Value {
    let mut item = finding(
        format!("references-not-checked-{}", index + 1),
        "Some references were not checked against Crossref".into(),
        "low",
        "high",
        limitation.to_string(),
        "Check these references by hand, or run the Review again.",
        Vec::new(),
    );
    item["consequence"] =
        "Problems in references that were not checked do not appear in this report.".into();
    item
}

fn crossref_outage_finding(outage: &str) -> Value {
    let mut item = finding(
        "references-crossref-unavailable".into(),
        "References were not checked against Crossref".into(),
        "medium",
        "high",
        outage.to_string(),
        "Run the Review again when Crossref can be reached, or check the references by hand.",
        Vec::new(),
    );
    item["consequence"] =
        "A clean result here says nothing about whether these works exist or were retracted."
            .into();
    item
}

fn malformed_doi_finding(references: &[Reference], indexes: &[usize]) -> Value {
    let listed: Vec<&Reference> = indexes
        .iter()
        .take(MAX_LISTED_MALFORMED)
        .filter_map(|index| references.get(index - 1))
        .collect();
    let mut names = listed
        .iter()
        .map(|item| label(item))
        .collect::<Vec<_>>()
        .join(", ");
    if indexes.len() > listed.len() {
        names.push_str(&format!(", and {} more", indexes.len() - listed.len()));
    }
    finding(
        "references-malformed-dois".into(),
        format!("{} references have a malformed DOI", indexes.len()),
        "low",
        "high",
        format!(
            "These references give a DOI that is not valid, so it could not be looked up: {names}."
        ),
        "Correct each DOI or remove it.",
        listed
            .iter()
            .take(10)
            .flat_map(|item| evidence(item))
            .collect(),
    )
}

fn findings(references: &[Reference], checks: &LocalChecks) -> Vec<Value> {
    let mut items: Vec<Value> = checks
        .crossref_outage
        .iter()
        .map(|outage| crossref_outage_finding(outage))
        .collect();
    let order = [
        ("retracted", "high", "high", "has been retracted or withdrawn", "Remove the citation or explain why the retracted work is cited."),
        ("expression_of_concern", "medium", "high", "has an expression of concern", "Check the notice and qualify any claim that relies on this work."),
        ("metadata_mismatch", "medium", "medium", "does not match its Crossref record", "Correct the authors, title, or DOI."),
        ("not_found", "medium", "low", "could not be found in Crossref", "Check that the work exists and that its details are correct. Books, reports, and data are often not in Crossref."),
        ("other_version", "low", "medium", "may cite an earlier version", "Cite the published version if one exists, or confirm that the earlier version is intended."),
        ("corrected", "low", "high", "has a published correction", "Check whether the correction affects the cited result."),
        ("ambiguous", "low", "low", "could not be confirmed against Crossref", "Compare the reference with the Crossref record by hand."),
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
                "matched" | "not_checked" | "lookup_failed"
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
    if !checks.malformed_dois.is_empty() {
        items.push(malformed_doi_finding(references, &checks.malformed_dois));
    }
    for (index, limitation) in checks.limitations.iter().enumerate() {
        items.push(limitation_finding(index, limitation));
    }
    for (index, limitation) in checks.lookup_limitations.iter().enumerate() {
        items.push(lookup_limitation_finding(index, limitation));
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
    let (references, checks) = collect(bundle, text, source_path);
    report(references, checks, &options, crossref::API).await
}

/// Runs the lookups the options ask for and assembles the step's product.
async fn report(
    mut references: Vec<Reference>,
    mut checks: LocalChecks,
    options: &Options,
    crossref_api: &str,
) -> Result<Value, String> {
    let requests = if options.network && !references.is_empty() {
        let mut client = crossref::Client::new(crossref_api)?;
        resolve(&mut client, &mut references, options).await?;
        record_lookup_gaps(&references, options, &mut checks);
        client.requests
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
