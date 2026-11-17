//! Deterministic publication of accepted edits. Literal fences are always
//! top-level and longer than every backtick run in the payload, so Markdown
//! rendering and math normalization cannot reinterpret exact source strings.
use super::{Context, Edit, Verification};

fn literal(text: &str) -> String {
    let longest = text
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}text\n{text}\n{fence}\n\n")
}

// Explanatory prose is escaped for display; exact source payloads use literal.
fn prose(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut escaped = String::new();
    for character in flat.chars() {
        if "\\`*_{}[]<>#!|$".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped.push_str("\n\n");
    escaped
}

pub(super) fn report(
    context: &Context,
    verification: &Verification,
    accepted: &[Edit],
) -> Result<String, String> {
    let mut report = String::from("# Declaude review\n\n");
    for (label, value) in [
        (
            "Document",
            context
                .brief
                .pointer("/document/type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown"),
        ),
        (
            "Audience",
            context
                .brief
                .pointer("/document/audience")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown"),
        ),
        (
            "Style target",
            context.brief["style_target"]
                .as_str()
                .unwrap_or("unspecified"),
        ),
    ] {
        report.push_str(&format!("**{label}**\n\n{}", prose(value)));
    }
    report.push_str("Original passages are matched exactly to the captured readable text. Check extraction uncertainties against the original document.\n\n");
    report.push_str(&format!("Readable-text SHA-256: `{}`\n\n", context.hash));
    let mut limitations = context.warnings.clone();
    limitations.extend(verification.coverage.limitations.clone());
    limitations.sort();
    limitations.dedup();
    report.push_str(
        if verification.coverage.incomplete || !limitations.is_empty() {
            "**Coverage: Partial**\n\n"
        } else {
            "**Coverage reported by the editors**\n\n"
        },
    );
    report.push_str(&prose(&verification.coverage.reviewed.join("; ")));
    for limitation in limitations {
        report.push_str(&prose(&limitation));
    }
    report.push_str("## Recommended edits\n\n");
    let mut ordered: Vec<_> = accepted
        .iter()
        .map(|edit| {
            Ok((
                edit,
                super::validate::anchor(&context.source, &edit.anchor)?,
            ))
        })
        .collect::<Result<_, String>>()?;
    ordered.sort_by_key(|(_, span)| span.start);
    if ordered.is_empty() {
        report.push_str("No edits met the review standard within the material reviewed.\n\n");
    }
    for (index, (edit, span)) in ordered.iter().enumerate() {
        report.push_str(&format!(
            "### {}. {}\n\n",
            index + 1,
            if edit.priority == "recommended" {
                "Recommended edit"
            } else {
                "Optional edit"
            }
        ));
        report.push_str(&format!(
            "**Location**\n\n{}Readable-text byte range: {}–{} (end excluded).\n\n",
            prose(&edit.location),
            span.start,
            span.end
        ));
        report.push_str("**Original**\n\n");
        report.push_str(&literal(&edit.anchor.original));
        if edit.operation == "delete" {
            report.push_str("**Action: Delete**\n\n");
        } else {
            report.push_str("**Replacement**\n\n");
            report.push_str(&literal(&edit.replacement));
        }
        let decision = verification
            .decisions
            .iter()
            .find(|decision| decision.candidate_id == edit.id)
            .ok_or("Accepted edit has no disposition")?;
        report.push_str("**Reason**\n\n");
        report.push_str(&prose(&edit.reason));
        report.push_str("**Verification**\n\n");
        report.push_str(&prose(&decision.reason));
        if !edit.requires.is_empty() {
            let companions: Vec<_> = ordered
                .iter()
                .enumerate()
                .filter(|(_, (companion, _))| edit.requires.contains(&companion.id))
                .map(|(number, _)| (number + 1).to_string())
                .collect();
            report.push_str(&format!(
                "**Accept together with edits:** {}.\n\n",
                companions.join(", ")
            ));
        }
        for retained in &edit.retained_at {
            report.push_str("**Content retained elsewhere**\n\n");
            report.push_str(&literal(&retained.original));
        }
    }
    let unresolved = context
        .upstream
        .iter()
        .flat_map(|set| &set.unresolved)
        .chain(&verification.unresolved);
    let mut seen = std::collections::HashSet::new();
    for issue in unresolved {
        if seen.insert((&issue.location, &issue.original, &issue.issue)) {
            if seen.len() == 1 {
                report.push_str("## Unresolved wording\n\n");
            }
            for text in [&issue.location, &issue.original, &issue.issue] {
                report.push_str(&literal(text));
            }
        }
    }
    if report.len() > crate::safety::MAX_EXPANDED_PROMPT_BYTES {
        return Err("Editorial report exceeds the text safety limit".into());
    }
    Ok(report)
}
