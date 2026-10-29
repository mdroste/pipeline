//! Claude Code subscription authentication. No credential files or tokens cross
//! this boundary. Reviews and Workspace own their processes independently.

use serde_json::{json, Value};
use std::process::Command;
use std::time::Duration;

const AUTH_ERROR: &str = "Claude subscription mode requires your Claude Code subscription sign-in. Run `claude auth login` in Terminal and select your Claude subscription. API keys, Console accounts, cloud providers, and unknown authentication methods are not accepted. No API fallback was attempted.";

/// Remove alternative billing paths and configuration injected by the parent.
/// Claude Code still owns the user's normal keychain/file-based sign-in.
pub(crate) fn configure_subscription(command: &mut Command) {
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

/// Disable user/project API helpers and settings-provided environment keys.
/// The native forced login method also rejects conflicting managed policies.
pub(crate) fn subscription_args(args: &[String]) -> Vec<String> {
    let mut result = vec![
        "--setting-sources".into(),
        String::new(),
        "--settings".into(),
        json!({"forceLoginMethod":"claudeai", "apiKeyHelper":"", "disableAllHooks":true, "autoMemoryEnabled":false}).to_string(),
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
            Some("pro" | "max" | "team" | "enterprise")
        )
        || value
            .get("apiKeySource")
            .is_some_and(|v| !v.is_null() && v != "none")
    {
        return Err(AUTH_ERROR.into());
    }
    Ok(())
}

pub(crate) fn subscription_status(command: &crate::deps::ResolvedCommand) -> Result<Value, String> {
    let args = subscription_args(&["auth".into(), "status".into()]);
    let mut process = command.command(&args);
    configure_subscription(&mut process);
    process.current_dir(std::env::temp_dir());
    let output =
        crate::process::run_bounded_independent(&mut process, Duration::from_secs(15), 64 * 1024)?;
    if output.stdout_truncated || output.stderr_truncated {
        return Err("Claude authentication status exceeded its output limit".into());
    }
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|_| {
        format!("{AUTH_ERROR} Update Claude Code if its authentication status is unsupported.")
    })?;
    validate_subscription_status(&value, output.status.success())?;
    Ok(value)
}

/// Resolve `claude` and check that this executable is signed in with a Claude
/// subscription. Prompts must be sent to the returned executable.
pub(crate) async fn verified_subscription() -> Result<crate::deps::ResolvedCommand, String> {
    let resolved = crate::deps::resolve_command("claude")
        .ok_or("Install Claude Code and run `claude auth login` in Terminal first.")?;
    let probe = resolved.clone();
    tokio::task::spawn_blocking(move || subscription_status(&probe))
        .await
        .map_err(|e| format!("Claude authentication check failed: {e}"))??;
    Ok(resolved)
}

/// Build a prompt command for an executable returned by
/// [`verified_subscription`]. Every invocation keeps the subscription-only
/// environment and settings, even when the status check is reused.
pub(crate) fn verified_subscription_command(
    resolved: &crate::deps::ResolvedCommand,
    args: &[String],
    cwd: Option<&str>,
) -> tokio::process::Command {
    let mut command = resolved.command(subscription_args(args));
    configure_subscription(&mut command);
    command.current_dir(
        cwd.map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir),
    );
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    command
}

/// Resolve once and check the very executable that will receive the prompt.
pub(crate) async fn subscription_command(
    args: &[String],
    cwd: Option<&str>,
) -> Result<tokio::process::Command, String> {
    let resolved = verified_subscription().await?;
    Ok(verified_subscription_command(&resolved, args, cwd))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subscription() -> Value {
        json!({"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty","forcedLoginMethod":"claudeai","subscriptionType":"max"})
    }

    #[test]
    fn rejects_api_cloud_unknown_and_unenforced_authentication() {
        assert!(validate_subscription_status(&subscription(), true).is_ok());
        for (field, value) in [
            ("authMethod", json!("api_key")),
            ("authMethod", json!("console")),
            ("apiProvider", json!("bedrock")),
            ("apiKeySource", json!("ANTHROPIC_API_KEY")),
            ("forcedLoginMethod", Value::Null),
            ("subscriptionType", Value::Null),
            ("loggedIn", json!(false)),
        ] {
            let mut status = subscription();
            status[field] = value;
            assert!(
                validate_subscription_status(&status, true).is_err(),
                "{field}"
            );
        }
        assert!(validate_subscription_status(&subscription(), false).is_err());
        assert!(validate_subscription_status(&json!({}), true).is_err());
    }

    #[test]
    fn strips_alternative_credentials_and_disables_settings_helpers() {
        let mut cmd = Command::new("claude");
        for key in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_PROFILE",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CONFIG_DIR",
            "CLAUDE_CODE_SIMPLE",
        ] {
            cmd.env(key, "must-not-reach-child");
        }
        configure_subscription(&mut cmd);
        assert!(cmd
            .get_envs()
            .filter(|(k, _)| k.to_string_lossy().starts_with("ANTHROPIC_")
                || k.to_string_lossy().starts_with("CLAUDE_"))
            .all(|(_, v)| v.is_none()));
        let args = subscription_args(&["-p".into()]);
        assert_eq!(args[1], "");
        let settings: Value = serde_json::from_str(&args[3]).unwrap();
        assert_eq!(settings["forceLoginMethod"], "claudeai");
        assert_eq!(settings["apiKeyHelper"], "");
    }
}
