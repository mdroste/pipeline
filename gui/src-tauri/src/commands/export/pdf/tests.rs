use super::*;

fn request(markdown: &str) -> ReportPdfRequest {
    ReportPdfRequest {
        markdown: markdown.into(),
        provenance_markdown: None,
        run_id: None,
        input_path: None,
    }
}

#[test]
fn direct_export_is_offline_and_never_opens_a_print_dialog() {
    let (html, _) = prepare(request("# Report\n\nAn equation $x_t^2$.\n\n![remote](https://example.invalid/track)\n\n<script>fetch('file:///secret')</script>")).unwrap();
    assert!(html.contains("window.pipelineReportReady="));
    assert!(html.contains("document.fonts.ready"));
    assert!(html.contains("connect-src 'none'"));
    assert!(html.contains("window.pipelineNativePdfState"));
    assert!(html.contains("data:font/woff2;base64,"));
    assert!(!html.contains("window.print()"));
    assert!(!html.contains("example.invalid"));
    assert!(!html.contains("file:///secret"));
    assert!(html.contains("\\(x_t^2\\)"));
}

#[test]
fn oversized_reports_are_refused_before_rendering() {
    assert!(prepare(request(&"x".repeat(MAX_MARKDOWN + 1)))
        .unwrap_err()
        .contains("too large"));
    let mut large = request("# Report");
    large.provenance_markdown = Some("x".repeat(MAX_PROVENANCE + 1));
    assert!(prepare(large).unwrap_err().contains("too large"));
}

#[test]
fn native_layout_does_not_rewrite_quoted_report_source() {
    let (html, _) = prepare(request("# Source example\n\n```\n@media print { color: black; }\nrequestAnimationFrame(resolve)\n```\n")).unwrap();
    assert!(html.contains("@media print { color: black; }"));
    assert!(html.contains("requestAnimationFrame(resolve)\n</code>"));
    assert!(html.contains("@media screen, print {"));
    assert!(html.contains("new Promise(function(resolve){setTimeout(resolve,0);})"));
}

#[test]
fn the_manuscript_cannot_be_selected_as_the_destination() {
    let folder = tempfile::tempdir().unwrap();
    let input = folder.path().join("paper.pdf");
    std::fs::write(&input, b"original").unwrap();
    let mut export = request("# Report");
    export.input_path = Some(input.to_string_lossy().into_owned());
    let (_, protected) = prepare(export).unwrap();
    assert!(formats::check_destination(&input, &protected)
        .unwrap_err()
        .contains("never replaces"));
    assert_eq!(std::fs::read(&input).unwrap(), b"original");
}

#[test]
fn missing_invalid_and_oversized_output_cannot_be_saved() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("report.pdf");
    assert!(read_pdf(&path).is_err());
    std::fs::write(&path, b"not a PDF").unwrap();
    assert!(read_pdf(&path).unwrap_err().contains("valid PDF"));
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_PDF + 1).unwrap();
    assert!(read_pdf(&path).unwrap_err().contains("64 MB"));
    assert!(read_pdf(folder.path()).is_err());
}

#[test]
fn a_failed_renderer_leaves_the_destination_untouched() {
    let folder = tempfile::tempdir().unwrap();
    let target = folder.path().join("saved.pdf");
    std::fs::write(&target, b"previous export").unwrap();
    let absent_renderer = folder.path().join("missing-renderer");
    assert!(render_and_save("report", &absent_renderer, &target, &[]).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"previous export");
}

