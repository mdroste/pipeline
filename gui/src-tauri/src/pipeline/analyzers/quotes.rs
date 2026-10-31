//! Host verification of quoted Review evidence.
//!
//! The verifier never edits model text. It sets each evidence item's
//! host-owned `verification_status` and records how its quote matched in the
//! `evidence_verification` run product. A match establishes that the quoted
//! text exists where it is cited, not that the finding is correct.
//!
//! - `verified`: the quote occurs at its cited node, page, or source file once
//!   whitespace is collapsed (extraction line breaks are not differences).
//! - `verified_with_normalization`: it occurs there only after typographic
//!   folding (see `fold.rs`), after LaTeX markup is removed, as ordered
//!   segments around an ellipsis, or without a leading or trailing ellipsis.
//! - `partially_verified`: it occurs elsewhere in the document
//!   (`locator_mismatch`), its cited node does not exist (`locator_unknown`),
//!   or only a close paraphrase was found (`paraphrase_suspected`).
//! - `not_found`; `not_checkable` when there is no quote, the locator is
//!   external, or the cited file was not captured. A quote missing from the
//!   document is external when its evidence also names a URL or DOI, because
//!   `findings-v1` has no URL field and its evidence can only say so in the
//!   description.

use super::fold::{fold, tokens, Fold};
use crate::document_bundle::DocumentBundle;
use crate::models::{FindingEvidence, NamedRunProduct, RunProducts};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

pub const VERIFIER_VERSION: &str = "evidence-verify-1";
pub const PRODUCT_KEY: &str = "evidence_verification";

pub const VERIFIED: &str = "verified";
pub const VERIFIED_NORMALIZED: &str = "verified_with_normalization";
pub const PARTIAL: &str = "partially_verified";
pub const NOT_FOUND: &str = "not_found";
pub const NOT_CHECKABLE: &str = "not_checkable";
const UNVERIFIED: &str = "unverified";

const MAX_QUOTE_CHARS: usize = 1_000;
const MAX_EVIDENCE_ITEMS: usize = 2_000;
const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
/// Largest captured source file read for verification.
pub const MAX_SOURCE_BYTES: u64 = 2 * 1024 * 1024;
const MIN_SEGMENT_CHARS: usize = 12;
const MIN_OVERLAP_TOKENS: usize = 6;
const OVERLAP_THRESHOLD: f64 = 0.8;
/// Following nodes searched with a cited node, for quotes that cross a
/// paragraph or line-block boundary.
const NODE_SPILL: usize = 2;
const LINE_SLACK: u32 = 3;

#[derive(Clone, Copy)]
struct Piece<'a> {
    node_id: &'a str,
    page: Option<u32>,
    text: &'a str,
}

/// Pieces folded at one level and joined by single spaces. `starts[i]` is
/// where piece `i` begins, so a match is attributed without a per-byte map.
struct Scope {
    text: String,
    starts: Vec<usize>,
}

impl Scope {
    fn new(pieces: &[Piece<'_>], mode: Fold) -> Self {
        let mut text = String::new();
        let mut starts = Vec::with_capacity(pieces.len());
        for piece in pieces {
            if !text.is_empty() {
                text.push(' ');
            }
            starts.push(text.len());
            text.push_str(&fold(piece.text, mode).text);
        }
        Self { text, starts }
    }

    fn piece_at(&self, offset: usize) -> usize {
        self.starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1)
    }
}

