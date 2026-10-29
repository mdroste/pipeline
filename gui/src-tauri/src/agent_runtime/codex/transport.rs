//! Bounded concurrent JSONL transport for one Codex App Server connection.
//! Token-refresh requests are answered through the account service and never
//! broadcast. The per-thread request sink is lossless, so a Review's tool calls
//! survive event-queue lag; ordinary events may lag and must be reconciled.

use super::wire::{
    normalize_notification, parse_frame, IncomingFrame, InitializeResult, NormalizedEvent,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

const FRAME_LIMIT: usize = 8 * 1024 * 1024;
const OUTBOUND_QUEUE_CAPACITY: usize = 64;
// Workspace coalesces streams, and a Review call may be inside a host tool
// while sibling threads stream deltas; keep enough headroom that an
// ordinary burst never lags a subscriber.
const EVENT_QUEUE_CAPACITY: usize = 4096;
const REQUEST_SINK_CAPACITY: usize = 256;
const WRITE_QUEUE_TIMEOUT: Duration = Duration::from_secs(2);
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

type PendingReply = oneshot::Sender<Result<Value, RequestError>>;

// Also runs when the caller drops a request future while queued or waiting.
struct PendingGuard<'a> {
    pending: &'a Mutex<HashMap<u64, PendingReply>>,
    id: u64,
}
impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.id);
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub rpc_code: Option<i64>,
    pub data: Option<Value>,
}

impl RequestError {
    fn transport(message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: "app_server_transport".to_string(),
            message: message.into(),
            retryable,
            rpc_code: None,
            data: None,
        }
    }

    fn timeout(method: &str) -> Self {
        Self {
            code: "app_server_timeout".to_string(),
            message: format!("Codex App Server request {method} timed out"),
            retryable: true,
            rpc_code: None,
            data: None,
        }
    }

    fn remote(value: Value) -> Self {
        let rpc_code = value.get("code").and_then(Value::as_i64);
        let message = value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Codex App Server request failed")
            .to_string();
        let retryable = rpc_code == Some(-32001);
        let data = value.get("data").cloned();
        Self {
            code: "app_server_request".to_string(),
            message,
            retryable,
            rpc_code,
            data,
        }
    }
}

impl fmt::Display for RequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RequestError {}

enum Outbound {
    Frame(Vec<u8>),
    Close(oneshot::Sender<()>),
}

pub(crate) trait AuthRefresh: Send + Sync {
    fn refresh(
        &self,
        params: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, RequestError>> + Send + '_>>;
}

struct Shared {
    auth_refresh: Mutex<Option<Arc<dyn AuthRefresh>>>,
    auth_requests: Arc<tokio::sync::Semaphore>,
    epoch: u64,
    pending: Mutex<HashMap<u64, PendingReply>>,
    events: broadcast::Sender<NormalizedEvent>,
    /// Lossless delivery of server requests for threads whose owner
    /// registered a sink; the broadcast queue may drop them under lag.
    request_sinks: Mutex<HashMap<String, mpsc::Sender<NormalizedEvent>>>,
    closed: AtomicBool,
    close_reason: Mutex<Option<RequestError>>,
    close_signal: watch::Sender<Option<RequestError>>,
}

impl Shared {
    fn close(&self, error: RequestError) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        *self.close_reason.lock().unwrap_or_else(|e| e.into_inner()) = Some(error.clone());
        self.close_signal.send_replace(Some(error.clone()));
        let pending = std::mem::take(&mut *self.pending.lock().unwrap_or_else(|e| e.into_inner()));
        for (_, sender) in pending {
            let _ = sender.send(Err(error.clone()));
        }
        let _ = self.events.send(NormalizedEvent::ConnectionClosed {
            epoch: self.epoch,
            reason: error.message,
            retryable: error.retryable,
        });
    }

