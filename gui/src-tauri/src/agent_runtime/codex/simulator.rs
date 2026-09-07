//! Scripted in-memory App Server used for transport and reconnect tests.

use super::transport::AppServerClient;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

pub(crate) enum SimulatorAction {
    Receive {
        label: &'static str,
        method: &'static str,
    },
    Reply {
        label: &'static str,
        result: Value,
    },
    Send(Value),
    Delay(Duration),
    Raw(&'static [u8]),
    Close,
}

pub(crate) struct ProtocolSimulator {
    pub client: AppServerClient,
    task: tokio::task::JoinHandle<Result<(), String>>,
}

impl ProtocolSimulator {
    pub(crate) fn spawn(epoch: u64, actions: Vec<SimulatorAction>) -> Self {
        let (client_io, server_io) = tokio::io::duplex(256 * 1024);
        let (client_reader, client_writer) = tokio::io::split(client_io);
        let (server_reader, mut server_writer) = tokio::io::split(server_io);
        let client = AppServerClient::from_io(epoch, client_reader, client_writer);
        let task = tokio::spawn(async move {
            let mut reader = BufReader::new(server_reader);
            let mut requests = HashMap::new();
            for action in actions {
                match action {
                    SimulatorAction::Receive { label, method } => {
                        let mut line = String::new();
                        if reader
                            .read_line(&mut line)
                            .await
                            .map_err(|error| error.to_string())?
                            == 0
                        {
                            return Err(format!(
                                "client closed before simulator received {method}"
                            ));
                        }
                        let value: Value = serde_json::from_str(&line)
                            .map_err(|error| format!("invalid client JSON: {error}"))?;
                        if value.get("method").and_then(Value::as_str) != Some(method) {
                            return Err(format!(
                                "expected client method {method}, received {:?}",
                                value.get("method")
                            ));
                        }
                        let id = value
                            .get("id")
                            .cloned()
                            .ok_or_else(|| format!("client request {method} omitted id"))?;
                        requests.insert(label, id);
                    }
                    SimulatorAction::Reply { label, result } => {
                        let id = requests
                            .get(label)
                            .cloned()
                            .ok_or_else(|| format!("no request recorded as {label}"))?;
                        write_value(
                            &mut server_writer,
                            &serde_json::json!({"id": id, "result": result}),
                        )
                        .await?;
                    }
                    SimulatorAction::Send(value) => {
                        write_value(&mut server_writer, &value).await?;
                    }
                    SimulatorAction::Delay(duration) => tokio::time::sleep(duration).await,
                    SimulatorAction::Raw(bytes) => server_writer
                        .write_all(bytes)
                        .await
                        .map_err(|error| error.to_string())?,
                    SimulatorAction::Close => return Ok(()),
                }
            }
            Ok(())
        });
        Self { client, task }
    }

    pub(crate) async fn finish(self) -> Result<(), String> {
        self.task
            .await
            .map_err(|error| format!("simulator task failed: {error}"))?
    }
}

async fn write_value<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    value: &Value,
) -> Result<(), String> {
    writer
        .write_all(format!("{value}\n").as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    writer.flush().await.map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::codex::NormalizedEvent;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn scripted_process_exit_and_reconnect_advance_the_connection_epoch() {
        runtime().block_on(async {
            let first = ProtocolSimulator::spawn(
                17,
                vec![
                    SimulatorAction::Receive {
                        label: "read",
                        method: "thread/read",
                    },
                    SimulatorAction::Delay(Duration::from_millis(10)),
                    SimulatorAction::Close,
                ],
            );
            let mut first_events = first.client.subscribe();
            let first_error = first
                .client
                .request(
                    "thread/read",
                    serde_json::json!({"threadId":"thread-1"}),
                    Duration::from_secs(1),
                )
                .await
                .unwrap_err();
            assert!(first_error.retryable);
            let closed = loop {
                let event = first_events.recv().await.unwrap();
                if let NormalizedEvent::ConnectionClosed { epoch, .. } = event {
                    break epoch;
                }
            };
            assert_eq!(closed, 17);
            first.finish().await.unwrap();

            let second = ProtocolSimulator::spawn(
                18,
                vec![
                    SimulatorAction::Receive {
                        label: "read",
                        method: "thread/read",
                    },
                    SimulatorAction::Send(serde_json::json!({
                        "method":"thread/status/changed",
                        "params":{"threadId":"thread-1","status":{"type":"idle"}}
                    })),
                    SimulatorAction::Reply {
                        label: "read",
                        result: serde_json::json!({"thread":{"id":"thread-1"}}),
                    },
                    SimulatorAction::Close,
                ],
            );
            let mut second_events = second.client.subscribe();
            let result = second
                .client
                .request(
                    "thread/read",
                    serde_json::json!({"threadId":"thread-1"}),
                    Duration::from_secs(1),
                )
                .await
                .unwrap();
            assert_eq!(result["thread"]["id"], "thread-1");
            assert!(matches!(
                second_events.recv().await.unwrap(),
                NormalizedEvent::ThreadStatusChanged { epoch: 18, .. }
            ));
            second.finish().await.unwrap();
        });
    }

    #[test]
    fn scripted_malformed_frame_fails_closed() {
        runtime().block_on(async {
            let simulator = ProtocolSimulator::spawn(
                22,
                vec![
                    SimulatorAction::Receive {
                        label: "read",
                        method: "thread/read",
                    },
                    SimulatorAction::Raw(b"{malformed}\n"),
                    SimulatorAction::Close,
                ],
            );
            let error = simulator
                .client
                .request("thread/read", serde_json::json!({}), Duration::from_secs(1))
                .await
                .unwrap_err();
            assert!(!error.retryable);
            assert!(error.message.contains("invalid JSON"));
            simulator.finish().await.unwrap();
        });
    }
}
