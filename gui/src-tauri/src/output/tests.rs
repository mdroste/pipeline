use super::*;

#[test]
fn nonce_report_envelope_accepts_two_unambiguous_boundaries() {
    let nonce = "abc123";
    let (start, end) = report_markers(nonce);
    let clean = format!("{start}\n## Report\n\nClean.\n{end}");
    assert_eq!(
        extract_report_envelope(&clean, nonce).unwrap(),
        "## Report\n\nClean."
    );

    assert_eq!(
        extract_report_envelope(&format!("Here is the report.\n{clean}\nDone."), nonce).unwrap(),
        "## Report\n\nClean."
    );
    assert_eq!(
        extract_report_envelope(
            &format!("{start}\n## Report\n\nRepeated opening label.\n{start}"),
            nonce
        )
        .unwrap(),
        "## Report\n\nRepeated opening label."
    );
    assert_eq!(
        extract_report_envelope(
            &format!("{end}\n## Report\n\nSwapped labels.\n{start}"),
            nonce
        )
        .unwrap(),
        "## Report\n\nSwapped labels."
    );

    assert!(extract_report_envelope(&format!("{start}\npartial"), nonce).is_err());
    assert!(extract_report_envelope(&clean, "different").is_err());
    assert!(extract_report_envelope(&format!("{clean}\n{clean}"), nonce).is_err());
    assert!(extract_report_envelope(&format!("{start}\n{end}"), nonce).is_err());
}

#[test]
fn nonce_report_envelope_rejects_apparent_refusals_but_not_methodological_limits() {
    let nonce = "abc123";
    let (start, end) = report_markers(nonce);
    let refusal = format!(
        "{start}\n# Response\n\nI'm sorry, but I can’t provide the requested report.\n{end}"
    );
    let error = extract_report_envelope(&refusal, nonce).unwrap_err();
    assert_eq!(error.kind(), ReportRejectionKind::Content);
    assert!(error.to_string().contains("apparent refusal"));

    let valid = format!(
        "{start}\n## Data limitation\n\nI cannot verify the reported standard errors from the material supplied.\n{end}"
    );
    assert!(extract_report_envelope(&valid, nonce).is_ok());
}

#[test]
fn report_nonces_are_fresh_and_fixed_width() {
    let first = new_report_nonce().unwrap();
    let second = new_report_nonce().unwrap();
    assert_eq!(first.len(), 32);
    assert_eq!(second.len(), 32);
    assert_ne!(first, second);
}

#[test]
fn math_delimiters_are_normalized_outside_code() {
    let input = "Inline \\(x+1\\).\n\n\\[\ny=2\n\\]\n\n`\\(code\\)`\n\n```tex\n\\[z\\]\n```\n";
    let expected = "Inline $x+1$.\n\n$$\ny=2\n$$\n\n`\\(code\\)`\n\n```tex\n\\[z\\]\n```\n";
    assert_eq!(normalize_math_delimiters(input), expected);
}

#[test]
fn latex_display_environments_are_normalized_outside_code() {
    let input = "\\begin{equation}\ny=x+1\n\\end{equation}\n\n\\begin{align*}\na&=b\\\\\nc&=d\n\\end{align*}\n\n```tex\n\\begin{equation}\nx\n\\end{equation}\n```\n";
    let expected = "$$\ny=x+1\n$$\n\n$$\n\\begin{aligned}\na&=b\\\\\nc&=d\n\\end{aligned}\n$$\n\n```tex\n\\begin{equation}\nx\n\\end{equation}\n```\n";
    assert_eq!(normalize_math_delimiters(input), expected);
}

#[test]
fn clean_export_removes_run_details_but_retains_report_footer() {
    let markdown = format!(
            "# Report\n\n{RUN_DETAILS_START}\nModel metadata\n{RUN_DETAILS_END}\n\nBody\n\n{RUN_DETAILS_START}\n---\n\n## Run summary\n\nTelemetry\n{RUN_DETAILS_END}\n\n---\n"
        );
    let clean = clean_export_markdown(&markdown);
    assert_eq!(clean, "# Report\n\nBody\n\n---\n");
}

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
fn strip_preamble_handles_curly_apostrophes() {
    let input =
        "I’ll read the instructions first.\nI’m using the configured voice.\n## Real Content";
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
    let c = estimate_cost("opus", 1_000_000, 1_000_000, 0, 0).unwrap();
    assert!((c - 90.0).abs() < 1e-9);
    assert!(estimate_cost("unknown", 100, 100, 0, 0).is_none());
}

