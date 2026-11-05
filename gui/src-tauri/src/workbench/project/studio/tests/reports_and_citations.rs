//! What a researcher reads back from the studio: build messages, citations,
//! referee comments, and the links that keep a number tied to its passage.
use super::*;

fn report(f: &Fixture, name: &str, paragraphs: usize) -> String {
    let text = (1..=paragraphs)
        .map(|n| format!("Comment paragraph {n}."))
        .collect::<Vec<_>>()
        .join("\n\n");
    paper(f, name, &text).revision.unwrap().id
}

#[test]
fn a_fatal_error_survives_hundreds_of_warnings_ahead_of_it() {
    let log = format!(
        "{}./main.tex:3: Undefined control sequence\n! Emergency stop.\n",
        "Underfull \\hbox (badness 10000) in paragraph\n".repeat(600)
    );
    let messages = parse_diagnostics(&log);
    assert_eq!(messages.len(), 500);
    assert_eq!(messages[0].message, "Undefined control sequence");
    assert_eq!(messages[0].path.as_deref(), Some("main.tex"));
    assert_eq!(messages[1].message, "! Emergency stop.");
    assert!(messages[2].message.starts_with("Underfull"));
    assert_eq!(
        messages[499].message,
        "103 more build messages are not shown."
    );
}

#[test]
fn a_short_log_keeps_every_message_and_no_note() {
    let messages = parse_diagnostics("LaTeX Warning: one\n! Fatal\nLaTeX Warning: two\n");
    let order: Vec<_> = messages.iter().map(|m| m.message.as_str()).collect();
    assert_eq!(
        order,
        ["! Fatal", "LaTeX Warning: one", "LaTeX Warning: two"]
    );
}

#[test]
fn starred_and_capitalized_citation_commands_are_found() {
    let f = fixture();
    let text = r"See \citep*{starred}, \citet*[p. 2]{located}, \Citet{capital}, \Citeauthor*{both}, and \cite{plain,second}.";
    let revision = paper(&f, "main.tex", text).revision.unwrap().id;
    let found = citation_navigation(&f.store, &f.ws, &revision).unwrap();
    let keys: Vec<_> = found["citations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        ["starred", "located", "capital", "both", "plain", "second"]
    );
    assert!(found["coverage"].as_str().unwrap().contains("macros"));
}

#[test]
fn a_direction_with_a_long_first_test_converts_and_keeps_all_of_it() {
    let f = fixture();
    let test = "é".repeat(3_000);
    let saved = save_direction(
        &f.store,
        &f.ws,
        None,
        0,
        ResearchDirection {
            question: "Does search friction explain the wage gap?".into(),
            mechanism: "Directed search".into(),
            closest_known_work: "Menzio and Shi".into(),
            minimal_model_or_data: "Two types".into(),
            first_discriminating_test: test.clone(),
            likely_failure_mode: "Gap vanishes".into(),
            next_action: "Write the model".into(),
            status: "idea".into(),
            task_id: None,
            theory_ids: vec![],
            drop_reason: String::new(),
        },
    )
    .unwrap();
    let converted = convert_direction(&f.store, &f.ws, &saved.id, saved.revision).unwrap();
    let body = &converted["task"]["body"];
    assert!(body["objective"].as_str().unwrap().contains(&test));
    let check = body["expectedChecks"][0].as_str().unwrap();
    assert!(check.len() <= 1000, "{}", check.len());
    assert!(check.ends_with('…'));
    assert!(test.starts_with(check.trim_end_matches('…')));
}

#[test]
fn a_new_referee_comment_shows_what_its_response_still_lacks() {
    let f = fixture();
    let revision = report(&f, "report.md", 2);
    let comments = preview_report(&f.store, &f.ws, &revision).unwrap().comments;
    let imported = import_report(&f.store, &f.ws, &revision, comments).unwrap();
    let flags = imported[0]["body"]["flags"].as_array().unwrap();
    assert!(!flags.is_empty(), "{imported}");
    let id = imported[0]["id"].as_str().unwrap();
    let letter = export_responses(&f.store, &f.ws, &[id.into()], "markdown").unwrap();
    assert_eq!(letter["warnings"].as_array().unwrap().len(), flags.len());
}

#[test]
fn the_letter_follows_comment_numbers_whatever_the_click_order() {
    let f = fixture();
    let revision = report(&f, "report.md", 12);
    let comments = preview_report(&f.store, &f.ws, &revision).unwrap().comments;
    let imported = import_report(&f.store, &f.ws, &revision, comments).unwrap();
    let clicked: Vec<String> = imported
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    let letter = export_responses(&f.store, &f.ws, &clicked, "markdown").unwrap();
    let text = letter["text"].as_str().unwrap();
    let at = |n: u32| text.find(&format!("## Comment {n}\n")).unwrap();
    assert!((1..12).all(|n| at(n) < at(n + 1)), "{text}");
    for (number, id) in (1..=12).rev().zip(&clicked) {
        let saved = record(&f.store, &f.ws, id, "response").unwrap();
        assert_eq!(saved.body["decision"]["number"], number.to_string());
    }
}

