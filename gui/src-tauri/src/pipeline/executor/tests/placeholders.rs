//! Placeholders regression coverage.

use super::*;

#[test]
fn step_pointer_references_select_into_structured_artifacts() {
    let output = findings_output(
        "extract",
        r#"{"findings":[{"id":"a","title":"First"}],"note":"n"}"#,
    );
    assert_eq!(step_ref_text(&output, None), output.raw_text);
    assert_eq!(step_ref_text(&output, Some("/note")), "n");
    assert_eq!(step_ref_text(&output, Some("/findings/0/title")), "First");
    assert!(step_ref_text(&output, Some("/missing")).contains("no value at '/missing'"));
    assert!(step_ref_text(&output, Some("bad")).contains("start with '/'"));
    let prose = findings_output("prose", "just text");
    assert!(step_ref_text(&prose, Some("/x")).contains("not structured JSON"));
}

#[test]
fn substitute_replaces_known_and_marks_optional_missing() {
    let mut vars = std::collections::HashMap::new();
    vars.insert("journal".to_string(), "AER".to_string());
    let out = substitute_placeholders(
        "Review for {var:journal}; persona {var:persona}.",
        "{var:",
        &vars,
    )
    .unwrap();
    assert_eq!(
        out,
        "Review for AER; persona (optional value 'persona' not supplied)."
    );
}

#[test]
fn substitute_no_placeholders_is_unchanged() {
    let vars = std::collections::HashMap::new();
    assert_eq!(
        substitute_placeholders("plain prompt", "{var:", &vars).unwrap(),
        "plain prompt"
    );
}

#[test]
fn substitute_unclosed_brace_passes_through() {
    let vars = std::collections::HashMap::new();
    assert_eq!(
        substitute_placeholders("oops {var:x", "{var:", &vars).unwrap(),
        "oops {var:x"
    );
}

#[test]
fn substitute_run_context_applies_vars_and_inputs() {
    let mut vars = std::collections::HashMap::new();
    vars.insert("journal".to_string(), "QJE".to_string());
    let mut inputs = std::collections::HashMap::new();
    inputs.insert("letter".to_string(), "/tmp/letter.txt".to_string());
    let out = substitute_run_context(
        "Journal {var:journal}; read the response at {input:letter}.",
        &vars,
        &inputs,
    )
    .unwrap();
    assert_eq!(out, "Journal QJE; read the response at /tmp/letter.txt.");
}

#[test]
fn expand_template_basic() {
    let prior = vec![
        StepOutput {
            step_id: "s1".into(),
            step_label: "Step 1".into(),
            phase: "parallel".into(),
            agent: String::new(),
            raw_text: "output1".into(),
            ..Default::default()
        },
        StepOutput {
            step_id: "s2".into(),
            step_label: "Step 2".into(),
            phase: "parallel".into(),
            agent: String::new(),
            raw_text: "output2".into(),
            ..Default::default()
        },
    ];
    let template = "Prior:\n{prior_outputs}\n\nLast: {last_output}";
    let result = expand_template(
        template,
        "/orient.json",
        "Read it.",
        &prior,
        "/paper.txt",
        "",
        "/source.tex",
    )
    .unwrap();
    assert!(result.contains("## Step 1"));
    assert!(result.contains("output1"));
    assert!(result.contains("output2"));
    assert!(result.contains("Last: output2"));
}

#[test]
fn expand_template_backward_compat_aliases() {
    let prior = vec![StepOutput {
        step_id: "s1".into(),
        step_label: "S1".into(),
        phase: "parallel".into(),
        agent: String::new(),
        raw_text: "text".into(),
        ..Default::default()
    }];
    let template = "{referee_reports} | {editor_synthesis}";
    let result =
        expand_template(template, "", "", &prior, "/paper.txt", "", "/source.tex").unwrap();
    assert!(result.contains("## S1"));
    assert!(result.contains("text | text"));
}

