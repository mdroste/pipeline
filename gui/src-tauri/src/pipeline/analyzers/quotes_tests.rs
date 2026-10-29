use super::*;
use crate::document_bundle::{
    DocumentBundle, DocumentExtraction, DocumentNode, DocumentProvenance,
};
use crate::models::{Finding, FindingSet};

fn node(id: &str, order: u32, page: Option<u32>, text: &str) -> DocumentNode {
    DocumentNode {
        id: id.into(),
        kind: "paragraph".into(),
        order,
        page,
        parent_id: None,
        label: None,
        number: None,
        text: text.into(),
        asset_ids: vec![],
        representations: vec![],
        provenance: DocumentProvenance {
            origin_id: "origin-1".into(),
            method: "fixture".into(),
            confidence: None,
        },
    }
}

fn bundle(source_kind: &str, nodes: Vec<DocumentNode>) -> DocumentBundle {
    DocumentBundle {
        schema_version: "1".into(),
        bundle_id: "bundle-1".into(),
        source_kind: source_kind.into(),
        origins: vec![],
        pages: vec![],
        nodes,
        assets: vec![],
        links: vec![],
        extraction: DocumentExtraction {
            method: "fixture".into(),
            source_path: "paper.pdf".into(),
            paper_hash: "hash".into(),
        },
        quality: vec![],
    }
}

fn paper() -> DocumentBundle {
    bundle(
        "pdf",
        vec![
            node("n1", 1, Some(1), "We study the effect of minimum wages on\nteen employment in 1990\u{2013}2010."),
            node("n2", 2, Some(2), "Our estimates imply an elasticity of \u{2212}0.12 with a\nstandard error of 0.04."),
            node("n3", 3, Some(2), "The \u{fb01}rm-level evidence is consistent with mono-\npsony models of the labor market."),
            node("n4", 4, Some(3), "We conclude that the policy had small effects."),
        ],
    )
}

fn evidence(quote: &str) -> FindingEvidence {
    FindingEvidence {
        evidence_type: "document".into(),
        verification_status: "unverified".into(),
        quote: quote.into(),
        ..Default::default()
    }
}

fn no_source(_: &str) -> Option<String> {
    None
}

fn check(bundle: &DocumentBundle, evidence: &FindingEvidence) -> Outcome {
    let index = DocumentIndex::from_bundle(bundle);
    verify_evidence(&index, evidence, &mut no_source)
}

#[test]
fn exact_quote_at_cited_node_is_verified_with_its_byte_range() {
    let paper = paper();
    let mut item = evidence("an elasticity of \u{2212}0.12 with a standard error");
    item.node_id = "n2".into();
    let outcome = check(&paper, &item);
    assert_eq!(outcome.status, VERIFIED);
    assert_eq!(outcome.method, "exact");
    let matched = outcome.matched.unwrap();
    assert_eq!(matched.scope, "node");
    assert_eq!(matched.node_id, "n2");
    let text = &paper.nodes[1].text;
    assert_eq!(
        &text[matched.start..matched.end],
        "an elasticity of \u{2212}0.12 with a\nstandard error"
    );
}

#[test]
fn typographic_differences_verify_with_normalization() {
    let paper = paper();
    let mut item = evidence("\u{201c}the firm-level evidence is consistent with monopsony models\u{201d}");
    item.page = Some(2);
    let outcome = check(&paper, &item);
    assert_eq!(outcome.status, VERIFIED_NORMALIZED);
    assert_eq!(outcome.method, "normalized");
    assert_eq!(outcome.matched.unwrap().node_id, "n3");
}

#[test]
fn quote_crossing_a_node_boundary_records_both_nodes() {
    let paper = paper();
    let mut item = evidence("standard error of 0.04. The");
    item.node_id = "n2".into();
    let outcome = check(&paper, &item);
    assert_eq!(outcome.status, VERIFIED);
    let matched = outcome.matched.unwrap();
    assert_eq!(matched.node_id, "n2");
    assert_eq!(matched.end_node_id, "n3");
    assert_eq!(&paper.nodes[2].text[..matched.end], "The");
}

#[test]
fn quote_on_another_page_is_partially_verified() {
    let paper = paper();
    let mut item = evidence("We conclude that the policy had small effects.");
    item.page = Some(1);
    let outcome = check(&paper, &item);
    assert_eq!(outcome.status, PARTIAL);
    assert_eq!(outcome.reason, "locator_mismatch");
    assert_eq!(outcome.matched.unwrap().page, Some(3));
}

#[test]
fn unknown_node_is_reported_even_when_the_quote_exists() {
    let paper = paper();
    let mut item = evidence("We conclude that the policy had small effects.");
    item.node_id = "n99".into();
    let outcome = check(&paper, &item);
    assert_eq!(outcome.status, PARTIAL);
    assert_eq!(outcome.reason, "locator_unknown");
}

