use super::*;

pub(super) fn format_duration(d: Duration) -> String {
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
    if let Some(price) = crate::model_catalog::price_for_model(model) {
        return Some(price);
    }
    let m = model.to_ascii_lowercase();
    // (needle, input $/Mtok, output $/Mtok)
    const TABLE: &[(&str, f64, f64)] = &[
        ("haiku", 0.80, 4.0),
        ("sonnet", 3.0, 15.0),
        ("opus", 15.0, 75.0),
        ("o4-mini", 1.10, 4.40),
        ("o3-mini", 1.10, 4.40),
        ("o3", 2.0, 8.0),
        ("gpt-5.6-sol", 5.0, 30.0),
        ("gpt-5.6-terra", 2.50, 15.0),
        ("gpt-5.6-luna", 1.0, 6.0),
        ("gpt-4.1-mini", 0.40, 1.60),
        ("gpt-4.1", 2.0, 8.0),
        ("gpt-4o-mini", 0.15, 0.60),
        ("gpt-4o", 2.50, 10.0),
        ("gemini-3.1-pro-preview", 2.0, 12.0),
        ("gemini-3.6-flash", 1.50, 7.50),
        ("gemini-3.5-flash-lite", 0.30, 2.50),
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

/// Whether a provider ran through a direct API (metered) rather than a
/// subscription CLI. Both transports receive a labelled API-list-price
/// estimate, but only the direct API amount approximates token-metered spend.
pub(super) fn provider_in_api_mode(settings: &Settings, provider: &str) -> bool {
    match provider {
        "codex" => !settings.openai_api_key.trim().is_empty(),
        "antigravity" => !settings.google_api_key.trim().is_empty(),
        "local" => true,
        // "claude" and the empty/default provider both map to Anthropic.
        _ => !settings.anthropic_api_key.trim().is_empty(),
    }
}

/// Cache-token list prices when Pipeline can identify the provider family.
/// Unknown cache schedules deliberately fall back to ordinary input price so
/// the estimate never treats reported cache reads as free.
fn model_cache_prices(model: &str, input_price: f64) -> (f64, f64) {
    let model = model.to_ascii_lowercase();
    if model.contains("gpt-5.6") {
        // OpenAI: cached reads are 10% of input; GPT-5.6 cache writes are
        // currently charged at 1.25x ordinary input.
        (input_price * 0.10, input_price * 1.25)
    } else if model.contains("claude")
        || model.contains("opus")
        || model.contains("sonnet")
        || model.contains("haiku")
    {
        // Anthropic's default five-minute prompt cache.
        (input_price * 0.10, input_price * 1.25)
    } else if model.contains("gemini") {
        // Gemini implicit caching discounts cache hits; cache creation has no
        // separately reported token rate in Pipeline.
        (input_price * 0.10, input_price)
    } else {
        (input_price, input_price)
    }
}

/// Estimated cache-adjusted API list price of a call, or None when the model
/// is unknown. Cached/cache-write counts partition logical input; they are not
/// added on top of it.
pub(super) fn estimate_cost(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cached_input_tokens: u64,
    cache_write_input_tokens: u64,
) -> Option<f64> {
    let (pin, pout) = model_price(model)?;
    let cached = cached_input_tokens.min(input_tokens);
    let cache_write = cache_write_input_tokens.min(input_tokens.saturating_sub(cached));
    let fresh = input_tokens
        .saturating_sub(cached)
        .saturating_sub(cache_write);
    let (pcached, pwrite) = model_cache_prices(model, pin);
    Some(
        (fresh as f64 / 1_000_000.0) * pin
            + (cached as f64 / 1_000_000.0) * pcached
            + (cache_write as f64 / 1_000_000.0) * pwrite
            + (output_tokens as f64 / 1_000_000.0) * pout,
    )
}

pub(super) fn format_cost(cost: f64) -> String {
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

fn fmt_token_usage(input: u64, output: u64, cached: u64, cache_write: u64) -> String {
    if input == 0 && output == 0 && cached == 0 && cache_write == 0 {
        return "—".to_string();
    }
    let cached = cached.min(input);
    let cache_write = cache_write.min(input.saturating_sub(cached));
    let fresh = input.saturating_sub(cached).saturating_sub(cache_write);
    let mut input_parts = vec![format!("{} fresh", fmt_tokens(fresh))];
    if cached > 0 {
        input_parts.push(format!("{} cache read", fmt_tokens(cached)));
    }
    if cache_write > 0 {
        input_parts.push(format!("{} cache write", fmt_tokens(cache_write)));
    }
    format!(
        "{} logical in = {} · {} out",
        fmt_tokens(input),
        input_parts.join(" + "),
        fmt_tokens(output)
    )
}

fn fmt_reported_activity(
    model_round_trips: u64,
    tool_calls: crate::models::ToolCallCounts,
) -> String {
    let mut parts = Vec::new();
    if model_round_trips > 0 {
        parts.push(format!("{model_round_trips} model rounds"));
    }
    let total_tools = tool_calls.total();
    if total_tools > 0 {
        let mut categories = Vec::new();
        for (count, label) in [
            (tool_calls.text_file, "text/file"),
            (tool_calls.image, "image"),
            (tool_calls.web, "web"),
            (tool_calls.shell_or_other, "shell/other"),
            (tool_calls.unknown, "unknown"),
        ] {
            if count > 0 {
                categories.push(format!("{count} {label}"));
            }
        }
        parts.push(format!("{total_tools} tools ({})", categories.join(", ")));
    }
    if parts.is_empty() {
        "—".to_string()
    } else {
        parts.join(" · ")
    }
}

/// A per-step run summary table (time, model, tokens, estimated cost), or None
/// when there is nothing worth showing (e.g. a legacy report with no metrics).
pub(super) fn render_run_summary(report: &PipelineReport, settings: &Settings) -> Option<String> {
    let outputs = report.all_outputs();
    let has_metrics = outputs.iter().any(|o| {
        o.duration_secs > 0
            || o.input_tokens > 0
            || o.output_tokens > 0
            || o.cached_input_tokens > 0
            || o.cache_write_input_tokens > 0
            || o.model_round_trips > 0
            || !o.tool_calls.is_empty()
    });
    if !has_metrics {
        return None;
    }

    let mut total_in = 0u64;
    let mut total_out = 0u64;
    let mut total_cached = 0u64;
    let mut total_cache_write = 0u64;
    let mut total_model_round_trips = 0u64;
    let mut total_tool_calls = crate::models::ToolCallCounts::default();
    let mut total_cost = 0f64;
    let mut any_cost = false;
    let mut any_cli_equivalent = false;
    let mut any_api_estimate = false;

    let mut rows = String::new();
    for o in &outputs {
        total_in += o.input_tokens;
        total_out += o.output_tokens;
        total_cached += o.cached_input_tokens;
        total_cache_write += o.cache_write_input_tokens;
        total_model_round_trips = total_model_round_trips.saturating_add(o.model_round_trips);
        total_tool_calls.add_counts(o.tool_calls);

        let mut provider_models = Vec::new();
        if o.calls.is_empty() {
            provider_models.push(format!(
                "{} · {}",
                if o.provider.trim().is_empty() {
                    "default".to_string()
                } else {
                    capitalize(&o.provider)
                },
                if o.model.trim().is_empty() {
                    "default"
                } else {
                    &o.model
                }
            ));
        } else {
            for call in &o.calls {
                let label = format!(
                    "{} · {}",
                    if call.provider.trim().is_empty() {
                        "default".to_string()
                    } else {
                        capitalize(&call.provider)
                    },
                    if call.model.trim().is_empty() {
                        "default"
                    } else {
                        &call.model
                    }
                );
                if !provider_models.contains(&label) {
                    provider_models.push(label);
                }
            }
        }
        let tokens = fmt_token_usage(
            o.input_tokens,
            o.output_tokens,
            o.cached_input_tokens,
            o.cache_write_input_tokens,
        );
        let activity = fmt_reported_activity(o.model_round_trips, o.tool_calls);
        let mut row_cost = 0f64;
        let mut row_has_cost = false;
        if o.calls.is_empty() {
            let api_transport = if o.model_transport.is_empty() {
                provider_in_api_mode(settings, &o.provider)
            } else {
                o.model_transport == "api"
            };
            if let Some(cost) = estimate_cost(
                &o.model,
                o.input_tokens,
                o.output_tokens,
                o.cached_input_tokens,
                o.cache_write_input_tokens,
            ) {
                row_cost += cost;
                row_has_cost = true;
                any_api_estimate |= api_transport;
                any_cli_equivalent |= !api_transport;
            }
        } else {
            for call in &o.calls {
                let api_transport = if call.model_transport.is_empty() {
                    provider_in_api_mode(settings, &call.provider)
                } else {
                    call.model_transport == "api"
                };
                if let Some(cost) = estimate_cost(
                    &call.model,
                    call.input_tokens,
                    call.output_tokens,
                    call.cached_input_tokens,
                    call.cache_write_input_tokens,
                ) {
                    row_cost += cost;
                    row_has_cost = true;
                    any_api_estimate |= api_transport;
                    any_cli_equivalent |= !api_transport;
                }
            }
        }
        let cost_cell = if row_has_cost {
            total_cost += row_cost;
            any_cost = true;
            format_cost(row_cost)
        } else {
            "—".to_string()
        };
        rows.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            o.step_label,
            provider_models.join("<br>"),
            format_secs(o.duration_secs),
            tokens,
            activity,
            cost_cell,
        ));
    }

    let total_tokens = fmt_token_usage(total_in, total_out, total_cached, total_cache_write);
    let total_activity = fmt_reported_activity(total_model_round_trips, total_tool_calls);
    let total_cost_cell = if any_cost {
        format_cost(total_cost)
    } else {
        "—".to_string()
    };

    let mut md = String::new();
    md.push_str("## Run summary\n\n");
    md.push_str(
        "| Step | Provider · Model | Time | Token accounting | Reported activity | API list-price equivalent |\n",
    );
    md.push_str(
        "|------|------------------|------|------------------|-------------------|---------------------------|\n",
    );
    md.push_str(&rows);
    md.push_str(&format!(
        "| **Total** | | | **{total_tokens}** | **{total_activity}** | **{total_cost_cell}** |\n\n"
    ));
    if total_model_round_trips > 0 || !total_tool_calls.is_empty() {
        md.push_str(
            "_Model rounds and tool calls appear only when the provider or CLI reports them; \
             unknown tool kinds remain visible in the unknown category._\n\n",
        );
    }
    if total_cached > 0 || total_cache_write > 0 {
        md.push_str(
            "_Logical input already includes cache reads and cache writes: fresh input = logical \
             input − cache reads − cache writes. Cache reads are discounted, not free. The \
             estimate applies known cache-token list rates._\n\n",
        );
    }
    if any_cli_equivalent {
        md.push_str(
            "_For CLI/subscription calls, this is an API-equivalent list-price estimate, not an \
             amount charged to the subscription._\n\n",
        );
    }
    if any_api_estimate {
        md.push_str(
            "_For direct API calls, this estimates token usage at list price; it is not a provider \
             invoice._\n\n",
        );
    }
    if any_cost {
        md.push_str(
            "_Estimates can exclude unreported cache writes, failed calls, long-context pricing \
             tiers, cache storage, and separately priced tools or search._\n\n",
        );
    }
    md.push_str(
        "_This table prices saved report-producing calls that retain per-model metadata. \
         Run-wide Console and History totals can additionally include preprocessing or \
         terminally failed work that has no saved per-model row._\n\n",
    );
    let mut provenance = Vec::new();
    for output in &outputs {
        if output.calls.is_empty() {
            if !output.model_source.is_empty() {
                provenance.push(format!(
                    "- **{}**: {} via {} `{}`{}",
                    output.step_label,
                    if output.model_policy.is_empty() {
                        "unspecified"
                    } else {
                        &output.model_policy
                    },
                    if output.model_transport.is_empty() {
                        "unknown transport"
                    } else {
                        &output.model_transport
                    },
                    output.model_source,
                    if output.model_catalog_updated_at.is_empty() {
                        String::new()
                    } else {
                        format!(" (catalog {})", output.model_catalog_updated_at)
                    },
                ));
            }
        } else {
            for call in &output.calls {
                if call.model_source.is_empty() {
                    continue;
                }
                provenance.push(format!(
                    "- **{} ({})**: {} via {} `{}`{}",
                    output.step_label,
                    if call.role.is_empty() {
                        "call"
                    } else {
                        &call.role
                    },
                    if call.model_policy.is_empty() {
                        "unspecified"
                    } else {
                        &call.model_policy
                    },
                    if call.model_transport.is_empty() {
                        "unknown transport"
                    } else {
                        &call.model_transport
                    },
                    call.model_source,
                    if call.model_catalog_updated_at.is_empty() {
                        String::new()
                    } else {
                        format!(" (catalog {})", call.model_catalog_updated_at)
                    },
                ));
            }
        }
    }
    if !provenance.is_empty() {
        md.push_str("<details><summary>Model resolution provenance</summary>\n\n");
        md.push_str(&provenance.join("\n"));
        md.push_str("\n\n</details>\n\n");
    }
    if any_cost {
        md.push_str(
            "*Cost is an estimate at published API list prices for steps run through a direct API. \
             It does not reflect subscription-plan billing.*\n\n",
        );
    }
    Some(md)
}