#[test]
fn estimate_cost_partitions_logical_input_at_cache_rates() {
    // Observed Full Review usage: 8.265M logical input already contains
    // 7.574M cache reads. At GPT-5.6-sol base-tier rates this is ~$10.04,
    // rather than charging the full logical total at the $5 fresh rate.
    let cost = estimate_cost("gpt-5.6-sol", 8_265_469, 93_252, 7_573_504, 0).unwrap();
    assert!((cost - 10.044_137).abs() < 1e-9);
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
    assert!(!provider_in_api_mode(&s, "antigravity"));
}

fn metric_output(
    label: &str,
    model: &str,
    provider: &str,
    secs: u64,
    tin: u64,
    tout: u64,
) -> crate::models::StepOutput {
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
fn failed_synthesis_is_not_replaced_by_a_parallel_analysis() {
    let mut parallel = metric_output("Technical", "opus", "claude", 0, 0, 0);
    parallel.raw_text = "This is one specialist analysis.".to_string();
    let mut report = report_with(vec![parallel]);
    report.failed_steps.push(crate::models::StepFailure {
        step_id: "synthesis".to_string(),
        step_label: "Consolidate issues".to_string(),
        phase: "sequential".to_string(),
        error: "response contained an apparent refusal".to_string(),
    });

    let markdown = render_markdown(&report, None, Duration::from_secs(1), &Settings::default());
    assert!(markdown.contains("## Final report unavailable"));
    assert!(markdown.contains("has not"));
    assert!(!markdown.contains("This is one specialist analysis."));
}

#[test]
fn run_summary_absent_without_metrics() {
    let report = report_with(vec![metric_output("Step", "opus", "claude", 0, 0, 0)]);
    assert!(render_run_summary(&report, &Settings::default()).is_none());
}

#[test]
fn run_summary_labels_api_and_cli_cost_estimates() {
    let report = report_with(vec![metric_output(
        "Technical",
        "opus",
        "claude",
        90,
        500_000,
        100_000,
    )]);

    // Subscription mode: tokens are shown with an API-equivalent cost,
    // explicitly distinguished from an amount charged to the plan.
    let sub = render_run_summary(&report, &Settings::default()).unwrap();
    assert!(sub.contains("Run summary"));
    assert!(sub.contains("Technical"));
    assert!(sub.contains("500.0k logical in = 500.0k fresh · 100.0k out"));
    assert!(sub.contains("1m 30s"));
    assert!(sub.contains("$15.00"));
    assert!(sub.contains("API-equivalent list-price estimate"));

    // API mode: cost estimated (0.5*15 + 0.1*75 = 7.5 + 7.5 = 15.00).
    let s = Settings {
        anthropic_api_key: "sk-x".into(),
        ..Default::default()
    };
    let api = render_run_summary(&report, &s).unwrap();
    assert!(api.contains("$15.00"));
    assert!(api.contains("direct API calls"));
}

#[test]
fn run_summary_distinguishes_cached_and_warmed_input() {
    let mut output = metric_output(
        "Technical",
        "claude-sonnet-4-6",
        "claude",
        30,
        50_000,
        2_000,
    );
    output.cached_input_tokens = 40_000;
    output.cache_write_input_tokens = 8_000;
    output.model_round_trips = 6;
    output.tool_calls = crate::models::ToolCallCounts {
        text_file: 2,
        image: 1,
        web: 3,
        ..Default::default()
    };
    let summary = render_run_summary(&report_with(vec![output]), &Settings::default()).unwrap();
    assert!(summary.contains(
        "50.0k logical in = 2.0k fresh + 40.0k cache read + 8.0k cache write · 2.0k out"
    ));
    assert!(summary.contains("6 model rounds · 6 tools (2 text/file, 1 image, 3 web)"));
    assert!(summary.contains("only when the provider or CLI reports them"));
    assert!(summary.contains("Cache reads are discounted, not free"));
    assert!(summary.contains("$0.0780"));
}
