use super::*;
use crate::models::StepCallRecord;
use crate::pipeline_config::StepConfig;

fn step(id: &str, phase: Phase, agents: &[&str]) -> StepConfig {
    StepConfig {
        id: id.into(),
        label: id.into(),
        phase,
        enabled: true,
        agents: agents.iter().map(|agent| agent.to_string()).collect(),
        ..Default::default()
    }
}

fn config(steps: Vec<StepConfig>) -> PipelineConfig {
    PipelineConfig {
        steps,
        merge: Default::default(),
        outputs: Default::default(),
        context_cache: Default::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: Default::default(),
        parallel_context_template: String::new(),
        variables: Vec::new(),
    }
}

fn api_output(input_tokens: u64, output_tokens: u64) -> StepOutput {
    StepOutput {
        step_id: "s".into(),
        calls: vec![StepCallRecord {
            provider: "claude".into(),
            model: "claude-sonnet-test".into(),
            model_transport: "api".into(),
            input_tokens,
            output_tokens,
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn subscription_output() -> StepOutput {
    StepOutput {
        step_id: "s".into(),
        calls: vec![StepCallRecord {
            provider: "codex".into(),
            model: "gpt-5.6-sol".into(),
            model_transport: "cli".into(),
            input_tokens: 5_000_000,
            output_tokens: 1_000_000,
            ..Default::default()
        }],
        ..Default::default()
    }
}

#[test]
fn zero_and_invalid_settings_mean_no_limit() {
    let mut settings = Settings::default();
    assert!(RunBudget::from_settings(&settings).is_unlimited());
    settings.review_max_cost_usd = f64::NAN;
    assert!(RunBudget::from_settings(&settings).is_unlimited());
    settings.review_max_cost_usd = -3.0;
    settings.review_max_calls = 12;
    let budget = RunBudget::from_settings(&settings);
    assert_eq!(budget.max_cost_usd, None);
    assert_eq!(budget.max_calls, Some(12));
}

#[test]
fn cost_limit_counts_only_direct_api_calls() {
    let settings = Settings {
        review_max_cost_usd: 1.0,
        ..Default::default()
    };
    let mut ledger = BudgetLedger::new(&settings, BudgetStart::now());
    ledger.record(&[subscription_output()], &settings);
    assert_eq!(
        ledger.exhausted(),
        None,
        "subscription calls are not metered"
    );
    // Sonnet-class list price: $3/M in, $15/M out -> 0.3 + 0.75 = $1.05.
    ledger.record(&[api_output(100_000, 50_000)], &settings);
    let reason = ledger.exhausted().expect("cost limit reached");
    assert!(reason.starts_with("cost limit of $1.00"), "{reason}");
}

#[test]
fn call_limit_counts_every_call_including_subscriptions() {
    let settings = Settings {
        review_max_calls: 2,
        ..Default::default()
    };
    let mut ledger = BudgetLedger::new(&settings, BudgetStart::now());
    ledger.record(&[subscription_output()], &settings);
    assert_eq!(ledger.exhausted(), None);
    let skipped = StepOutput {
        skipped: true,
        ..Default::default()
    };
    ledger.record(&[skipped], &settings);
    assert_eq!(ledger.exhausted(), None, "skipped steps make no calls");
    ledger.record(&[subscription_output()], &settings);
    assert_eq!(
        ledger.exhausted().as_deref(),
        Some("limit of 2 model calls (2 made)")
    );
}

#[test]
fn call_limit_counts_attempts_that_leave_no_output() {
    let settings = Settings {
        review_max_calls: 5,
        ..Default::default()
    };
    let mut ledger = BudgetLedger::new(&settings, BudgetStart::now());
    // Orientation plus a unit that failed all three attempts.
    ledger.observe(&CallUsage {
        provider_attempts: 4,
        ..Default::default()
    });
    assert_eq!(ledger.exhausted(), None);
    ledger.record(&[subscription_output()], &settings);
    ledger.observe(&CallUsage {
        provider_attempts: 5,
        ..Default::default()
    });
    assert_eq!(
        ledger.exhausted().as_deref(),
        Some("limit of 5 model calls (5 made)")
    );
}

#[test]
fn cost_limit_counts_calls_of_failed_units() {
    let settings = Settings {
        review_max_cost_usd: 1.0,
        ..Default::default()
    };
    let mut ledger = BudgetLedger::new(&settings, BudgetStart::now());
    ledger.record_failed_calls(&subscription_output().calls, &settings);
    assert_eq!(ledger.exhausted(), None);
    // Three failed Sonnet-class attempts of 35k in / 17k out: about $1.08.
    let failed = StepCallRecord {
        model: "claude-sonnet-test".into(),
        model_transport: "api".into(),
        input_tokens: 105_000,
        output_tokens: 51_000,
        attempt_count: 3,
        ..Default::default()
    };
    ledger.record_failed_calls(&[failed], &settings);
    let reason = ledger.exhausted().expect("failed attempts are paid for");
    assert!(reason.starts_with("cost limit of $1.00"), "{reason}");
}

#[test]
fn limits_count_spend_and_time_before_the_first_wave() {
    let settings = Settings {
        review_max_cost_usd: 1.0,
        ..Default::default()
    };
    let orientation = BudgetStart {
        started: Instant::now(),
        api_cost_usd: 1.25,
    };
    let reason = BudgetLedger::new(&settings, orientation)
        .exhausted()
        .expect("orientation spend counts");
    assert!(reason.contains("spend so far: $1.25"), "{reason}");

    let settings = Settings {
        review_max_minutes: 1,
        ..Default::default()
    };
    let Some(started) = Instant::now().checked_sub(std::time::Duration::from_secs(61)) else {
        return;
    };
    let ledger = BudgetLedger::new(
        &settings,
        BudgetStart {
            started,
            api_cost_usd: 0.0,
        },
    );
    assert_eq!(
        ledger.exhausted().as_deref(),
        Some("time limit of 1 minutes")
    );
}

#[test]
fn run_start_is_scoped_to_the_run() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let Some(started) = Instant::now().checked_sub(std::time::Duration::from_secs(5)) else {
        return;
    };
    let start = BudgetStart {
        started,
        api_cost_usd: 0.5,
    };
    let seen = runtime.block_on(counted_from(start, async { run_start() }));
    assert_eq!(seen.started, started);
    assert_eq!(seen.api_cost_usd, 0.5);
    let unscoped = run_start();
    assert!(unscoped.started > started);
    assert_eq!(unscoped.api_cost_usd, 0.0);
}

#[test]
fn orientation_spend_is_priced_only_for_direct_api_orientation() {
    let before = CallUsage {
        input_tokens: 1_000,
        output_tokens: 500,
        ..Default::default()
    };
    let after = CallUsage {
        input_tokens: 101_000,
        output_tokens: 50_500,
        ..Default::default()
    };
    let usage = usage_since(&before, &after);
    assert_eq!((usage.input_tokens, usage.output_tokens), (100_000, 50_000));

    let mut settings = Settings {
        default_orientation_agent: "claude".into(),
        ..Default::default()
    };
    assert_eq!(
        priced_usage("claude", "claude-sonnet-test", &usage, &settings),
        0.0,
        "subscription orientation is not metered"
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert_eq!(runtime.block_on(orientation_cost(&usage, &settings)), 0.0);
    settings.claude_access_mode = "api".into();
    let cost = priced_usage("claude", "claude-sonnet-test", &usage, &settings);
    // $3/M in, $15/M out -> 0.3 + 0.75.
    assert!((cost - 1.05).abs() < 1e-9, "{cost}");
    assert_eq!(
        runtime.block_on(orientation_cost(&CallUsage::default(), &settings)),
        0.0,
        "a reused orientation made no calls"
    );
}

fn priced(provider: &str) -> ProviderPricing {
    ProviderPricing {
        provider: provider.into(),
        model: "model".into(),
        api: provider != "codex",
        price: (provider == "claude").then_some((3.0, 15.0)),
    }
}

#[test]
fn estimate_spans_a_range_and_flags_limits() {
    let mut synthesis = step("synthesis", Phase::Sequential, &["claude"]);
    synthesis.after = vec!["a".into(), "b".into()];
    let workflow = config(vec![
        step("a", Phase::Parallel, &["claude"]),
        step("b", Phase::Parallel, &["claude"]),
        synthesis,
    ]);
    let estimate = estimate_usage(
        &workflow,
        20_000,
        "claude",
        &priced,
        RunBudget {
            max_cost_usd: Some(0.10),
            max_calls: Some(3),
            max_minutes: None,
        },
    );
    // Orientation plus three steps.
    assert_eq!(estimate.calls_low, 4);
    assert_eq!(estimate.calls_high, 4);
    assert!(estimate.input_tokens_low >= 4 * 20_000);
    assert!(estimate.input_tokens_high > estimate.input_tokens_low);
    let (low, high) = (
        estimate.api_cost_low.unwrap(),
        estimate.api_cost_high.unwrap(),
    );
    assert!(low > 0.0 && high > low);
    assert!(estimate.exceeds_cost_limit);
    assert!(estimate.exceeds_call_limit);
    assert_eq!(estimate.subscription_calls_high, 0);
}

#[test]
fn subscription_steps_have_no_cost_and_unknown_prices_are_counted() {
    let workflow = config(vec![
        step("a", Phase::Parallel, &["codex"]),
        step("b", Phase::Parallel, &["local"]),
    ]);
    let estimate = estimate_usage(&workflow, 1_000, "codex", &priced, RunBudget::default());
    // Orientation and step a use codex (subscription); b is unpriced API.
    assert_eq!(estimate.subscription_calls_high, 2);
    assert_eq!(estimate.unpriced_calls, 1);
    assert_eq!(estimate.api_cost_low, Some(0.0));
    assert!(!estimate.exceeds_cost_limit);
}

#[test]
fn fan_out_and_multi_agent_steps_widen_the_range() {
    let mut fan_out = step("each", Phase::Parallel, &["claude", "codex"]);
    fan_out.for_each = Some(crate::pipeline_config::ForEach {
        glob: "*.tex".into(),
        max: 8,
        artifact: None,
    });
    let mut workflow = config(vec![fan_out]);
    workflow.merge.enabled = true;
    let estimate = estimate_usage(&workflow, 1_000, "codex", &priced, RunBudget::default());
    // Orientation 1 + (2 agents + merge) low; 2 agents x 8 units + merge high.
    assert_eq!(estimate.calls_low, 1 + 3);
    assert_eq!(estimate.calls_high, 1 + 17);
}

#[test]
fn document_estimate_uses_pages_for_pdfs_and_size_for_text() {
    assert_eq!(document_token_estimate("paper.pdf", Some(30)), 21_000);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("paper.md");
    std::fs::write(&path, "x".repeat(4_000)).unwrap();
    assert_eq!(document_token_estimate(path.to_str().unwrap(), None), 1_000);
    assert_eq!(document_token_estimate("/missing/file.tex", None), 0);
}
