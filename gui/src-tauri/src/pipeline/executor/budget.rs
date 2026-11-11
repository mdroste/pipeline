//! Per-Review spending and effort limits, and the preflight usage estimate.
//!
//! Normal runs use `review_budget` to admit every provider invocation, from
//! extraction through revision comparison. The scheduler also checks before
//! each wave and records unstarted steps in a labeled partial report. Calls
//! already in flight finish. Direct-API usage is charged at known list prices;
//! subscription calls count toward call and elapsed-time limits. The local
//! ledger below supports executor-only callers without a shared run budget.

use crate::models::{StepCallRecord, StepOutput};
use crate::pipeline::logging::CallUsage;
use crate::pipeline_config::{Phase, PipelineConfig};
use crate::settings::{ModelSelection, Settings};
use serde::Serialize;
use std::time::Instant;

/// Prefix of every failure recorded for a step the limit prevented.
pub const BUDGET_STOP_PREFIX: &str = "Not started: this Review reached its";

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RunBudget {
    pub max_cost_usd: Option<f64>,
    pub max_calls: Option<u32>,
    pub max_minutes: Option<u32>,
}

impl RunBudget {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            max_cost_usd: (settings.review_max_cost_usd.is_finite()
                && settings.review_max_cost_usd > 0.0)
                .then_some(settings.review_max_cost_usd),
            max_calls: (settings.review_max_calls > 0).then_some(settings.review_max_calls),
            max_minutes: (settings.review_max_minutes > 0).then_some(settings.review_max_minutes),
        }
    }

    pub fn is_unlimited(&self) -> bool {
        self.max_cost_usd.is_none() && self.max_calls.is_none() && self.max_minutes.is_none()
    }
}

/// Where a run's limits start counting: when the run began, and the
/// direct-API spend before its first wave (orientation).
#[derive(Debug, Clone, Copy)]
pub struct BudgetStart {
    pub started: Instant,
    pub api_cost_usd: f64,
}

impl BudgetStart {
    /// Counts from now, with nothing spent.
    pub fn now() -> Self {
        Self {
            started: Instant::now(),
            api_cost_usd: 0.0,
        }
    }

    /// Counts from `started`, plus orientation's spend: the run-wide usage
    /// added since `before_orientation`, priced at the orientation model.
    pub async fn after_orientation(
        started: Instant,
        before_orientation: &CallUsage,
        settings: &Settings,
    ) -> Self {
        let usage = usage_since(before_orientation, &crate::pipeline::logging::run_usage());
        Self {
            started,
            api_cost_usd: orientation_cost(&usage, settings).await,
        }
    }
}

tokio::task_local! {
    static RUN_START: BudgetStart;
}

/// Runs `fut`, the run's `execute_steps`, with the limits counted from
/// `start` instead of from the first wave.
pub async fn counted_from<F: std::future::Future>(start: BudgetStart, fut: F) -> F::Output {
    RUN_START.scope(start, fut).await
}

/// The start set by [`counted_from`], or now when none is set.
pub(super) fn run_start() -> BudgetStart {
    RUN_START
        .try_with(|start| *start)
        .unwrap_or_else(|_| BudgetStart::now())
}

fn usage_since(before: &CallUsage, after: &CallUsage) -> CallUsage {
    CallUsage {
        input_tokens: after.input_tokens.saturating_sub(before.input_tokens),
        output_tokens: after.output_tokens.saturating_sub(before.output_tokens),
        cached_input_tokens: after
            .cached_input_tokens
            .saturating_sub(before.cached_input_tokens),
        cache_write_input_tokens: after
            .cache_write_input_tokens
            .saturating_sub(before.cache_write_input_tokens),
        ..Default::default()
    }
}

/// API list price of orientation `usage` when orientation uses a direct API
/// connection; zero for subscriptions and for a model with no known price.
async fn orientation_cost(usage: &CallUsage, settings: &Settings) -> f64 {
    let provider = settings.orientation_agent();
    if (usage.input_tokens == 0 && usage.output_tokens == 0)
        || settings.model_transport(provider) != "api"
    {
        return 0.0;
    }
    // Orientation resolved this model moments ago, so the catalog is cached.
    let selection = settings.orientation_model_selection(provider);
    let resolved = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        crate::model_catalog::resolve(provider, settings, selection.as_ref()),
    )
    .await;
    match resolved {
        Ok(Ok(resolution)) => priced_usage(provider, &resolution.resolved_model, usage, settings),
        _ => 0.0,
    }
}

