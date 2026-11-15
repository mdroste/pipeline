//! Exit statuses a script can rely on, as `docs/cli.md` lists them.
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

#[test]
fn paper_review_stops_after_orientation_for_a_restaurant_menu() {
    use serde_json::{json, Value};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};

    let reason = "It appears to be a restaurant menu listing dishes and prices.";
    let home = tempfile::tempdir().unwrap();
    let settings_dir = home.path().join(".pipeline");
    std::fs::create_dir(&settings_dir).unwrap();
    let menu = home.path().join("menu.txt");
    std::fs::write(&menu, "Dinner menu\nTomato soup $8\nGrilled salmon $24\n").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::fs::write(
        settings_dir.join("settings.json"),
        json!({
            "preferred_provider": "local",
            "local_base_url": format!("http://{address}/v1"),
            "local_model": "fixture"
        })
        .to_string(),
    )
    .unwrap();
    let server = std::thread::spawn(move || {
        let mut requests = Vec::<Value>::new();
        for stream in listener.incoming() {
            let mut reader = BufReader::new(stream.unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap() == 0 {
                break; // The test closed a connection to stop the listener.
            }
            let mut length = 0;
            for header in reader.by_ref().lines() {
                let header = header.unwrap().to_ascii_lowercase();
                if header.is_empty() {
                    break;
                }
                if let Some(value) = header.strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
            }
            let reply = if line.starts_with("GET ") {
                json!({"data": [{"id": "fixture"}]})
            } else {
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                requests.push(serde_json::from_slice(&body).unwrap());
                let rejection = json!({"orientation": null, "rejection_reason": reason});
                json!({
                    "choices": [{
                        "message": {"role": "assistant", "content": rejection.to_string()},
                        "finish_reason": "stop"
                    }],
                    "usage": {"prompt_tokens": 1, "completion_tokens": 1}
                })
            }
            .to_string();
            write!(
                reader.into_inner(),
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            )
            .unwrap();
        }
        requests
    });
    let output = cli(
        home.path(),
        &[
            "run",
            "--profile",
            "auto-paper-review",
            "--input",
            menu.to_str().unwrap(),
        ],
    );
    drop(TcpStream::connect(address).unwrap());
    let requests = server.join().unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains(&format!(
            "Review stopped. The document couldn’t be identified as an academic paper. {reason}"
        )),
        "{stderr}"
    );
    assert!(output.stdout.is_empty());
    assert_eq!(requests.len(), 1, "{stderr}");
    let messages = requests[0]["messages"].to_string();
    assert!(messages.contains("Dinner menu"));
    assert!(messages.contains("First decide whether"));
}