#[test]
fn equal_comment_numbers_follow_their_place_in_the_report() {
    let f = fixture();
    let first = report(&f, "first.md", 2);
    let second = report(&f, "second.md", 3);
    let import = |revision: &str, keep: usize| {
        let mut comments = preview_report(&f.store, &f.ws, revision).unwrap().comments;
        let mut comment = comments.remove(keep);
        comment.number = "R1.10".into();
        let text = comment.text.clone();
        let imported = import_report(&f.store, &f.ws, revision, vec![comment]).unwrap();
        (imported[0]["id"].as_str().unwrap().to_string(), text)
    };
    let (later, later_text) = import(&second, 2);
    let (earlier, earlier_text) = import(&first, 0);
    let mut plain = preview_report(&f.store, &f.ws, &first).unwrap().comments;
    let mut lower = plain.remove(1);
    lower.number = "R1.2".into();
    let lower = import_report(&f.store, &f.ws, &first, vec![lower]).unwrap();
    let selected = [later, earlier, lower[0]["id"].as_str().unwrap().to_string()];
    let letter = export_responses(&f.store, &f.ws, &selected, "markdown").unwrap();
    let text = letter["text"].as_str().unwrap();
    assert!(text.find("## Comment R1.2\n").unwrap() < text.find("## Comment R1.10\n").unwrap());
    assert!(text.find(&earlier_text).unwrap() < text.find(&later_text).unwrap());
}

#[test]
fn a_failed_check_flags_what_depends_on_it() {
    use crate::workbench::desk::ResearchObjectRef;
    use crate::workbench::project::relations::{impact, save_relation, Relation};
    let f = fixture();
    let note = save_theory(&f.store, &f.ws, None, 0, theory_note("proposition", "open")).unwrap();
    let failed = save_check(
        &f.store,
        &f.ws,
        None,
        0,
        TheoryCheck {
            theory_id: note.id.clone(),
            method: "numerical_verification".into(),
            outcome: "failed".into(),
            summary: "Fails at beta 0.9.".into(),
            domain: "beta in (0.5,0.99)".into(),
            tolerance: Some(1e-8),
            precision: Some(10),
            execution_id: None,
            recipe_run_id: None,
            anchor_ids: vec![],
            claims_generality: false,
            origin: "manual".into(),
        },
    )
    .unwrap();
    let object = |id: &str, revision: i64| ResearchObjectRef {
        kind: "record".into(),
        id: id.into(),
        revision: revision.to_string(),
        start: None,
        end: None,
    };
    save_relation(
        &f.store,
        &f.ws,
        Relation {
            input: object(&failed.id, failed.revision),
            dependent: object(&note.id, note.revision),
            origin: "manual".into(),
            accepted: true,
            reason: "The proposition rests on this check".into(),
        },
        "relation",
    )
    .unwrap();
    let report = impact(&f.store, &f.ws).unwrap();
    let found = |id: &str| {
        report
            .impacts
            .iter()
            .find(|i| i.object.id == id)
            .unwrap_or_else(|| panic!("{:?}", report.impacts))
    };
    assert_eq!(found(&failed.id).status, "check_failed");
    assert_eq!(found(&note.id).status, "review_needed");
    assert!(found(&note.id).reason.contains("A linked check failed"));
}

#[cfg(unix)]
#[test]
fn a_lost_passage_leaves_only_its_own_row_unavailable() {
    let f = fixture();
    let (_, result) = exported_result(&f, 0.126, "log points");
    let revision = paper(&f, "main.tex", "The effect is 0.13 log points.")
        .revision
        .unwrap();
    let anchor = || {
        documents::annotate(
            &f.store,
            &f.ws,
            DocumentSelection {
                revision_id: revision.id.clone(),
                revision_hash: revision.content_hash.clone(),
                start: Some(0),
                end: Some(30),
                page: None,
                region: None,
                quote: "The effect is 0.13 log points.".into(),
            },
            "Headline".into(),
        )
        .unwrap()
    };
    let (kept, lost) = (anchor(), anchor());
    for anchor_id in [&kept.id, &lost.id] {
        bind_number(
            &f.store,
            &f.ws,
            None,
            0,
            NumericBinding {
                result: result.clone(),
                anchor_id: anchor_id.clone(),
                role: "prose".into(),
                component: "estimate".into(),
                printed: "0.13".into(),
                precision: 2,
                reported_units: "log points".into(),
                origin: "manual".into(),
                confirmed: true,
                imported_from: None,
                imported_status: None,
            },
        )
        .unwrap();
    }
    f.store
        .connection()
        .unwrap()
        .execute("DELETE FROM project_records WHERE id=?1", [&lost.id])
        .unwrap();

    let rows = binding_coverage(&f.store, &f.ws).unwrap();
    assert_eq!(rows.len(), 2);
    let row = |id: &str| {
        rows.iter()
            .find(|r| r["record"]["body"]["anchorId"] == id)
            .unwrap()
    };
    assert_eq!(row(&kept.id)["state"], "current");
    assert_eq!(row(&lost.id)["state"], "unavailable");
    assert!(row(&lost.id)["anchor"].is_null());
    assert!(row(&lost.id)["reasons"][0]
        .as_str()
        .unwrap()
        .contains("no longer in this project"));
}
