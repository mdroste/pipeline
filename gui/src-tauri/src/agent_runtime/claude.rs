//! Claude Code native subscription and Console authentication. No credential files or tokens cross
//! this boundary. Reviews and Workspace own their processes independently.
//!
//! If the CLI's authentication behavior changes, fail and requalify; never
//! restore subscription mode by supplying a key. `--bare` stays off because bare
//! mode skips subscription authentication. This guard prevents API-key billing
//! only: paid extra usage enabled on the subscription still applies. The status
//! probe has its own process owner, so Review cancellation can't kill a
//! conversation's check.

use serde_json::{json, Value};
use std::process::Command;
use std::time::Duration;

const AUTH_ERROR: &str = "[claude-auth] Claude subscription mode requires your Claude Code subscription sign-in. Run `claude auth login` in Terminal and select your Claude subscription. API keys, Console accounts, cloud providers, and unknown authentication methods are not accepted. No API fallback was attempted.";

/// Billing is selected by the saved connection, never inferred from credentials.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AuthMode {
    #[default]
    Subscription,
    Api,
}
impl AuthMode {
    pub fn from_runtime(runtime: &str) -> Result<Self, String> {
        match runtime {
            "claude" => Ok(Self::Subscription),
            "claude-code-api" => Ok(Self::Api),
            _ => Err("Choose a Claude Code connection".into()),
        }
    }
    pub fn runtime(self) -> &'static str {
        match self {
            Self::Subscription => "claude",
            Self::Api => "claude-code-api",
        }
    }
    fn login_method(self) -> &'static str {
        match self {
            Self::Subscription => "claudeai",
            Self::Api => "console",
        }
    }
    pub fn error(self) -> &'static str {
        match self {
            Self::Subscription => AUTH_ERROR,
            Self::Api => "[claude-auth] Claude Code API requires a saved Anthropic Console API-key login. Run `claude auth login --console` in Terminal; if offered, choose Create an API key. Keyless profiles and cloud-provider accounts are not supported by this connection. Pipeline does not pass its saved API key to Claude Code or switch to subscription billing.",
        }
    }
}

/// Remove alternative billing paths and configuration injected by the parent.
/// Claude Code still owns the user's normal keychain/file-based sign-in.
pub(crate) fn configure_native_auth(command: &mut Command) {
    let keys = std::env::vars_os()
        .map(|(key, _)| key)
        .chain(command.get_envs().map(|(key, _)| key.to_owned()))
        .filter(|key| {
            let key = key.to_string_lossy().to_ascii_uppercase();
            key.starts_with("ANTHROPIC_") || key.starts_with("CLAUDE_")
        })
        .collect::<Vec<_>>();
    for key in keys {
        command.env_remove(key);
    }
    command.env("PATH", crate::env::full_path());
    crate::process::configure_isolation(command);
}

/// Disable inherited settings, API helpers, and hooks in either billing mode.
/// The only caller-supplied setting is the sandbox, nested beneath auth policy.
pub(crate) fn sandbox_auth_args(
    mode: AuthMode,
    sandbox: Option<&Value>,
    args: &[String],
) -> Vec<String> {
    let mut settings = json!({"forceLoginMethod":mode.login_method(), "apiKeyHelper":"", "disableAllHooks":true, "autoMemoryEnabled":false});
    if let Some(sandbox) = sandbox {
        settings["sandbox"] = sandbox.clone();
    }
    let mut result = vec![
        "--setting-sources".into(),
        String::new(),
        "--settings".into(),
        settings.to_string(),
    ];
    result.extend_from_slice(args);
    result
}

pub(crate) fn validate_subscription_status(value: &Value, success: bool) -> Result<(), String> {
    if !success
        || value["loggedIn"] != true
        || value["authMethod"] != "claude.ai"
        || value["apiProvider"] != "firstParty"
        || value["forcedLoginMethod"] != "claudeai"
        || !matches!(
            value["subscriptionType"].as_str(),
            Some("pro" | "max" | "ultra" | "team" | "enterprise")
        )
        || value
            .get("apiKeySource")
            .is_some_and(|v| !v.is_null() && v != "none")
    {
        return Err(AUTH_ERROR.into());
    }
    Ok(())
}

pub(crate) fn validate_auth_status(
    mode: AuthMode,
    value: &Value,
    success: bool,
) -> Result<(), String> {
    match mode {
        AuthMode::Subscription => validate_subscription_status(value, success),
        AuthMode::Api
            if success
                && value["loggedIn"] == true
                && value["authMethod"] == "api_key"
                && value["apiProvider"] == "firstParty"
                && value["forcedLoginMethod"] == "console"
                && value.get("subscriptionType") == Some(&Value::Null) =>
        {
            Ok(())
        }
        AuthMode::Api => Err(mode.error().into()),
    }
}

pub(crate) fn auth_status(
    command: &crate::deps::ResolvedCommand,
    mode: AuthMode,
) -> Result<Value, String> {
    let args = sandbox_auth_args(mode, None, &["auth".into(), "status".into()]);
    let mut process = command.command(&args);
    configure_native_auth(&mut process);
    process.current_dir(std::env::temp_dir());
    let output =
        crate::process::run_bounded_independent(&mut process, Duration::from_secs(15), 64 * 1024)
            .map_err(|e| format!("{} Authentication check failed: {e}", mode.error()))?;
    if output.stdout_truncated || output.stderr_truncated {
        return Err(format!(
            "{} Authentication status exceeded its output limit.",
            mode.error()
        ));
    }
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|_| {
        format!(
            "{} Update Claude Code if its authentication status is unsupported.",
            mode.error()
        )
    })?;
    validate_auth_status(mode, &value, output.status.success())?;
    Ok(value)
}

pub(crate) async fn verified_subscription_with_status(
) -> Result<(crate::deps::ResolvedCommand, Value), String> {
    verified_auth_with_status(AuthMode::Subscription).await
}

pub(crate) async fn verified_auth(mode: AuthMode) -> Result<crate::deps::ResolvedCommand, String> {
    verified_auth_with_status(mode)
        .await
        .map(|(resolved, _)| resolved)
}

pub(crate) async fn verified_auth_with_status(
    mode: AuthMode,
) -> Result<(crate::deps::ResolvedCommand, Value), String> {
    let resolved = crate::deps::resolve_command("claude")
        .ok_or_else(|| format!("Install Claude Code first. {}", mode.error()))?;
    verified_resolved_auth(resolved, mode).await
}

/// Authentication is checked for every process, including a reused executable.
pub(crate) async fn verified_resolved_auth(
    resolved: crate::deps::ResolvedCommand,
    mode: AuthMode,
) -> Result<(crate::deps::ResolvedCommand, Value), String> {
    let probe = resolved.clone();
    let status = tokio::task::spawn_blocking(move || auth_status(&probe, mode))
        .await
        .map_err(|e| format!("{} Authentication check failed: {e}", mode.error()))??;
    Ok((resolved, status))
}

pub(crate) fn verified_auth_command(
    resolved: &crate::deps::ResolvedCommand,
    mode: AuthMode,
    sandbox: Option<&Value>,
    args: &[String],
    cwd: Option<&str>,
) -> tokio::process::Command {
    prompt_command(resolved, sandbox_auth_args(mode, sandbox, args), cwd)
}

fn prompt_command(
    resolved: &crate::deps::ResolvedCommand,
    args: Vec<String>,
    cwd: Option<&str>,
) -> tokio::process::Command {
    let mut command = resolved.command(args);
    configure_native_auth(&mut command);
    command.current_dir(
        cwd.map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir),
    );
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    command
}

#[cfg(test)]
mod tests;
