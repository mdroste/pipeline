//! Report rendering.

use crate::models::PipelineReport;
use crate::settings::Settings;
use regex::Regex;
use std::sync::LazyLock;
use std::time::Duration;

static PREAMBLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?mi)^(I(?:'|’)(?:ve|ll|m)|I (?:have|need|can|should|will)|Let me|Now (?:I|let)|OK[,.]|Alright[,.]|Here (?:is|are)|Looking at|After reading|Having read)[^\n]*\n*"
    ).expect("preamble regex is invalid")
});

const RUN_DETAILS_START: &str = "<!-- PIPELINE RUN DETAILS START -->";
const RUN_DETAILS_END: &str = "<!-- PIPELINE RUN DETAILS END -->";

/// Generate a fresh identifier for one logical report-producing call. Static
/// markers are easy for quoted source material or stale artifacts to collide
/// with; a random call nonce makes the report boundary unambiguous.
pub fn new_report_nonce() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|error| format!("RNG failed: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub fn report_markers(nonce: &str) -> (String, String) {
    (
        format!("<!-- PIPELINE REPORT {nonce} START -->"),
        format!("<!-- PIPELINE REPORT {nonce} END -->"),
    )
}

/// Produce the provider-neutral terminal response contract. Models may write
/// supporting artifacts, but the report itself always travels through the
/// provider's terminal assistant-response channel.
pub fn report_output_format(write_dir: Option<&str>, nonce: &str) -> String {
    let (start, end) = report_markers(nonce);
    let artifact_note = write_dir
        .map(|dir| {
            format!(
                "\nSupporting files (data tables, extracted figures) may be saved under \
                 {dir}/files/ and referenced from the report by relative path. Do not write \
                 the report itself to a file."
            )
        })
        .unwrap_or_default();
    format!(
        "OUTPUT FORMAT:\n\
         These instructions supersede any earlier output-format or report-file instructions.\n\
         Return the complete markdown report in your final response between these exact markers:\n\
         {start}\n\
         [complete report]\n\
         {end}\n\
         Put nothing before the start marker or after the end marker. Do not include progress \
         narration, acknowledgments, or a description of your process.{artifact_note}\n\
         Use `$...$` for inline math and `$$...$$` for display math. Do not use `\\(...\\)` or \
         `\\[...\\]` delimiters."
    )
}

/// Extract a complete, nonce-delimited report. Unlike the legacy helper below,
/// this fails closed: missing/duplicate markers, partial output, surrounding
/// narration, and empty reports are all rejected and can trigger a retry.
pub fn extract_report_envelope(text: &str, nonce: &str) -> Result<String, String> {
    let (start, end) = report_markers(nonce);
    if text.matches(&start).count() != 1 || text.matches(&end).count() != 1 {
        return Err("missing or duplicate report boundary markers".to_string());
    }
    let start_pos = text.find(&start).ok_or("missing report start marker")?;
    let content_start = start_pos + start.len();
    let relative_end = text[content_start..]
        .find(&end)
        .ok_or("missing report end marker")?;
    let end_pos = content_start + relative_end;
    if !text[..start_pos].trim().is_empty() || !text[end_pos + end.len()..].trim().is_empty() {
        return Err("text appeared outside the report boundary markers".to_string());
    }
    let report = text[content_start..end_pos].trim();
    if report.is_empty() {
        return Err("report between boundary markers was empty".to_string());
    }
    Ok(report.to_string())
}

/// Extract content between `<!-- REPORT START -->` and `<!-- REPORT END -->` markers.
/// Falls back to the full text (with preamble stripping) if markers aren't present.
pub fn strip_to_report(text: &str) -> String {
    let trimmed = text.trim();

    const START: &str = "<!-- REPORT START -->";
    const END: &str = "<!-- REPORT END -->";

    if let Some(start_pos) = trimmed.find(START) {
        let content_start = start_pos + START.len();
        if let Some(end_pos) = trimmed[content_start..].find(END) {
            return trimmed[content_start..content_start + end_pos]
                .trim()
                .to_string();
        }
        // START found but no END — take everything after START
        return trimmed[content_start..].trim().to_string();
    }

    // No markers — fall back to preamble stripping
    strip_preamble(trimmed)
}

pub(crate) fn fence_marker(line: &str) -> Option<(u8, usize)> {
    let bytes = line.trim_start().as_bytes();
    let marker = *bytes.first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let count = bytes.iter().take_while(|byte| **byte == marker).count();
    (count >= 3).then_some((marker, count))
}

