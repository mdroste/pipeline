//! Scripted loopback servers cover transport behavior. The ignored tests reach
//! the real hosts through the production client so CI notices when a pinned
//! artifact moves, disappears, or starts arriving differently.
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Serve one scripted raw response per connection, then return the request heads.
async fn serve(responses: Vec<Vec<u8>>) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for response in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.windows(4).any(|window| window == b"\r\n\r\n") {
                let mut buffer = [0u8; 1024];
                let count = socket.read(&mut buffer).await.unwrap();
                assert_ne!(count, 0);
                head.extend_from_slice(&buffer[..count]);
            }
            requests.push(String::from_utf8_lossy(&head).to_ascii_lowercase());
            socket.write_all(&response).await.unwrap();
            socket.shutdown().await.unwrap();
        }
        requests
    });
    (origin, task)
}

fn response(status: &str, headers: &str, body: &[u8]) -> Vec<u8> {
    let head = format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    [head.as_bytes(), body].concat()
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

struct Outcome {
    result: Result<u64, String>,
    stored: Option<Vec<u8>>,
}

async fn fetch(sources: &[String], expected: &str, max_bytes: u64) -> Outcome {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("artifact");
    let app: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    let client = download_client(false).unwrap();
    let job = Job {
        client: &client,
        app: &app,
        max_bytes,
        label: "test artifact",
    };
    let result = fetch_verified(&job, sources, &destination, expected).await;
    Outcome {
        result,
        stored: std::fs::read(&destination).ok(),
    }
}

#[derive(Default)]
struct RecordedDownloads(Mutex<Vec<serde_json::Value>>);

impl crate::emit::Events for RecordedDownloads {
    fn emit_event(
        &self,
        event: &str,
        payload: serde_json::Value,
    ) -> Result<(), crate::emit::EmitError> {
        if event == "engines:download" {
            self.0.lock().unwrap().push(payload["download"].clone());
        }
        Ok(())
    }
}

#[tokio::test]
async fn reports_the_size_on_connection_and_clears_when_finished() {
    let body = b"release asset".to_vec();
    let (origin, _) = serve(vec![response("200 OK", "", &body)]).await;
    let recorded = std::sync::Arc::new(RecordedDownloads::default());
    let app: crate::emit::EventBus = recorded.clone();
    let client = download_client(false).unwrap();
    let job = Job {
        client: &client,
        app: &app,
        max_bytes: 1024,
        label: "test artifact",
    };
    let directory = tempfile::tempdir().unwrap();

    fetch_verified(
        &job,
        &[format!("{origin}/asset")],
        &directory.path().join("artifact"),
        &digest(&body),
    )
    .await
    .unwrap();

    let events = recorded.0.lock().unwrap();
    assert_eq!(events[0]["label"], "test artifact");
    assert_eq!(events[0]["total_bytes"], serde_json::Value::Null);
    assert_eq!(events[1]["total_bytes"], body.len() as u64);
    assert_eq!(events[1]["retrying"], false);
    assert!(events.last().unwrap().is_null());
}

#[tokio::test]
async fn a_failed_download_also_clears_its_progress() {
    let (origin, _) = serve(vec![response("404 Not Found", "", b"")]).await;
    let recorded = std::sync::Arc::new(RecordedDownloads::default());
    let app: crate::emit::EventBus = recorded.clone();
    let client = download_client(false).unwrap();
    let job = Job {
        client: &client,
        app: &app,
        max_bytes: 1024,
        label: "test artifact",
    };
    let directory = tempfile::tempdir().unwrap();

    let result = fetch_verified(
        &job,
        &[format!("{origin}/asset")],
        &directory.path().join("artifact"),
        &digest(b"x"),
    )
    .await;

    assert!(result.is_err());
    assert!(recorded.0.lock().unwrap().last().unwrap().is_null());
}

#[test]
fn the_meter_paces_reports_and_smooths_the_rate() {
    let start = Instant::now();
    let mut meter = Meter::new(start, 1_000);

    assert!(!meter.sample(start + Duration::from_millis(100), 2_000));
    assert!(meter.sample(start + Duration::from_secs(1), 1_001_000));
    assert_eq!(meter.bytes_per_second as u64, 1_000_000);

    assert!(meter.sample(start + Duration::from_secs(2), 1_001_000));
    assert_eq!(meter.bytes_per_second as u64, 800_000);
}

#[tokio::test]
async fn follows_a_redirect_to_another_origin() {
    let body = b"release asset".to_vec();
    let (cdn, cdn_requests) = serve(vec![response("200 OK", "", &body)]).await;
    let location = format!("Location: {cdn}/signed/asset?token=abc\r\n");
    let (host, _) = serve(vec![response("302 Found", &location, b"")]).await;

    let outcome = fetch(&[format!("{host}/asset")], &digest(&body), 1024).await;

    assert_eq!(outcome.result, Ok(body.len() as u64));
    assert_eq!(outcome.stored, Some(body));
    assert!(cdn_requests.await.unwrap()[0].starts_with("get /signed/asset?token=abc "));
}

#[tokio::test]
async fn an_interrupted_transfer_resumes_where_it_stopped() {
    let body = b"0123456789abcdefghij".to_vec();
    let truncated = [
        b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\n".as_slice(),
        &body[..8],
    ]
    .concat();
    let resumed = response(
        "206 Partial Content",
        "Content-Range: bytes 8-19/20\r\n",
        &body[8..],
    );
    let (origin, requests) = serve(vec![truncated, resumed]).await;

    let outcome = fetch(&[format!("{origin}/asset")], &digest(&body), 1024).await;

    assert_eq!(outcome.result, Ok(body.len() as u64));
    assert_eq!(outcome.stored, Some(body));
    let requests = requests.await.unwrap();
    assert!(!requests[0].contains("range:"));
    assert!(requests[1].contains("range: bytes=8-\r\n"));
}

#[tokio::test]
async fn a_server_that_ignores_the_range_restarts_the_file() {
    let body = b"0123456789abcdefghij".to_vec();
    let truncated = [
        b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\n".as_slice(),
        &body[..8],
    ]
    .concat();
    let (origin, _) = serve(vec![truncated, response("200 OK", "", &body)]).await;

    let outcome = fetch(&[format!("{origin}/asset")], &digest(&body), 1024).await;

    assert_eq!(outcome.result, Ok(body.len() as u64));
    assert_eq!(outcome.stored, Some(body));
}

#[tokio::test]
async fn a_range_at_the_wrong_offset_is_never_appended() {
    let body = b"0123456789abcdefghij".to_vec();
    let truncated = [
        b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\n".as_slice(),
        &body[..8],
    ]
    .concat();
    let misplaced = response(
        "206 Partial Content",
        "Content-Range: bytes 4-19/20\r\n",
        &body[4..],
    );
    let (origin, requests) = serve(vec![truncated, misplaced, response("200 OK", "", &body)]).await;

    let outcome = fetch(&[format!("{origin}/asset")], &digest(&body), 1024).await;

    assert_eq!(outcome.result, Ok(body.len() as u64));
    assert_eq!(outcome.stored, Some(body));
    assert!(!requests.await.unwrap()[2].contains("range:"));
}

#[tokio::test]
async fn server_errors_retry_against_the_same_source() {
    let body = b"release asset".to_vec();
    let (origin, requests) = serve(vec![
        response("503 Service Unavailable", "", b""),
        response("429 Too Many Requests", "Retry-After: 0\r\n", b""),
        response("200 OK", "", &body),
    ])
    .await;

    let outcome = fetch(&[format!("{origin}/asset")], &digest(&body), 1024).await;

    assert_eq!(outcome.result, Ok(body.len() as u64));
    assert_eq!(requests.await.unwrap().len(), 3);
}

#[tokio::test]
async fn a_missing_artifact_falls_through_to_the_next_source_without_retrying() {
    let body = b"release asset".to_vec();
    let (primary, primary_requests) = serve(vec![response("404 Not Found", "", b"")]).await;
    let (mirror, _) = serve(vec![response("200 OK", "", &body)]).await;

    let outcome = fetch(
        &[format!("{primary}/asset"), format!("{mirror}/asset")],
        &digest(&body),
        1024,
    )
    .await;

    assert_eq!(outcome.result, Ok(body.len() as u64));
    assert_eq!(outcome.stored, Some(body));
    assert_eq!(primary_requests.await.unwrap().len(), 1);
}

#[tokio::test]
async fn wrong_bytes_from_one_source_do_not_taint_the_next() {
    let body = b"release asset".to_vec();
    let (primary, _) = serve(vec![response("200 OK", "", b"something else entirely")]).await;
    let (mirror, _) = serve(vec![response("200 OK", "", &body)]).await;

    let outcome = fetch(
        &[format!("{primary}/asset"), format!("{mirror}/asset")],
        &digest(&body),
        1024,
    )
    .await;

    assert_eq!(outcome.result, Ok(body.len() as u64));
    assert_eq!(outcome.stored, Some(body));
}

#[tokio::test]
async fn a_checksum_mismatch_from_every_source_is_refused() {
    let (origin, _) = serve(vec![response("200 OK", "", b"tampered")]).await;

    let outcome = fetch(&[format!("{origin}/asset")], &digest(b"genuine"), 1024).await;

    let error = outcome.result.unwrap_err();
    assert!(error.contains("checksum mismatch"), "{error}");
    assert!(
        error.starts_with("test artifact download failed: "),
        "{error}"
    );
}

#[tokio::test]
async fn the_size_limit_applies_across_a_resumed_transfer() {
    let body = vec![7u8; 32];
    let truncated = [
        b"HTTP/1.1 200 OK\r\nContent-Length: 32\r\nConnection: close\r\n\r\n".as_slice(),
        &body[..16],
    ]
    .concat();
    let resumed = response(
        "206 Partial Content",
        "Content-Range: bytes 16-47/48\r\n",
        &[7u8; 32],
    );
    let (origin, _) = serve(vec![truncated, resumed]).await;

    let outcome = fetch(&[format!("{origin}/asset")], &digest(&body), 40).await;

    let error = outcome.result.unwrap_err();
    assert!(error.contains("download safety limit"), "{error}");
}

#[tokio::test]
async fn redirect_loops_are_bounded_and_name_the_source() {
    let loops = (0..=MAX_REDIRECTS)
        .map(|_| response("302 Found", "Location: /asset\r\n", b""))
        .collect();
    let (origin, requests) = serve(loops).await;

    let outcome = fetch(&[format!("{origin}/asset")], &digest(b"x"), 1024).await;

    let error = outcome.result.unwrap_err();
    assert!(error.contains(source_host(&origin)), "{error}");
    assert_eq!(requests.await.unwrap().len(), MAX_REDIRECTS + 1);
}

#[tokio::test]
async fn the_production_client_refuses_plain_http() {
    let error = download_client(true)
        .unwrap()
        .get("http://127.0.0.1:9/asset")
        .send()
        .await
        .unwrap_err();
    assert!(matches!(
        request_failure("127.0.0.1:9", error),
        Failure::Source(_)
    ));
}

#[test]
fn source_hosts_exclude_paths_and_queries() {
    assert_eq!(
        source_host("https://github.com/ggml-org/llama.cpp"),
        "github.com"
    );
    assert_eq!(source_host("https://example.test?x=1"), "example.test");
}

fn every_pinned_source() -> Vec<String> {
    let mut sources = vec![
        PADDLE_LAYOUT_MODEL_URL.to_string(),
        paddle_model_url(PADDLE_MODEL_FILE),
        paddle_model_url(PADDLE_MMPROJ_FILE),
    ];
    sources.extend(LLAMA_ARTIFACTS.iter().map(llama_download_url));
    sources.extend(UV_ARTIFACTS.iter().flat_map(uv_download_urls));
    sources.extend(PYTHON_ARTIFACTS.iter().flat_map(python_download_urls));
    sources
}

#[test]
fn managed_artifacts_with_two_publishers_list_both() {
    for artifact in UV_ARTIFACTS {
        assert_eq!(uv_download_urls(artifact).len(), 2);
    }
    for artifact in PYTHON_ARTIFACTS {
        assert_eq!(python_download_urls(artifact).len(), 2);
    }
}

#[tokio::test]
#[ignore = "requires the public release hosts"]
async fn pinned_engine_sources_answer_through_the_production_client() {
    let client = download_client(true).unwrap();
    let mut unavailable = Vec::new();
    for url in every_pinned_source() {
        let answer = client.get(&url).header(RANGE, "bytes=0-0").send().await;
        match answer {
            Ok(response) if response.status().is_success() => {}
            Ok(response) => unavailable.push(format!("{url}: HTTP {}", response.status())),
            Err(error) => unavailable.push(describe_error(&url, error)),
        }
    }
    assert!(unavailable.is_empty(), "{}", unavailable.join("\n"));
}

#[tokio::test]
#[ignore = "requires the public release hosts"]
async fn pinned_windows_runtime_downloads_and_verifies_from_github() {
    let artifact = llama_artifact_for("windows", "x86_64").unwrap();
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join(artifact.filename);
    let app: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);

    let bytes = download_verified(
        &app,
        &[llama_download_url(artifact)],
        &destination,
        artifact.sha256,
        MAX_LLAMA_ARCHIVE_BYTES,
        "llama.cpp runtime",
    )
    .await
    .unwrap();

    assert_eq!(std::fs::metadata(&destination).unwrap().len(), bytes);
}
