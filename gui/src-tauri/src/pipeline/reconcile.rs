use super::call::{execute_text, OwnedRequest};
use crate::models::PipelineReport;
use crate::output::{
    extract_report_envelope, new_report_nonce, normalize_math_delimiters, report_output_format,
};

/// Produce a markdown diff between two reports on the same paper.
pub async fn reconcile(
    app: &crate::emit::EventBus,
    prior: &PipelineReport,
    current: &PipelineReport,
) -> Result<String, String> {
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

    let prior_final = prior.final_output().unwrap_or("(no consolidated output)");
    let current_final = current.final_output().unwrap_or("(no consolidated output)");

    let prior_date = prior.report_date;
    let current_date = current.report_date;

    let mut prompt = format!(
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
What is the most important remaining issue?]"#
    );
    let report_nonce = new_report_nonce()?;
    crate::safety::push_str_limited(
        &mut prompt,
        "\n\n",
        crate::safety::MAX_EXPANDED_PROMPT_BYTES,
        "Reconciliation prompt",
    )?;
    crate::safety::push_str_limited(
        &mut prompt,
        &report_output_format(None, &report_nonce),
        crate::safety::MAX_EXPANDED_PROMPT_BYTES,
        "Reconciliation prompt",
    )?;

    let settings = crate::settings::load();
    let timeout = settings.step_timeout_secs.max(60);
    let mut last_error = String::new();
    for attempt in 0..=settings.max_retries {
        let mut attempt_prompt = prompt.clone();
        if attempt > 0 {
            crate::safety::push_str_limited(
                &mut attempt_prompt,
                &format!(
                    "\n\nRETRY NOTICE:\nThe previous response was rejected: {last_error}\n\
                     Return the complete diff again and obey the OUTPUT FORMAT contract exactly."
                ),
                crate::safety::MAX_EXPANDED_PROMPT_BYTES,
                "Reconciliation retry prompt",
            )?;
        }
        let mut request = OwnedRequest::new(
            app,
            "reconciliation",
            "Revision reconciliation",
            attempt_prompt,
            timeout,
        );
        request.settings = std::sync::Arc::new(settings.clone());
        let raw = match execute_text(request).await {
            Ok(raw) => raw,
            Err(error) if error.to_ascii_lowercase().contains("cancel") => return Err(error),
            Err(error) if super::provider_error::is_usage_limit_error(&error) => return Err(error),
            Err(error) => {
                last_error = error;
                continue;
            }
        };
        match extract_report_envelope(&raw, &report_nonce) {
            Ok(report) => return Ok(normalize_math_delimiters(&report)),
            Err(error) => {
                last_error = format!("Revision reconciliation returned an invalid report: {error}");
            }
        }
    }
    Err(if last_error.is_empty() {
        "Revision reconciliation produced no report".to_string()
    } else {
        last_error
    })
}
