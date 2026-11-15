use super::*;

#[test]
fn strip_json_fences_clean_json() {
    let input = r#"{"key": "value"}"#;
    assert_eq!(strip_json_fences(input), input);
}

#[test]
fn strip_json_fences_with_markdown() {
    let input = "```json\n{\"key\": \"value\"}\n```";
    assert_eq!(strip_json_fences(input), r#"{"key": "value"}"#);
}

#[test]
fn strip_json_fences_with_preamble() {
    let input = "Here is the JSON output:\n\n{\"metadata\": {\"title\": \"Test\"}}";
    assert_eq!(
        strip_json_fences(input),
        r#"{"metadata": {"title": "Test"}}"#
    );
}

#[test]
fn strip_json_fences_nested_braces() {
    let input = r#"{"a": {"b": {"c": 1}}}"#;
    assert_eq!(strip_json_fences(input), input);
}

#[test]
fn strip_json_fences_braces_in_strings() {
    let input = r#"{"text": "contains {braces} inside"}"#;
    assert_eq!(strip_json_fences(input), input);
}

#[test]
fn strip_json_fences_no_json() {
    let input = "No JSON here at all";
    assert_eq!(strip_json_fences(input), input);
}

#[test]
fn strip_json_fences_trailing_text() {
    let input = "{\"key\": 1}\n\nSome trailing commentary.";
    assert_eq!(strip_json_fences(input), r#"{"key": 1}"#);
}

#[test]
fn strip_json_fences_prefers_the_survey_over_a_preamble_object() {
    // A small balanced object echoed in prose must not hijack the survey.
    let input = "I return {} when unsure. Actual survey:\n{\"metadata\": {\"title\": \"T\"}, \"sections\": []}";
    assert_eq!(
        strip_json_fences(input),
        r#"{"metadata": {"title": "T"}, "sections": []}"#
    );
    // Same when the junk object trails the real one.
    let input = "{\"metadata\": {\"title\": \"T\"}, \"sections\": []}\nAs requested: {\"ok\": 1}";
    assert_eq!(
        strip_json_fences(input),
        r#"{"metadata": {"title": "T"}, "sections": []}"#
    );
}

#[test]
fn strip_json_fences_rejects_an_empty_object_as_the_survey() {
    // An empty object is not a survey; returning the raw text makes the
    // caller's parse fail and triggers the retry with the error appended.
    let input = "The result is {} — nothing else.";
    assert_eq!(strip_json_fences(input), input.trim());
}

#[test]
fn strip_json_fences_invalid_then_valid() {
    // First brace starts invalid JSON, second starts valid
    let input = "text with {broken then {\"valid\": true}";
    assert_eq!(strip_json_fences(input), r#"{"valid": true}"#);
}

// ── extraction quality notes ──────────────────────────────────

#[test]
fn quality_notes_are_required_by_compatible_schemas_without_host_mutation() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {"extraction_quality_notes": {"type": "array"}}
    });
    let notes = ["math broken".to_string()];
    let good = serde_json::json!({
        "extraction_quality_notes": [
            {"page_range": "p. 3", "description": "garbled"},
            {"page_range": "global", "description": "math broken"}
        ]
    });
    assert!(validate_quality_notes(&good, &notes, &schema).is_ok());
    let bad = serde_json::json!({"extraction_quality_notes": []});
    assert!(validate_quality_notes(&bad, &notes, &schema).is_err());
}

#[test]
fn quality_note_instruction_respects_schema_shape() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {"extraction_quality_notes": {"type": "array"}}
    });
    let mut prompt = "Survey".to_string();
    append_quality_note_instruction(
        &mut prompt,
        &["ligatures mangled".to_string()],
        Some(&schema),
    )
    .unwrap();
    assert!(prompt.contains("Include every object exactly once"));
    assert!(prompt.contains("ligatures mangled"));
}

