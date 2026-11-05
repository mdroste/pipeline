//! Exit statuses a script can rely on, as `docs/cli.md` lists them. Each command
//! here stops before any provider, parser, or model call.
#![cfg(unix)]

use std::process::{Command, Output};

/// `home` stands in for the user's home folder, so no real data is read or made.
fn cli(home: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pipeline-cli"))
        .args(args)
        .env("HOME", home)
        .output()
        .expect("CLI starts")
}

fn saved_workflow(directory: &std::path::Path) -> String {
    let template = cli(directory, &["workflow", "template"]);
    let path = directory.join("workflow.json");
    std::fs::write(&path, template.stdout).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn invalid_run_input_exits_2_like_check() {
    let home = tempfile::tempdir().unwrap();
    let workflow = saved_workflow(home.path());
    let paper = home.path().join("paper.pdf");
    std::fs::write(&paper, b"%PDF-1.4\n").unwrap();
    let paper = paper.to_str().unwrap();
    let missing = home.path().join("missing.pdf");
    let missing = missing.to_str().unwrap();
    for arguments in [
        vec!["--input", missing],
        vec!["--profile", "no-such-workflow", "--input", paper],
        vec!["--workflow", workflow.as_str(), "--input", missing],
        vec![
            "--workflow",
            workflow.as_str(),
            "--input",
            paper,
            "--var",
            "bogus=1",
        ],
    ] {
        for command in ["run", "check"] {
            let output = cli(home.path(), &[&[command], arguments.as_slice()].concat());
            assert_eq!(
                output.status.code(),
                Some(2),
                "{command} {arguments:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn an_unknown_profile_exits_2_when_shown() {
    let home = tempfile::tempdir().unwrap();
    let output = cli(home.path(), &["profiles", "show", "no-such-workflow"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_workflow_that_validation_rejects_exits_1() {
    let home = tempfile::tempdir().unwrap();
    let missing = home.path().join("missing.json");
    let output = cli(
        home.path(),
        &["workflow", "validate", missing.to_str().unwrap()],
    );
    assert_eq!(output.status.code(), Some(1));
}