#[test]
#[ignore = "Runs the native GUI renderer; set PIPELINE_TEST_PDF_EXECUTABLE to the built Pipeline GUI executable; requires a desktop session"]
fn native_renderer_exports_a_readable_multipage_pdf() {
    let executable = PathBuf::from(
        std::env::var_os("PIPELINE_TEST_PDF_EXECUTABLE")
            .expect("Set PIPELINE_TEST_PDF_EXECUTABLE to the GUI executable"),
    );
    let mut markdown = String::from("# Review of a sample paper\n\nAn equation: $y_t = \\alpha + \\beta x_t + \\varepsilon_t$.\n\nCurrency: $5 to $8.\n\n");
    for i in 1..=40 {
        markdown.push_str(&format!("**#{i}. Identification and interpretation**\n\nThe estimate describes the response of output to an identified monetary policy shock. Explain how the identifying restriction distinguishes the policy innovation from news about future productivity. The comparison should hold the information set fixed.\n\n$$\\sum_{{j=0}}^\\infty \\beta^j u(c_{{t+j}})$$\n\n"));
    }
    markdown.push_str("## Last section\n\nEnd of the complete report.\n");
    markdown.push_str("\n## Estimates\n\n| Specification | Estimate | Standard error |\n| --- | ---: | ---: |\n| Baseline | 0.25 | 0.08 |\n| With controls | 0.21 | 0.09 |\n\nA wide equation:\n\n");
    markdown.push_str(&format!(
        "$$Y_t = {}$$\n",
        (1..=8)
            .map(|i| format!("\\alpha_{{{i}}} X_{{{i},t}}"))
            .collect::<Vec<_>>()
            .join(" + ")
    ));
    markdown.push_str(
        "\n## Extended estimates\n\n| Sample | Coefficient | Notes |\n| --- | ---: | --- |\n",
    );
    for row in 1..=90 {
        markdown.push_str(&format!(
            "| Sample-{row:03} | 0.25 | Robust estimate for José's sample |\n"
        ));
    }
    markdown.push_str("\nComplete after the final table.\n");
    let mut export = request(&markdown);
    export.provenance_markdown = Some(
        "# Pipeline review\n\n**Document:** Sample paper\n\n**Workflow:** Paper Review".into(),
    );
    let (html, _) = prepare(export).unwrap();
    let destination = tempfile::tempdir().unwrap();
    let target = destination.path().join("José review.pdf");
    std::fs::write(&target, b"previous export").unwrap();
    let saved = render_and_save(&html, &executable, &target, &[]).unwrap();
    assert_eq!(
        super::super::desktop::validated_recent_export(&saved).unwrap(),
        target.canonicalize().unwrap()
    );
    let bytes = read_pdf(&target).unwrap();
    if let Some(path) = std::env::var_os("PIPELINE_TEST_PDF_OUTPUT") {
        std::fs::write(path, &bytes).unwrap();
    }
    let pdf = lopdf::Document::load_mem(&bytes).unwrap();
    let pages: Vec<u32> = pdf.get_pages().keys().copied().collect();
    assert!(pages.len() >= 3, "Expected a paginated report");
    // Poppler handles the native engines' embedded fonts and CMaps.
    let layout = crate::output::formats::pdftotext_bbox_layout(&bytes).unwrap();
    assert_eq!(layout.len(), pages.len());
    let text = layout
        .iter()
        .flat_map(|page| page.words.iter().map(|word| word.text.as_str()))
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(
        text.matches("Identification and interpretation").count(),
        40
    );
    assert_eq!(text.matches('∑').count(), 40, "A display equation was lost");
    assert!(
        text.contains('α') && text.contains('β'),
        "Math fonts were lost"
    );
    assert!(text.contains("Currency: $5 to $8."), "Currency was changed");
    assert!(
        text.contains("End of the complete report"),
        "The end was omitted"
    );
    // PDF text readers may visit table columns before rows. Verify the row's
    // positions instead of depending on their extraction order.
    let estimates = layout
        .iter()
        .find(|page| page.words.iter().any(|word| word.text == "Baseline"))
        .unwrap();
    let baseline = estimates
        .words
        .iter()
        .find(|word| word.text == "Baseline")
        .unwrap();
    let estimate = estimates
        .words
        .iter()
        .find(|word| word.text == "0.25")
        .unwrap();
    let error = estimates
        .words
        .iter()
        .find(|word| word.text == "0.08")
        .unwrap();
    assert!(baseline.x_max < estimate.x_min && estimate.x_max < error.x_min);
    assert!((baseline.y_min - estimate.y_min).abs() < 1.);
    assert!((baseline.y_min - error.y_min).abs() < 1.);
    assert!(text.contains("Complete after the final table."));
    for row in 1..=90 {
        assert_eq!(
            text.matches(&format!("Sample-{row:03}")).count(),
            1,
            "Table row missing or duplicated"
        );
    }
    let table_pages: Vec<_> = layout
        .iter()
        .filter(|page| {
            page.words
                .iter()
                .any(|word| word.text.starts_with("Sample-"))
        })
        .collect();
    assert!(table_pages.len() >= 2, "Expected a table spanning pages");
    assert!(
        table_pages.len() < 10,
        "Table rows were spread across nearly empty pages"
    );
    for page in table_pages {
        let first_row = page
            .words
            .iter()
            .filter(|word| word.text.starts_with("Sample-"))
            .map(|word| word.y_min)
            .reduce(f32::min)
            .unwrap();
        // Poppler can split letter-spaced headings into several word boxes.
        // Join only the heading band immediately above this page's first row.
        let header = page
            .words
            .iter()
            .filter(|word| word.y_max <= first_row && word.y_min > first_row - 40.)
            .map(|word| word.text.as_str())
            .collect::<String>();
        assert!(
            header.contains("COEFFICIENT"),
            "Missing repeated table header; page words: {:?}",
            page.words.iter().map(|word| &word.text).collect::<Vec<_>>()
        );
    }
    for (index, page) in layout.iter().enumerate() {
        for heading in page
            .words
            .iter()
            .filter(|word| word.text == "Identification")
        {
            assert!(
                page.words.iter().any(|word| word.text == "estimate"
                    && word.y_min > heading.y_min
                    && word.y_min < heading.y_min + 90.),
                "Issue heading was separated from its paragraph"
            );
        }
        let footer = page
            .words
            .iter()
            .filter(|word| word.y_min > 760.)
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(footer, format!("{} / {}", index + 1, layout.len()));
    }
    for word in layout.iter().flat_map(|page| &page.words) {
        assert!(
            word.x_min >= 0. && word.x_max <= 612. && word.y_min >= 0. && word.y_max <= 792.,
            "Word falls off the page: {word:?}"
        );
    }
}