#[test]
fn page_notes_coalesce_into_ranges_and_excess_notes_into_a_count() {
    let mut notes = vec!["Math is garbled.".to_string()];
    notes.extend((1..=30).map(|page| {
        format!(
            "Page {page}: recognized {} of 900 characters in the PDF text layer.",
            page * 7
        )
    }));
    notes.push("Page 40: recovered incomplete recognition".to_string());
    notes.push("Page 41: recovered incomplete recognition".to_string());
    notes.push("Page 43: recovered incomplete recognition".to_string());

    let bounded = bounded_host_notes(&notes, 3);
    assert_eq!(bounded[0], "Math is garbled.");
    assert!(
        bounded[1].starts_with("Pages 1-30: 30 similar notes"),
        "{}",
        bounded[1]
    );
    assert_eq!(
        bounded[2],
        "Pages 40-41, 43: 3 similar notes, for example: recovered incomplete recognition"
    );

    let capped = bounded_host_notes(&notes, 2);
    assert_eq!(capped.len(), 2);
    assert!(capped[1].starts_with("2 further extraction notes"));

    // Under the limit the notes stay verbatim.
    assert_eq!(bounded_host_notes(&notes, notes.len()), notes);
}

#[test]
fn host_note_limit_stays_below_the_schema_cap() {
    let schema = |max_items: u64| {
        serde_json::json!({
            "type": "object",
            "properties": {
                "extraction_quality_notes": {"type": "array", "maxItems": max_items}
            }
        })
    };
    assert_eq!(host_note_limit(None), MAX_HOST_QUALITY_NOTES);
    assert_eq!(host_note_limit(Some(&schema(50))), MAX_HOST_QUALITY_NOTES);
    assert_eq!(host_note_limit(Some(&schema(5))), 4);
    assert_eq!(host_note_limit(Some(&schema(1))), 1);
    assert_eq!(host_note_limit(Some(&schema(0))), 0);
}

/// A local OpenAI-compatible server that returns a Paper Review survey
/// echoing exactly the host notes in the prompt it received, and counts
/// its calls.
fn serve_survey(
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    response: impl Fn(usize, &str) -> serde_json::Value + Send + 'static,
) -> String {
    use std::io::{BufRead, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let mut reader = std::io::BufReader::new(stream);
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            if body.is_empty() {
                // The model list request made while resolving the model.
                let models = r#"{"data":[{"id":"fixture"}]}"#;
                let _ = write!(
                    reader.into_inner(),
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{models}",
                    models.len()
                );
                continue;
            }
            let call = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let prompt = body["messages"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|message| message["content"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let survey = response(call, &prompt);
            let reply = serde_json::json!({
                "choices": [{
                    "message": {"role": "assistant", "content": survey.to_string()},
                    "finish_reason": "stop",
                }],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1},
            })
            .to_string();
            let _ = write!(
                reader.into_inner(),
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
        }
    });
    format!("http://{address}/v1")
}

