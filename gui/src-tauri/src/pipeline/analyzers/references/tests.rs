use super::bibliography::{self, Block};
use super::bibtex;
use super::crossref::{self, Work};
use super::*;

#[test]
fn bibtex_reads_nested_braces_quotes_and_concatenation() {
    let parsed = bibtex::parse(
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
    assert_eq!(parsed.entries.len(), 1);
    assert_eq!(
        parsed.stopped.as_deref(),
        Some("reading stopped at an unfinished entry")
    );
    let entry = &parsed.entries[0];
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
fn tex_letter_commands_become_letters_and_match_crossref_spellings() {
    assert_eq!(bibtex::plain(r"S{\o}rensen"), "Sørensen");
    assert_eq!(
        bibtex::plain(r"Gro\ss e Stra{\ss}e \AE sop"),
        "Große Straße Æsop"
    );
    assert_eq!(bibtex::plain(r"{\L}ukasz Garc{\'\i}a"), "Łukasz Garcia");
    let named = |family: &str| Work {
        families: vec![family.to_string()],
        ..Default::default()
    };
    for (crossref_name, cited) in [
        ("Müller", r#"M{\"u}ller"#),
        ("Sørensen", r"S{\o}rensen"),
        ("García", r"Garc{\'\i}a"),
        ("Muller", r#"M{\"u}ller"#),
        ("Sorensen", r"S{\o}rensen"),
        ("Łukasz", r"{\L}ukasz"),
        ("Nguyễn", "Nguyen"),
    ] {
        let text = format!("{}. 2001. A title.", bibtex::plain(cited));
        assert!(
            crossref::author_present(&named(crossref_name), &text),
            "{crossref_name} against {text}"
        );
    }
    assert!(!crossref::author_present(&named("Miller"), "Müller. 2001."));
    assert_eq!(
        crossref::title_coverage("Überblick über Märkte", "Uberblick uber Markte"),
        1.0
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
fn cited_keys_ignore_style_commands_macro_parameters_and_comments() {
    let keys = bibtex::cited_keys(
        "\\setcitestyle{authoryear,round} \\citestyle{aer} % \\cite{oldkey}\n\\newcommand{\\mycite}[1]{\\citep{#1}}\n\\bibliographystyle{plain}\n\\citep{real}\n\\citeasnoun{also} 50\\% \\Citet*[p.~1]{third}\n\\cite{a=b}",
    );
    assert_eq!(keys, vec!["real", "also", "third"]);
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
fn appendices_end_the_list_and_their_text_is_never_a_reference() {
    const LIST: &str = "References\n\nCard, David. 1994. A paper on wages.\n\nNeumark, David. 2000. Another paper.\n\nAutor, David. 2013. A third paper here.\n\n";
    const APPENDIX: &str = "Proof of Proposition 1. Suppose the unpublished estimator is consistent, then the result follows.\n\nThis paragraph describes a robustness exercise that is not yet public.";
    for heading in [
        "Appendix",
        "Appendix A",
        "Appendix A: Proofs",
        "## Appendix B. Data Construction",
        "Online Appendix A",
        "Web Appendix",
        "Internet Appendix for the paper",
        "Data Appendix",
        "Supplementary Results",
        "Tables and Figures",
        "Figures and Tables",
        "Table 1: Summary statistics",
        "Figure A1. Event study",
    ] {
        let text = format!("{LIST}{heading}\n\n{APPENDIX}");
        let entries = bibliography::entries(&bibliography::text_blocks(&text));
        assert_eq!(entries.len(), 3, "{heading}");
        assert!(
            entries[2].text.ends_with("A third paper here."),
            "{heading}: {}",
            entries[2].text
        );
    }
    // Without any heading, prose that cannot be a reference is still kept out.
    let text = format!("{LIST}{APPENDIX}");
    assert_eq!(
        bibliography::entries(&bibliography::text_blocks(&text)).len(),
        3
    );
}

const MAIN_LIST: &str = "References\n\nCard, David. 1994. A paper on wages.\n\nNeumark, David. 2000. Another paper.\n\nAutor, David. 2013. A third paper here.\n\n";

#[test]
fn a_later_list_does_not_replace_the_papers_bibliography() {
    let text = format!(
        "{MAIN_LIST}Online Appendix\n\nProof of Proposition 1 follows.\n\nReferences\n\nLee, David. 2008. An appendix citation.\n\nPei, Zhuan. 2019. A second appendix citation.\n\nKatz, Lawrence. 1992. A third appendix citation."
    );
    let (references, checks) = collect(None, &text, "paper.pdf");
    let first_words: Vec<_> = references
        .iter()
        .map(|reference| reference.first_author.as_deref().unwrap())
        .collect();
    assert_eq!(
        first_words,
        ["Card", "Neumark", "Autor", "Lee", "Pei", "Katz"]
    );
    assert_eq!(
        references.iter().map(|r| r.index).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6]
    );
    assert!(checks.limitations.is_empty(), "{:?}", checks.limitations);
}

#[test]
fn a_work_in_two_chapter_bibliographies_is_read_once() {
    let text = format!(
        "{MAIN_LIST}Chapter 2\n\nThe second chapter begins here with a discussion of unpublished results from the fieldwork in 2021 and what they imply for the argument, which goes on at some length before it reaches any citation at all, and then continues with more of the same manuscript text that must stay on this computer, because a reference check has no reason to send it anywhere, and the paragraph keeps going past the length of any plausible entry in a bibliography, with further remarks on the design, the sample, the instruments, the weights, the standard errors, the placebo tests, the robustness exercises, the appendix tables, the data construction, and the replication files, none of which is a reference, and all of which belongs to a chapter that the author has not yet shared with anyone outside the committee or the department.\n\nReferences\n\nCard, David. 1994. A paper on wages.\n\nLee, David. 2008. A chapter citation.\n\nPei, Zhuan. 2019. A second chapter citation."
    );
    let (references, checks) = collect(None, &text, "thesis.pdf");
    assert_eq!(references.len(), 5, "{references:#?}");
    assert!(checks.duplicates.is_empty());
    assert!(references
        .iter()
        .all(|reference| !reference.text.contains("fieldwork")));
    assert!(checks.limitations.is_empty(), "{:?}", checks.limitations);
}

#[test]
fn a_duplicate_inside_one_list_is_still_reported() {
    let text = "References\n\nCard, David. 1994. A paper on wages.\n\nCard, David. 1994. A paper on wages.\n\nAutor, David. 2013. A third paper here.";
    let (references, checks) = collect(None, text, "paper.pdf");
    assert_eq!(references.len(), 3);
    assert_eq!(checks.duplicates, [(1, 2)]);
}

#[test]
fn body_text_after_an_earlier_list_never_reaches_a_lookup() {
    // Nothing marks where the first list ends, so the text after it is not read.
    let body = "We then turn to results from 2021 that the author has not yet shared. The estimates differ from those in Card (1994) in the expected direction, and the design rules out the usual concerns about selection into treatment across the sampled districts and periods, which took several seasons of fieldwork to establish, with attrition small in every wave and balanced across arms, and with enumerators blind to assignment for the whole of the follow-up, so that the differences reported in this chapter can be read as the effects of the program rather than as artifacts of the survey, the sample, or the weighting that the analysis applies to the raw responses collected from the households in the study, and which the chapter returns to below in more detail.";
    let text = format!(
        "{MAIN_LIST}{body}\n\nReferences\n\nLee, David. 2008. A chapter citation.\n\nPei, Zhuan. 2019. A second chapter citation.\n\nKatz, Lawrence. 1992. A third chapter citation."
    );
    let (references, checks) = collect(None, &text, "thesis.pdf");
    assert_eq!(references.len(), 6);
    assert!(references
        .iter()
        .all(|reference| !reference.text.contains("fieldwork")));
    assert_eq!(checks.limitations.len(), 1, "{:?}", checks.limitations);
    assert!(checks.limitations[0].starts_with("1 earlier reference list stopped"));
}

#[test]
fn an_earlier_heading_over_body_text_is_named_rather_than_read() {
    let body = "We then turn to results from 2021 that the author has not yet shared with anyone outside the committee, in a paragraph long enough to rule out that it is a bibliography entry, since it runs on across the design, the sample, the instruments, the weights, the standard errors, the placebo tests, the robustness exercises, the appendix tables, the data construction, and the replication files, with attrition small in every wave and balanced across arms, and enumerators blind to assignment for the whole of the follow-up, so that the differences reported here can be read as effects of the program rather than as artifacts of the survey, the sample, or the weighting that the analysis applies to the responses collected from the households in the study, as the discussion below explains at greater length.";
    let text = format!("References\n\n{body}\n\n{body}\n\n{body}\n\n{MAIN_LIST}");
    let (references, checks) = collect(None, &text, "paper.pdf");
    assert_eq!(references.len(), 3);
    assert_eq!(checks.limitations.len(), 1, "{:?}", checks.limitations);
    assert!(checks.limitations[0].starts_with("1 earlier section headed like"));
}

#[test]
fn a_reference_that_begins_like_a_heading_stays_in_the_list() {
    let text = "References\n\nAppendix to the Economic Report of the President. 2012. Washington.\n\nCard, David. 1994. A paper on wages.\n\nNeumark, David. 2000. Another paper.";
    assert_eq!(
        bibliography::entries(&bibliography::text_blocks(text)).len(),
        3
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
fn serial_item_dois_keep_their_brackets() {
    const WILEY: &str = "10.1002/(sici)1099-1255(199601)11:1<1::aid-jae372>3.0.co;2-d";
    const WEATHER: &str = "10.1175/1520-0442(2001)014<0001:tpotso>2.0.co;2";
    for (text, expected) in [
        ("Journal of Applied Econometrics, doi:10.1002/(SICI)1099-1255(199601)11:1<1::AID-JAE372>3.0.CO;2-D.", WILEY),
        ("(https://doi.org/10.1002/(SICI)1099-1255(199601)11:1<1::AID-JAE372>3.0.CO;2-D)", WILEY),
        ("<a href=\"x\">10.1002/(SICI)1099-1255(199601)11:1<1::AID-JAE372>3.0.CO;2-D</a> and more", WILEY),
        ("https://doi.org/10.1002/(SICI)1099-1255(199601)11:1%3C1::AID-JAE372%3E3.0.CO;2-D", WILEY),
        ("10.1002/(SICI)1099-1255(199601)11:1&lt;1::AID-JAE372&gt;3.0.CO;2-D, 1996", WILEY),
        ("J. Climate 14, 10.1175/1520-0442(2001)014<0001:TPOTSO>2.0.CO;2", WEATHER),
    ] {
        assert_eq!(bibliography::doi(text).as_deref(), Some(expected), "{text}");
    }
}

#[test]
fn a_tag_or_bracket_after_a_doi_is_not_part_of_it() {
    for text in [
        "10.1257/aer.84.4.772</a>",
        "10.1257/aer.84.4.772<br>Next entry",
        "10.1257/aer.84.4.772<br/>2.0.CO;2",
        "10.1257/aer.84.4.772<br>3.0.CO;2",
        "<https://doi.org/10.1257/aer.84.4.772>",
        "&lt;https://doi.org/10.1257/aer.84.4.772&gt;.",
        "<doi>10.1257/aer.84.4.772</doi> Autor, D. 2013.",
        "10.1257/aer.84.4.772 <1::AID> 3.0.CO;2",
    ] {
        assert_eq!(
            bibliography::doi(text).as_deref(),
            Some("10.1257/aer.84.4.772"),
            "{text}"
        );
    }
}

#[test]
fn counts_of_undefined_keys_and_duplicates_are_the_true_counts() {
    let source = "\\documentclass{article}\\begin{document}\n\\cite{"
        .to_string()
        + &(0..80)
            .map(|index| format!("key{index}"))
            .collect::<Vec<_>>()
            .join(",")
        + "}\n\\begin{thebibliography}{1}\\bibitem{other} Other.\\end{thebibliography}\n\\end{document}";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("paper.tex");
    std::fs::write(&path, &source).unwrap();
    let (_, checks) = collect(None, &source, path.to_str().unwrap());
    assert_eq!(checks.undefined_citation_count, 80);
    assert_eq!(checks.undefined_citations.len(), MAX_LISTED_UNDEFINED);

    let checks = LocalChecks {
        duplicates: (0..13)
            .map(|index| (2 * index + 1, 2 * index + 2))
            .collect(),
        ..checks
    };
    let items = findings(&[], &checks);
    let undefined = items
        .iter()
        .find(|item| item["id"] == "references-undefined-citations")
        .unwrap();
    assert_eq!(
        undefined["title"],
        "80 citation keys have no bibliography entry"
    );
    assert!(undefined["problem"]
        .as_str()
        .unwrap()
        .ends_with(", and 30 more."));
    let more = items
        .iter()
        .find(|item| item["id"] == "references-duplicates-more")
        .expect("duplicates past the list are counted");
    assert_eq!(
        more["title"],
        "3 more pairs of references may be duplicates"
    );
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
    let output = run(None, text, "", &serde_json::json!({}), crossref::API)
        .await
        .unwrap();
    assert_eq!(output["network"], false);
    assert_eq!(output["requests"], 0);
    assert_eq!(output["reference_count"], 3);
    assert_eq!(output["summary"]["not_checked"], 3);
    assert!(output["findings"].as_array().unwrap().is_empty());
    // The output parses as a Review findings product.
    assert!(crate::findings::parse_value(&output, "refs", "References").is_some());
}

#[path = "tests/sources.rs"]
mod sources;

#[path = "tests/lookups.rs"]
mod lookups;

#[tokio::test]
#[ignore = "contacts Crossref"]
async fn live_crossref_flags_a_known_retraction() {
    let text = "Intro.\n\nReferences\n[1] Wakefield AJ, et al. Ileal-lymphoid-nodular hyperplasia, non-specific colitis, and pervasive developmental disorder in children. Lancet. 1998. doi:10.1016/S0140-6736(97)11096-0\n[2] Card D, Krueger AB. Minimum Wages and Employment: A Case Study of the Fast-Food Industry in New Jersey and Pennsylvania. American Economic Review. 1994.\n[3] Placeholder, Q. 2031. Entirely invented results on nothing at all. Journal of Nothing.";
    let output = run(
        None,
        text,
        "",
        &serde_json::json!({"network": true}),
        crossref::API,
    )
    .await
    .unwrap();
    let references = output["references"].as_array().unwrap();
    assert_eq!(references[0]["status"], "retracted");
    assert_eq!(references[1]["status"], "matched");
    assert_ne!(references[2]["status"], "matched");
}
