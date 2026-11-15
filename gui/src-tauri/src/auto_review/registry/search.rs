//! Deterministic lexical retrieval. Exclusion text is returned for judgment,
//! but never scored as positive evidence that a role is relevant.
use super::{cards, RoutingCard};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

fn boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn terms(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter_map(|word| {
            let mut word = word.to_lowercase();
            if word.len() < 3
                || [
                    "the", "and", "for", "with", "from", "that", "this", "are", "when", "which",
                    "paper", "research", "review", "method", "methods", "general", "central",
                    "claims", "using", "used", "study", "analysis",
                ]
                .contains(&word.as_str())
            {
                return None;
            }
            if word.len() > 5 && word.ends_with('s') {
                word.pop();
            }
            Some(word)
        })
        .collect()
}

struct SearchEntry {
    terms: BTreeSet<String>,
    title: BTreeSet<String>,
}

pub(super) fn rank(
    queries: &[String],
    kind: &str,
    group: &str,
    limit: usize,
) -> Vec<&'static RoutingCard> {
    static INDEX: LazyLock<(Vec<SearchEntry>, BTreeMap<String, usize>)> = LazyLock::new(|| {
        let entries = cards()
            .iter()
            .map(|card| SearchEntry {
                terms: terms(&format!(
                    "{} {} {} {}",
                    card.id, card.label, card.group_label, card.description
                )),
                title: terms(&format!("{} {}", card.id, card.label)),
            })
            .collect::<Vec<_>>();
        let mut frequencies = BTreeMap::new();
        for entry in &entries {
            for term in &entry.terms {
                *frequencies.entry(term.clone()).or_insert(0) += 1;
            }
        }
        (entries, frequencies)
    });
    let query_terms = queries.iter().map(|q| terms(q)).collect::<Vec<_>>();
    let mut ranked = cards()
        .iter()
        .enumerate()
        .filter(|(_, card)| {
            (kind.is_empty() || card.kind == kind) && (group.is_empty() || card.group == group)
        })
        .map(|(index, card)| {
            let entry = &INDEX.0[index];
            let score = query_terms
                .iter()
                .map(|query| {
                    query
                        .intersection(&entry.terms)
                        .map(|term| {
                            let idf = ((cards().len() + 1) as f64 / (INDEX.1[term] + 1) as f64)
                                .ln()
                                + 1.0;
                            idf * if entry.title.contains(term) { 2.0 } else { 1.0 }
                        })
                        .sum::<f64>()
                        / (query.len().max(1) as f64).sqrt()
                })
                .fold(0.0, f64::max);
            (card, score)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|(a, sa), (b, sb)| sb.total_cmp(sa).then_with(|| a.id.cmp(b.id)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(card, _)| card)
        .collect()
}

/// Front matter plus independent methods passages reduces bibliography and
/// long-introduction bias. The caller supplies its bounded document sample.
pub(super) fn document_queries(text: &str) -> Vec<String> {
    let mut queries = vec![text[..boundary(text, 12_000)].to_owned()];
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let heading = line.trim().to_lowercase();
        if heading.len() < 180
            && [
                "method",
                "empirical",
                "identification",
                "model",
                "data",
                "experiment",
                "proof",
                "estimation",
            ]
            .iter()
            .any(|word| heading.contains(word))
        {
            let end = boundary(text, offset + 6_000);
            queries.push(text[offset..end].to_owned());
            if queries.len() == 9 {
                break;
            }
        }
        offset += line.len();
    }
    // A compact stratified passage retains a route to late interdisciplinary
    // evidence even when the extractor did not recognize section headings.
    for fraction in [1, 2, 3] {
        let start = boundary(text, text.len() * fraction / 4);
        let end = boundary(text, start + 4_000);
        queries.push(text[start..end].to_owned());
    }
    queries
}