    fn closed_error(&self) -> Option<RequestError> {
        self.close_reason
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn emit(&self, event: NormalizedEvent) {
        let _ = self.events.send(event);
    }

    /// Deliver a server request to its thread's registered sink. Returns the
    /// event back when no sink takes it, so it falls through to broadcast.
    fn route_server_request(&self, event: NormalizedEvent) -> Option<NormalizedEvent> {
        let NormalizedEvent::ServerRequest { params, .. } = &event else {
            return Some(event);
        };
        let Some(thread_id) = params.get("threadId").and_then(Value::as_str) else {
            return Some(event);
        };
        let sender = self
            .request_sinks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(thread_id)
            .cloned();
        let Some(sender) = sender else {
            return Some(event);
        };
        match sender.try_send(event) {
            Ok(()) => None,
            Err(mpsc::error::TrySendError::Full(event))
            | Err(mpsc::error::TrySendError::Closed(event)) => Some(event),
        }
    }
}

/// Server requests for one thread, delivered in order and never dropped by
/// event-queue lag. Dropping the sink returns the thread to broadcast delivery.
pub struct ServerRequestSink {
    shared: Arc<Shared>,
    thread_id: String,
    receiver: mpsc::Receiver<NormalizedEvent>,
}

impl ServerRequestSink {
    /// The next server request for this thread; `None` only after the
    /// transport is gone.
    pub async fn recv(&mut self) -> Option<NormalizedEvent> {
        self.receiver.recv().await
    }
}

impl Drop for ServerRequestSink {
    fn drop(&mut self) {
        self.shared
            .request_sinks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.thread_id);
    }
}

#[derive(Clone)]
pub struct AppServerClient {
    epoch: u64,
    next_id: Arc<AtomicU64>,
    outbound: mpsc::Sender<Outbound>,
    shared: Arc<Shared>,
}

