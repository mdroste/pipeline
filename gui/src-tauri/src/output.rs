//! Report rendering.

use crate::models::PipelineReport;
use crate::settings::Settings;
use regex::Regex;
use std::sync::LazyLock;
use std::time::Duration;

static PREAMBLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?mi)^(I'(?:ve|ll|m)|I (?:have|need|can|should|will)|Let me|Now (?:I|let)|OK[,.]|Alright[,.]|Here (?:is|are)|Looking at|After reading|Having read)[^\n]*\n*"
    ).expect("preamble regex is invalid")
});

/// Extract content between `<!-- REPORT START -->` and `<!-- REPORT END -->` markers.
/// Falls back to the full text (with preamble stripping) if markers aren't present.
pub fn strip_to_report(text: &str) -> String {
    let trimmed = text.trim();

    const START: &str = "<!-- REPORT START -->";
    const END: &str = "<!-- REPORT END -->";

    if let Some(start_pos) = trimmed.find(START) {
        let content_start = start_pos + START.len();
        if let Some(end_pos) = trimmed[content_start..].find(END) {
            return trimmed[content_start..content_start + end_pos].trim().to_string();
        }
        // START found but no END — take everything after START
        return trimmed[content_start..].trim().to_string();
    }

    // No markers — fall back to preamble stripping
    strip_preamble(trimmed)
}

/// Strip LLM chain-of-thought preamble from output.
/// Removes lines like "I've now read the full paper. Let me compile..."
/// that appear before the actual content.
fn strip_preamble(text: &str) -> String {
    let trimmed = text.trim();

    // Only strip from the beginning — don't touch mid-text occurrences.
    // Guard against infinite loops (e.g., zero-width matches).
    let mut result = trimmed;
    for _ in 0..50 {
        let stripped = result.trim_start();
        if let Some(m) = PREAMBLE_RE.find(stripped) {
            if m.start() == 0 && m.end() > 0 {
                result = &stripped[m.end()..];
                continue;
            }
        }
        break;
    }

    result.trim().to_string()
}

fn format_duration(d: Duration) -> String {
    format_secs(d.as_secs())
}