fn priced_usage(provider: &str, model: &str, usage: &CallUsage, settings: &Settings) -> f64 {
    let call = StepOutput {
        provider: provider.to_string(),
        model: model.to_string(),
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cached_input_tokens: usage.cached_input_tokens,
        cache_write_input_tokens: usage.cache_write_input_tokens,
        ..Default::default()
    };
    crate::output::api_cost_of(&call, settings).0
}

/// Spend, calls and time one run has used against its limits.
pub(super) struct BudgetLedger {
    budget: RunBudget,
    started: Instant,
    api_cost: f64,
    calls: u64,
    /// Provider invocations of the whole run, including orientation, retries,
    /// failed units, and merges, which leave no successful output.
    provider_attempts: u64,
}

fn calls_in(output: &StepOutput) -> u64 {
    if output.skipped {
        0
    } else if output.calls.is_empty() {
        u64::from(output.input_tokens > 0 || output.output_tokens > 0)
    } else {
        output.calls.len() as u64
    }
}

impl BudgetLedger {
    pub(super) fn new(settings: &Settings, start: BudgetStart) -> Self {
        Self {
            budget: RunBudget::from_settings(settings),
            started: start.started,
            api_cost: start.api_cost_usd,
            calls: 0,
            provider_attempts: 0,
        }
    }

    pub(super) fn record(&mut self, outputs: &[StepOutput], settings: &Settings) {
        for output in outputs {
            self.api_cost += crate::output::api_cost_of(output, settings).0;
            self.calls += calls_in(output);
        }
    }

    /// Adds the spend of calls no output records: the attempts of units that
    /// failed, or whose response the run could not keep. The call count
    /// already includes them through [`Self::observe`].
    pub(super) fn record_failed_calls(&mut self, calls: &[StepCallRecord], settings: &Settings) {
        if calls.is_empty() {
            return;
        }
        let failed = StepOutput {
            calls: calls.to_vec(),
            ..Default::default()
        };
        self.api_cost += crate::output::api_cost_of(&failed, settings).0;
    }

    /// Takes the run-wide usage total (`logging::run_usage`, reset when the
    /// run begins), so every provider invocation counts toward the call limit.
    pub(super) fn observe(&mut self, run_usage: &CallUsage) {
        self.provider_attempts = self.provider_attempts.max(run_usage.provider_attempts);
    }

    /// The limit reached, phrased to complete [`BUDGET_STOP_PREFIX`].
    pub(super) fn exhausted(&self) -> Option<String> {
        limit_reason(
            self.budget,
            self.started,
            self.api_cost,
            self.calls.max(self.provider_attempts),
        )
    }
}

pub(crate) fn limit_reason(
    budget: RunBudget,
    started: Instant,
    api_cost: f64,
    calls: u64,
) -> Option<String> {
    if let Some(limit) = budget.max_cost_usd {
        if api_cost >= limit {
            return Some(format!(
                "cost limit of ${limit:.2} (estimated API list-price spend so far: ${api_cost:.2})"
            ));
        }
    }
    if let Some(limit) = budget.max_calls {
        if calls >= u64::from(limit) {
            return Some(format!(
                "limit of {limit} model calls ({calls} made or reserved)"
            ));
        }
    }
    if let Some(limit) = budget.max_minutes {
        if started.elapsed().as_secs() / 60 >= u64::from(limit) {
            return Some(format!("time limit of {limit} minutes"));
        }
    }
    None
}

/// How a step's calls are priced in the estimate.
#[derive(Debug, Clone)]
pub struct ProviderPricing {
    pub provider: String,
    pub model: String,
    pub api: bool,
    /// Input and output US dollars per million tokens.
    pub price: Option<(f64, f64)>,
}