impl AppServerClient {
    pub(crate) fn from_io<R, W>(epoch: u64, reader: R, writer: W) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        Self::from_io_with_limit(epoch, reader, writer, FRAME_LIMIT)
    }

    fn from_io_with_limit<R, W>(epoch: u64, reader: R, writer: W, frame_limit: usize) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (outbound, outbound_rx) = mpsc::channel(OUTBOUND_QUEUE_CAPACITY);
        let (events, _) = broadcast::channel(EVENT_QUEUE_CAPACITY);
        let (close_signal, _) = watch::channel(None);
        let shared = Arc::new(Shared {
            epoch,
            auth_refresh: Mutex::new(None),
            auth_requests: Arc::new(tokio::sync::Semaphore::new(1)),
            pending: Mutex::new(HashMap::new()),
            events,
            request_sinks: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
            close_reason: Mutex::new(None),
            close_signal,
        });
        tokio::spawn(writer_loop(writer, outbound_rx, shared.clone()));
        tokio::spawn(reader_loop(
            BufReader::new(reader),
            shared.clone(),
            outbound.clone(),
            frame_limit,
        ));
        Self {
            epoch,
            next_id: Arc::new(AtomicU64::new(1)),
            outbound,
            shared,
        }
    }

    pub(crate) fn same_connection(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.shared, &other.shared)
    }

    pub(crate) fn set_auth_refresh(&self, handler: Option<Arc<dyn AuthRefresh>>) {
        *self
            .shared
            .auth_refresh
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = handler;
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NormalizedEvent> {
        self.shared.events.subscribe()
    }

    /// Register for lossless, ordered delivery of this thread's server
    /// requests (tool calls, approvals). While the sink lives those requests
    /// bypass the broadcast queue; the owner must still respond exactly once.
    pub fn server_request_sink(&self, thread_id: &str) -> ServerRequestSink {
        let (sender, receiver) = mpsc::channel(REQUEST_SINK_CAPACITY);
        self.shared
            .request_sinks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(thread_id.to_string(), sender);
        ServerRequestSink {
            shared: self.shared.clone(),
            thread_id: thread_id.to_string(),
            receiver,
        }
    }

    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }

    pub async fn initialize(&self) -> Result<InitializeResult, RequestError> {
        self.initialize_as("pipeline_workbench", "Pipeline Workbench")
            .await
    }

    pub async fn initialize_as(
        &self,
        name: &str,
        title: &str,
    ) -> Result<InitializeResult, RequestError> {
        let result = self
            .request(
                "initialize",
                json!({
                    "clientInfo": {
                        "name": name,
                        "title": title,
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "capabilities": { "experimentalApi": true }
                }),
                DEFAULT_REQUEST_TIMEOUT,
            )
            .await?;
        let initialize: InitializeResult = serde_json::from_value(result).map_err(|error| {
            RequestError::transport(
                format!("Invalid App Server initialize response: {error}"),
                false,
            )
        })?;
        self.notify("initialized", json!({})).await?;
        self.shared.emit(NormalizedEvent::ConnectionReady {
            epoch: self.epoch,
            initialize: initialize.clone(),
        });
        Ok(initialize)
    }

    pub async fn request(
        &self,
        method: &str,
        params: Value,
        deadline: Duration,
    ) -> Result<Value, RequestError> {
        validate_method(method)?;
        if let Some(error) = self.shared.closed_error() {
            return Err(error);
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let bytes = encode_frame(&json!({"id": id, "method": method, "params": params}))?;
        let (sender, receiver) = oneshot::channel();
        self.shared
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, sender);
        let _pending = PendingGuard {
            pending: &self.shared.pending,
            id,
        };
        if tokio::time::timeout(
            WRITE_QUEUE_TIMEOUT,
            self.outbound.send(Outbound::Frame(bytes)),
        )
        .await
        .map_err(|_| RequestError::transport("App Server write queue is backpressured", true))?
        .is_err()
        {
            self.shared
                .pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            return Err(self
                .shared
                .closed_error()
                .unwrap_or_else(|| RequestError::transport("App Server writer stopped", true)));
        }
        match tokio::time::timeout(deadline, receiver).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => Err(self.shared.closed_error().unwrap_or_else(|| {
                RequestError::transport("App Server response channel closed", true)
            })),
            Err(_) => {
                self.shared
                    .pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&id);
                Err(RequestError::timeout(method))
            }
        }
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<(), RequestError> {
        validate_method(method)?;
        if let Some(error) = self.shared.closed_error() {
            return Err(error);
        }
        let bytes = encode_frame(&json!({"method": method, "params": params}))?;
        tokio::time::timeout(
            WRITE_QUEUE_TIMEOUT,
            self.outbound.send(Outbound::Frame(bytes)),
        )
        .await
        .map_err(|_| RequestError::transport("App Server write queue is backpressured", true))?
        .map_err(|_| RequestError::transport("App Server writer stopped", true))
    }

    pub async fn respond_to_server_request(
        &self,
        request_id: Value,
        result: Result<Value, RequestError>,
    ) -> Result<(), RequestError> {
        if !request_id.is_string() && !request_id.is_number() {
            return Err(RequestError::transport(
                "Server request id must be a string or number",
                false,
            ));
        }
        let value = match result {
            Ok(result) => json!({"id": request_id, "result": result}),
            Err(error) => json!({
                "id": request_id,
                "error": {
                    "code": error.rpc_code.unwrap_or(-32000),
                    "message": error.message,
                    "data": error.data
                }
            }),
        };
        let bytes = encode_frame(&value)?;
        tokio::time::timeout(
            WRITE_QUEUE_TIMEOUT,
            self.outbound.send(Outbound::Frame(bytes)),
        )
        .await
        .map_err(|_| RequestError::transport("App Server write queue is backpressured", true))?
        .map_err(|_| RequestError::transport("App Server writer stopped", true))
    }

    pub async fn close_writer(&self) -> Result<(), RequestError> {
        if self.is_closed() {
            return Ok(());
        }
        let (sender, receiver) = oneshot::channel();
        tokio::time::timeout(WRITE_QUEUE_TIMEOUT, async {
            self.outbound
                .send(Outbound::Close(sender))
                .await
                .map_err(|_| RequestError::transport("App Server writer stopped", true))?;
            receiver.await.map_err(|_| {
                RequestError::transport("App Server close acknowledgement dropped", true)
            })
        })
        .await
        .map_err(|_| RequestError::transport("Timed out closing App Server stdin", true))?
    }

    pub async fn wait_closed(&self) -> RequestError {
        if let Some(error) = self.shared.closed_error() {
            return error;
        }
        let mut signal = self.shared.close_signal.subscribe();
        loop {
            if let Some(error) = signal.borrow().clone() {
                return error;
            }
            if signal.changed().await.is_err() {
                return RequestError::transport("App Server close signal stopped", true);
            }
        }
    }

    pub(crate) fn fail(&self, message: impl Into<String>, retryable: bool) {
        self.shared
            .close(RequestError::transport(message, retryable));
    }

    pub(crate) fn warn(&self, message: impl Into<String>) {
        self.shared.emit(NormalizedEvent::ProtocolWarning {
            epoch: self.epoch,
            message: message.into(),
        });
    }

    #[cfg(test)]
    fn pending_count(&self) -> usize {
        self.shared
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
}

