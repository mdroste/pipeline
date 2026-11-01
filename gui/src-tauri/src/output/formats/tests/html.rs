use super::*;

fn hostile_export() -> ReportExport {
    let mut first = finding(
        1,
        "Estimate <script>alert('title')</script> & bias",
        "high",
        vec![
            evidence(
                "He said \"<img src=x onerror=alert(1)>\"",
                Some(4),
                "verified",
            ),
            evidence(
                "Absent quotation that cannot be verified",
                None,
                "not_found",
            ),
        ],
    );
    first.problem = "See [the appendix](javascript:alert(1)) and <iframe src=\"https://evil.example\"></iframe>.".to_string();
    let mut second = finding(2, "Undefined notation", "", vec![]);
    second.problem.clear();
    second.body = "**Body** with ![chart](https://tracker.example/pixel.png) and <b onclick=\"x()\">bold</b>.".to_string();
    second.category = "Exposition \"quoted\" <tag>".to_string();
    export_with(vec![first, second])
}

#[test]
fn escapes_and_sanitizes_every_model_written_value() {
    let html = render_html_report(&hostile_export());
    let body = html.split("<body>").nth(1).unwrap();
    let markup = body.rsplit_once("<script>").unwrap().0;
    assert!(!markup.contains("<script"));
    assert!(!markup.contains("<iframe"));
    assert!(!markup.contains("<img"));
    let handler = regex::Regex::new(r"(?i)<[^>]*\son[a-z]+\s*=").unwrap();
    assert!(
        !handler.is_match(markup),
        "event handler attribute survived"
    );
    assert!(!markup.contains("javascript:"));
    assert!(!markup.contains("tracker.example"));
    assert!(
        markup.contains("Estimate &lt;script&gt;alert(&#39;title&#39;)&lt;/script&gt; &amp; bias")
    );
    assert!(markup.contains("He said &quot;&lt;img src=x onerror=alert(1)&gt;&quot;"));
    assert!(markup.contains("Exposition &quot;quoted&quot; &lt;tag&gt;"));
    assert!(markup.contains("<strong>Body</strong>"));
    assert!(markup.contains("[Image: chart]"));
    assert_eq!(html.matches("<script>").count(), 1);
}

#[test]
fn is_self_contained_with_a_hash_pinned_content_security_policy() {
    let html = render_html_report(&hostile_export());
    assert!(html.starts_with("<!DOCTYPE html>"));
    for forbidden in ["<link", "src=\"http", "url(http", "@import", "<base"] {
        assert!(!html.contains(forbidden), "found {forbidden}");
    }
    let policy = html
        .split("http-equiv=\"Content-Security-Policy\" content=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    assert!(policy.starts_with("default-src 'none'"));
    assert!(policy.contains("form-action 'none'"));
    let hash = |tag: &str| {
        let content = html
            .split(&format!("<{tag}>"))
            .nth(1)
            .unwrap()
            .split(&format!("</{tag}>"))
            .next()
            .unwrap();
        use base64::Engine as _;
        use sha2::Digest as _;
        format!(
            "'sha256-{}'",
            base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(content))
        )
    };
    assert!(policy.contains(&format!("style-src {}", hash("style"))));
    assert!(policy.contains(&format!("script-src {}", hash("script"))));
}

#[test]
fn renders_filterable_cards_with_badges_and_evidence_states() {
    let html = render_html_report(&hostile_export());
    assert_eq!(html.matches("<article class=\"finding\"").count(), 2);
    let card = concat!(
        r#"<article class="finding" id="finding-1" data-severity="high" data-category="c0" data-verification="verified not-found">"#,
        r#"<header><span class="number">1</span><h3>Estimate &lt;script&gt;alert(&#39;title&#39;)&lt;/script&gt; &amp; bias</h3></header>"#,
        r#"<p class="badges"><span class="badge severity-high">High severity</span><span class="badge">Confidence: medium</span>"#,
        r#"<span class="badge">Correctness</span></p>"#
    );
    assert!(html.contains(card), "card markup changed:\n{html}");
    assert!(html.contains(
        r#"<p class="locator"><span class="status status-verified">Quote found</span> · Page 4</p>"#
    ));
    assert!(html.contains(r#"<span class="status status-not-found">Quote not found</span>"#));
    assert!(html.contains(r#"data-severity="unrated""#));
    assert!(html
        .contains(r#"<option value="high">High</option><option value="unrated">Unrated</option>"#));
    assert!(
        html.contains(r#"<option value="c1">Exposition &quot;quoted&quot; &lt;tag&gt;</option>"#)
    );
    assert!(
        html.contains(r#"<option value="verified">Quote found</option><option value="not-found">"#)
    );
    assert!(html.contains("1 of 2 evidence items have a quotation Pipeline verified"));
    assert!(html.contains("Equations were extracted as text."));
    assert!(html.contains("<details><summary>Complete report text</summary>"));
}

#[test]
fn unverified_runs_and_reports_without_findings_still_render() {
    let unverified = export_with(vec![finding(
        1,
        "Claim",
        "low",
        vec![evidence("A quotation long enough", Some(2), "unverified")],
    )]);
    let html = render_html_report(&unverified);
    assert!(html.contains(r#"data-verification="unchecked""#));
    assert!(html.contains("Not checked"));
    assert!(html.contains("0 of 1 evidence items"));

    let mut plain = export_with(Vec::new());
    plain.report_markdown =
        "## Overall assessment\n\nSound paper.\n\n## Quality and limitations\n\nNone.".to_string();
    let html = render_html_report(&plain);
    assert!(html.contains("<h2>Overall assessment</h2>"));
    assert!(!html.contains("id=\"filter-severity\""));
    assert!(!html.contains("<script>"));
    assert_eq!(html.matches("Quality and limitations").count(), 1);
}

#[test]
fn latex_in_a_report_keeps_its_source_and_stays_escaped() {
    let mut export = export_with(Vec::new());
    export.report_markdown =
        "Both $x^*$ and $y^*$ and $\\{a_i\\}$ at $a\\,b$, from \\(z^*\\) and *real* emphasis.\n\n$$\\sum_i x_i * y_i$$\n\nCosts $5 and $8.\n\nHostile $<script>alert(1)</script>$.".to_string();
    let html = render_html_report(&export);
    let body = html.split("<body>").nth(1).unwrap();
    for span in [
        "$x^*$ and $y^*$ and $\\{a_i\\}$ at $a\\,b$",
        "$z^*$",
        "$$\\sum_i x_i * y_i$$",
        "Costs $5 and $8.",
    ] {
        assert!(body.contains(span), "{span} in {body}");
    }
    assert!(body.contains("<em>real</em>"));
    assert!(!body.contains("x^<em>"));
    assert!(!body.contains("PIPELINEMATHPLACEHOLDER"));
    assert!(!body.contains("<script>alert"));
    assert!(body.contains("$&lt;script&gt;alert(1)&lt;/script&gt;$"));
}
