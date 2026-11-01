//! Read-only file delivery to the webview over the `pipeline-file` scheme, so
//! viewers can use `<img>`, `<video>` and range requests instead of base64 IPC.
//!
//! Stateless mechanics only. Each owner validates its own path and registers a
//! `FileSource`, so this module holds no root and applies no scope policy, and
//! Workspace and Review sources never see each other. A URL carries an
//! unguessable in-memory token, so model-written Markdown cannot reach a file
//! and nothing survives a restart. A source is reopened per request, because a
//! held handle would interfere with sync clients and rename on Windows; a
//! changed stamp answers 409 and the viewer reloads.
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, LazyLock, Mutex};
use tauri::http::{header, HeaderValue, Method, Request, Response, StatusCode};

pub const SCHEME: &str = "pipeline-file";
/// The most one ranged response holds in memory. Clients continue from where
/// it stops, as they must for any server that answers a shorter range.
const MAX_PART: u64 = 4 * 1024 * 1024;
/// `<img>` and sandboxed frames send no Range header.
const MAX_WHOLE: u64 = 256 * 1024 * 1024;
const MAX_TOKENS: usize = 64;
const BYTES_POLICY: &str = "sandbox; default-src 'none'";
/// A rendered document may style itself. It cannot script, navigate or fetch.
const DOCUMENT_POLICY: &str =
    "sandbox; default-src 'none'; style-src 'unsafe-inline' data:; img-src data:; font-src data:";

pub struct OpenedSource {
    pub file: File,
    pub stamp: String,
    pub size: u64,
}
/// One validated file. `open` repeats the owner's scope checks on every request.
pub trait FileSource: Send + Sync {
    fn open(&self) -> Result<Option<OpenedSource>, String>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presentation {
    Bytes,
    /// Rendered in a sandboxed frame.
    Document,
}
struct Entry {
    source: Arc<dyn FileSource>,
    mime: &'static str,
    stamp: String,
    presentation: Presentation,
    used: u64,
}
#[derive(Default)]
struct Registry {
    entries: HashMap<String, Entry>,
    clock: u64,
}
static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(Mutex::default);

fn registry() -> std::sync::MutexGuard<'static, Registry> {
    REGISTRY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
/// Authorize one file for this run of the app. The oldest token gives way at
/// capacity, and its viewer gets a fresh one when it reloads.
pub fn issue(
    source: Arc<dyn FileSource>,
    mime: &'static str,
    stamp: String,
    presentation: Presentation,
) -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| error.to_string())?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let mut registry = registry();
    if registry.entries.len() >= MAX_TOKENS {
        if let Some(oldest) = registry
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.used)
            .map(|(token, _)| token.clone())
        {
            registry.entries.remove(&oldest);
        }
    }
    registry.clock += 1;
    let used = registry.clock;
    registry.entries.insert(
        token.clone(),
        Entry {
            source,
            mime,
            stamp,
            presentation,
            used,
        },
    );
    Ok(token)
}
pub fn revoke(token: &str) {
    registry().entries.remove(token);
}

