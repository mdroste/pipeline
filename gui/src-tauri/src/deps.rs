//! Dependency detection for the GUI startup check.

use crate::env;
use serde::Serialize;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_OUTPUT_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct DepStatus {
    pub name: String,
    pub found: bool,
    pub version: String,
    pub path: String,
    pub required: bool,
    pub hint: String,
    /// Optional official setup documentation. Windows uses this instead of
    /// presenting npm commands that may not be available on a fresh system.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_url: Option<String>,
    /// Whether the provider has usable authentication, either through its CLI
    /// session or a configured direct-API key. None = not applicable or the
    /// result could not be verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authenticated: Option<bool>,
    /// Sign-in state reported by the installed CLI itself. This stays
    /// separate from `authenticated`: a configured direct-API key can make a
    /// provider ready without saying anything about the CLI's own session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_auth_status: Option<CliAuthStatus>,
}

fn cli_setup_recommendation(
    product: &str,
    npm_command: &str,
    upgrade: bool,
    windows: bool,
) -> (String, Option<String>) {
    let action = if upgrade { "Upgrade" } else { "Install" };
    if windows {
        let url = match product {
            "Claude Code" => "https://code.claude.com/docs/en/installation",
            "Codex CLI" => "https://developers.openai.com/codex/cli/",
            "Antigravity CLI" => "https://antigravity.google/docs/cli",
            _ => {
                return (
                    format!("{action} {product} using its official Windows instructions."),
                    None,
                )
            }
        };
        return (
            format!("{action} {product} using the official Windows instructions."),
            Some(url.to_string()),
        );
    }
    (format!("{action} {product}: {npm_command}"), None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CliAuthStatus {
    SignedIn,
    SignedOut,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct DepsReport {
    pub deps: Vec<DepStatus>,
    pub ready: bool,
}

fn dependency_available(dependency: &DepStatus) -> bool {
    if !dependency.found {
        return false;
    }
    if let Some(authenticated) = dependency.authenticated {
        return authenticated;
    }
    match dependency.cli_auth_status {
        Some(CliAuthStatus::SignedIn) => true,
        Some(CliAuthStatus::SignedOut) => false,
        Some(CliAuthStatus::Unknown) => false,
        // Native binaries and managed engines have no authentication state.
        None => true,
    }
}

pub fn dependency_ready(dependency: &DepStatus) -> bool {
    !dependency.required || dependency_available(dependency)
}

mod checks;
mod command;

pub use checks::check_snapshot;
use command::{configure_probe_command, probe_resolved};
pub(crate) use command::{find_on_path, resolve_command, ResolvedCommand};

#[cfg(test)]
use checks::{
    antigravity_version_supported, check_antigravity_auth, check_claude_auth, cli_auth_status,
    parse_host_port, pdf_dependency_requirements, pdf_extraction_may_run, required_providers,
};
#[cfg(test)]
use command::{resolve_command_in, windows_pathexts};

#[cfg(test)]
mod tests;