fn serve_echoing_survey(calls: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> String {
    serve_survey(calls, |_, prompt| {
        let start = prompt.find("HOST EXTRACTION QUALITY NOTES").unwrap();
        let start = start + prompt[start..].find("\n[").unwrap() + 1;
        let notes = serde_json::Deserializer::from_str(&prompt[start..])
            .into_iter::<serde_json::Value>()
            .next()
            .unwrap()
            .unwrap();
        serde_json::json!({
            "metadata": {
                "title": "T", "authors": [], "date": "", "paper_type": "mixed",
                "page_count": 0, "has_appendix": false, "has_online_appendix": false,
            },
            "sections": [], "formal_results": [], "tables_figures": [], "notation": [],
            "stated_contribution": "", "key_references": [],
            "extraction_quality_notes": notes,
        })
    })
}

#[tokio::test]
async fn orientation_succeeds_first_time_when_extraction_has_more_notes_than_the_schema_allows() {
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let settings = crate::settings::Settings {
        default_orientation_agent: "local".into(),
        local_base_url: serve_echoing_survey(calls.clone()),
        local_model: "fixture".into(),
        ..Default::default()
    };
    let extraction = ExtractionResult {
        text: "Paper text".to_string(),
        quality_notes: (1..=60)
            .map(|page| format!("Page {page}: recovered incomplete recognition"))
            .collect(),
        method: "full_structure".to_string(),
        source_path: "paper.pdf".to_string(),
        paper_hash: "hash".to_string(),
    };
    let schema = crate::orientation_contract::paper_schema();
    let app: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    // A private run scope, so the frozen settings reach only this call.
    let run = crate::run_scope::ActiveScope::begin("orientation-host-notes-test");
    let survey = crate::run_scope::scoped(run.scope().clone(), async {
        let _settings = crate::settings::freeze_for_run(settings);
        build_orientation_map(
            &app,
            &extraction,
            Some("Survey {input_text}"),
            Some(&schema),
            None,
            None,
        )
        .await
    })
    .await
    .unwrap();

    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    let notes = survey["extraction_quality_notes"].as_array().unwrap();
    assert_eq!(notes.len(), 1);
    assert!(notes[0]["description"]
        .as_str()
        .unwrap()
        .starts_with("Pages 1-60: 60 similar notes"));
}

#[tokio::test]
async fn registry_planning_batches_evidence_and_never_uses_an_invalid_final_plan() {
    for (follow_up, valid) in [(false, true), (true, true), (true, false)] {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let base_url = serve_survey(calls.clone(), move |call, prompt| {
            assert!(prompt.contains("REVIEWER REGISTRY"));
            assert!(prompt.contains("WHOLE-DOCUMENT INDEX"));
            if follow_up && call == 0 {
                return serde_json::json!({"evidence_requests":[{"locator":"page:2","question":"What supports the equilibrium?"}]});
            }
            if follow_up {
                assert!(prompt.contains("READ DOCUMENT EVIDENCE"));
            }
            if !valid {
                return serde_json::json!({"orientation":[]});
            }
            serde_json::json!({"orientation": {
                "metadata": {"title":"Wages","authors":[],"date":"","paper_type":"theory","page_count":2,"has_appendix":false,"has_online_appendix":false},
                "stated_contribution":"A wage theorem.",
                "review_plan": {
                    "primary_domain":"economics","subject":"labor","paper_forms":["formal_theory"],
                    "subject_specialist_ids":["subject_economics_labor"],"method_specialist_ids":["formal_proofs"],
                    "genre":"research_article","routing_uncertainty":[],
                    "selection_notes":[
                        {"id":"subject_economics_labor","reason":"Wages.","references":["page:1"]},
                        {"id":"formal_proofs","reason":"Equilibrium proof.","references":["page:2"]}
                    ]
                }
            }})
        });
        let settings = crate::settings::Settings {
            default_orientation_agent: "local".into(),
            local_base_url: base_url,
            local_model: "fixture".into(),
            ..Default::default()
        };
        let extraction = ExtractionResult {
            text: "<!-- PAGE 1 -->\nLabor economics: wages and labor supply.\n<!-- PAGE 2 -->\nTheorem 1. Formal equilibrium proof with nontrivial derivations.".into(),
            quality_notes: Vec::new(), method: "test".into(), source_path: "paper.pdf".into(), paper_hash: "fixture".into(),
        };
        let schema = crate::auto_review::orientation_schema();
        let app: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        let run = crate::run_scope::ActiveScope::begin("registry-planning-test");
        let result = crate::run_scope::scoped(run.scope().clone(), async {
            let _settings = crate::settings::freeze_for_run(settings);
            build_paper_orientation(
                &app,
                &extraction,
                None,
                Some("Plan {input_text}"),
                Some(&schema),
                None,
                None,
                None,
                &[],
            )
            .await
        })
        .await;
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            if valid { 1 + usize::from(follow_up) } else { 4 }
        );
        if valid {
            let result = result.unwrap();
            assert_eq!(
                result.orientation["review_plan"]["selection_notes"][1]["references"][0],
                "page:2"
            );
            let index: serde_json::Value = serde_json::from_str(&result.index.unwrap()).unwrap();
            assert_eq!(index["follow_up_completed"], follow_up);
            assert!(result.registry.is_some());
        } else {
            assert!(result.is_err());
        }
    }
}

