//! Event-sink abstraction.
//!
//! The pipeline emits progress as `pipeline:*` / `batch:*` / `watch:*` events.
//! In the GUI these go through a Tauri `AppHandle`; in the headless CLI there is
//! no Tauri runtime, so both implement this trait and the pipeline code depends
//! only on `&EventBus` rather than a concrete `AppHandle`.

use std::sync::Arc;

#[derive(Debug)]
pub struct EmitError;

/// Anything the pipeline can emit events to. `emit_event` returns a `Result` so
/// existing call sites (`app.emit_event(...).ok()`, `let _ = app.emit_event(...)`)
/// keep working unchanged after the AppHandle→trait migration.
pub trait Events: Send + Sync {
    fn emit_event(&self, event: &str, payload: serde_json::Value) -> Result<(), EmitError>;
}

/// Shared handle to an event sink. Cloneable (it's an `Arc`), so spawned tasks
/// can own a copy the way they used to clone an `AppHandle`.
pub type EventBus = Arc<dyn Events>;

impl Events for tauri::AppHandle {
    fn emit_event(&self, event: &str, payload: serde_json::Value) -> Result<(), EmitError> {
        use tauri::Emitter;
        self.emit(event, payload).map_err(|_| EmitError)
    }
}

/// Wrap a Tauri `AppHandle` as an `EventBus`.
pub fn from_app(app: tauri::AppHandle) -> EventBus {
    Arc::new(app)
}

/// A headless sink that prints progress to stderr, for the CLI. Log lines and
/// stage/pass transitions are shown; token-usage and preprocess events are
/// summarized elsewhere.
pub struct CliEvents;

impl Events for CliEvents {
    fn emit_event(&self, event: &str, payload: serde_json::Value) -> Result<(), EmitError> {
        match event {
            "pipeline:log" => {
                if let Some(line) = payload.get("line").and_then(|v| v.as_str()) {
                    eprintln!("{line}");
                }
            }
            "pipeline:stage" => {
                if let Some(s) = payload.get("stage").and_then(|v| v.as_str()) {
                    eprintln!("── {s} ──");
                }
            }
            "pipeline:pass" => {
                let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let status = payload.get("status").and_then(|v| v.as_str()).unwrap_or("");
                eprintln!("  {name}: {status}");
            }
            _ => {}
        }
        Ok(())
    }
}

/// A sink that discards everything (for tests / non-interactive runs).
pub struct NullEvents;

impl Events for NullEvents {
    fn emit_event(&self, _event: &str, _payload: serde_json::Value) -> Result<(), EmitError> {
        Ok(())
    }
}