fn normalize_math_line(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut output = String::with_capacity(line.len());
    let mut index = 0usize;
    let mut inline_ticks = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'`' {
            let count = bytes[index..]
                .iter()
                .take_while(|byte| **byte == b'`')
                .count();
            output.push_str(&line[index..index + count]);
            if inline_ticks == 0 {
                inline_ticks = count;
            } else if inline_ticks == count {
                inline_ticks = 0;
            }
            index += count;
            continue;
        }
        if inline_ticks == 0
            && bytes[index] == b'\\'
            && index + 1 < bytes.len()
            && (index == 0 || bytes[index - 1] != b'\\')
        {
            match bytes[index + 1] {
                b'(' | b')' => {
                    output.push('$');
                    index += 2;
                    continue;
                }
                b'[' | b']' => {
                    output.push_str("$$");
                    index += 2;
                    continue;
                }
                _ => {}
            }
        }
        let character = line[index..]
            .chars()
            .next()
            .expect("index remains on a UTF-8 boundary");
        output.push(character);
        index += character.len_utf8();
    }
    output
}

/// Canonicalize the two common LaTeX delimiter families to the dollar form
/// understood consistently by remark-math, the Markdown export path, and
/// KaTeX. Fenced and inline code are intentionally left untouched.
pub fn normalize_math_delimiters(markdown: &str) -> String {
    let mut output = String::with_capacity(markdown.len());
    let mut fence: Option<(u8, usize)> = None;
    for line in markdown.split_inclusive('\n') {
        if let Some((marker, count)) = fence_marker(line) {
            if let Some((open_marker, open_count)) = fence {
                if marker == open_marker && count >= open_count {
                    fence = None;
                }
            } else {
                fence = Some((marker, count));
            }
            output.push_str(line);
        } else if fence.is_some() {
            output.push_str(line);
        } else if let Some(normalized) = normalize_display_environment(line) {
            output.push_str(&normalized);
        } else {
            output.push_str(&normalize_math_line(line));
        }
    }
    output
}

fn normalize_display_environment(line: &str) -> Option<String> {
    static ENVIRONMENT_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(
            r"^([ \t]*)\\(begin|end)\{(equation\*?|displaymath|align\*?|gather\*?)\}[ \t]*(\r?\n)?$",
        )
        .expect("display-environment regex must compile")
    });
    let captures = ENVIRONMENT_RE.captures(line)?;
    let indent = captures.get(1).map(|value| value.as_str()).unwrap_or("");
    let boundary = captures.get(2)?.as_str();
    let environment = captures.get(3)?.as_str();
    let newline = captures.get(4).map(|value| value.as_str()).unwrap_or("");
    let replacement = match (boundary, environment) {
        ("begin", "equation" | "equation*" | "displaymath") => "$$",
        ("end", "equation" | "equation*" | "displaymath") => "$$",
        ("begin", "align" | "align*") => "$$\n\\begin{aligned}",
        ("end", "align" | "align*") => "\\end{aligned}\n$$",
        ("begin", "gather" | "gather*") => "$$\n\\begin{gathered}",
        ("end", "gather" | "gather*") => "\\end{gathered}\n$$",
        _ => return None,
    };
    Some(format!("{indent}{replacement}{newline}"))
}

