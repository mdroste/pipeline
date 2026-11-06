//! Per-Review spending and effort limits, and the preflight usage estimate.
//!
//! The scheduler checks between waves. Dispatch additionally reserves each
//! submission, including API retries and tool continuations. In-flight calls
//! finish; steps not yet started are recorded as stopped at the limit, so the
//! run still ends with a labeled partial report. Cost counts only direct-API
//! calls at API list prices; subscription calls are limited by call count and
//! elapsed time instead. The call count includes every provider invocation of
//! the run (orientation, retries, failed units, merges). Cost includes the
//! orientation call and every attempt of a parallel unit, kept or not. Time
//! runs from the start of the run, so document preparation counts.

use crate::models::{StepCallRecord, StepOutput};
use crate::pipeline::logging::CallUsage;
use crate::settings::Settings;
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
        self.provider_attempts = self
            .provider_attempts
            .max(run_usage.provider_attempts)
            .max(super::budget_dispatch::admitted_attempts());
    }

    /// The limit reached, phrased to complete [`BUDGET_STOP_PREFIX`].
    pub(super) fn exhausted(&self) -> Option<String> {
        if let Some(limit) = self.budget.max_cost_usd {
            if self.api_cost >= limit {
                return Some(format!(
                    "cost limit of ${limit:.2} (estimated API list-price spend so far: ${:.2})",
                    self.api_cost
                ));
            }
        }
        if let Some(limit) = self.budget.max_calls {
            let calls = self.calls.max(self.provider_attempts);
            if calls >= u64::from(limit) {
                return Some(format!("limit of {limit} model calls ({calls} made)"));
            }
        }
        if let Some(limit) = self.budget.max_minutes {
            let minutes = self.started.elapsed().as_secs() / 60;
            if minutes >= u64::from(limit) {
                return Some(format!("time limit of {limit} minutes"));
            }
        }
        None
    }
}

#[path = "budget_estimate.rs"]
mod estimate;
pub use estimate::{
    document_token_estimate, estimate_usage, pdf_page_count, preview_estimate, ProviderPricing,
    UsageEstimate,
};

#[cfg(test)]
#[path = "budget_tests.rs"]
mod tests;