/// A deliberately rough preflight range. Actual use depends on the document,
/// tool reads, retries, and which specialists orientation selects.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageEstimate {
    pub document_tokens: u64,
    pub calls_low: u64,
    pub calls_high: u64,
    pub input_tokens_low: u64,
    pub input_tokens_high: u64,
    pub output_tokens_low: u64,
    pub output_tokens_high: u64,
    /// API list-price range for direct-API calls; None when no step uses a
    /// direct API connection.
    pub api_cost_low: Option<f64>,
    pub api_cost_high: Option<f64>,
    /// Direct-API calls whose model has no known price.
    pub unpriced_calls: u64,
    /// Calls through subscriptions, which are not billed per token.
    pub subscription_calls_high: u64,
    pub max_cost_usd: Option<f64>,
    pub max_calls: Option<u32>,
    pub max_minutes: Option<u32>,
    pub exceeds_cost_limit: bool,
    pub exceeds_call_limit: bool,
}

const PROMPT_TOKENS: u64 = 3_000;
const REPORT_TOKENS: u64 = 3_000;
const MAX_UPSTREAM_TOKENS: u64 = 40_000;
const SPECIALISTS_LOW: u64 = 2;
const SPECIALISTS_HIGH: u64 = 6;

struct Calls {
    low: u64,
    high: u64,
    input_low: u64,
    input_high: u64,
    output_low: u64,
    output_high: u64,
}

/// Specialists Paper Review may add after orientation, from the orientation
/// schema's item limits (the same source the preview UI reads).
fn specialist_range(config: &PipelineConfig) -> (u64, u64) {
    let Some(schema) = config.orientation_schema.as_ref() else {
        return (SPECIALISTS_LOW, SPECIALISTS_HIGH);
    };
    let read = |field: &str, bound: &str| {
        schema
            .pointer(&format!(
                "/properties/review_plan/properties/{field}/{bound}"
            ))
            .and_then(serde_json::Value::as_u64)
    };
    match (
        read("subject_specialist_ids", "minItems"),
        read("subject_specialist_ids", "maxItems"),
        read("method_specialist_ids", "minItems"),
        read("method_specialist_ids", "maxItems"),
    ) {
        (Some(subject_min), Some(subject_max), Some(method_min), Some(method_max)) => (
            (subject_min + method_min).min(16),
            (subject_max + method_max).clamp(subject_min + method_min, 16),
        ),
        _ => (SPECIALISTS_LOW, SPECIALISTS_HIGH),
    }
}

fn add_cost(cost: &mut (f64, f64), price: Option<(f64, f64)>, calls: &Calls) -> bool {
    let Some((input, output)) = price else {
        return false;
    };
    cost.0 += (calls.input_low as f64 * input + calls.output_low as f64 * output) / 1_000_000.0;
    cost.1 += (calls.input_high as f64 * input + calls.output_high as f64 * output) / 1_000_000.0;
    true
}

// The same merge inheritance used by pipeline::merge and the model roster.
fn merge_selection(
    config: &PipelineConfig,
    settings: &Settings,
) -> (String, Option<ModelSelection>) {
    match config.merge.agents.first() {
        Some(provider) => (provider.clone(), None),
        None => {
            let provider = settings.merge_agent().to_string();
            let selection = settings.merge_model_selection(&provider);
            (provider, selection)
        }
    }
}

