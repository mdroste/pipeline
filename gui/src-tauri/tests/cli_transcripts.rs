//! Observable CLI transcripts captured before the CLI module extraction.
//! These commands parse or validate portable input without loading user stores,
//! checking provider credentials, installing engines, or submitting model work.
use serde::Deserialize;
use std::io::Write as _;
use std::process::{Command, Stdio};

#[derive(Deserialize)]
struct Transcript {
    name: String,
    args: Vec<String>,
    stdin: Option<String>,
    status: i32,
    stdout: String,
    stderr: String,
}

#[test]
fn cli_transcripts_preserve_output_channels_and_status() {
    let cases: Vec<Transcript> =
        serde_json::from_str(include_str!("fixtures/cli/transcripts.json")).unwrap();
    for case in cases {
        let mut child = Command::new(env!("CARGO_BIN_EXE_pipeline-cli"))
            .args(&case.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|error| panic!("{}: could not start CLI: {error}", case.name));
        let mut input = child.stdin.take().expect("piped stdin");
        if let Some(text) = case.stdin {
            input
                .write_all(text.as_bytes())
                .unwrap_or_else(|error| panic!("{}: could not write stdin: {error}", case.name));
        }
        drop(input);
        let output = child
            .wait_with_output()
            .unwrap_or_else(|error| panic!("{}: could not collect output: {error}", case.name));
        assert_eq!(
            output.status.code(),
            Some(case.status),
            "{}: status",
            case.name
        );
        assert_eq!(
            output.stdout,
            case.stdout.as_bytes(),
            "{}: stdout",
            case.name
        );
        assert_eq!(
            output.stderr,
            case.stderr.as_bytes(),
            "{}: stderr",
            case.name
        );
    }
}