/// Remove UI-only run telemetry from a report before public export.
pub fn clean_export_markdown(markdown: &str) -> String {
    let mut clean = markdown.to_string();
    while let Some(start) = clean.find(RUN_DETAILS_START) {
        let Some(relative_end) = clean[start + RUN_DETAILS_START.len()..].find(RUN_DETAILS_END)
        else {
            break;
        };
        let end = start + RUN_DETAILS_START.len() + relative_end + RUN_DETAILS_END.len();
        let mut next = String::with_capacity(clean.len() - (end - start));
        next.push_str(clean[..start].trim_end());
        next.push_str("\n\n");
        next.push_str(clean[end..].trim_start());
        clean = next;
    }
    clean
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
fn provider_in_api_mode(settings: &Settings, provider: &str) -> bool {
    match provider {
        "codex" => !settings.openai_api_key.trim().is_empty(),
        "gemini" => !settings.google_api_key.trim().is_empty(),
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
fn estimate_cost(
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
fn render_run_summary(report: &PipelineReport, settings: &Settings) -> Option<String> {
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

/// Render a PipelineReport to a markdown string.
pub fn render_markdown(
    report: &PipelineReport,
    diff_text: Option<&str>,
    elapsed: Duration,
    settings: &Settings,
) -> String {
    // Paper-shaped surveys get title/authors/type headers; custom surveys
    // get a generic header.
    let meta = crate::models::paper_view(&report.orientation).map(|v| v.metadata);
    let mut md = String::new();

    // The report is the document; the reviewed paper is its subject. Keeping
    // those roles distinct avoids presenting the paper title/author block as
    // though it were authored by Pipeline.
    let subject = meta
        .as_ref()
        .map(|m| m.title.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Selected input".to_string());
    md.push_str("# Referee Report\n\n");

    let provider = capitalize(&settings.preferred_provider);
    let model = settings
        .model_selection(&settings.preferred_provider)
        .label();
    let effort = match settings.preferred_provider.as_str() {
        "codex" => {
            if settings.codex_effort.is_empty() {
                "default".to_string()
            } else {
                settings.codex_effort.clone()
            }
        }
        "gemini" => "n/a".to_string(),
        _ => {
            if settings.claude_effort.is_empty() {
                "default".to_string()
            } else {
                settings.claude_effort.clone()
            }
        }
    };

    let type_prefix = meta
        .as_ref()
        .map(|m| format!("**Type**: {} · ", m.paper_type))
        .unwrap_or_default();
    md.push_str(RUN_DETAILS_START);
    md.push('\n');
    md.push_str(&format!("**Review subject**: {subject}  \n"));
    if let Some(m) = &meta {
        if !m.authors.is_empty() {
            md.push_str(&format!("**Authors**: {}  \n", m.authors.join(", ")));
        }
    }
    md.push_str(&format!(
        "{}**Reviewed**: {} · `{}`  \n**LLM**: {} · **Model**: {} · **Effort**: {} · **Generated in**: {}  \n*Report generated by Pipeline*\n\n",
        type_prefix, report.report_date, report.paper_hash,
        provider, model, effort, format_duration(elapsed)
    ));
    md.push_str(RUN_DETAILS_END);
    md.push_str("\n\n");

    // Warning for failed steps
    if !report.failed_steps.is_empty() {
        let labels: Vec<&str> = report
            .failed_steps
            .iter()
            .map(|f| f.step_label.as_str())
            .collect();
        md.push_str(&format!(
            "> **Incomplete report.** The following steps failed and are not reflected below: {}.\n\n",
            labels.join(", ")
        ));
    }

    md.push_str("---\n\n");

    // Consolidated issues — use final_output() which handles both new and legacy formats
    if let Some(final_text) = report.final_output() {
        md.push_str(&normalize_math_delimiters(&strip_to_report(final_text)));
        md.push_str("\n\n");
    }

    // Revision diff
    if let Some(diff) = diff_text {
        md.push_str("---\n\n");
        md.push_str("## Revision Diff\n\n");
        md.push_str(&normalize_math_delimiters(&strip_to_report(diff)));
        md.push_str("\n\n");
    }

    // Per-step timing / tokens / cost, when the run recorded any.
    if let Some(summary) = render_run_summary(report, settings) {
        md.push_str(RUN_DETAILS_START);
        md.push('\n');
        md.push_str("---\n\n");
        md.push_str(&summary);
        md.push_str(RUN_DETAILS_END);
        md.push_str("\n\n");
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

    #[test]
    fn nonce_report_envelope_accepts_only_complete_clean_boundaries() {
        let nonce = "abc123";
        let (start, end) = report_markers(nonce);
        let clean = format!("{start}\n## Report\n\nClean.\n{end}");
        assert_eq!(
            extract_report_envelope(&clean, nonce).unwrap(),
            "## Report\n\nClean."
        );

        assert!(extract_report_envelope(&format!("Narration\n{clean}"), nonce).is_err());
        assert!(extract_report_envelope(&format!("{start}\npartial"), nonce).is_err());
        assert!(extract_report_envelope(&clean, "different").is_err());
        assert!(extract_report_envelope(&format!("{clean}\n{clean}"), nonce).is_err());
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
        let input =
            "preamble\n<!-- REPORT START -->\nActual report.\n<!-- REPORT END -->\ntrailing";
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
        assert!(!provider_in_api_mode(&s, "gemini"));
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
}