/// Estimates one run of `config` over a document of `document_tokens`.
/// `pricing` resolves each effective selection to its model and transport.
pub fn estimate_usage(
    config: &PipelineConfig,
    document_tokens: u64,
    settings: &Settings,
    pricing: &dyn Fn(&str, Option<&ModelSelection>) -> ProviderPricing,
    budget: RunBudget,
    author_note: Option<&str>,
) -> UsageEstimate {
    let mut estimate = UsageEstimate {
        document_tokens,
        max_cost_usd: budget.max_cost_usd,
        max_calls: budget.max_calls,
        max_minutes: budget.max_minutes,
        ..Default::default()
    };
    let mut cost = (0f64, 0f64);
    let mut any_api = false;
    let note_tokens = crate::pipeline::author_note::prefix(author_note)
        .len()
        .div_ceil(4) as u64;
    let mut account = |provider: &str,
                       selection: Option<ModelSelection>,
                       mut calls: Calls,
                       estimate: &mut UsageEstimate| {
        calls.input_low += note_tokens * calls.low;
        calls.input_high += note_tokens * calls.high;
        estimate.calls_low += calls.low;
        estimate.calls_high += calls.high;
        estimate.input_tokens_low += calls.input_low;
        estimate.input_tokens_high += calls.input_high;
        estimate.output_tokens_low += calls.output_low;
        estimate.output_tokens_high += calls.output_high;
        let resolved = pricing(provider, selection.as_ref());
        if resolved.api {
            any_api = true;
            if !add_cost(&mut cost, resolved.price, &calls) {
                estimate.unpriced_calls += calls.high;
            }
        } else {
            estimate.subscription_calls_high += calls.high;
        }
    };

    // Orientation reads the document once.
    account(
        settings.orientation_agent(),
        settings.orientation_model_selection(settings.orientation_agent()),
        Calls {
            low: 1,
            high: 1,
            input_low: document_tokens + PROMPT_TOKENS,
            input_high: (document_tokens + PROMPT_TOKENS) * 3 / 2,
            output_low: 2_000,
            output_high: 6_000,
        },
        &mut estimate,
    );

    // Analyzer steps run on this computer and make no model call.
    let enabled: Vec<_> = config
        .steps
        .iter()
        .filter(|step| step.enabled && step.kind.is_model())
        .collect();
    let parallel = enabled
        .iter()
        .filter(|step| step.phase == Phase::Parallel)
        .count() as u64;
    let (specialists_low, specialists_high) =
        if crate::auto_review::uses_auto_review_contract(config) {
            specialist_range(config)
        } else {
            (0, 0)
        };
    for step in &enabled {
        let providers: Vec<&str> = if step.agents.is_empty() {
            vec![settings.preferred_provider.as_str()]
        } else if step.phase == Phase::Sequential {
            vec![step.agents[0].as_str()]
        } else {
            step.agents.iter().map(String::as_str).collect()
        };
        let agents = providers.len() as u64;
        // A fan-out runs once per matching item, up to its declared cap.
        let units_high = if step.phase == Phase::Parallel {
            step.for_each
                .as_ref()
                .map_or(1, |fan_out| u64::from(fan_out.max.max(1)))
        } else {
            1
        };
        let merge = agents > 1 && config.merge.enabled && step.phase == Phase::Parallel;
        let (input, output_low, output_high) = if step.phase == Phase::Parallel {
            (document_tokens + PROMPT_TOKENS, 3_000, 8_000)
        } else {
            let upstream = ((parallel + specialists_high) * REPORT_TOKENS).min(MAX_UPSTREAM_TOKENS);
            (document_tokens + PROMPT_TOKENS + upstream, 6_000, 16_000)
        };
        for provider in providers {
            account(
                provider,
                step.model_selection_for(settings, provider),
                Calls {
                    low: 1,
                    high: units_high,
                    input_low: input,
                    input_high: input * units_high * 3 / 2,
                    output_low,
                    output_high: output_high * units_high,
                },
                &mut estimate,
            );
        }
        if merge {
            let (provider, selection) = merge_selection(config, settings);
            account(
                &provider,
                selection,
                Calls {
                    low: 1,
                    high: units_high,
                    input_low: input,
                    input_high: input * units_high * 3 / 2,
                    output_low,
                    output_high: output_high * units_high,
                },
                &mut estimate,
            );
        }
    }
    if specialists_high > 0 {
        if let Some(donor) = enabled.iter().find(|step| step.phase == Phase::Parallel) {
            let input = document_tokens + PROMPT_TOKENS;
            let providers = if donor.agents.is_empty() {
                std::slice::from_ref(&settings.preferred_provider)
            } else {
                donor.agents.as_slice()
            };
            for provider in providers {
                account(
                    provider,
                    donor.model_selection_for(settings, provider),
                    Calls {
                        low: specialists_low,
                        high: specialists_high,
                        input_low: input * specialists_low,
                        input_high: input * specialists_high * 3 / 2,
                        output_low: 3_000 * specialists_low,
                        output_high: 8_000 * specialists_high,
                    },
                    &mut estimate,
                );
            }
            if donor.agents.len() > 1 && config.merge.enabled {
                let (provider, selection) = merge_selection(config, settings);
                account(
                    &provider,
                    selection,
                    Calls {
                        low: specialists_low,
                        high: specialists_high,
                        input_low: input * specialists_low,
                        input_high: input * specialists_high * 3 / 2,
                        output_low: 3_000 * specialists_low,
                        output_high: 8_000 * specialists_high,
                    },
                    &mut estimate,
                );
            }
        }
    }
    if any_api {
        estimate.api_cost_low = Some(cost.0);
        estimate.api_cost_high = Some(cost.1);
    }
    estimate.exceeds_cost_limit = matches!(
        (budget.max_cost_usd, estimate.api_cost_high),
        (Some(limit), Some(high)) if high > limit
    );
    estimate.exceeds_call_limit = budget
        .max_calls
        .is_some_and(|limit| estimate.calls_high > u64::from(limit));
    estimate
}