#[test]
fn aggregate_placeholders_omit_skipped_reports_but_named_refs_keep_them() {
    let prior = vec![
        StepOutput {
            step_id: "selected".into(),
            step_label: "Selected".into(),
            raw_text: "useful finding".into(),
            ..Default::default()
        },
        StepOutput {
            step_id: "not_selected".into(),
            step_label: "Not Selected".into(),
            raw_text: "_(skipped: run_if condition not met)_".into(),
            skipped: true,
            ..Default::default()
        },
    ];
    let result = expand_template(
        "{prior_outputs}\nLAST={last_output}\nEXPLICIT={step:not_selected}",
        "",
        "",
        &prior,
        "",
        "",
        "",
    )
    .unwrap();
    assert!(result.contains("## Selected\n\nuseful finding"));
    assert!(!result.contains("## Not Selected"));
    assert!(result.contains("LAST=useful finding"));
    assert!(result.contains("EXPLICIT=_(skipped: run_if condition not met)_"));
}

#[test]
fn expand_template_no_prior() {
    let template = "Last: {last_output}";
    let result = expand_template(template, "", "", &[], "/paper.txt", "", "/source.tex").unwrap();
    assert!(result.contains("(not yet generated)"));
}

#[test]
fn inserted_step_outputs_are_not_rescanned_for_placeholders() {
    // Model output may quote placeholder-shaped text from the reviewed
    // document; it must reach the prompt verbatim, not be re-expanded.
    let prior = vec![StepOutput {
        step_id: "analysis".into(),
        step_label: "Analysis".into(),
        raw_text: "quotes {step:analysis}, {paper_path}, and {last_output}".into(),
        ..Default::default()
    }];
    let result = expand_template(
        "{prior_outputs}\nPATH={paper_path}\nREF={step:analysis}",
        "",
        "",
        &prior,
        "/paper.txt",
        "",
        "",
    )
    .unwrap();
    assert_eq!(
        result
            .matches("quotes {step:analysis}, {paper_path}, and {last_output}")
            .count(),
        2, // once via {prior_outputs}, once via the explicit {step:analysis}
    );
    assert!(result.contains("PATH=/paper.txt"));
}

#[test]
fn step_ref_exact_match() {
    let prior = vec![
        out("technical", "Technical", "tech body"),
        out("empirical", "Empirical", "emp body"),
    ];
    let result = expand_template("Tech: {step:technical}", "", "", &prior, "p", "", "s").unwrap();
    assert!(result.contains("Tech: tech body"));
    assert!(!result.contains("emp body"));
}

#[test]
fn step_ref_multi_agent_base_id_joins() {
    let prior = vec![
        out("technical/claude", "Technical (Claude)", "claude says"),
        out(
            "technical/antigravity",
            "Technical (Antigravity)",
            "antigravity says",
        ),
    ];
    let result = expand_template("All: {step:technical}", "", "", &prior, "p", "", "s").unwrap();
    assert!(result.contains("claude says"));
    assert!(result.contains("antigravity says"));
    assert!(result.contains("---"));
}

#[test]
fn step_ref_multi_agent_specific_id() {
    let prior = vec![
        out("technical/claude", "Technical (Claude)", "claude says"),
        out(
            "technical/antigravity",
            "Technical (Antigravity)",
            "antigravity says",
        ),
    ];
    let result = expand_template(
        "Just one: {step:technical/claude}",
        "",
        "",
        &prior,
        "p",
        "",
        "s",
    )
    .unwrap();
    assert!(result.contains("claude says"));
    assert!(!result.contains("antigravity says"));
}

#[test]
fn step_ref_unknown_id_emits_notice() {
    let prior = vec![out("technical", "Technical", "tech body")];
    let result =
        expand_template("Missing: {step:nonexistent}", "", "", &prior, "p", "", "s").unwrap();
    assert!(result.contains("(no output for step 'nonexistent')"));
}

#[test]
fn step_ref_unclosed_brace_passes_through() {
    let prior = vec![out("a", "A", "aa")];
    let result = expand_template("Broken: {step:a", "", "", &prior, "p", "", "s").unwrap();
    assert!(result.contains("{step:a"));
}

#[test]
fn step_ref_empty_id() {
    let prior = vec![out("a", "A", "aa")];
    let result = expand_template("{step:}", "", "", &prior, "p", "", "s").unwrap();
    assert!(result.contains("(empty step reference)"));
}
