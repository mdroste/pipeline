use super::claude::call_llm;
use crate::models::PipelineReport;
use crate::output::strip_to_report;
use tauri::AppHandle;

/// Produce a markdown diff between two reports on the same paper.
pub async fn reconcile(app: &AppHandle, prior: &PipelineReport, current: &PipelineReport) -> Result<String, String> {
    let prior_outputs = prior.all_outputs();
    let current_outputs = current.all_outputs();

    let prior_steps: String = prior_outputs
        .iter()
        .filter(|o| o.phase == "parallel")
        .map(|o| format!("## {}\n\n{}", o.step_label, o.raw_text))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let current_steps: String = current_outputs
        .iter()
        .filter(|o| o.phase == "parallel")
        .map(|o| format!("## {}\n\n{}", o.step_label, o.raw_text))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let prior_final = prior.final_output()
        .unwrap_or("(no consolidated output)");
    let current_final = current.final_output()
        .unwrap_or("(no consolidated output)");

    let prior_date = prior.report_date;
    let current_date = current.report_date;

    let prompt = format!(
        r#"You are comparing two referee reports on successive versions of the same paper.

PRIOR REPORT (from {prior_date}):

{prior_final}

{prior_steps}

CURRENT REPORT (from {current_date}):

{current_final}

{current_steps}

For each major concern raised in the PRIOR report, determine its status
in the CURRENT version:

- ADDRESSED: The concern has been resolved in the revision.
- STILL_PRESENT: The concern remains unresolved.
- UNCLEAR: Cannot determine from the current report.
- NEW: A concern that appears only in the current report (not in the prior).

Write a structured diff in markdown:

## Revision Diff: {prior_date} → {current_date}

### Addressed Concerns
1. [Prior concern description] — [How it was addressed]

### Remaining Concerns
1. [Prior concern description] — [Status and notes]

### New Concerns
1. [New concern from current report]

### Summary
[1-2 paragraph assessment: Did the revision make meaningful progress?
What is the most important remaining issue?]

OUTPUT FORMAT:
Begin your report with exactly `<!-- REPORT START -->` and end with exactly `<!-- REPORT END -->`.
Include ONLY your markdown report between those markers — no preamble, no commentary, no acknowledgments outside them."#
    );

    let timeout = crate::settings::load().step_timeout_secs.max(60);
    let raw = call_llm(app, &prompt, &[], None, "text", timeout, "Revision reconciliation", None, None).await?;
    Ok(strip_to_report(&raw))
}
