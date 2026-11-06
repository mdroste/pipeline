//! Preflight call ranges, using each call's effective provider and model policy.
//! A merge belongs to each parallel fan-out group, and adaptive specialists
//! inherit every agent and model override from their parallel donor step.
use super::RunBudget;
use crate::pipeline_config::{Phase, PipelineConfig, StepConfig};
use crate::settings::{ModelSelection, Settings};
use serde::Serialize;

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
    pub local_calls_high: u64,
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

#[derive(Clone, PartialEq, Eq)]
struct CallModel {
    provider: String,
    selection: ModelSelection,
}

fn call_model(settings: &Settings, provider: &str, selection: Option<ModelSelection>) -> CallModel {
    CallModel {
        provider: provider.into(),
        selection: selection.unwrap_or_else(|| settings.model_selection(provider)),
    }
}

fn call_range(low: u64, high: u64, input: u64, output_low: u64, output_high: u64) -> Calls {
    Calls {
        low,
        high,
        input_low: input * low,
        input_high: input * high * 3 / 2,
        output_low: output_low * low,
        output_high: output_high * high,
    }
}

fn planned_calls(
    config: &PipelineConfig,
    settings: &Settings,
    document_tokens: u64,
) -> Vec<(CallModel, Calls)> {
    let mut config = config.clone();
    crate::pipeline_config::apply_agent_defaults(&mut config, settings);
    let orientation = settings.orientation_agent();
    let mut calls = vec![(
        call_model(
            settings,
            orientation,
            settings.orientation_model_selection(orientation),
        ),
        call_range(1, 1, document_tokens + PROMPT_TOKENS, 2_000, 6_000),
    )];
    let (provider, selection, _) = config.merge.call_policy(settings);
    let merge = CallModel {
        provider,
        selection,
    };
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
        if crate::auto_review::uses_auto_review_contract(&config) {
            specialist_range(&config)
        } else {
            (0, 0)
        };
    let mut add_step = |step: &StepConfig, low: u64, high: u64| {
        let (input, output_low, output_high) = if step.phase == Phase::Parallel {
            (document_tokens + PROMPT_TOKENS, 3_000, 8_000)
        } else {
            let upstream = ((parallel + specialists_high) * REPORT_TOKENS).min(MAX_UPSTREAM_TOKENS);
            (document_tokens + PROMPT_TOKENS + upstream, 6_000, 16_000)
        };
        for provider in &step.agents {
            calls.push((
                call_model(
                    settings,
                    provider,
                    step.model_selection_for(settings, provider),
                ),
                call_range(low, high, input, output_low, output_high),
            ));
        }
        if step.phase == Phase::Parallel && step.agents.len() > 1 && config.merge.enabled {
            let reports = (step.agents.len() as u64 * REPORT_TOKENS).min(MAX_UPSTREAM_TOKENS);
            calls.push((
                merge.clone(),
                call_range(low, high, input + reports, output_low, output_high),
            ));
        }
    };
    for step in &enabled {
        let high = step
            .for_each
            .as_ref()
            .map_or(1, |fan_out| u64::from(fan_out.max.max(1)));
        add_step(step, 1, high);
    }
    if specialists_high > 0 {
        if let Some(donor) = enabled.iter().find(|step| step.phase == Phase::Parallel) {
            add_step(donor, specialists_low, specialists_high);
        }
    }
    calls
}

/// Estimates each provider invocation at its effective model selection.
pub fn estimate_usage(
    config: &PipelineConfig,
    document_tokens: u64,
    settings: &Settings,
    pricing: &dyn Fn(&str, &ModelSelection) -> ProviderPricing,
    budget: RunBudget,
    author_note: Option<&str>,
) -> UsageEstimate {
    estimate_plan(
        &planned_calls(config, settings, document_tokens),
        document_tokens,
        pricing,
        budget,
        author_note,
    )
}

fn estimate_plan(
    plan: &[(CallModel, Calls)],
    document_tokens: u64,
    pricing: &dyn Fn(&str, &ModelSelection) -> ProviderPricing,
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
    for (model, range) in plan {
        let calls = Calls {
            input_low: range.input_low + note_tokens * range.low,
            input_high: range.input_high + note_tokens * range.high,
            ..*range
        };
        estimate.calls_low += calls.low;
        estimate.calls_high += calls.high;
        estimate.input_tokens_low += calls.input_low;
        estimate.input_tokens_high += calls.input_high;
        estimate.output_tokens_low += calls.output_low;
        estimate.output_tokens_high += calls.output_high;
        let resolved = pricing(&model.provider, &model.selection);
        if resolved.provider.eq_ignore_ascii_case("local") {
            estimate.local_calls_high += calls.high;
        } else if resolved.api {
            any_api = true;
            if !add_cost(&mut cost, resolved.price, &calls) {
                estimate.unpriced_calls += calls.high;
            }
        } else {
            estimate.subscription_calls_high += calls.high;
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

/// Resolve each distinct provider/model policy once with a short timeout.
/// A failed resolution stays unpriced rather than borrowing a default price.
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
    let plan = planned_calls(config, settings, document_tokens);
    let mut resolved: Vec<(CallModel, ProviderPricing)> = Vec::new();
    for (request, _) in &plan {
        if resolved.iter().any(|(known, _)| known == request) {
            continue;
        }
        let provider = &request.provider;
        let api = settings.model_transport(provider) == "api";
        let model = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            crate::model_catalog::resolve(provider, settings, Some(&request.selection)),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .map(|resolution| resolution.resolved_model)
        .unwrap_or_default();
        let price = (!model.is_empty())
            .then(|| crate::output::model_price(&model))
            .flatten();
        resolved.push((
            request.clone(),
            ProviderPricing {
                provider: provider.clone(),
                model,
                api,
                price,
            },
        ));
    }
    estimate_plan(
        &plan,
        document_tokens,
        &|provider, selection| {
            resolved
                .iter()
                .find(|(key, _)| key.provider == provider && key.selection == *selection)
                .expect("every planned call was resolved")
                .1
                .clone()
        },
        RunBudget::from_settings(settings),
        author_note,
    )
}
