//! What the event bridge reads from one connection, in the server's order.
use crate::agent_runtime::codex::{PlacedRequest, ServerRequestSink};
use crate::agent_runtime::events::NormalizedEvent;
use tokio::sync::broadcast::{error::RecvError, Receiver};

/// A connection's server requests, which lag can't drop, and everything else.
/// A card needs the item its request is about, and a request's resolution must
/// not overtake it, so a request waits for the events ahead of it and goes
/// before the ones behind it.
pub(in crate::workbench::commands) struct BridgeEvents {
    events: Receiver<NormalizedEvent>,
    requests: ServerRequestSink,
    /// Broadcast events read or lost to lag so far.
    read: u64,
    waiting: Option<PlacedRequest>,
    ended: bool,
}

impl BridgeEvents {
    /// `events` must be subscribed before this is called.
    pub(in crate::workbench::commands) fn new(
        events: Receiver<NormalizedEvent>,
        requests: ServerRequestSink,
    ) -> Self {
        Self {
            read: requests.emitted(),
            events,
            requests,
            waiting: None,
            ended: false,
        }
    }

    /// `None` after the connection's last event. The bridge holds the
    /// connection open, so `Closed` alone would never arrive.
    pub(in crate::workbench::commands) async fn next(
        &mut self,
    ) -> Option<Result<NormalizedEvent, RecvError>> {
        if self.ended {
            return None;
        }
        let received = loop {
            match self.waiting.take() {
                Some(request) if request.after <= self.read => break Ok(request.event),
                Some(request) => {
                    self.waiting = Some(request);
                    break self.event().await;
                }
                None => {}
            }
            tokio::select! {
                biased;
                Some(request) = self.requests.recv_placed() => self.waiting = Some(request),
                received = self.events.recv() => {
                    self.count(&received);
                    break received;
                }
            }
        };
        self.ended = matches!(received, Ok(NormalizedEvent::ConnectionClosed { .. }));
        Some(received)
    }

    async fn event(&mut self) -> Result<NormalizedEvent, RecvError> {
        let received = self.events.recv().await;
        self.count(&received);
        received
    }

    fn count(&mut self, received: &Result<NormalizedEvent, RecvError>) {
        self.read += match received {
            Ok(_) => 1,
            Err(RecvError::Lagged(skipped)) => *skipped,
            Err(RecvError::Closed) => 0,
        };
    }
}
