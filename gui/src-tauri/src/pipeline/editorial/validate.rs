//! Exact anchoring and set-level checks. These establish text identity and
//! coherent application, not semantic equivalence (which remains an editorial
//! judgment). Offsets always refer to UTF-8 bytes in the captured readable text.
use super::{Anchor, Candidates, Coverage, Edit, Unresolved, Verification};
use std::collections::{HashMap, HashSet};
use std::ops::Range;

pub(super) fn anchor(source: &str, anchor: &Anchor) -> Result<Range<usize>, String> {
    if anchor.original.is_empty() {
        return Err("An editorial quotation cannot be empty".into());
    }
    let needle = format!("{}{}{}", anchor.prefix, anchor.original, anchor.suffix);
    let first = source
        .find(&needle)
        .ok_or("Editorial quotation/context does not match the readable source exactly")?;
    // Advance one character, rather than needle.len(): overlapping occurrences
    // are ambiguous too (for example `aaa` in `aaaa`).
    let next = first + source[first..].chars().next().unwrap().len_utf8();
    if source[next..].contains(&needle) {
        return Err("Editorial quotation is ambiguous; supply exact adjacent prefix/suffix".into());
    }
    let start = first + anchor.prefix.len();
    Ok(start..start + anchor.original.len())
}

pub(super) fn coverage(value: &Coverage) -> Result<(), String> {
    if !value.limitations.is_empty() && !value.incomplete {
        return Err("Editorial coverage with limitations must be marked incomplete".into());
    }
    if value.reviewed.is_empty() && !value.incomplete {
        return Err("No reviewed sections cannot be declared complete coverage".into());
    }
    Ok(())
}