fn is_blank(value: &&str) -> bool {
    value.is_empty()
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MatchLocation {
    /// `node`, `page`, `document`, or `source`.
    pub scope: &'static str,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub node_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub end_node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// UTF-8 byte range: `start` in the first node's text (or the source file),
    /// `end` in the last node's text.
    pub start: usize,
    pub end: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Outcome {
    pub status: &'static str,
    /// `exact`, `normalized`, `latex_markup`, `ellipsis`, or `overlap`.
    #[serde(skip_serializing_if = "is_blank")]
    pub method: &'static str,
    #[serde(skip_serializing_if = "is_blank")]
    pub reason: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched: Option<MatchLocation>,
}

impl Outcome {
    fn plain(status: &'static str, reason: &'static str) -> Self {
        Self {
            status,
            method: "",
            reason,
            matched: None,
        }
    }
}

type Mode = (Fold, &'static str, &'static str);

fn modes(latex: bool) -> Vec<Mode> {
    let mut modes = vec![
        (Fold::Whitespace, VERIFIED, "exact"),
        (Fold::Typographic, VERIFIED_NORMALIZED, "normalized"),
    ];
    if latex {
        modes.push((Fold::Latex, VERIFIED_NORMALIZED, "latex_markup"));
    }
    modes
}

/// Document text available to the verifier, folded once per run.
pub struct DocumentIndex<'a> {
    pieces: Vec<Piece<'a>>,
    node_positions: HashMap<&'a str, usize>,
    latex: bool,
    too_large: bool,
    document: Vec<Scope>,
}

impl<'a> DocumentIndex<'a> {
    pub fn from_bundle(bundle: &'a DocumentBundle) -> Self {
        let mut nodes: Vec<_> = bundle
            .nodes
            .iter()
            .filter(|node| !node.text.trim().is_empty())
            .collect();
        nodes.sort_by_key(|node| node.order);
        let pieces = nodes
            .iter()
            .map(|node| Piece {
                node_id: node.id.as_str(),
                page: node.page,
                text: node.text.as_str(),
            })
            .collect();
        Self::new(pieces, bundle.source_kind == "latex")
    }

    /// Fallback for runs without a DocumentBundle: one unlocated piece.
    pub fn from_text(text: &'a str, latex: bool) -> Self {
        Self::new(
            vec![Piece {
                node_id: "",
                page: None,
                text,
            }],
            latex,
        )
    }

    fn new(pieces: Vec<Piece<'a>>, latex: bool) -> Self {
        let bytes: usize = pieces.iter().map(|piece| piece.text.len()).sum();
        let too_large = bytes > MAX_DOCUMENT_BYTES;
        let node_positions = pieces
            .iter()
            .enumerate()
            .filter(|(_, piece)| !piece.node_id.is_empty())
            .map(|(index, piece)| (piece.node_id, index))
            .collect();
        let document = if too_large {
            Vec::new()
        } else {
            modes(latex)
                .into_iter()
                .map(|(mode, _, _)| Scope::new(&pieces, mode))
                .collect()
        };
        Self {
            pieces,
            node_positions,
            latex,
            too_large,
            document,
        }
    }

    fn page_pieces(&self, page: u32) -> Vec<Piece<'a>> {
        self.pieces
            .iter()
            .filter(|piece| piece.page == Some(page))
            .copied()
            .collect()
    }
}

const ELLIPSES: [&str; 4] = ["[...]", "[\u{2026}]", "...", "\u{2026}"];

/// Quote text without wrapping quotation marks, surrounding whitespace, or
/// leading and trailing ellipses. The flag reports whether an ellipsis was
/// removed.
fn trimmed_quote(quote: &str) -> (&str, bool) {
    let mut rest = quote;
    let mut elided = false;
    loop {
        let before = rest.len();
        rest = rest.trim().trim_matches(|c: char| {
            matches!(
                c,
                '"' | '\''
                    | '\u{201c}'
                    | '\u{201d}'
                    | '\u{2018}'
                    | '\u{2019}'
                    | '\u{ab}'
                    | '\u{bb}'
            )
        });
        for marker in ELLIPSES {
            if let Some(stripped) = rest.strip_prefix(marker) {
                rest = stripped;
                elided = true;
            }
            if let Some(stripped) = rest.strip_suffix(marker) {
                rest = stripped;
                elided = true;
            }
        }
        if rest.len() == before {
            return (rest, elided);
        }
    }
}

/// A quote that matched only after a leading or trailing ellipsis was removed
/// is a normalized match, like one matched as ellipsis segments.
fn mark_elided(mut outcome: Outcome, elided: bool) -> Outcome {
    if elided && matches!(outcome.method, "exact" | "normalized" | "latex_markup") {
        outcome.method = "ellipsis";
        if outcome.status == VERIFIED {
            outcome.status = VERIFIED_NORMALIZED;
        }
    }
    outcome
}