fn handle_auth_refresh(
    shared: &Arc<Shared>,
    outbound: &mpsc::Sender<Outbound>,
    id: Value,
    params: Value,
) {
    let permit = shared.auth_requests.clone().try_acquire_owned();
    let handler = shared
        .auth_refresh
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let outbound = outbound.clone();
    let shared = shared.clone();
    // A broken server cannot create an unbounded number of refresh tasks.
    let Ok(permit) = permit else {
        shared.close(RequestError::transport(
            "Overlapping ChatGPT token refresh requests",
            false,
        ));
        return;
    };
    tokio::spawn(async move {
        let _permit = permit;
        let result = match handler {
            Some(handler) => tokio::time::timeout(Duration::from_secs(8), handler.refresh(params))
                .await
                .ok()
                .and_then(Result::ok),
            None => None,
        };
        let frame = match result {
            Some(tokens) => json!({"id":id,"result":tokens}),
            None => {
                json!({"id":id,"error":{"code":-32000,"message":"ChatGPT sign-in needs attention; reconnect in Settings"}})
            }
        };
        if let Ok(bytes) = encode_frame(&frame) {
            if !matches!(
                tokio::time::timeout(WRITE_QUEUE_TIMEOUT, outbound.send(Outbound::Frame(bytes)))
                    .await,
                Ok(Ok(()))
            ) {
                shared.close(RequestError::transport(
                    "Could not answer ChatGPT token refresh",
                    false,
                ));
            }
        }
    });
}

fn validate_method(method: &str) -> Result<(), RequestError> {
    if method.is_empty()
        || method.len() > 160
        || method
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(RequestError::transport("Invalid App Server method", false));
    }
    Ok(())
}

fn encode_frame(value: &Value) -> Result<Vec<u8>, RequestError> {
    let mut bytes = serde_json::to_vec(value).map_err(|error| {
        RequestError::transport(format!("Cannot encode App Server frame: {error}"), false)
    })?;
    if bytes.len() > FRAME_LIMIT {
        return Err(RequestError::transport(
            "Outbound App Server frame exceeds 8 MiB",
            false,
        ));
    }
    bytes.push(b'\n');
    Ok(bytes)
}