fn unresolved(source: &str, values: &[Unresolved]) -> Result<(), String> {
    for item in values {
        if item.original.is_empty() || !source.contains(&item.original) {
            return Err("Unresolved wording must quote the readable source exactly".into());
        }
    }
    Ok(())
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

pub(super) fn interactions(source: &str, edits: &[Edit]) -> Result<Vec<Range<usize>>, String> {
    let ids: HashSet<_> = edits.iter().map(|edit| edit.id.as_str()).collect();
    if ids.len() != edits.len() {
        return Err("Editorial edit IDs must be unique".into());
    }
    let spans = edits
        .iter()
        .map(|edit| anchor(source, &edit.anchor))
        .collect::<Result<Vec<_>, _>>()?;
    for (index, edit) in edits.iter().enumerate() {
        for dependency in &edit.requires {
            if dependency == &edit.id || !ids.contains(dependency.as_str()) {
                return Err(format!("Edit '{}' requires an unavailable or self-referencing companion '{dependency}'", edit.id));
            }
        }
        if spans[..index]
            .iter()
            .any(|prior| overlaps(prior, &spans[index]))
        {
            return Err(format!(
                "Edit '{}' overlaps another edit; consolidate the spans",
                edit.id
            ));
        }
        if edit.operation == "delete" {
            if !edit.replacement.is_empty() || edit.deletion_basis == "not_deletion" {
                return Err(format!(
                    "Deletion '{}' needs an empty replacement and a deletion basis",
                    edit.id
                ));
            }
            if edit.deletion_basis == "redundant" && edit.retained_at.is_empty() {
                return Err(format!(
                    "Deletion '{}' must identify where its content survives",
                    edit.id
                ));
            }
        } else if edit.replacement.trim().is_empty() || edit.deletion_basis != "not_deletion" {
            return Err(format!(
                "Replacement '{}' needs nonempty text and not_deletion",
                edit.id
            ));
        }
        if edit.operation == "replace" && edit.replacement == edit.anchor.original {
            return Err(format!("Edit '{}' makes no change", edit.id));
        }
        for retained in &edit.retained_at {
            let retained_span = anchor(source, retained)?;
            if spans
                .iter()
                .any(|changed| overlaps(changed, &retained_span))
            {
                return Err(format!(
                    "Edit '{}' relies on retained content that another proposed edit changes",
                    edit.id
                ));
            }
        }
    }
    Ok(spans)
}

pub(super) fn candidates(
    source: &str,
    hash: &str,
    namespace: &str,
    referees: &[String],
    upstream: &[Candidates],
    value: &Candidates,
) -> Result<(), String> {
    if value.source_hash != hash {
        return Err("Editorial source_hash does not match the captured readable text".into());
    }
    coverage(&value.coverage)?;
    unresolved(source, &value.unresolved)?;
    let spans = interactions(source, &value.edits)?;
    let origins: HashMap<_, _> = upstream
        .iter()
        .flat_map(|set| &set.edits)
        .map(|edit| (edit.id.as_str(), edit))
        .collect();
    let mut used = HashSet::new();
    for (edit, span) in value.edits.iter().zip(spans) {
        if !edit.id.starts_with(&format!("{namespace}:")) || edit.id.len() > 256 {
            return Err(format!(
                "Edit IDs must start with '{namespace}:' and fit 256 bytes"
            ));
        }
        if edit.referee_ids.is_empty() || edit.referee_ids.iter().any(|id| !referees.contains(id)) {
            return Err(format!(
                "Edit '{}' must name selected subject editors only",
                edit.id
            ));
        }
        if value.stage == super::STAGES[0] {
            if !edit.source_ids.is_empty() {
                return Err("Initial edits must have empty source_ids".into());
            }
        } else {
            if edit.source_ids.is_empty() {
                return Err("Consolidated edits must retain producer source_ids".into());
            }
            for id in &edit.source_ids {
                let origin = origins
                    .get(id.as_str())
                    .ok_or_else(|| format!("Unknown original edit '{id}'"))?;
                if !used.insert(id) {
                    return Err(format!(
                        "Original edit '{id}' was consolidated more than once"
                    ));
                }
                let origin_span = anchor(source, &origin.anchor)?;
                if origin_span.start < span.start || origin_span.end > span.end {
                    return Err(format!(
                        "Consolidated edit '{}' must contain its cited original spans",
                        edit.id
                    ));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn verified(
    source: &str,
    hash: &str,
    candidates: &Candidates,
    value: &Verification,
) -> Result<Vec<Edit>, String> {
    if value.source_hash != hash || candidates.source_hash != hash {
        return Err(
            "Verification source_hash does not match the candidates and captured text".into(),
        );
    }
    coverage(&value.coverage)?;
    unresolved(source, &value.unresolved)?;
    let mut decisions = HashMap::new();
    for decision in &value.decisions {
        if decisions
            .insert(decision.candidate_id.as_str(), decision)
            .is_some()
        {
            return Err("Verification has duplicate candidate dispositions".into());
        }
    }
    if decisions.len() != candidates.edits.len() {
        return Err("Verification must give exactly one disposition for every candidate".into());
    }
    let mut accepted = Vec::new();
    for edit in &candidates.edits {
        let decision = decisions
            .get(edit.id.as_str())
            .ok_or_else(|| format!("Missing verification disposition for '{}'", edit.id))?;
        if decision.decision == "accept" {
            accepted.push(edit.clone());
        }
    }
    interactions(source, &accepted)?;
    Ok(accepted)
}

/// Apply exactly the selected edits to a copy, never the input file. Sorting
/// descending preserves all earlier byte offsets, including Unicode boundaries.
pub(super) fn apply(source: &str, edits: &[Edit]) -> Result<String, String> {
    let spans = interactions(source, edits)?;
    let mut ordered: Vec<_> = edits.iter().zip(spans).collect();
    ordered.sort_by_key(|(_, span)| std::cmp::Reverse(span.start));
    let total = edits
        .iter()
        .try_fold(source.len(), |size, edit| {
            size.checked_sub(edit.anchor.original.len())
                .and_then(|size| size.checked_add(edit.replacement.len()))
        })
        .ok_or("Combined editorial document size overflow")?;
    if total > crate::safety::MAX_EXPANDED_PROMPT_BYTES {
        return Err("Combined editorial document exceeds the text safety limit".into());
    }
    let mut result = source.to_string();
    for (edit, span) in ordered {
        result.replace_range(span, &edit.replacement);
    }
    Ok(result)
}