/// Rough token count of a run's primary input before extraction. PDFs use
/// their page count; text formats their size; DOCX its compressed size.
pub fn document_token_estimate(path: &str, pdf_pages: Option<u32>) -> u64 {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".pdf") {
        if let Some(pages) = pdf_pages {
            return u64::from(pages) * 700;
        }
    }
    let bytes = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    if lower.ends_with(".docx") {
        bytes / 5
    } else if lower.ends_with(".pdf") {
        bytes / 20
    } else {
        bytes / 4
    }
}

/// Page count reported by Poppler's `pdfinfo`, if available. Blocking.
pub fn pdf_page_count(path: &str) -> Option<u32> {
    let bin = crate::deps::resolve_command("pdfinfo")?;
    let mut command = bin.command([std::ffi::OsStr::new(path)]);
    command.env("LC_ALL", "C");
    let output =
        crate::process::run_bounded(&mut command, std::time::Duration::from_secs(5), 16 * 1024)
            .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            line.strip_prefix("Pages:")
                .and_then(|count| count.trim().parse::<u32>().ok())
        })
}

/// Preflight estimate for the Review preview. Each provider's default model
/// is resolved once with a short timeout; an unresolved model is unpriced.
pub async fn preview_estimate(
    config: &PipelineConfig,
    settings: &Settings,
    paper_path: Option<&str>,
    author_note: Option<&str>,
) -> UsageEstimate {
    let document_tokens = match paper_path.filter(|path| !path.trim().is_empty()) {
        Some(path) => {
            let owned = path.to_string();
            tokio::task::spawn_blocking(move || {
                let pages = owned
                    .to_ascii_lowercase()
                    .ends_with(".pdf")
                    .then(|| pdf_page_count(&owned))
                    .flatten();
                document_token_estimate(&owned, pages)
            })
            .await
            .unwrap_or(0)
        }
        None => 0,
    };
    // Collect through the estimator itself, so every reviewer, stage override,
    // and merge is resolved with exactly the policy that will be priced.
    let selections = {
        let selections = std::cell::RefCell::new(Vec::<(String, Option<ModelSelection>)>::new());
        let unpriced = |provider: &str, selection: Option<&ModelSelection>| {
            let entry = (provider.to_string(), selection.cloned());
            if !selections.borrow().contains(&entry) {
                selections.borrow_mut().push(entry);
            }
            ProviderPricing {
                provider: provider.into(),
                model: String::new(),
                api: settings.model_transport(provider) == "api",
                price: None,
            }
        };
        estimate_usage(
            config,
            document_tokens,
            settings,
            &unpriced,
            RunBudget::from_settings(settings),
            author_note,
        );
        selections.into_inner()
    };
    let mut resolved = Vec::new();
    for (provider, selection) in selections {
        let model = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            crate::model_catalog::resolve(&provider, settings, selection.as_ref()),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .map(|r| r.resolved_model)
        .unwrap_or_default();
        let price = crate::output::model_price(&model);
        let pricing = ProviderPricing {
            api: settings.model_transport(&provider) == "api",
            provider: provider.clone(),
            model,
            price,
        };
        resolved.push((provider, selection, pricing));
    }
    let pricing = |provider: &str, selection: Option<&ModelSelection>| {
        resolved
            .iter()
            .find(|(p, s, _)| p == provider && s.as_ref() == selection)
            .map(|(_, _, price)| price.clone())
            .expect("every estimated invocation was resolved")
    };
    estimate_usage(
        config,
        document_tokens,
        settings,
        &pricing,
        RunBudget::from_settings(settings),
        author_note,
    )
}

#[cfg(test)]
#[path = "budget_tests.rs"]
mod tests;
