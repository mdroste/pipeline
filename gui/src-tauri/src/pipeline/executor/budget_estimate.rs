//! Preflight estimates include indexed reading and the effective reviewer and merge policies.
use super::RunBudget;
use crate::auto_review::{adaptive_agent_count_range, specialist_donor};
use crate::pipeline_config::{Phase, PipelineConfig};
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

struct Calls {
    low: u64,
    high: u64,
    input_low: u64,
    input_high: u64,
    output_low: u64,
    output_high: u64,
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
        } else if resolved.provider == "local" {
            estimate.local_calls_high += calls.high;
        } else {
            estimate.subscription_calls_high += calls.high;
        }
    };

    // Paper Review can batch a second read of up to four locations.
    let indexed =
        crate::pipeline::orient::uses_manuscript_index(config.orientation_schema.as_ref());
    account(
        settings.orientation_agent(),
        settings.orientation_model_selection(settings.orientation_agent()),
        Calls {
            low: 1,
            high: 1 + u64::from(indexed),
            input_low: document_tokens + PROMPT_TOKENS,
            input_high: (document_tokens + PROMPT_TOKENS) * 3 / 2
                + if indexed {
                    (crate::pipeline::orient::INDEX_PROMPT_BYTES
                        + crate::pipeline::orient::FOLLOW_UP_BYTES)
                        .div_ceil(4) as u64
                        + PROMPT_TOKENS
                        + 6_000
                } else {
                    0
                },
            output_low: 2_000,
            output_high: if indexed { 12_000 } else { 6_000 },
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
    let adaptive = specialist_donor(config).zip(adaptive_agent_count_range(config));
    let specialists_high = adaptive.map_or(0, |(_, (_, high))| high as u64);
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
                    input_low: input + (agents * REPORT_TOKENS).min(MAX_UPSTREAM_TOKENS),
                    input_high: (input + (agents * REPORT_TOKENS).min(MAX_UPSTREAM_TOKENS))
                        * units_high
                        * 3
                        / 2,
                    output_low,
                    output_high: output_high * units_high,
                },
                &mut estimate,
            );
        }
    }
    if let Some((donor, (low, high))) = adaptive {
        let (low, high) = (low as u64, high as u64);
        let input = document_tokens + PROMPT_TOKENS;
        let calls = || Calls {
            low,
            high,
            input_low: input * low,
            input_high: input * high * 3 / 2,
            output_low: 3_000 * low,
            output_high: 8_000 * high,
        };
        let providers = if donor.agents.is_empty() {
            std::slice::from_ref(&settings.preferred_provider)
        } else {
            donor.agents.as_slice()
        };
        for provider in providers {
            account(
                provider,
                donor.model_selection_for(settings, provider),
                calls(),
                &mut estimate,
            );
        }
        if donor.agents.len() > 1 && config.merge.enabled {
            let (provider, selection) = merge_selection(config, settings);
            account(&provider, selection, calls(), &mut estimate);
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