async fn writer_loop<W>(mut writer: W, mut outbound: mpsc::Receiver<Outbound>, shared: Arc<Shared>)
where
    W: AsyncWrite + Unpin,
{
    while let Some(message) = outbound.recv().await {
        match message {
            Outbound::Frame(bytes) => {
                if let Err(error) = writer.write_all(&bytes).await {
                    shared.close(RequestError::transport(
                        format!("Failed writing to App Server: {error}"),
                        true,
                    ));
                    return;
                }
                if let Err(error) = writer.flush().await {
                    shared.close(RequestError::transport(
                        format!("Failed flushing App Server request: {error}"),
                        true,
                    ));
                    return;
                }
            }
            Outbound::Close(acknowledge) => {
                let result = writer.shutdown().await;
                let _ = acknowledge.send(());
                if let Err(error) = result {
                    shared.close(RequestError::transport(
                        format!("Failed closing App Server stdin: {error}"),
                        true,
                    ));
                }
                return;
            }
        }
    }
    let _ = writer.shutdown().await;
}

async fn reader_loop<R>(
    mut reader: BufReader<R>,
    shared: Arc<Shared>,
    outbound: mpsc::Sender<Outbound>,
    frame_limit: usize,
) where
    R: AsyncRead + Unpin,
{
    loop {
        let record =
            match crate::pipeline::logging::next_bounded_line(&mut reader, frame_limit).await {
                Ok(Some(record)) => record,
                Ok(None) => {
                    shared.close(RequestError::transport(
                        "Codex App Server closed stdout",
                        true,
                    ));
                    return;
                }
                Err(error) => {
                    shared.close(RequestError::transport(
                        format!("Failed reading Codex App Server: {error}"),
                        true,
                    ));
                    return;
                }
            };
        if record.truncated {
            shared.close(RequestError::transport(
                format!("Codex App Server frame exceeded {frame_limit} bytes"),
                false,
            ));
            return;
        }
        let value: Value = match serde_json::from_str(&record.text) {
            Ok(value) => value,
            Err(error) => {
                shared.close(RequestError::transport(
                    format!("Codex App Server emitted invalid JSON: {error}"),
                    false,
                ));
                return;
            }
        };
        match parse_frame(value) {
            Ok(IncomingFrame::Response { id, result, error }) => {
                let sender = shared
                    .pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&id);
                if let Some(sender) = sender {
                    let response = match (result, error) {
                        (Some(result), None) => Ok(result),
                        (None, Some(error)) => Err(RequestError::remote(error)),
                        _ => unreachable!("parse_frame enforces response shape"),
                    };
                    let _ = sender.send(response);
                } else {
                    shared.emit(NormalizedEvent::ProtocolWarning {
                        epoch: shared.epoch,
                        message: format!("Ignored response for unknown or expired request {id}"),
                    });
                }
            }
            Ok(IncomingFrame::Notification { method, params }) => {
                match normalize_notification(shared.epoch, method, params) {
                    Ok(event) => shared.emit(event),
                    Err(error) => {
                        shared.close(RequestError::transport(
                            format!("Invalid known App Server notification: {error}"),
                            false,
                        ));
                        return;
                    }
                }
            }
            Ok(IncomingFrame::ServerRequest { id, method, params }) => {
                if method == "account/chatgptAuthTokens/refresh" {
                    handle_auth_refresh(&shared, &outbound, id, params);
                    continue;
                }
                let request = NormalizedEvent::ServerRequest {
                    epoch: shared.epoch,
                    request_id: id,
                    method,
                    params,
                };
                if let Some(request) = shared.route_server_request(request) {
                    shared.emit(request);
                }
            }
            Err(error) => {
                shared.close(RequestError::transport(
                    format!("Invalid App Server frame: {error}"),
                    false,
                ));
                return;
            }
        }
    }
}

