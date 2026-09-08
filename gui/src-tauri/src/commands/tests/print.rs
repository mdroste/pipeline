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