#[test]
fn ordered_ellipsis_segments_verify_with_normalization() {
    let paper = paper();
    let item = evidence("We study the effect of minimum wages ... in 1990\u{2013}2010");
    let outcome = check(&paper, &item);
    assert_eq!(outcome.status, VERIFIED_NORMALIZED);
    assert_eq!(outcome.method, "ellipsis");
    // Segments out of order are not an ellipsis match; at most a paraphrase.
    let reversed = evidence("teen employment in 1990 ... We study the effect of minimum");
    let outcome = check(&paper, &reversed);
    assert_ne!(outcome.method, "ellipsis");
    assert!(![VERIFIED, VERIFIED_NORMALIZED].contains(&outcome.status));
}

#[test]
fn close_paraphrase_is_flagged_not_verified() {
    let paper = paper();
    let item = evidence("We conclude that the reform had small effects.");
    let outcome = check(&paper, &item);
    assert_eq!(outcome.status, PARTIAL);
    assert_eq!(outcome.reason, "paraphrase_suspected");
}

#[test]
fn fabricated_quotes_are_never_promoted() {
    let paper = paper();
    for quote in [
        "The elasticity is large and statistically significant.",
        "We use administrative data from Denmark.",
        "minimum wages raise teen employment substantially",
        "0.21",
    ] {
        let status = check(&paper, &evidence(quote)).status;
        assert!(
            ![VERIFIED, VERIFIED_NORMALIZED].contains(&status),
            "{quote} -> {status}"
        );
    }
}

#[test]
fn quotes_without_document_locators_are_not_checkable() {
    let paper = paper();
    assert_eq!(check(&paper, &evidence("  ")).reason, "no_quote");
    let mut web = evidence("A quote from a website");
    web.url = "https://example.org".into();
    assert_eq!(check(&paper, &web).reason, "external_locator");
    let mut source = evidence("let x = 1;");
    source.source_path = "code/main.do".into();
    assert_eq!(check(&paper, &source).reason, "source_not_captured");
}

#[test]
fn latex_markup_is_removed_for_latex_sources() {
    let tex = bundle(
        "latex",
        vec![node(
            "s1",
            1,
            None,
            "We find \\emph{large} effects of the policy~\\citep{card1994}.",
        )],
    );
    let outcome = check(&tex, &evidence("We find large effects of the policy"));
    assert_eq!(outcome.status, VERIFIED_NORMALIZED);
    assert_eq!(outcome.method, "latex_markup");
}

#[test]
fn source_quotes_check_their_cited_lines() {
    let index = DocumentIndex::from_text("", false);
    let text = "* setup\nuse data, clear\nreg y x, robust\n\n\n\n\n\ndisplay 1\n";
    let mut read = |path: &str| (path == "evidence/main.do").then(|| text.to_string());
    let mut item = evidence("reg y x, robust");
    item.evidence_type = "source".into();
    item.source_path = "code/main.do".into();
    item.artifact_path = "evidence/main.do".into();
    item.line_start = Some(3);
    let outcome = verify_evidence(&index, &item, &mut read);
    assert_eq!(outcome.status, VERIFIED);
    assert_eq!(outcome.matched.unwrap().line, Some(3));
    item.line_start = Some(9);
    item.line_end = Some(9);
    item.quote = "use data, clear".into();
    let outcome = verify_evidence(&index, &item, &mut read);
    assert_eq!(outcome.status, PARTIAL);
    assert_eq!(outcome.reason, "locator_mismatch");
}

#[test]
fn verify_products_overwrites_model_claims_and_records_a_product() {
    let paper = paper();
    let mut claimed = evidence("This sentence does not appear anywhere in the paper at all.");
    claimed.verification_status = "verified".into();
    let mut real = evidence("We conclude that the policy had small effects.");
    real.page = Some(3);
    let mut products = RunProducts {
        findings: Some(FindingSet {
            source_step_id: "validate".into(),
            findings: vec![Finding {
                id: "small-effects".into(),
                title: "t".into(),
                body: "b".into(),
                evidence: vec![claimed, real],
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let index = DocumentIndex::from_bundle(&paper);
    verify_products(&mut products, &index, "bundle-1", &mut no_source);
    let evidence = &products.findings.as_ref().unwrap().findings[0].evidence;
    assert_eq!(evidence[0].verification_status, NOT_FOUND);
    assert_eq!(evidence[1].verification_status, VERIFIED);
    let product = products
        .named
        .iter()
        .find(|product| product.key == PRODUCT_KEY)
        .unwrap();
    assert_eq!(product.source_step_id, "validate");
    assert_eq!(product.content["counts"]["verified"], 1);
    assert_eq!(product.content["counts"]["not_found"], 1);
    assert_eq!(product.content["items"][1]["matched"]["page"], 3);
    // Re-verification replaces rather than duplicates the product.
    verify_products(&mut products, &index, "bundle-1", &mut no_source);
    assert_eq!(
        products
            .named
            .iter()
            .filter(|product| product.key == PRODUCT_KEY)
            .count(),
        1
    );
}

#[test]
fn text_fallback_without_a_bundle_still_verifies() {
    let text = "Plain extracted text with a memorable sentence inside it.";
    let index = DocumentIndex::from_text(text, false);
    let outcome = verify_evidence(&index, &evidence("a memorable sentence"), &mut no_source);
    assert_eq!(outcome.status, VERIFIED);
}
