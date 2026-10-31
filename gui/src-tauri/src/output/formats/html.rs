//! One self-contained HTML report that opens offline in any browser.
//!
//! Model-written Markdown is rendered and then sanitized with ammonia; every
//! other model-written value is escaped. A Content-Security-Policy pinned to
//! the hashes of the inline stylesheet and filter script blocks any other
//! script, style, network request, or form submission even if sanitization
//! were bypassed. Images lose their sources, so nothing remote is fetched.

use super::model::{severity_label, ExportFinding, ReportExport, Verification};
use super::text::escape_xml as escape;

const STYLE: &str = r#"
:root { color-scheme: light dark; --ink: #1f2328; --muted: #5f6b76; --rule: #d8dbdd; --paper: #ffffff;
  --canvas: #f3f2ef; --accent: #18364d; --quote: #4f5961; --chip: #eef0f2;
  --high: #b42318; --medium: #b54708; --low: #175cd3; --ok: #067647; --warn: #b54708; --bad: #b42318; }
@media (prefers-color-scheme: dark) { :root { --ink: #e6e8ea; --muted: #a0a9b2; --rule: #3a4047; --paper: #181b1f;
  --canvas: #0f1114; --accent: #9cc3e4; --quote: #b9c1c8; --chip: #262b31;
  --high: #ff8f84; --medium: #f7b267; --low: #8ab4ff; --ok: #6fd3a6; --warn: #f7b267; --bad: #ff8f84; } }
* { box-sizing: border-box; }
html { background: var(--canvas); }
body { margin: 0; color: var(--ink); font: 16px/1.62 "Iowan Old Style", Palatino, Georgia, "Times New Roman", serif; }
.report { width: min(52rem, 100%); margin: 2rem auto; padding: 3rem 3.5rem 3.5rem; background: var(--paper);
  border: 1px solid var(--rule); border-radius: 4px; }
h1, h2, h3, .eyebrow, .details, .filters, .badges, .locator, .number, summary, footer {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Arial, sans-serif; }
h1 { margin: 0.2rem 0 1rem; font-size: 1.65rem; line-height: 1.25; letter-spacing: -0.02em; }
h2 { margin: 2.6rem 0 1rem; padding-bottom: 0.4rem; border-bottom: 1px solid var(--rule); color: var(--accent);
  font-size: 0.8rem; letter-spacing: 0.1em; text-transform: uppercase; }
h3 { margin: 0; font-size: 1rem; line-height: 1.4; }
.eyebrow { margin: 0; color: var(--muted); font-size: 0.72rem; font-weight: 700; letter-spacing: 0.12em; text-transform: uppercase; }
.details { display: grid; grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr)); gap: 0.5rem 1rem; margin: 0 0 1rem;
  font-size: 0.8rem; }
.details dt { color: var(--muted); font-weight: 700; }
.details dd { margin: 0; overflow-wrap: anywhere; }
.notice { margin: 0; color: var(--muted); font-size: 0.85rem; font-style: italic; }
.filters { display: flex; flex-wrap: wrap; gap: 0.6rem 1rem; align-items: end; margin: 0 0 1.25rem; font-size: 0.8rem; }
.filters label { display: grid; gap: 0.2rem; color: var(--muted); font-weight: 700; }
.filters select { min-width: 9rem; padding: 0.3rem; color: var(--ink); background: var(--paper); border: 1px solid var(--rule); border-radius: 4px; }
#filter-count { margin: 0 0 0.35rem; color: var(--muted); }
.finding { margin: 0 0 1.4rem; padding: 1rem 0 0; border-top: 1px solid var(--rule); }
.finding[hidden] { display: none; }
.finding > header { display: grid; grid-template-columns: 2rem 1fr; gap: 0.6rem; align-items: start; }
.number { display: inline-flex; width: 1.6rem; height: 1.6rem; align-items: center; justify-content: center; border-radius: 999px;
  background: var(--accent); color: var(--paper); font-size: 0.7rem; font-weight: 750; }
.badges { display: flex; flex-wrap: wrap; gap: 0.35rem; margin: 0.5rem 0 0.6rem 2.6rem; font-size: 0.72rem; }
.badge { padding: 0.1rem 0.5rem; border-radius: 999px; background: var(--chip); color: var(--muted); font-weight: 650; }
.severity-high { color: var(--high); } .severity-medium { color: var(--medium); } .severity-low { color: var(--low); }
.finding-body, .evidence { margin-left: 2.6rem; }
.evidence { padding: 0; list-style: none; }
.evidence li { margin: 0.7rem 0; }
blockquote { margin: 0.4rem 0; padding: 0.3rem 0 0.3rem 0.9rem; border-left: 2px solid var(--rule); color: var(--quote); font-style: italic; }
.locator { margin: 0.2rem 0 0; color: var(--muted); font-size: 0.75rem; }
.status { font-weight: 750; }
.status-verified, .status-verified-normalized { color: var(--ok); }
.status-partial { color: var(--warn); } .status-not-found { color: var(--bad); }
code { padding: 0.1rem 0.3rem; border-radius: 3px; background: var(--chip); font-size: 0.85em; overflow-wrap: anywhere; }
pre { padding: 0.8rem; overflow: auto; border-radius: 4px; background: var(--chip); white-space: pre-wrap; }
pre code { padding: 0; }
table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
th, td { padding: 0.35rem 0.5rem; border-bottom: 1px solid var(--rule); text-align: left; vertical-align: top; }
details { margin-top: 2.5rem; }
summary { cursor: pointer; color: var(--accent); font-weight: 700; }
footer { margin-top: 2.5rem; padding-top: 1rem; border-top: 1px solid var(--rule); color: var(--muted); font-size: 0.75rem; }
@media (max-width: 640px) { .report { margin: 0; padding: 1.5rem 1rem 2rem; border: 0; border-radius: 0; }
  .badges, .finding-body, .evidence { margin-left: 0; } }
@media print { html, body { background: #fff; } .report { margin: 0; padding: 0; border: 0; } .filters { display: none; } }
"#;

const FILTER_SCRIPT: &str = r#"
(function () {
  var severity = document.getElementById('filter-severity');
  var category = document.getElementById('filter-category');
  var evidence = document.getElementById('filter-verification');
  var count = document.getElementById('filter-count');
  if (!severity || !category || !evidence || !count) return;
  var cards = Array.prototype.slice.call(document.querySelectorAll('article.finding'));
  function apply() {
    var shown = 0;
    cards.forEach(function (card) {
      var states = ' ' + card.getAttribute('data-verification') + ' ';
      var visible = (!severity.value || card.getAttribute('data-severity') === severity.value) &&
        (!category.value || card.getAttribute('data-category') === category.value) &&
        (!evidence.value || states.indexOf(' ' + evidence.value + ' ') >= 0);
      card.hidden = !visible;
      if (visible) shown += 1;
    });
    count.textContent = shown + ' of ' + cards.length + ' findings shown';
  }
  [severity, category, evidence].forEach(function (control) { control.addEventListener('change', apply); });
  apply();
})();
"#;

fn csp_hash(source: &str) -> String {
    use base64::Engine as _;
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(source.as_bytes());
    format!(
        "'sha256-{}'",
        base64::engine::general_purpose::STANDARD.encode(digest)
    )
}

/// Render untrusted Markdown to sanitized HTML.
pub(super) fn sanitized_markdown(markdown: &str) -> String {
    use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag, TagEnd};
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    // An offline file cannot show remote images; keep their alt text instead.
    let events = Parser::new_ext(markdown, options).map(|event| match event {
        Event::Start(Tag::Image { .. }) => Event::Text(CowStr::Borrowed("[Image: ")),
        Event::End(TagEnd::Image) => Event::Text(CowStr::Borrowed("]")),
        other => other,
    });
    let mut rendered = String::new();
    html::push_html(&mut rendered, events);
    ammonia::Builder::default()
        .url_schemes(["http", "https", "mailto"].into_iter().collect())
        .url_relative(ammonia::UrlRelative::Deny)
        .rm_tags(["img"])
        .clean(&rendered)
        .to_string()
}

fn token(value: &str) -> String {
    let token: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let token = token.trim_matches('-').to_string();
    if token.is_empty() {
        "unrated".to_string()
    } else {
        token
    }
}

fn severity_rank(severity: &str) -> usize {
    match severity {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        "" => 4,
        _ => 3,
    }
}

fn option(value: &str, label: &str) -> String {
    format!(
        r#"<option value="{}">{}</option>"#,
        escape(value),
        escape(label)
    )
}

fn filters(export: &ReportExport) -> String {
    let mut severities: Vec<&str> = export
        .findings
        .iter()
        .map(|finding| finding.severity.as_str())
        .collect();
    severities.sort_by_key(|severity| (severity_rank(severity), severity.to_string()));
    severities.dedup();
    let mut states: Vec<Verification> = export
        .findings
        .iter()
        .flat_map(ExportFinding::verification_states)
        .collect();
    states.sort();
    states.dedup();
    let mut html =
        String::from(r#"<div class="filters" role="group" aria-label="Filter findings">"#);
    html.push_str(r#"<label>Severity<select id="filter-severity"><option value="">All</option>"#);
    for severity in severities {
        html.push_str(&option(&token(severity), &severity_label(severity)));
    }
    html.push_str(r#"</select></label><label>Category<select id="filter-category"><option value="">All</option>"#);
    for (index, category) in export.categories().iter().enumerate() {
        let label = if category.is_empty() {
            "Uncategorized"
        } else {
            category
        };
        html.push_str(&option(&format!("c{index}"), label));
    }
    html.push_str(r#"</select></label><label>Evidence<select id="filter-verification"><option value="">All</option>"#);
    for state in states {
        html.push_str(&option(state.slug(), state.label()));
    }
    html.push_str(r#"</select></label><p id="filter-count" aria-live="polite"></p></div>"#);
    html
}

fn finding_card(export: &ReportExport, finding: &ExportFinding) -> String {
    let category_index = export
        .categories()
        .iter()
        .position(|category| *category == finding.category)
        .unwrap_or(0);
    let states: Vec<&str> = finding
        .verification_states()
        .into_iter()
        .map(Verification::slug)
        .collect();
    let mut html = format!(
        r#"<article class="finding" id="finding-{number}" data-severity="{severity}" data-category="c{category_index}" data-verification="{states}">"#,
        number = finding.number,
        severity = escape(&token(&finding.severity)),
        states = states.join(" "),
    );
    html.push_str(&format!(
        r#"<header><span class="number">{}</span><h3>{}</h3></header><p class="badges">"#,
        finding.number,
        escape(&finding.title)
    ));
    html.push_str(&format!(
        r#"<span class="badge severity-{}">{} severity</span>"#,
        escape(&token(&finding.severity)),
        escape(&finding.severity_label())
    ));
    if !finding.confidence.is_empty() {
        html.push_str(&format!(
            r#"<span class="badge">Confidence: {}</span>"#,
            escape(&finding.confidence)
        ));
    }
    if !finding.category.is_empty() {
        html.push_str(&format!(
            r#"<span class="badge">{}</span>"#,
            escape(&finding.category)
        ));
    }
    html.push_str(r#"</p><div class="finding-body">"#);
    if finding.problem.is_empty() {
        html.push_str(&sanitized_markdown(&finding.body));
    } else {
        for (label, value) in [
            ("Problem.", &finding.problem),
            ("Consequence.", &finding.consequence),
            ("Recommended action.", &finding.recommended_action),
        ] {
            if !value.is_empty() {
                html.push_str(&sanitized_markdown(&format!("**{label}** {value}")));
            }
        }
    }
    html.push_str("</div>");
    if !finding.evidence.is_empty() {
        html.push_str(r#"<ul class="evidence" aria-label="Evidence">"#);
        for evidence in &finding.evidence {
            html.push_str("<li>");
            if !evidence.quote.is_empty() {
                html.push_str(&format!(
                    "<blockquote>{}</blockquote>",
                    escape(&evidence.quote)
                ));
            }
            html.push_str(&format!(
                r#"<p class="locator"><span class="status status-{}">{}</span>"#,
                evidence.verification.slug(),
                escape(evidence.verification.label())
            ));
            if !evidence.locator.is_empty() {
                html.push_str(&format!(" · {}", escape(&evidence.locator)));
            }
            html.push_str("</p></li>");
        }
        html.push_str("</ul>");
    }
    html.push_str("</article>");
    html
}

fn summary(export: &ReportExport) -> String {
    let count = |severity: &str| {
        export
            .findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .count()
    };
    let total = export.findings.len();
    let mut parts = vec![format!(
        "{total} finding{}",
        if total == 1 { "" } else { "s" }
    )];
    for severity in ["high", "medium", "low"] {
        let number = count(severity);
        if number > 0 {
            parts.push(format!(
                "{number} {}",
                severity_label(severity).to_lowercase()
            ));
        }
    }
    let evidence: Vec<_> = export
        .findings
        .iter()
        .flat_map(|finding| finding.evidence.iter())
        .collect();
    let verified = evidence
        .iter()
        .filter(|item| {
            matches!(
                item.verification,
                Verification::Verified | Verification::VerifiedWithNormalization
            )
        })
        .count();
    format!(
        r#"<section aria-labelledby="summary-heading"><h2 id="summary-heading">Summary</h2><p>{}.</p><p>{verified} of {} evidence items have a quotation Pipeline verified in the reviewed document.</p></section>"#,
        escape(&parts.join(" · ")),
        evidence.len()
    )
}

fn quality(export: &ReportExport) -> String {
    let quality = &export.quality;
    let mut html = String::from(
        r#"<section aria-labelledby="quality-heading"><h2 id="quality-heading">Quality and limitations</h2>"#,
    );
    if !export.meta.status.is_empty() {
        html.push_str(&format!(
            "<p>Final status: {}</p>",
            escape(&export.meta.status)
        ));
    }
    if quality.schema_version > 0 {
        html.push_str(&format!(
            "<p>Evidence: {} verified, {} partially verified, {} unverified, {} not checkable; {} item(s) for manual review.</p>",
            quality.verified_evidence,
            quality.partially_verified_evidence,
            quality.unverified_evidence,
            quality.not_checkable_evidence,
            quality.manual_review_items
        ));
    }
    if quality.limitations.is_empty() {
        html.push_str("<p>No limitations were recorded for this run.</p>");
    } else {
        html.push_str("<ul>");
        for limitation in &quality.limitations {
            html.push_str(&format!("<li>{}</li>", escape(limitation)));
        }
        html.push_str("</ul>");
    }
    html.push_str("</section>");
    html
}

/// Build the complete HTML document.
pub fn render_html_report(export: &ReportExport) -> String {
    let meta = &export.meta;
    let mut body = String::new();
    body.push_str(
        r#"<main class="report"><header class="masthead"><p class="eyebrow">Review report</p>"#,
    );
    body.push_str(&format!(
        "<h1>{}</h1><dl class=\"details\">",
        escape(&meta.title)
    ));
    for (label, value) in [
        ("Reviewed file", &meta.input_name),
        ("Review", &meta.workflow),
        ("Date", &meta.date),
        ("Status", &meta.status),
    ] {
        if !value.is_empty() {
            body.push_str(&format!(
                "<div><dt>{label}</dt><dd>{}</dd></div>",
                escape(value)
            ));
        }
    }
    body.push_str(concat!(
        r#"</dl><p class="notice">Generated by Pipeline. Findings are model-written. "#,
        "Each evidence label states whether Pipeline found the quotation in the reviewed document.</p></header>"
    ));
    if export.findings.is_empty() {
        body.push_str(r#"<section class="markdown" aria-label="Report">"#);
        body.push_str(&sanitized_markdown(&export.report_markdown));
        body.push_str("</section>");
        if !export
            .report_markdown
            .contains("## Quality and limitations")
        {
            body.push_str(&quality(export));
        }
    } else {
        body.push_str(&summary(export));
        body.push_str(r#"<section aria-labelledby="findings-heading"><h2 id="findings-heading">Findings</h2>"#);
        body.push_str(&filters(export));
        for finding in &export.findings {
            body.push_str(&finding_card(export, finding));
        }
        body.push_str("</section>");
        body.push_str(&quality(export));
        if !export.report_markdown.is_empty() {
            body.push_str("<details><summary>Complete report text</summary>");
            body.push_str(&sanitized_markdown(&export.report_markdown));
            body.push_str("</details>");
        }
    }
    body.push_str(concat!(
        "<footer>Created with Pipeline. This file is self-contained: ",
        "it loads no fonts, images, or scripts from the internet.</footer></main>"
    ));
    let policy = format!(
        "default-src 'none'; img-src data:; style-src {}; script-src {}; base-uri 'none'; form-action 'none'",
        csp_hash(STYLE),
        csp_hash(FILTER_SCRIPT)
    );
    format!(
        concat!(
            "<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">",
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
            "<meta http-equiv=\"Content-Security-Policy\" content=\"{policy}\">",
            "<meta name=\"referrer\" content=\"no-referrer\">",
            "<title>{title}</title><style>{style}</style></head><body>{body}{script}</body></html>\n"
        ),
        policy = policy,
        title = escape(&meta.title),
        style = STYLE,
        body = body,
        script = if export.findings.is_empty() {
            String::new()
        } else {
            format!("<script>{FILTER_SCRIPT}</script>")
        }
    )
}
