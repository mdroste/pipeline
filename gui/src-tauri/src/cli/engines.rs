//! Managed engine status, installation, and removal commands.

use super::args::EnginesArgs;
use super::interrupt::{install_engine_with_interrupt, report_run_error};
use super::io::emit_stdout;
use pipeline_gui_lib::emit::{CliEvents, EventBus};
use pipeline_gui_lib::engines;
use std::sync::Arc;

pub(super) async fn cmd_engines(args: EnginesArgs) -> i32 {
    match args {
        EnginesArgs::Status { json } => cmd_engine_status(json).await,
        EnginesArgs::Install { id } => {
            let status = tokio::task::spawn_blocking(engines::engine_statuses)
                .await
                .ok()
                .and_then(|statuses| statuses.into_iter().find(|status| status.id == id));
            if let Some(status) = status.as_ref() {
                if !status.available {
                    eprintln!("Error: {}", status.unavailable_reason);
                    return 1;
                }
                eprintln!(
                    "{} {} (about {} MB download, {} MB installed)",
                    if status.installed {
                        "Repairing"
                    } else {
                        "Installing"
                    },
                    status.label,
                    status.est_download_mb,
                    status.est_disk_mb
                );
            }
            let bus: EventBus = Arc::new(CliEvents);
            match install_engine_with_interrupt(bus, &id).await {
                Ok(()) => {
                    eprintln!("PaddleOCR-VL Full Parser installation complete.");
                    0
                }
                Err(error) => report_run_error(&error),
            }
        }
        EnginesArgs::Uninstall { id } => {
            let bus: EventBus = Arc::new(CliEvents);
            match engines::uninstall_engine(&bus, &id).await {
                Ok(()) => {
                    eprintln!("PaddleOCR-VL Full Parser bundle removed.");
                    0
                }
                Err(error) => {
                    eprintln!("Error: {error}");
                    1
                }
            }
        }
    }
}

async fn cmd_engine_status(json: bool) -> i32 {
    let statuses = match tokio::task::spawn_blocking(engines::engine_statuses).await {
        Ok(statuses) => statuses,
        Err(error) => {
            eprintln!("Error: engine status task failed: {error}");
            return 1;
        }
    };
    if json {
        match serde_json::to_string_pretty(&statuses) {
            Ok(encoded) => emit_stdout(&encoded),
            Err(error) => {
                eprintln!("Error: failed to encode engine status: {error}");
                return 1;
            }
        }
        return 0;
    }
    for status in statuses {
        let state = if !status.available {
            "unavailable"
        } else if status.installing {
            "installing"
        } else if status.installed {
            "installed"
        } else {
            "not installed"
        };
        emit_stdout(&format!("{}: {state}", status.label));
        emit_stdout(&format!("  id: {}", status.id));
        if !status.version.is_empty() {
            emit_stdout(&format!("  version: {}", status.version));
        }
        if !status.entry_path.is_empty() {
            emit_stdout(&format!("  entry: {}", status.entry_path));
        }
        emit_stdout(&format!(
            "  estimated download/install: {} MB / {} MB",
            status.est_download_mb, status.est_disk_mb
        ));
        emit_stdout(&format!(
            "  managed stack on disk: {} MB",
            status.managed_stack_mb
        ));
        if !status.unavailable_reason.is_empty() {
            emit_stdout(&format!("  reason: {}", status.unavailable_reason));
        }
    }
    0
}