/// A single satisfiable range. `Ok(None)` means ignore the header and send the
/// whole file, which is how multi-range and malformed values are answered.
fn parse_range(value: &str, size: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(spec) = value.trim().strip_prefix("bytes=") else {
        return Ok(None);
    };
    if spec.contains(',') {
        return Ok(None);
    }
    let Some((start, end)) = spec.split_once('-') else {
        return Ok(None);
    };
    let (start, end) = (start.trim(), end.trim());
    if start.is_empty() {
        let Ok(suffix) = end.parse::<u64>() else {
            return Ok(None);
        };
        if suffix == 0 || size == 0 {
            return Err(());
        }
        return Ok(Some((size.saturating_sub(suffix), size - 1)));
    }
    let Ok(first) = start.parse::<u64>() else {
        return Ok(None);
    };
    if first >= size {
        return Err(());
    }
    if end.is_empty() {
        return Ok(Some((first, size - 1)));
    }
    let Ok(last) = end.parse::<u64>() else {
        return Ok(None);
    };
    if last < first {
        return Ok(None);
    }
    Ok(Some((first, last.min(size - 1))))
}
fn allowed_origin(origin: &str) -> bool {
    matches!(
        origin,
        "tauri://localhost" | "http://tauri.localhost" | "https://tauri.localhost"
    ) || (cfg!(debug_assertions)
        && matches!(origin, "http://localhost:1420" | "http://127.0.0.1:1420"))
}
fn status(code: StatusCode) -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = code;
    response
}
fn read_part(file: &mut File, start: u64, length: u64) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(length).read_to_end(&mut bytes)?;
    Ok(bytes)
}
fn serve(request: &Request<Vec<u8>>, is_main: bool) -> Response<Vec<u8>> {
    if !is_main {
        return status(StatusCode::FORBIDDEN);
    }
    if request.method() == Method::OPTIONS {
        return status(StatusCode::NO_CONTENT);
    }
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return status(StatusCode::METHOD_NOT_ALLOWED);
    }
    let token = request.uri().path().trim_start_matches('/');
    let entry = {
        let mut registry = registry();
        registry.clock += 1;
        let clock = registry.clock;
        registry.entries.get_mut(token).map(|entry| {
            entry.used = clock;
            (
                entry.source.clone(),
                entry.mime,
                entry.stamp.clone(),
                entry.presentation,
            )
        })
    };
    // Gone, not missing: a viewer holding a released token asks for a new one.
    let Some((source, mime, stamp, presentation)) = entry else {
        return status(StatusCode::GONE);
    };
    let opened = match source.open() {
        Ok(Some(opened)) => opened,
        Ok(None) => return status(StatusCode::NOT_FOUND),
        Err(_) => return status(StatusCode::FORBIDDEN),
    };
    if opened.stamp != stamp {
        return status(StatusCode::CONFLICT);
    }
    let OpenedSource { mut file, size, .. } = opened;
    let range = request
        .headers()
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(|value| parse_range(value, size))
        .transpose();
    let range = match range {
        Ok(range) => range.flatten(),
        Err(()) => {
            let mut response = status(StatusCode::RANGE_NOT_SATISFIABLE);
            if let Ok(value) = HeaderValue::from_str(&format!("bytes */{size}")) {
                response.headers_mut().insert(header::CONTENT_RANGE, value);
            }
            return response;
        }
    };
    let (start, end, partial) = match range {
        Some((start, end)) => (start, end.min(start + MAX_PART - 1), true),
        None if size > MAX_WHOLE => return status(StatusCode::PAYLOAD_TOO_LARGE),
        None => (0, size.saturating_sub(1), false),
    };
    let length = if size == 0 { 0 } else { end - start + 1 };
    let body = if request.method() == Method::HEAD {
        Vec::new()
    } else {
        match read_part(&mut file, start, length) {
            Ok(body) => body,
            Err(_) => return status(StatusCode::INTERNAL_SERVER_ERROR),
        }
    };
    let mut response = Response::new(body);
    if partial {
        *response.status_mut() = StatusCode::PARTIAL_CONTENT;
    }
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    if partial {
        if let Ok(value) = HeaderValue::from_str(&format!("bytes {start}-{end}/{size}")) {
            headers.insert(header::CONTENT_RANGE, value);
        }
    }
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(match presentation {
            Presentation::Bytes => BYTES_POLICY,
            Presentation::Document => DOCUMENT_POLICY,
        }),
    );
    response
}
/// Answer one request. `is_main` is whether it came from the main webview.
pub fn handle(request: &Request<Vec<u8>>, is_main: bool) -> Response<Vec<u8>> {
    let mut response = serve(request, is_main);
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(origin) = request
        .headers()
        .get(header::ORIGIN)
        .filter(|origin| origin.to_str().is_ok_and(allowed_origin))
        .cloned()
    {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, HEAD, OPTIONS"),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("range"),
        );
        headers.insert(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            HeaderValue::from_static("content-range, content-length, accept-ranges"),
        );
        headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    struct Disk(PathBuf);
    impl FileSource for Disk {
        fn open(&self) -> Result<Option<OpenedSource>, String> {
            let Ok(file) = File::open(&self.0) else {
                return Ok(None);
            };
            let size = file.metadata().map_err(|e| e.to_string())?.len();
            Ok(Some(OpenedSource {
                file,
                stamp: format!("size:{size}"),
                size,
            }))
        }
    }
    fn fixture(bytes: &[u8]) -> (tempfile::TempDir, String) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("file.bin");
        std::fs::write(&path, bytes).unwrap();
        let token = issue(
            Arc::new(Disk(path)),
            "application/pdf",
            format!("size:{}", bytes.len()),
            Presentation::Bytes,
        )
        .unwrap();
        (temp, token)
    }
    fn get(token: &str, range: Option<&str>) -> Response<Vec<u8>> {
        let mut request = Request::builder().uri(format!("pipeline-file://localhost/{token}"));
        if let Some(range) = range {
            request = request.header("range", range);
        }
        handle(&request.body(Vec::new()).unwrap(), true)
    }

    #[test]
    fn ranges_follow_the_http_rules_we_support() {
        assert_eq!(parse_range("bytes=0-9", 100), Ok(Some((0, 9))));
        assert_eq!(parse_range("bytes=90-", 100), Ok(Some((90, 99))));
        assert_eq!(parse_range("bytes=-10", 100), Ok(Some((90, 99))));
        assert_eq!(parse_range("bytes=-500", 100), Ok(Some((0, 99))));
        assert_eq!(parse_range("bytes=90-500", 100), Ok(Some((90, 99))));
        assert_eq!(parse_range("bytes=100-", 100), Err(()));
        assert_eq!(parse_range("bytes=-0", 100), Err(()));
        assert_eq!(parse_range("bytes=0-1,4-5", 100), Ok(None));
        assert_eq!(parse_range("bytes=5-1", 100), Ok(None));
        assert_eq!(parse_range("items=0-1", 100), Ok(None));
        assert_eq!(parse_range("bytes=abc", 100), Ok(None));
    }

    #[test]
    fn serves_whole_files_and_ranges_with_safe_headers() {
        let (_temp, token) = fixture(b"0123456789");
        let whole = get(&token, None);
        assert_eq!(whole.status(), StatusCode::OK);
        assert_eq!(whole.body(), b"0123456789");
        assert_eq!(whole.headers()["x-content-type-options"], "nosniff");
        assert_eq!(whole.headers()["content-type"], "application/pdf");
        assert!(whole.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .starts_with("sandbox"));
        let part = get(&token, Some("bytes=2-5"));
        assert_eq!(part.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(part.body(), b"2345");
        assert_eq!(part.headers()["content-range"], "bytes 2-5/10");
        let past = get(&token, Some("bytes=50-"));
        assert_eq!(past.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(past.headers()["content-range"], "bytes */10");
    }

    #[test]
    fn long_ranges_stop_at_the_part_limit() {
        let (_temp, token) = fixture(&vec![7u8; (MAX_PART + 10) as usize]);
        let part = get(&token, Some("bytes=0-"));
        assert_eq!(part.body().len() as u64, MAX_PART);
        assert_eq!(
            part.headers()["content-range"],
            format!("bytes 0-{}/{}", MAX_PART - 1, MAX_PART + 10).as_str()
        );
    }

    #[test]
    fn unknown_tokens_other_webviews_and_writes_are_refused() {
        let (_temp, token) = fixture(b"secret");
        assert_eq!(get("unknown", None).status(), StatusCode::GONE);
        assert_eq!(get("../../etc/passwd", None).status(), StatusCode::GONE);
        let request = Request::builder()
            .uri(format!("pipeline-file://localhost/{token}"))
            .body(Vec::new())
            .unwrap();
        let other = handle(&request, false);
        assert_eq!(other.status(), StatusCode::FORBIDDEN);
        assert!(other.body().is_empty());
        let post = Request::builder()
            .method("POST")
            .uri(format!("pipeline-file://localhost/{token}"))
            .body(Vec::new())
            .unwrap();
        assert_eq!(handle(&post, true).status(), StatusCode::METHOD_NOT_ALLOWED);
        revoke(&token);
        assert_eq!(get(&token, None).status(), StatusCode::GONE);
    }

    #[test]
    fn a_changed_or_removed_file_is_never_served_under_the_old_token() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("file.bin");
        std::fs::write(&path, b"first").unwrap();
        let token = issue(
            Arc::new(Disk(path.clone())),
            "text/plain",
            "size:5".into(),
            Presentation::Bytes,
        )
        .unwrap();
        assert_eq!(get(&token, None).status(), StatusCode::OK);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"more").unwrap();
        let changed = get(&token, None);
        assert_eq!(changed.status(), StatusCode::CONFLICT);
        assert!(changed.body().is_empty());
        std::fs::remove_file(&path).unwrap();
        assert_eq!(get(&token, None).status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn cross_origin_reads_are_open_only_to_the_app() {
        let (_temp, token) = fixture(b"data");
        let request = |origin: &str| {
            Request::builder()
                .uri(format!("pipeline-file://localhost/{token}"))
                .header("origin", origin)
                .body(Vec::new())
                .unwrap()
        };
        let app = handle(&request("tauri://localhost"), true);
        assert_eq!(
            app.headers()["access-control-allow-origin"],
            "tauri://localhost"
        );
        for origin in ["null", "https://example.com"] {
            assert!(!handle(&request(origin), true)
                .headers()
                .contains_key("access-control-allow-origin"));
        }
    }

    #[test]
    fn capacity_retires_the_least_recently_used_token() {
        let (_temp, first) = fixture(b"first");
        let (_kept, kept) = fixture(b"kept");
        for _ in 0..MAX_TOKENS - 1 {
            let (_temp, _) = fixture(b"filler");
            get(&kept, None);
        }
        assert_eq!(get(&first, None).status(), StatusCode::GONE);
        assert_eq!(get(&kept, None).status(), StatusCode::OK);
    }
}