/// The segments a reviewer kept verbatim around ellipses.
fn ellipsis_segments(quote: &str) -> Vec<&str> {
    let mut parts = vec![quote];
    for marker in ELLIPSES {
        parts = parts
            .into_iter()
            .flat_map(|part| part.split(marker))
            .collect();
    }
    parts
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

/// Finds the folded quote, or all its ellipsis segments in order.
fn find_folded(haystack: &str, needle: &str, segments: &[String]) -> Option<(usize, usize, bool)> {
    if needle.is_empty() {
        return None;
    }
    if let Some(start) = haystack.find(needle) {
        return Some((start, start + needle.len(), false));
    }
    if segments.len() < 2
        || segments
            .iter()
            .any(|segment| segment.chars().count() < MIN_SEGMENT_CHARS)
    {
        return None;
    }
    let first = haystack.find(segments[0].as_str())?;
    let mut cursor = first + segments[0].len();
    for segment in &segments[1..] {
        cursor += haystack[cursor..].find(segment.as_str())? + segment.len();
    }
    Some((first, cursor, true))
}

/// Original byte offset of the character behind folded byte `at` (start) or
/// just after the character behind folded byte `at - 1` (end).
fn original_offset(piece: &str, mode: Fold, at: usize, end: bool) -> usize {
    let folded = fold(piece, mode);
    if end {
        if at == 0 {
            return 0;
        }
        return folded
            .map
            .get(at.min(folded.map.len()) - 1)
            .map(|offset| {
                offset
                    + piece[*offset..]
                        .chars()
                        .next()
                        .map(char::len_utf8)
                        .unwrap_or(0)
            })
            .unwrap_or(piece.len());
    }
    folded.map.get(at).copied().unwrap_or(piece.len())
}

fn locate(
    pieces: &[Piece<'_>],
    scope: &Scope,
    mode: Fold,
    (start, end): (usize, usize),
    label: &'static str,
) -> MatchLocation {
    let first = scope.piece_at(start);
    let last = scope.piece_at(end.saturating_sub(1)).max(first);
    let start_offset =
        original_offset(pieces[first].text, mode, start - scope.starts[first], false);
    let end_offset = original_offset(pieces[last].text, mode, end - scope.starts[last], true);
    MatchLocation {
        scope: label,
        node_id: pieces[first].node_id.to_string(),
        end_node_id: if first == last {
            String::new()
        } else {
            pieces[last].node_id.to_string()
        },
        page: pieces[first].page,
        start: start_offset,
        end: if first == last {
            end_offset.max(start_offset)
        } else {
            end_offset
        },
        line: None,
    }
}

/// Searches pieces at each folding level, cheapest first. `prebuilt` holds
/// already-folded scopes in the same order as `modes`.
fn search(
    pieces: &[Piece<'_>],
    modes: &[Mode],
    prebuilt: &[Scope],
    quote: &str,
    label: &'static str,
) -> Option<Outcome> {
    for (index, (mode, status, method)) in modes.iter().enumerate() {
        let needle = fold(quote, *mode).text;
        let segments: Vec<String> = ellipsis_segments(quote)
            .into_iter()
            .map(|segment| fold(segment, *mode).text)
            .collect();
        let built;
        let scope = match prebuilt.get(index) {
            Some(scope) => scope,
            None => {
                built = Scope::new(pieces, *mode);
                &built
            }
        };
        if let Some((start, end, by_segments)) = find_folded(&scope.text, &needle, &segments) {
            return Some(Outcome {
                status: if by_segments {
                    VERIFIED_NORMALIZED
                } else {
                    status
                },
                method: if by_segments { "ellipsis" } else { method },
                reason: "",
                matched: Some(locate(pieces, scope, *mode, (start, end), label)),
            });
        }
    }
    None
}

/// The best window of scope words sharing at least 80% of the quote's words.
fn overlap(pieces: &[Piece<'_>], quote: &str, label: &'static str) -> Option<Outcome> {
    let folded_quote = fold(quote, Fold::Typographic).text;
    let wanted_tokens = tokens(&folded_quote);
    let width = wanted_tokens.len();
    if width < MIN_OVERLAP_TOKENS {
        return None;
    }
    let scope = Scope::new(pieces, Fold::Typographic);
    let words = tokens(&scope.text);
    if words.len() < width {
        return None;
    }
    let mut wanted: HashMap<&str, i64> = HashMap::new();
    for (_, token) in &wanted_tokens {
        *wanted.entry(token).or_default() += 1;
    }
    let mut window: HashMap<&str, i64> = HashMap::new();
    let mut shared = 0i64;
    let mut best = (0i64, 0usize);
    for (index, (_, word)) in words.iter().enumerate() {
        let count = window.entry(word).or_default();
        *count += 1;
        if *count <= wanted.get(word).copied().unwrap_or(0) {
            shared += 1;
        }
        if index >= width {
            let leaving = words[index - width].1;
            let count = window.entry(leaving).or_default();
            if *count <= wanted.get(leaving).copied().unwrap_or(0) {
                shared -= 1;
            }
            *count -= 1;
        }
        if index + 1 >= width && shared > best.0 {
            best = (shared, index + 1 - width);
        }
    }
    if (best.0 as f64) < OVERLAP_THRESHOLD * width as f64 {
        return None;
    }
    let start = words[best.1].0;
    let (last_start, last_word) = words[best.1 + width - 1];
    Some(Outcome {
        status: PARTIAL,
        method: "overlap",
        reason: "paraphrase_suspected",
        matched: Some(locate(
            pieces,
            &scope,
            Fold::Typographic,
            (start, last_start + last_word.len()),
            label,
        )),
    })
}

fn verify_document(index: &DocumentIndex<'_>, evidence: &FindingEvidence, quote: &str) -> Outcome {
    if index.too_large {
        return Outcome::plain(NOT_CHECKABLE, "document_too_large");
    }
    let modes = modes(index.latex);
    let node_unknown = !evidence.node_id.is_empty()
        && !index.node_positions.contains_key(evidence.node_id.as_str());
    let cited: Option<(Vec<Piece<'_>>, &'static str)> =
        if let Some(position) = index.node_positions.get(evidence.node_id.as_str()) {
            let end = (position + 1 + NODE_SPILL).min(index.pieces.len());
            Some((index.pieces[*position..end].to_vec(), "node"))
        } else {
            evidence
                .page
                .map(|page| index.page_pieces(page))
                .filter(|pieces| !pieces.is_empty())
                .map(|pieces| (pieces, "page"))
        };

    if let Some((pieces, label)) = &cited {
        if let Some(mut outcome) = search(pieces, &modes, &[], quote, label) {
            if node_unknown {
                outcome.status = PARTIAL;
                outcome.reason = "locator_unknown";
            }
            return outcome;
        }
    }
    if let Some(mut outcome) = search(&index.pieces, &modes, &index.document, quote, "document") {
        if node_unknown {
            outcome.status = PARTIAL;
            outcome.reason = "locator_unknown";
        } else if cited.is_some() {
            outcome.status = PARTIAL;
            outcome.reason = "locator_mismatch";
        }
        return outcome;
    }
    if let Some((pieces, label)) = &cited {
        if let Some(outcome) = overlap(pieces, quote, label) {
            return outcome;
        }
    }
    overlap(&index.pieces, quote, "document").unwrap_or_else(|| Outcome::plain(NOT_FOUND, ""))
}

fn verify_source(text: &str, path: &str, evidence: &FindingEvidence, quote: &str) -> Outcome {
    let lower = path.to_ascii_lowercase();
    let pieces = [Piece {
        node_id: "",
        page: None,
        text,
    }];
    let modes = modes(lower.ends_with(".tex") || lower.ends_with(".bib"));
    let mut outcome = search(&pieces, &modes, &[], quote, "source")
        .or_else(|| overlap(&pieces, quote, "source"))
        .unwrap_or_else(|| Outcome::plain(NOT_FOUND, ""));
    if let Some(matched) = outcome.matched.as_mut() {
        let start = matched.start.min(text.len());
        let line = text[..start].bytes().filter(|byte| *byte == b'\n').count() as u32 + 1;
        matched.line = Some(line);
        if let Some(cited) = evidence.line_start {
            let cited_end = evidence.line_end.unwrap_or(cited).max(cited);
            let near = line + LINE_SLACK >= cited && line <= cited_end + LINE_SLACK;
            if !near && outcome.status != PARTIAL {
                outcome.status = PARTIAL;
                outcome.reason = "locator_mismatch";
            }
        }
    }
    outcome
}

/// Verifies one evidence item. `read_source` returns a captured source file's
/// text by the evidence's run-relative artifact path.
pub fn verify_evidence(
    index: &DocumentIndex<'_>,
    evidence: &FindingEvidence,
    read_source: &mut dyn FnMut(&str) -> Option<String>,
) -> Outcome {
    let (quote, elided) = trimmed_quote(&evidence.quote);
    if quote.is_empty() {
        return Outcome::plain(NOT_CHECKABLE, "no_quote");
    }
    if quote.chars().count() > MAX_QUOTE_CHARS {
        return Outcome::plain(NOT_CHECKABLE, "quote_too_long");
    }
    let kind = evidence.evidence_type.as_str();
    if kind == "source" || !evidence.source_path.is_empty() {
        if evidence.artifact_path.is_empty() {
            return Outcome::plain(NOT_CHECKABLE, "source_not_captured");
        }
        return match read_source(&evidence.artifact_path) {
            Some(text) => mark_elided(
                verify_source(&text, &evidence.source_path, evidence, quote),
                elided,
            ),
            None => Outcome::plain(NOT_CHECKABLE, "source_not_captured"),
        };
    }
    let located = evidence.page.is_some() || !evidence.node_id.is_empty();
    let external = !evidence.url.is_empty()
        || !evidence.doi.is_empty()
        || !evidence.query_id.is_empty()
        || !evidence.call_id.is_empty();
    if matches!(kind, "external" | "call") || (external && !located) {
        return Outcome::plain(NOT_CHECKABLE, "external_locator");
    }
    let outcome = mark_elided(verify_document(index, evidence, quote), elided);
    if outcome.status == NOT_FOUND && (external || names_outside_source(&evidence.description)) {
        return Outcome::plain(NOT_CHECKABLE, "external_locator");
    }
    outcome
}

fn names_outside_source(description: &str) -> bool {
    let description = description.to_ascii_lowercase();
    ["http://", "https://", "doi.org/", "doi:"]
        .iter()
        .any(|marker| description.contains(marker))
}

#[derive(Serialize)]
struct Record {
    finding_id: String,
    evidence_index: usize,
    #[serde(flatten)]
    outcome: Outcome,
}

/// Sets every finding evidence item's host status and adds the
/// `evidence_verification` product. Statuses a model claimed are overwritten.
pub fn verify_products(
    products: &mut RunProducts,
    index: &DocumentIndex<'_>,
    bundle_id: &str,
    read_source: &mut dyn FnMut(&str) -> Option<String>,
) {
    let Some(findings) = products.findings.as_mut() else {
        return;
    };
    let mut records = Vec::new();
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut checked = 0usize;
    for finding in &mut findings.findings {
        for (evidence_index, evidence) in finding.evidence.iter_mut().enumerate() {
            if checked >= MAX_EVIDENCE_ITEMS {
                evidence.verification_status = UNVERIFIED.to_string();
                *counts.entry("skipped_at_limit").or_default() += 1;
                continue;
            }
            checked += 1;
            let outcome = verify_evidence(index, evidence, read_source);
            evidence.verification_status = outcome.status.to_string();
            *counts.entry(outcome.status).or_default() += 1;
            records.push(Record {
                finding_id: finding.id.clone(),
                evidence_index,
                outcome,
            });
        }
    }
    let source_step_id = findings.source_step_id.clone();
    let content = serde_json::json!({
        "schema_version": 1,
        "verifier": VERIFIER_VERSION,
        "bundle_id": bundle_id,
        "source_step_id": source_step_id,
        "counts": counts,
        "limits": {"max_evidence_items": MAX_EVIDENCE_ITEMS, "max_quote_chars": MAX_QUOTE_CHARS},
        "meaning": "A verified quote exists where it is cited. Verification does not assess whether a finding is correct.",
        "items": records,
    });
    products.named.retain(|product| product.key != PRODUCT_KEY);
    products.named.push(NamedRunProduct {
        key: PRODUCT_KEY.to_string(),
        source_step_id,
        media_type: "application/json".to_string(),
        viewer: "json".to_string(),
        export_policy: String::new(),
        sensitivity: String::new(),
        content,
    });
}

/// Reads a captured source file by its run-relative path. Only regular files
/// inside the run directory are read; links, special files, and oversized
/// files are treated as not captured.
fn read_captured(run_dir: &std::path::Path, relative: &str) -> Option<String> {
    let relative = std::path::Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return None;
    }
    let root = run_dir.canonicalize().ok()?;
    let path = root.join(relative);
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE_BYTES {
        return None;
    }
    if !path.canonicalize().ok()?.starts_with(&root) {
        return None;
    }
    String::from_utf8(std::fs::read(&path).ok()?).ok()
}

/// Verifies a finished run's evidence against its DocumentBundle, or its
/// extracted text when no bundle exists. Called before the quality ledger is
/// built so verified evidence is counted.
pub fn verify_run(
    products: &mut RunProducts,
    bundle: Option<&DocumentBundle>,
    extraction: &crate::models::ExtractionResult,
    run_dir: Option<&std::path::Path>,
) {
    if products
        .findings
        .as_ref()
        .is_none_or(|findings| findings.findings.is_empty())
    {
        return;
    }
    let mut read_source = |relative: &str| run_dir.and_then(|dir| read_captured(dir, relative));
    match bundle {
        Some(bundle) => verify_products(
            products,
            &DocumentIndex::from_bundle(bundle),
            &bundle.bundle_id,
            &mut read_source,
        ),
        None => {
            let lower = extraction.source_path.to_ascii_lowercase();
            let latex = extraction.method == "latex" || lower.ends_with(".tex");
            verify_products(
                products,
                &DocumentIndex::from_text(&extraction.text, latex),
                "",
                &mut read_source,
            )
        }
    }
}

#[cfg(test)]
#[path = "quotes_tests.rs"]
mod tests;