// ── resolve_survey_template ────────────────────────────────────
// Note: load_prompt checks ~/.pipeline/prompts/ overrides first, so
// these assert against load_prompt output rather than literal text.

#[test]
fn resolve_empty_prompt_document_mode_uses_default() {
    assert!(resolve_survey_template("", "document").is_none());
    assert!(resolve_survey_template("  \n", "document").is_none());
}

#[test]
fn resolve_empty_prompt_folder_mode_uses_folder_survey() {
    let resolved = resolve_survey_template("", "folder").expect("folder survey should load");
    let folder = crate::prompts::load_prompt("orientation_folder").unwrap();
    assert_eq!(resolved, folder);
}

#[test]
fn resolve_stock_generic_prompt_swaps_in_folder_mode() {
    let generic = crate::prompts::compiled_default("orientation_generic").unwrap();
    let resolved = resolve_survey_template(generic, "folder").expect("folder survey should load");
    let folder = crate::prompts::load_prompt("orientation_folder").unwrap();
    assert_eq!(resolved, folder);
    // ...but stays generic for document inputs.
    assert_eq!(
        resolve_survey_template(generic, "document").as_deref(),
        Some(generic)
    );
}

#[test]
fn resolve_stock_paper_prompt_swaps_in_folder_mode() {
    let paper = crate::prompts::compiled_default("orientation").unwrap();
    let resolved = resolve_survey_template(paper, "folder").expect("folder survey should load");
    let folder = crate::prompts::load_prompt("orientation_folder").unwrap();
    assert_eq!(resolved, folder);
}

#[test]
fn stock_prompt_resolution_keeps_schema_in_lockstep() {
    assert_eq!(
        resolve_survey_schema("", None, "document"),
        Some(crate::orientation_contract::paper_schema())
    );
    assert_eq!(
        resolve_survey_schema("", None, "folder"),
        Some(crate::orientation_contract::folder_schema())
    );

    let generic_prompt = crate::prompts::compiled_default("orientation_generic").unwrap();
    let generic_schema = crate::orientation_contract::generic_schema();
    assert_eq!(
        resolve_survey_schema(generic_prompt, Some(&generic_schema), "folder"),
        Some(crate::orientation_contract::folder_schema())
    );
}

#[test]
fn custom_prompt_and_schema_remain_authoritative() {
    let prompt = "Survey this input using the supplied contract. {input_text}";
    let schema = serde_json::json!({
        "type": "object",
        "required": ["custom"],
        "properties": {"custom": {"type": "string"}}
    });
    assert_eq!(resolve_survey_schema(prompt, None, "folder"), None);
    assert_eq!(
        resolve_survey_schema(prompt, Some(&schema), "folder"),
        Some(schema)
    );
}

#[test]
fn resolve_custom_prompt_wins_in_any_mode() {
    let custom = "Survey this thing my way. {input_text}";
    assert_eq!(
        resolve_survey_template(custom, "folder").as_deref(),
        Some(custom)
    );
    assert_eq!(
        resolve_survey_template(custom, "document").as_deref(),
        Some(custom)
    );
}

#[test]
fn document_text_containing_placeholder_tokens_is_not_rescanned() {
    let paper = "This paper studies {paper_text} and {input_text} as literals.";
    let prompt = substitute_input_text("Survey:\n{input_text}\nEnd.", paper).unwrap();
    assert_eq!(prompt, format!("Survey:\n{paper}\nEnd."));
    // Both aliases resolve, each exactly once, from the template only.
    let both = substitute_input_text("A {input_text} B {paper_text} C", "X").unwrap();
    assert_eq!(both, "A X B X C");
}
