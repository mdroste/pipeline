use super::*;

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn subcommand_help_is_handled_before_required_options() {
    assert!(matches!(
        parse_cli(&strings(&["run", "--help"])),
        Ok(ParseAction::Help(help)) if help == RUN_HELP
    ));
    assert!(matches!(
        parse_cli(&strings(&["batch", "-h"])),
        Ok(ParseAction::Help(help)) if help == BATCH_HELP
    ));
}

#[test]
fn run_parser_supports_current_input_and_runtime_options() {
    let action = parse_cli(&strings(&[
        "run",
        "--input",
        "/tmp/project",
        "--interpret-as",
        "latex-project",
        "--profile",
        "deep-review",
        "--var",
        "audience=editor",
        "--var",
        "optional=",
        "--extra-input",
        "appendix=/tmp/appendix.pdf",
        "--out",
        "/tmp/report.md",
        "--force",
    ]))
    .unwrap();
    let ParseAction::Execute(Command::Run(parsed)) = action else {
        panic!("expected parsed run command");
    };
    assert_eq!(parsed.input.as_deref(), Some("/tmp/project"));
    assert_eq!(
        parsed.input_interpretation.as_deref(),
        Some("latex_project")
    );
    assert_eq!(parsed.profile_id.as_deref(), Some("deep-review"));
    assert_eq!(
        parsed.variables.get("audience").map(String::as_str),
        Some("editor")
    );
    assert_eq!(
        parsed.variables.get("optional").map(String::as_str),
        Some("")
    );
    assert_eq!(
        parsed.extra_inputs.get("appendix").map(String::as_str),
        Some("/tmp/appendix.pdf")
    );
    assert_eq!(parsed.out.as_deref(), Some("/tmp/report.md"));
    assert!(parsed.force);
}

#[test]
fn run_parser_allows_no_input_but_not_an_orphaned_interpretation() {
    assert!(matches!(
        parse_cli(&strings(&["run", "--profile", "prompt-only"])),
        Ok(ParseAction::Execute(Command::Run(_)))
    ));
    assert!(parse_cli(&strings(&["run", "--interpret-as", "document"])).is_err());
}

#[test]
fn parser_rejects_unknown_duplicate_and_missing_arguments() {
    assert!(parse_cli(&strings(&["run", "--wat"])).is_err());
    assert!(parse_cli(&strings(&["run", "--input"])).is_err());
    assert!(parse_cli(&strings(&["run", "--profile", "one", "--profile", "two"])).is_err());
    assert!(parse_cli(&strings(&["run", "--var", "key=one", "--var", "key=two"])).is_err());
    assert!(parse_cli(&strings(&[
        "run",
        "--profile",
        "one",
        "--workflow",
        "workflow.json"
    ]))
    .is_err());
}

#[test]
fn workflow_commands_and_ephemeral_sources_parse() {
    assert!(matches!(
        parse_cli(&strings(&["workflow", "schema"])),
        Ok(ParseAction::Execute(Command::Workflow(
            WorkflowArgs::Schema
        )))
    ));
    assert!(matches!(
        parse_cli(&strings(&[
            "workflow",
            "validate",
            "-",
            "--json"
        ])),
        Ok(ParseAction::Execute(Command::Workflow(
            WorkflowArgs::Validate { source, json: true }
        ))) if source == "-"
    ));
    let action = parse_cli(&strings(&[
        "check",
        "--workflow",
        "workflow.json",
        "--json",
    ]))
    .unwrap();
    let ParseAction::Execute(Command::Check(parsed)) = action else {
        panic!("expected check command");
    };
    assert_eq!(parsed.workflow_path.as_deref(), Some("workflow.json"));
    assert!(parsed.profile_id.is_none());
}

#[test]
fn workflow_loader_uses_the_strict_portable_contract() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("workflow.json");
    let template = pipeline_config::workflow_template().unwrap();
    std::fs::write(&path, &template.canonical_json).unwrap();
    let loaded = load_cli_workflow(path.to_str().unwrap()).unwrap();
    assert_eq!(loaded.document.fingerprint, template.fingerprint);
    assert_eq!(loaded.document.config.outputs.primary_step, "synthesis");
}

