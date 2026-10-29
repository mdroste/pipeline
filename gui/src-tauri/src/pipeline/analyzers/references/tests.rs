use super::bibliography::{self, Block};
use super::bibtex;
use super::crossref::{self, Work};
use super::*;

#[test]
fn bibtex_reads_nested_braces_quotes_and_concatenation() {
    let entries = bibtex::parse(
        r#"@comment{ignored}
@string{aer = "American Economic Review"}
@Article{card1994,
  author = {Card, David and Krueger, Alan B.},
  title = {Minimum Wages and Employment: {A} Case Study},
  journal = aer,
  year = 1994,
  doi = "10.1257/aer.84.4.772" # "",
}
@misc{broken, title = {unterminated"#,
    );
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.kind, "article");
    assert_eq!(entry.key, "card1994");
    assert_eq!(entry.fields["year"], "1994");
    assert_eq!(entry.fields["journal"], "aer");
    assert_eq!(
        bibtex::plain(&entry.fields["title"]),
        "Minimum Wages and Employment: A Case Study"
    );
    assert_eq!(
        bibtex::family_names(&entry.fields["author"]),
        vec!["Card", "Krueger"]
    );
}

#[test]
fn plain_text_drops_markup_and_accents_but_keeps_symbols() {
    assert_eq!(
        bibtex::plain(r#"Gr{\"o}bner \& {Sons} 50\%~off"#),
        "Grobner & Sons 50% off"
    );
    assert_eq!(
        bibtex::family_names("Alan B. Krueger and others"),
        vec!["Krueger"]
    );
}

#[test]
fn cited_keys_cover_natbib_and_biblatex_forms() {
    let keys = bibtex::cited_keys(
        r"As \citet{card1994} and \citep[p.~3][]{neumark2000, card1994} note; \textcite{autor2013}; \nocite{*}",
    );
    assert_eq!(keys, vec!["card1994", "neumark2000", "autor2013"]);
}

#[test]
fn numbered_bibliographies_split_at_markers() {
    let blocks = [
        Block { node_id: "n1", text: "We cite [1] and [2]." },
        Block { node_id: "n2", text: "References" },
        Block {
            node_id: "n3",
            text: "[1] D. Card and A. B. Krueger. Minimum wages and employment. AER, 1994.\n[2] D. Neumark and W. Wascher. Minimum wages and employment. 2000.\n[3] D. Autor, D. Dorn, G. Hanson. The China syndrome. AER 2013. doi:10.1257/aer.103.6.2121.",
        },
        Block { node_id: "n4", text: "Appendix" },
        Block { node_id: "n5", text: "[4] Not a reference after the appendix heading at all." },
    ];
    let entries = bibliography::entries(&blocks);
    assert_eq!(entries.len(), 3);
    assert!(entries.iter().all(|entry| entry.node_id == "n3"));
    assert_eq!(
        bibliography::doi(&entries[2].text).as_deref(),
        Some("10.1257/aer.103.6.2121")
    );
    assert_eq!(bibliography::year(&entries[0].text), Some(1994));
    assert_eq!(
        bibliography::first_family(&entries[0].text).as_deref(),
        Some("Card")
    );
}

#[test]
fn author_year_bibliographies_split_at_new_author_lines() {
    let text = "Card, David, and Alan B. Krueger. 1994. \"Minimum Wages and\nEmployment.\" American Economic Review 84 (4): 772-793.\nNeumark, David, and William Wascher. 2000. \"Minimum Wages\nand Employment: Comment.\" AER 90 (5).\nvan der Berg, J. 2001. Another paper that runs on for a while.";
    let blocks = [
        Block {
            node_id: "",
            text: "References",
        },
        Block { node_id: "", text },
    ];
    let entries = bibliography::entries(&blocks);
    assert_eq!(entries.len(), 3, "{entries:#?}");
    assert!(entries[0].text.starts_with("Card, David"));
    assert!(entries[0].text.contains("Minimum Wages and Employment"));
    assert_eq!(
        bibliography::first_family(&entries[1].text).as_deref(),
        Some("Neumark")
    );
    assert_eq!(
        bibliography::first_family(&entries[2].text).as_deref(),
        Some("Berg")
    );
    assert_eq!(
        bibliography::first_family("Card D, Krueger AB (1994)").as_deref(),
        Some("Card")
    );
}

#[test]
fn text_blocks_separate_a_heading_glued_to_its_first_entry() {
    let text = "Body text.\n\nREFERENCES\nCard, David. 1994. A paper on wages.\nNeumark, David. 2000. Another paper.\nAutor, David. 2013. A third paper here.";
    let entries = bibliography::entries(&bibliography::text_blocks(text));
    assert_eq!(entries.len(), 3);
}

#[test]
fn identifiers_are_normalized() {
    assert_eq!(
        bibliography::doi("https://doi.org/10.1016/S0140-6736(97)11096-0.").as_deref(),
        Some("10.1016/s0140-6736(97)11096-0")
    );
    assert_eq!(
        bibliography::arxiv("arXiv:2101.01234v2").as_deref(),
        Some("2101.01234")
    );
    assert_eq!(bibliography::year("Smith (2019b) wrote"), Some(2019));
    assert_eq!(bibliography::doi("no identifier"), None);
}

#[test]
fn latex_inputs_use_cited_bib_entries_and_report_local_problems() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("refs.bib"),
        "@article{a, author={Card, David}, title={Minimum Wages}, year={1994}, doi={10.1257/aer.84.4.772}}\n@article{b, author={Neumark, David}, title={Comment}, year={2000}, doi={not a doi}}\n@article{unused, title={Never cited}}\n@article{dup, author={Card, David}, title={Minimum Wages}, year={1994}, doi={10.1257/AER.84.4.772}}",
    )
    .unwrap();
    let tex = dir.path().join("paper.tex");
    std::fs::write(&tex, "x").unwrap();
    let source = r"\cite{a,b} \citet{dup} \citep{missing} \bibliography{refs}";
    let (references, checks) = collect(None, source, tex.to_str().unwrap());
    assert_eq!(
        references
            .iter()
            .map(|reference| reference.key.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b", "dup"]
    );
    assert_eq!(references[0].source_file, "refs.bib");
    assert_eq!(references[0].doi.as_deref(), Some("10.1257/aer.84.4.772"));
    assert_eq!(checks.undefined_citations, vec!["missing"]);
    assert_eq!(checks.uncited_entries, 1);
    assert_eq!(checks.malformed_dois, vec![2]);
    assert_eq!(checks.duplicates, vec![(1, 3)]);
}

fn work(title: &str, families: &[&str], year: u16, kind: &str) -> Work {
    Work {
        doi: "10.1/x".into(),
        title: title.into(),
        families: families.iter().map(|family| family.to_string()).collect(),
        year: Some(year),
        container: "Journal".into(),
        kind: kind.into(),
    }
}

fn reference(text: &str) -> Reference {
    Reference {
        index: 1,
        text: text.into(),
        year: bibliography::year(text),
        origin: "bibliography",
        status: "not_checked",
        ..Default::default()
    }
}

#[test]
fn classification_distinguishes_matches_versions_and_mismatches() {
    let cited = "Card, David, and Alan Krueger. 1994. Minimum Wages and Employment: A Case Study of the Fast-Food Industry. AER.";
    let published = work(
        "Minimum Wages and Employment: A Case Study of the Fast-Food Industry in New Jersey and Pennsylvania",
        &["Card", "Krueger"],
        1994,
        "journal-article",
    );
    let mut item = reference(cited);
    classify(&mut item, &published, false);
    assert_eq!(
        item.status, "ambiguous",
        "partial title coverage is not a match"
    );

    let exact = work(
        "Minimum Wages and Employment: A Case Study of the Fast-Food Industry",
        &["Card", "Krueger"],
        1994,
        "journal-article",
    );
    let mut item = reference(cited);
    classify(&mut item, &exact, false);
    assert_eq!(item.status, "matched");

    let mut item = reference(&cited.replace("1994", "1992. NBER Working Paper 4509"));
    classify(&mut item, &exact, false);
    assert_eq!(item.status, "other_version");

    let mut item = reference(&cited.replace("Card, David, and Alan Krueger", "Smith, John"));
    classify(&mut item, &exact, false);
    assert_eq!(item.status, "metadata_mismatch");

    let mut item = reference(cited);
    item.doi = Some("10.1/x".into());
    classify(
        &mut item,
        &work(
            "Unrelated Title About Trade",
            &["Autor"],
            2013,
            "journal-article",
        ),
        true,
    );
    assert_eq!(item.status, "metadata_mismatch");

    let mut item = reference(cited);
    classify(
        &mut item,
        &work(
            "Unrelated Title About Trade",
            &["Autor"],
            2013,
            "journal-article",
        ),
        false,
    );
    assert_eq!(item.status, "not_found");
    assert!(item.matched.is_none());
}

#[test]
fn crossref_records_and_notices_parse() {
    let record = serde_json::json!({
        "DOI": "10.1016/S0140-6736(97)11096-0",
        "title": ["Ileal-lymphoid-nodular hyperplasia"],
        "author": [{"family": "Wakefield", "given": "A"}],
        "issued": {"date-parts": [[1998, 2, 28]]},
        "container-title": ["The Lancet"],
        "type": "journal-article"
    });
    let parsed = crossref::parse_work(&record);
    assert_eq!(parsed.doi, "10.1016/s0140-6736(97)11096-0");
    assert_eq!(parsed.year, Some(1998));
    assert_eq!(parsed.families, vec!["Wakefield"]);
    let notices = serde_json::json!({"message": {"items": [
        {"DOI": "10.1016/S0140-6736(10)60175-4", "update-to": [
            {"DOI": "10.1016/S0140-6736(97)11096-0", "type": "retraction", "source": "retraction-watch"},
            {"DOI": "10.9999/other", "type": "correction"}
        ]}
    ]}});
    let parsed = crossref::parse_notices(&notices, "10.1016/s0140-6736(97)11096-0");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].kind, "retraction");
    assert_eq!(parsed[0].source, "retraction-watch");
    assert_eq!(notice_flag(&parsed), Some("retracted"));
}