impl RequestError {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_input".to_string(),
            message: message.into(),
            retryable: false,
            rpc_code: None,
            data: None,
        }
    }

    pub(crate) fn unavailable(message: impl Into<String>) -> Self {
        Self {
            code: "app_server_unavailable".to_string(),
            message: message.into(),
            retryable: true,
            rpc_code: None,
            data: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncBufReadExt as _;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
    }

    async fn read_json<R: tokio::io::AsyncBufRead + Unpin>(reader: &mut R) -> Value {
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        serde_json::from_str(&line).unwrap()
    }

    async fn write_json<W: AsyncWrite + Unpin>(writer: &mut W, value: Value) {
        writer
            .write_all(format!("{value}\n").as_bytes())
            .await
            .unwrap();
        writer.flush().await.unwrap();
    }

    struct FixtureAuth;
    impl AuthRefresh for FixtureAuth {
        fn refresh(
            &self,
            params: Value,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Value, RequestError>> + Send + '_>,
        > {
            Box::pin(async move {
                assert_eq!(params["previousAccountId"], "fixture-account");
                Ok(json!({"accessToken":"fixture-secret","chatgptAccountId":"fixture-account"}))
            })
        }
    }

    #[test]
    fn token_refresh_is_answered_privately_without_reaching_product_events() {
        runtime().block_on(async {
            let (client_io, server_io) = tokio::io::duplex(4096);
            let (read, write) = tokio::io::split(client_io);
            let client = AppServerClient::from_io(7, read, write);
            client.set_auth_refresh(Some(Arc::new(FixtureAuth)));
            let mut events = client.subscribe();
            let (read, mut write) = tokio::io::split(server_io);
            let mut read = BufReader::new(read);
            write_json(&mut write, json!({"id":"refresh-1","method":"account/chatgptAuthTokens/refresh","params":{"reason":"unauthorized","previousAccountId":"fixture-account"}})).await;
            let response = read_json(&mut read).await;
            assert_eq!(response["id"], "refresh-1");
            assert_eq!(response["result"]["accessToken"], "fixture-secret");
            assert!(matches!(events.try_recv(), Err(broadcast::error::TryRecvError::Empty)));
            client.set_auth_refresh(None);
            write_json(&mut write, json!({"id":42,"method":"account/chatgptAuthTokens/refresh","params":{"reason":"unauthorized"}})).await;
            let rejected = read_json(&mut read).await;
            assert_eq!(rejected["id"], 42);
            assert!(rejected.get("error").is_some());
            assert!(!rejected.to_string().contains("fixture-secret"));
            assert!(matches!(events.try_recv(), Err(broadcast::error::TryRecvError::Empty)));
        });
    }

    #[test]
    fn correlates_reversed_replies_while_streaming_requests_and_notifications() {
        runtime().block_on(async {
            let (client_io, server_io) = tokio::io::duplex(64 * 1024);
            let (client_reader, client_writer) = tokio::io::split(client_io);
            let (server_reader, mut server_writer) = tokio::io::split(server_io);
            let mut server_reader = BufReader::new(server_reader);
            let server = tokio::spawn(async move {
                let initialize = read_json(&mut server_reader).await;
                write_json(
                    &mut server_writer,
                    json!({
                        "id": initialize["id"],
                        "result": {
                            "userAgent": "codex_cli_rs/0.147.0",
                            "platformFamily": "unix",
                            "platformOs": "macos",
                            "codexHome": "/tmp/workbench/codex"
                        }
                    }),
                )
                .await;
                let initialized = read_json(&mut server_reader).await;
                assert_eq!(initialized["method"], "initialized");
                let first = read_json(&mut server_reader).await;
                let second = read_json(&mut server_reader).await;
                write_json(
                    &mut server_writer,
                    json!({"id":"approval-1","method":"item/commandExecution/requestApproval","params":{"threadId":"thread-1"}}),
                )
                .await;
                write_json(
                    &mut server_writer,
                    json!({"method":"turn/started","params":{"threadId":"thread-1","turn":{"id":"turn-1","status":"inProgress","items":[],"error":null}}}),
                )
                .await;
                write_json(
                    &mut server_writer,
                    json!({"method":"future/event","params":{"value":1}}),
                )
                .await;
                write_json(
                    &mut server_writer,
                    json!({"id": second["id"], "result":{"method":second["method"]}}),
                )
                .await;
                write_json(
                    &mut server_writer,
                    json!({"id": first["id"], "result":{"method":first["method"]}}),
                )
                .await;
                let approval = read_json(&mut server_reader).await;
                assert_eq!(approval["id"], "approval-1");
                assert_eq!(approval["result"]["decision"], "decline");
            });

            let client = AppServerClient::from_io(41, client_reader, client_writer);
            let mut events = client.subscribe();
            let initialized = client.initialize().await.unwrap();
            assert_eq!(initialized.user_agent, "codex_cli_rs/0.147.0");

            let one_client = client.clone();
            let one = tokio::spawn(async move {
                one_client
                    .request("thread/read", json!({"threadId":"thread-1"}), Duration::from_secs(1))
                    .await
            });
            let two_client = client.clone();
            let two = tokio::spawn(async move {
                two_client
                    .request("thread/resume", json!({"threadId":"thread-1"}), Duration::from_secs(1))
                    .await
            });

            let mut saw_turn = false;
            let mut saw_unknown = false;
            let mut server_request = None;
            for _ in 0..4 {
                match tokio::time::timeout(Duration::from_secs(1), events.recv())
                    .await
                    .unwrap()
                    .unwrap()
                {
                    NormalizedEvent::TurnStarted { turn_id, .. } if turn_id == "turn-1" => {
                        saw_turn = true
                    }
                    NormalizedEvent::UnknownNotification { method, .. }
                        if method == "future/event" =>
                    {
                        saw_unknown = true
                    }
                    NormalizedEvent::ServerRequest { request_id, .. } => {
                        server_request = Some(request_id)
                    }
                    _ => {}
                }
            }
            assert!(saw_turn && saw_unknown);
            client
                .respond_to_server_request(
                    server_request.unwrap(),
                    Ok(json!({"decision":"decline"})),
                )
                .await
                .unwrap();
            let methods = [
                one.await.unwrap().unwrap()["method"].as_str().unwrap().to_string(),
                two.await.unwrap().unwrap()["method"].as_str().unwrap().to_string(),
            ];
            assert!(methods.contains(&"thread/read".to_string()));
            assert!(methods.contains(&"thread/resume".to_string()));
            server.await.unwrap();
        });
    }

    #[test]
    fn deadlines_remove_pending_requests_and_malformed_frames_close_the_epoch() {
        runtime().block_on(async {
            let (client_io, server_io) = tokio::io::duplex(4096);
            let (client_reader, client_writer) = tokio::io::split(client_io);
            let (server_reader, mut server_writer) = tokio::io::split(server_io);
            let mut server_reader = BufReader::new(server_reader);
            tokio::spawn(async move {
                let _request = read_json(&mut server_reader).await;
                tokio::time::sleep(Duration::from_millis(75)).await;
                server_writer.write_all(b"not-json\n").await.unwrap();
            });
            let client = AppServerClient::from_io(3, client_reader, client_writer);
            let error = client
                .request("thread/read", json!({}), Duration::from_millis(20))
                .await
                .unwrap_err();
            assert_eq!(error.code, "app_server_timeout");
            assert_eq!(client.pending_count(), 0);
            let closed = tokio::time::timeout(Duration::from_secs(1), client.wait_closed())
                .await
                .unwrap();
            assert!(!closed.retryable);
            assert!(closed.message.contains("invalid JSON"));
        });
    }

    #[test]
    fn oversized_input_frame_fails_closed_without_unbounded_allocation() {
        runtime().block_on(async {
            let (client_io, mut server_io) = tokio::io::duplex(4096);
            let (client_reader, client_writer) = tokio::io::split(client_io);
            let client = AppServerClient::from_io_with_limit(5, client_reader, client_writer, 128);
            tokio::spawn(async move {
                server_io.write_all(&vec![b'x'; 1024]).await.unwrap();
                server_io.write_all(b"\n").await.unwrap();
            });
            let closed = tokio::time::timeout(Duration::from_secs(1), client.wait_closed())
                .await
                .unwrap();
            assert!(closed.message.contains("exceeded 128 bytes"));
        });
    }

    #[test]
    fn registered_request_sinks_keep_server_requests_that_lag_would_drop() {
        runtime().block_on(async {
            let (client_io, mut server_io) = tokio::io::duplex(1024 * 1024);
            let (client_reader, client_writer) = tokio::io::split(client_io);
            let client = AppServerClient::from_io(13, client_reader, client_writer);
            let mut events = client.subscribe();
            let mut sink = client.server_request_sink("thread-1");
            write_json(
                &mut server_io,
                json!({"id":"tool-1","method":"item/tool/call","params":{"threadId":"thread-1","turnId":"turn-1","callId":"call-1","tool":"Read","arguments":{}}}),
            )
            .await;
            for index in 0..(EVENT_QUEUE_CAPACITY + 40) {
                write_json(
                    &mut server_io,
                    json!({"method":"simulator/progress","params":{"index":index}}),
                )
                .await;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert!(matches!(
                events.recv().await,
                Err(broadcast::error::RecvError::Lagged(_))
            ));
            match sink.recv().await.unwrap() {
                NormalizedEvent::ServerRequest { request_id, .. } => {
                    assert_eq!(request_id, json!("tool-1"))
                }
                other => panic!("unexpected {other:?}"),
            }
            // Another thread's request still reaches broadcast subscribers, and
            // dropping the sink returns the thread to broadcast delivery.
            drop(sink);
            write_json(
                &mut server_io,
                json!({"id":"tool-2","method":"item/tool/call","params":{"threadId":"thread-1","turnId":"turn-1","callId":"call-2","tool":"Read","arguments":{}}}),
            )
            .await;
            loop {
                match events.recv().await {
                    Ok(NormalizedEvent::ServerRequest { request_id, .. }) => {
                        assert_eq!(request_id, json!("tool-2"));
                        break;
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(error) => panic!("{error}"),
                }
            }
        });
    }

    #[test]
    fn slow_event_consumers_receive_an_explicit_lag_signal() {
        runtime().block_on(async {
            let (client_io, mut server_io) = tokio::io::duplex(1024 * 1024);
            let (client_reader, client_writer) = tokio::io::split(client_io);
            let client = AppServerClient::from_io(12, client_reader, client_writer);
            let mut events = client.subscribe();
            for index in 0..(EVENT_QUEUE_CAPACITY + 40) {
                write_json(
                    &mut server_io,
                    json!({"method":"simulator/progress","params":{"index":index}}),
                )
                .await;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            let error = events.recv().await.unwrap_err();
            assert!(matches!(error, broadcast::error::RecvError::Lagged(skipped) if skipped > 0));
        });
    }
}

#[cfg(test)]
mod backpressure_tests {
    use super::*;
    #[tokio::test]
    async fn full_queue_bounds_close_and_response_and_cleans_requests() {
        let (io, _stalled_peer) = tokio::io::duplex(1);
        let (read, write) = tokio::io::split(io);
        let client = AppServerClient::from_io(1, read, write);
        client
            .outbound
            .send(Outbound::Frame(vec![0; 8]))
            .await
            .unwrap();
        tokio::task::yield_now().await;
        while client
            .outbound
            .try_send(Outbound::Frame(vec![0; 8]))
            .is_ok()
        {}
        let close = client.close_writer();
        let response = client.respond_to_server_request(json!(1), Ok(json!({})));
        let request = client.request("fixture", json!({}), Duration::from_secs(1));
        let (close, response, request) = tokio::time::timeout(Duration::from_secs(4), async {
            tokio::join!(close, response, request)
        })
        .await
        .expect("All writes must be bounded");
        assert!(close.is_err() && response.is_err() && request.is_err());
        assert!(client.shared.pending.lock().unwrap().is_empty());
        let request = client.request("fixture", json!({}), Duration::from_secs(60));
        assert!(tokio::time::timeout(Duration::from_millis(10), request)
            .await
            .is_err());
        assert!(client.shared.pending.lock().unwrap().is_empty());
    }
}
