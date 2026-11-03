//! Print regression coverage.

use super::*;

#[test]
fn print_html_is_self_contained_and_waits_for_fonts() {
    let html = build_print_report_html(
            "**#12. Identification needs work**\n\nAn equation: $y=x$.\n\n![Remote figure](https://example.invalid/pixel.png)",
            Some("# Pipeline\n\n**Document:** A paper\n\n**Workflow:** Full review\n\n## Run provenance\n\nBOTTOM DETAILS MUST NOT PRINT"),
        )
        .unwrap();
    assert!(html.contains("data:font/woff2;base64,"));
    assert!(html.contains("document.fonts.ready"));
    assert!(html.contains("class=\"report-document\""));
    assert!(html.contains("class=\"report-masthead\""));
    assert!(html.contains("class=\"report-body\""));
    assert!(!html.contains("class=\"report-provenance\""));
    assert!(!html.contains("BOTTOM DETAILS MUST NOT PRINT"));
    assert!(html.contains("class=\"comment-header\""));
    assert!(html.contains("class=\"comment-num\">12</span>"));
    assert!(!html.contains("<p><strong>#12."));
    assert!(html.contains("-apple-system"));
    assert!(html.contains("Iowan Old Style"));
    assert!(html.contains("@page { margin: 0 0 0.28in; }"));
    assert!(html.contains("counter(page) \" / \" counter(pages)"));
    assert!(html.contains("padding: 0.72in 0.82in 0.7in"));
    assert!(html.contains("box-decoration-break: clone"));
    assert!(html.contains(".report-body p { text-align: justify; hyphens: auto; }"));
    assert!(html.contains("Remote figure"));
    assert!(!html.contains("https://example.invalid/pixel.png"));
    assert!(!html.contains("url(fonts/"));
    assert!(!html.contains("cdn.jsdelivr"));
    assert!(!html.contains("<script src="));
    assert!(!html.contains("<link rel=\"stylesheet\""));
}

#[test]
fn printing_keeps_text_that_parses_as_html_and_removes_real_html() {
    let html = render_print_markdown(
        "In the <treated> group, n<N> units, and if p<q and r>s or a<b>c.\n\nBold <b>word</b>.\n\n<script>alert(2)</script>\n\n<iframe src=\"https://evil.example\"></iframe>",
    );
    assert!(
        html.contains(
            "In the &lt;treated&gt; group, n&lt;N&gt; units, and if p&lt;q and r&gt;s or a&lt;b&gt;c."
        ),
        "{html}"
    );
    assert!(html.contains("<b>word</b>"));
    assert!(!html.contains("alert(2)"));
    assert!(!html.contains("iframe"));
}

#[test]
fn printing_removes_html_that_spans_lines_and_keeps_its_text() {
    let html = render_print_markdown(
        "<!-- first hidden line\nsecond hidden line -->\n\n<style>\nhidden-rule { color: red; }\n</style>\n\n<script>\nhiddenCall(1);\n</script>\n\n<div>\nif p<q and r>s holds\n</div>",
    );
    assert!(html.contains("if p&lt;q and r&gt;s holds"), "{html}");
    for removed in ["hidden line", "hidden-rule", "hiddenCall"] {
        assert!(!html.contains(removed), "{removed}: {html}");
    }
}

#[test]
fn print_html_preserves_latex_until_katex_rendering() {
    let html = build_print_report_html(
            "Inline $x_t^* = \\frac{a_b}{c^2}$ and display:\n\n$$\\sum_{i=1}^n \\beta_i x_i$$\n\nTariffs rose from $5 to $8 per unit.\n\n`$code_with_underscore$`",
            Some("# Referee report\n\n## Run provenance\n\n| Provider | Model | Input | Output | Cached input |\n| --- | --- | ---: | ---: | ---: |\n| Codex | gpt-5.6-sol | 100 | 20 | 80 |"),
        )
        .unwrap();

    // Protected math is restored with backslash delimiters so the client-side
    // KaTeX pass does not scan for `$`, leaving currency amounts as prose.
    assert!(html.contains("\\(x_t^* = \\frac{a_b}{c^2}\\)"));
    assert!(html.contains("\\[\\sum_{i=1}^n \\beta_i x_i\\]"));
    assert!(html.contains("$5 to $8 per unit"));
    assert!(!html.contains("{left:'$'"));
    assert!(!html.contains("{left:'$$'"));
    assert!(html.contains("<code>$code_with_underscore$</code>"));
    assert!(!html.contains("x<em>t"));
    assert!(!html.contains("PIPELINEMATHPLACEHOLDER"));
    assert!(!html.contains("Run provenance"));
    assert!(!html.contains("gpt-5.6-sol"));
    assert!(html.contains("'\\\\bm':'\\\\boldsymbol'"));
}