#[test]
fn findings_order_by_severity_and_quote_the_reference() {
    let mut retracted =
        reference("Wakefield, A. 1998. Ileal-lymphoid-nodular hyperplasia. Lancet.");
    retracted.index = 2;
    retracted.node_id = "n9".into();
    retracted.status = "retracted";
    retracted.matched = Some(
        serde_json::json!({"doi": "10.1016/s0140-6736(97)11096-0", "title": "Ileal", "year": 1998}),
    );
    let mut missing = reference("Nobody, A. 2031. A paper that does not exist.");
    missing.status = "not_found";
    let checks = LocalChecks {
        undefined_citations: vec!["k1".into()],
        ..Default::default()
    };
    let items = findings(&[missing, retracted], &checks);
    assert_eq!(items[0]["severity"], "high");
    assert_eq!(items[0]["evidence"][0]["node_id"], "n9");
    assert_eq!(
        items[0]["evidence"][1]["doi"],
        "10.1016/s0140-6736(97)11096-0"
    );
    assert_eq!(items[1]["confidence"], "low");
    assert!(items[1]["problem"]
        .as_str()
        .unwrap()
        .contains("could not be found in Crossref"));
    assert!(items
        .iter()
        .any(|item| item["id"] == "references-undefined-citations"));
}

#[tokio::test]
async fn offline_run_reports_references_without_network() {
    let text = "Intro.\n\nReferences\n[1] Card, D. 1994. Minimum wages. AER.\n[2] Neumark, D. 2000. A comment on wages. AER.\n[3] Autor, D. 2013. The China syndrome. AER.";
    let output = run(None, text, "", &serde_json::json!({})).await.unwrap();
    assert_eq!(output["network"], false);
    assert_eq!(output["requests"], 0);
    assert_eq!(output["reference_count"], 3);
    assert_eq!(output["summary"]["not_checked"], 3);
    assert!(output["findings"].as_array().unwrap().is_empty());
    // The output parses as a Review findings product.
    assert!(crate::findings::parse_value(&output, "refs", "References").is_some());
}

#[tokio::test]
#[ignore = "contacts Crossref"]
async fn live_crossref_flags_a_known_retraction() {
    let text = "Intro.\n\nReferences\n[1] Wakefield AJ, et al. Ileal-lymphoid-nodular hyperplasia, non-specific colitis, and pervasive developmental disorder in children. Lancet. 1998. doi:10.1016/S0140-6736(97)11096-0\n[2] Card D, Krueger AB. Minimum Wages and Employment: A Case Study of the Fast-Food Industry in New Jersey and Pennsylvania. American Economic Review. 1994.\n[3] Placeholder, Q. 2031. Entirely invented results on nothing at all. Journal of Nothing.";
    let output = run(None, text, "", &serde_json::json!({"network": true}))
        .await
        .unwrap();
    let references = output["references"].as_array().unwrap();
    assert_eq!(references[0]["status"], "retracted");
    assert_eq!(references[1]["status"], "matched");
    assert_ne!(references[2]["status"], "matched");
}