#[test]
fn profiles_support_machine_readable_listing_and_inspection() {
    assert!(matches!(
        parse_cli(&strings(&["profiles", "--json"])),
        Ok(ParseAction::Execute(Command::Profiles(
            ProfilesArgs::List { json: true }
        )))
    ));
    assert!(matches!(
        parse_cli(&strings(&["profiles", "show", "quick-review"])),
        Ok(ParseAction::Execute(Command::Profiles(
            ProfilesArgs::Show { id }
        ))) if id == "quick-review"
    ));
}

#[test]
fn dependency_check_parser_keeps_the_concrete_pdf_input() {
    let action = parse_cli(&strings(&[
        "check",
        "--profile",
        "paddle-review",
        "--input",
        "/tmp/paper.PDF",
        "--interpret-as",
        "document",
        "--json",
    ]))
    .unwrap();
    let ParseAction::Execute(Command::Check(parsed)) = action else {
        panic!("expected parsed dependency check");
    };
    assert_eq!(parsed.input.as_deref(), Some("/tmp/paper.PDF"));
    assert_eq!(parsed.input_interpretation.as_deref(), Some("document"));
    assert_eq!(parsed.profile_id.as_deref(), Some("paddle-review"));
    assert!(parsed.json);
}

#[test]
fn engine_commands_use_the_managed_parser_id_and_confirm_uninstall() {
    assert!(matches!(
        parse_cli(&strings(&["engines", "install", "paddle"])),
        Ok(ParseAction::Execute(Command::Engines(
            EnginesArgs::Install { id }
        ))) if id == "paddleocr-vl-parser"
    ));
    assert!(parse_cli(&strings(&["engines", "uninstall", "paddle"])).is_err());
    assert!(matches!(
        parse_cli(&strings(&[
            "engines",
            "uninstall",
            "paddleocr-vl-parser",
            "--yes"
        ])),
        Ok(ParseAction::Execute(Command::Engines(
            EnginesArgs::Uninstall { id }
        ))) if id == "paddleocr-vl-parser"
    ));
}

#[test]
fn batch_parser_requires_a_folder_and_supports_report_export() {
    let action = parse_cli(&strings(&[
        "batch",
        "--input-dir",
        "/tmp/papers",
        "--profile",
        "quick-review",
        "--out-dir",
        "/tmp/reports",
        "--force",
    ]))
    .unwrap();
    let ParseAction::Execute(Command::Batch(parsed)) = action else {
        panic!("expected parsed batch command");
    };
    assert_eq!(parsed.input_dir, "/tmp/papers");
    assert_eq!(parsed.profile_id.as_deref(), Some("quick-review"));
    assert_eq!(parsed.out_dir.as_deref(), Some("/tmp/reports"));
    assert!(parsed.force);
    assert!(parse_cli(&strings(&["batch", "--profile", "quick-review"])).is_err());
}

#[test]
fn batch_output_names_fail_before_overwriting_colliding_stems() {
    let temp = tempfile::tempdir().unwrap();
    let inputs = strings(&["/tmp/paper.pdf", "/tmp/paper.docx"]);
    let error = prepare_batch_outputs(temp.path().to_str(), &inputs, false).unwrap_err();
    assert!(error.contains("multiple inputs would write"));
}

#[test]
fn report_outputs_require_force_before_replacing_a_file() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("report.md");
    write_report_file(&output, "first", false).unwrap();
    assert!(write_report_file(&output, "second", false).is_err());
    write_report_file(&output, "second", true).unwrap();
    assert_eq!(std::fs::read_to_string(output).unwrap(), "second");
}

#[test]
fn output_validation_rejects_existing_files_and_missing_directories() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("report.md");
    std::fs::write(&output, "existing").unwrap();
    assert!(validate_output_path(&output, false).is_err());
    assert!(validate_output_path(&output, true).is_ok());
    assert!(validate_output_path(&temp.path().join("missing/report.md"), false).is_err());
}

#[test]
fn interruption_errors_map_to_shell_interrupt_status() {
    assert!(is_interruption("Pipeline cancelled by interrupt"));
    assert!(is_interruption(
        "Pipeline cancellation timed out; child processes were terminated"
    ));
    assert!(is_interruption(
        "Engine installation cancelled by interrupt"
    ));
    assert!(is_interruption("Pipeline cancelled"));
    assert!(is_interruption("Pass 'technical/claude' cancelled"));
    assert!(!is_interruption("provider authentication failed"));
    assert!(!is_interruption("provider subscription cancelled"));
    assert!(!is_interruption(
        "provider process was interrupted unexpectedly"
    ));
}