fn format_secs(total_secs: u64) -> String {
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    if mins > 0 {
        format!("{mins}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

/// Published API list price for a model, as (input, output) US dollars per
/// million tokens. Matched by substring so aliases ("opus") and full ids
/// ("claude-opus-4-8") both resolve. Deliberately coarse and approximate —
/// used only for a labelled *estimate* in the run summary, never billing.
/// Order matters: more specific names are checked before broad ones.
pub fn model_price(model: &str) -> Option<(f64, f64)> {
    let m = model.to_ascii_lowercase();
    // (needle, input $/Mtok, output $/Mtok)
    const TABLE: &[(&str, f64, f64)] = &[
        ("haiku", 0.80, 4.0),
        ("sonnet", 3.0, 15.0),
        ("opus", 15.0, 75.0),
        ("o4-mini", 1.10, 4.40),
        ("o3-mini", 1.10, 4.40),
        ("o3", 2.0, 8.0),
        ("gpt-4.1-mini", 0.40, 1.60),
        ("gpt-4.1", 2.0, 8.0),
        ("gpt-4o-mini", 0.15, 0.60),
        ("gpt-4o", 2.50, 10.0),
        ("gemini-2.5-flash", 0.30, 2.50),
        ("gemini-2.5-pro", 1.25, 10.0),
        ("gemini-1.5-flash", 0.075, 0.30),
        ("gemini-1.5-pro", 1.25, 5.0),
    ];
    TABLE
        .iter()
        .find(|(needle, _, _)| m.contains(needle))
        .map(|(_, i, o)| (*i, *o))
}

/// Whether a provider ran through a direct API (metered, priceable) rather than
/// a subscription CLI. Cost estimates are only meaningful in API mode; on a
/// subscription plan the run is billed by the plan, not per token.
fn provider_in_api_mode(settings: &Settings, provider: &str) -> bool {
    match provider {
        "codex" => !settings.openai_api_key.trim().is_empty(),
        "gemini" => !settings.google_api_key.trim().is_empty(),
        "local" => true,
        // "claude" and the empty/default provider both map to Anthropic.
        _ => !settings.anthropic_api_key.trim().is_empty(),
    }
}

/// Estimated cost of a call at list prices, or None when the model is unknown.
fn estimate_cost(model: &str, input_tokens: u64, output_tokens: u64) -> Option<f64> {
    let (pin, pout) = model_price(model)?;
    Some((input_tokens as f64 / 1_000_000.0) * pin + (output_tokens as f64 / 1_000_000.0) * pout)
}

fn format_cost(cost: f64) -> String {
    if cost >= 1.0 {
        format!("${cost:.2}")
    } else {
        format!("${cost:.4}")
    }
}

/// Compact token count for the summary table: 1_234_567 → "1.2M".
fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// A per-step run summary table (time, model, tokens, estimated cost), or None
/// when there is nothing worth showing (e.g. a legacy report with no metrics).
fn render_run_summary(report: &PipelineReport, settings: &Settings) -> Option<String> {
    let outputs = report.all_outputs();
    let has_metrics = outputs
        .iter()
        .any(|o| o.duration_secs > 0 || o.input_tokens > 0 || o.output_tokens > 0);
    if !has_metrics {
        return None;
    }

    let mut total_in = 0u64;
    let mut total_out = 0u64;
    let mut total_cost = 0f64;
    let mut any_cost = false;

    let mut rows = String::new();
    for o in &outputs {
        total_in += o.input_tokens;
        total_out += o.output_tokens;

        let model = if o.model.trim().is_empty() { "default".to_string() } else { o.model.clone() };
        let provider = if o.provider.trim().is_empty() { "default".to_string() } else { capitalize(&o.provider) };
        let tokens = if o.input_tokens == 0 && o.output_tokens == 0 {
            "—".to_string()
        } else {
            format!("{} / {}", fmt_tokens(o.input_tokens), fmt_tokens(o.output_tokens))
        };
        let cost_cell = if provider_in_api_mode(settings, &o.provider) {
            match estimate_cost(&o.model, o.input_tokens, o.output_tokens) {
                Some(c) => {
                    total_cost += c;
                    any_cost = true;
                    format_cost(c)
                }
                None => "—".to_string(),
            }
        } else {
            "—".to_string()
        };
        rows.push_str(&format!(
            "| {} | {} · {} | {} | {} | {} |\n",
            o.step_label,
            provider,
            model,
            format_secs(o.duration_secs),
            tokens,
            cost_cell,
        ));
    }

    let total_tokens = if total_in == 0 && total_out == 0 {
        "—".to_string()
    } else {
        format!("{} / {}", fmt_tokens(total_in), fmt_tokens(total_out))
    };
    let total_cost_cell = if any_cost { format_cost(total_cost) } else { "—".to_string() };

    let mut md = String::new();
    md.push_str("## Run summary\n\n");
    md.push_str("| Step | Provider · Model | Time | Tokens (in / out) | Est. cost |\n");
    md.push_str("|------|------------------|------|-------------------|-----------|\n");
    md.push_str(&rows);
    md.push_str(&format!(
        "| **Total** | | | **{total_tokens}** | **{total_cost_cell}** |\n\n"
    ));
    if any_cost {
        md.push_str(
            "*Cost is an estimate at published API list prices for steps run through a direct API. \
             It does not reflect subscription-plan billing.*\n\n",
        );
    }
    Some(md)
}

/// Render a PipelineReport to a markdown string.
pub fn render_markdown(report: &PipelineReport, diff_text: Option<&str>, elapsed: Duration, settings: &Settings) -> String {
    // Paper-shaped surveys get title/authors/type headers; custom surveys
    // get a generic header.
    let meta = crate::models::paper_view(&report.orientation).map(|v| v.metadata);
    let mut md = String::new();

    // Title
    let title = meta
        .as_ref()
        .map(|m| m.title.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Report".to_string());
    md.push_str(&format!("# {title}\n\n"));

    // Metadata — single compact paragraph with line breaks
    if let Some(m) = &meta {
        if !m.authors.is_empty() {
            md.push_str(&format!("**Authors**: {}  \n", m.authors.join(", ")));
        }
    }

    let provider = capitalize(&settings.preferred_provider);
    let model = match settings.preferred_provider.as_str() {
        "codex" => if settings.codex_model.is_empty() { "default".to_string() } else { settings.codex_model.clone() },
        "gemini" => if settings.gemini_model.is_empty() { "default".to_string() } else { settings.gemini_model.clone() },
        _ => if settings.claude_model.is_empty() { "default".to_string() } else { settings.claude_model.clone() },
    };
    let effort = match settings.preferred_provider.as_str() {
        "codex" => if settings.codex_effort.is_empty() { "default".to_string() } else { settings.codex_effort.clone() },
        "gemini" => "n/a".to_string(),
        _ => if settings.claude_effort.is_empty() { "default".to_string() } else { settings.claude_effort.clone() },
    };

    let type_prefix = meta
        .as_ref()
        .map(|m| format!("**Type**: {} · ", m.paper_type))
        .unwrap_or_default();
    md.push_str(&format!(
        "{}**Reviewed**: {} · `{}`  \n**LLM**: {} · **Model**: {} · **Effort**: {} · **Generated in**: {}  \n*Report generated by Pipeline*\n\n",
        type_prefix, report.report_date, &report.paper_hash,
        provider, model, effort, format_duration(elapsed)
    ));

    // Warning for failed steps
    if !report.failed_steps.is_empty() {
        let labels: Vec<&str> = report.failed_steps.iter().map(|f| f.step_label.as_str()).collect();
        md.push_str(&format!(
            "> **Incomplete report.** The following steps failed and are not reflected below: {}.\n\n",
            labels.join(", ")
        ));
    }

    md.push_str("---\n\n");

    // Consolidated issues — use final_output() which handles both new and legacy formats
    if let Some(final_text) = report.final_output() {
        md.push_str(&strip_to_report(final_text));
        md.push_str("\n\n");
    }

    // Revision diff
    if let Some(diff) = diff_text {
        md.push_str("---\n\n");
        md.push_str("## Revision Diff\n\n");
        md.push_str(&strip_to_report(diff));
        md.push_str("\n\n");
    }

    // Per-step timing / tokens / cost, when the run recorded any.
    if let Some(summary) = render_run_summary(report, settings) {
        md.push_str("---\n\n");
        md.push_str(&summary);
    }

    md.push_str("---\n");
    md
}

pub(crate) fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            let rest: String = chars.collect();
            upper + &rest
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── strip_to_report ────────────────────────────────────────────

    #[test]
    fn strip_to_report_extracts_between_markers() {
        let input = "preamble\n<!-- REPORT START -->\nActual report.\n<!-- REPORT END -->\ntrailing";
        assert_eq!(strip_to_report(input), "Actual report.");
    }

    #[test]
    fn strip_to_report_start_without_end() {
        let input = "<!-- REPORT START -->\nContent without end marker";
        assert_eq!(strip_to_report(input), "Content without end marker");
    }

    #[test]
    fn strip_to_report_no_markers_strips_preamble() {
        let input = "I've now read the full paper.\nActual content here.";
        assert_eq!(strip_to_report(input), "Actual content here.");
    }

    #[test]
    fn strip_to_report_no_markers_no_preamble() {
        let input = "## Issues\n\n1. First issue.";
        assert_eq!(strip_to_report(input), "## Issues\n\n1. First issue.");
    }

    #[test]
    fn strip_to_report_empty() {
        assert_eq!(strip_to_report(""), "");
        assert_eq!(strip_to_report("   "), "");
    }

    // ── strip_preamble ─────────────────────────────────────────────

    #[test]
    fn strip_preamble_removes_chain_of_thought() {
        let input = "Let me analyze this paper.\nI'll focus on methodology.\n## Real Content";
        assert_eq!(strip_preamble(input), "## Real Content");
    }

    #[test]
    fn strip_preamble_preserves_clean_text() {
        let input = "## Issues\n\n1. The regression specification...";
        assert_eq!(strip_preamble(input), input);
    }

    #[test]
    fn strip_preamble_empty() {
        assert_eq!(strip_preamble(""), "");
    }

    // ── format_duration ────────────────────────────────────────────

    #[test]
    fn format_duration_seconds_only() {
        assert_eq!(format_duration(Duration::from_secs(45)), "45s");
    }

    #[test]
    fn format_duration_minutes_and_seconds() {
        assert_eq!(format_duration(Duration::from_secs(125)), "2m 5s");
    }

    #[test]
    fn format_duration_zero() {
        assert_eq!(format_duration(Duration::from_secs(0)), "0s");
    }

    #[test]
    fn format_duration_exact_minute() {
        assert_eq!(format_duration(Duration::from_secs(60)), "1m 0s");
    }

    // ── capitalize ─────────────────────────────────────────────────

    #[test]
    fn capitalize_normal() {
        assert_eq!(capitalize("claude"), "Claude");
    }

    #[test]
    fn capitalize_empty() {
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn capitalize_already_upper() {
        assert_eq!(capitalize("Claude"), "Claude");
    }

    #[test]
    fn capitalize_single_char() {
        assert_eq!(capitalize("c"), "C");
    }

    #[test]
    fn capitalize_multibyte() {
        assert_eq!(capitalize("über"), "Über");
    }

    // ── pricing / run summary ──────────────────────────────────────

    #[test]
    fn model_price_matches_aliases_and_ids() {
        assert_eq!(model_price("opus"), Some((15.0, 75.0)));
        assert_eq!(model_price("claude-opus-4-8"), Some((15.0, 75.0)));
        assert_eq!(model_price("sonnet"), Some((3.0, 15.0)));
        assert_eq!(model_price("gemini-2.5-flash"), Some((0.30, 2.50)));
        // Specific-before-broad: the mini variant must not match plain gpt-4.1.
        assert_eq!(model_price("gpt-4.1-mini"), Some((0.40, 1.60)));
        assert_eq!(model_price("gpt-4.1"), Some((2.0, 8.0)));
        assert_eq!(model_price("some-unknown-model"), None);
        assert_eq!(model_price(""), None);
    }

    #[test]
    fn estimate_cost_computes_from_tokens() {
        // 1M input + 1M output on opus = 15 + 75 = 90.
        let c = estimate_cost("opus", 1_000_000, 1_000_000).unwrap();
        assert!((c - 90.0).abs() < 1e-9);
        assert!(estimate_cost("unknown", 100, 100).is_none());
    }

    #[test]
    fn format_cost_precision() {
        assert_eq!(format_cost(2.5), "$2.50");
        assert_eq!(format_cost(0.0123), "$0.0123");
    }

    #[test]
    fn provider_api_mode_follows_keys() {
        let mut s = Settings::default();
        assert!(!provider_in_api_mode(&s, "claude"));
        assert!(provider_in_api_mode(&s, "local"));
        s.anthropic_api_key = "sk-x".into();
        assert!(provider_in_api_mode(&s, "claude"));
        assert!(provider_in_api_mode(&s, "")); // empty provider = Anthropic
        assert!(!provider_in_api_mode(&s, "gemini"));
    }

    fn metric_output(label: &str, model: &str, provider: &str, secs: u64, tin: u64, tout: u64) -> crate::models::StepOutput {
        crate::models::StepOutput {
            step_id: label.into(),
            step_label: label.into(),
            phase: "parallel".into(),
            provider: provider.into(),
            agent: provider.into(),
            raw_text: String::new(),
            duration_secs: secs,
            input_tokens: tin,
            output_tokens: tout,
            model: model.into(),
            ..Default::default()
        }
    }

    fn report_with(outputs: Vec<crate::models::StepOutput>) -> PipelineReport {
        PipelineReport {
            orientation: serde_json::json!({}),
            step_outputs: outputs,
            failed_steps: vec![],
            referee_reports: vec![],
            editor: None,
            report_date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            paper_hash: "hash".into(),
        }
    }

    #[test]
    fn run_summary_absent_without_metrics() {
        let report = report_with(vec![metric_output("Step", "opus", "claude", 0, 0, 0)]);
        assert!(render_run_summary(&report, &Settings::default()).is_none());
    }

    #[test]
    fn run_summary_shows_tokens_and_gates_cost_on_api_mode() {
        let report = report_with(vec![metric_output("Technical", "opus", "claude", 90, 500_000, 100_000)]);

        // Subscription mode (no key): tokens shown, cost withheld.
        let sub = render_run_summary(&report, &Settings::default()).unwrap();
        assert!(sub.contains("Run summary"));
        assert!(sub.contains("Technical"));
        assert!(sub.contains("500.0k / 100.0k"));
        assert!(sub.contains("1m 30s"));
        assert!(!sub.contains("$"));

        // API mode: cost estimated (0.5*15 + 0.1*75 = 7.5 + 7.5 = 15.00).
        let mut s = Settings::default();
        s.anthropic_api_key = "sk-x".into();
        let api = render_run_summary(&report, &s).unwrap();
        assert!(api.contains("$15.00"));
        assert!(api.contains("list prices"));
    }
}
