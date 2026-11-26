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
    configure_native_auth(&mut cmd);
    assert!(cmd
        .get_envs()
        .filter(|(k, _)| k.to_string_lossy().starts_with("ANTHROPIC_")
            || k.to_string_lossy().starts_with("CLAUDE_"))
        .all(|(_, v)| v.is_none()));
    let args = sandbox_auth_args(AuthMode::Subscription, None, &["-p".into()]);
    assert_eq!(args[1], "");
    let settings: Value = serde_json::from_str(&args[3]).unwrap();
    assert_eq!(settings["forceLoginMethod"], "claudeai");
    assert_eq!(settings["apiKeyHelper"], "");
    assert!(settings.get("sandbox").is_none());
}

#[test]
fn a_sandbox_cannot_replace_subscription_overrides() {
    let sandbox = json!({"enabled":true,"forceLoginMethod":"console"});
    let args = sandbox_auth_args(AuthMode::Subscription, Some(&sandbox), &["-p".into()]);
    assert_eq!(args.iter().filter(|a| *a == "--settings").count(), 1);
    let settings: Value = serde_json::from_str(&args[3]).unwrap();
    assert_eq!(settings["forceLoginMethod"], "claudeai");
    assert_eq!(settings["disableAllHooks"], true);
    assert_eq!(settings["sandbox"], sandbox);
    assert_eq!(args[4], "-p");
}

fn console() -> Value {
    json!({"loggedIn":true,"authMethod":"api_key","apiProvider":"firstParty","forcedLoginMethod":"console","subscriptionType":null,"apiKeySource":"/login managed key"})
}

#[test]
fn modes_reject_each_others_accounts_and_unknown_console_methods() {
    assert!(validate_auth_status(AuthMode::Api, &console(), true).is_ok());
    assert!(validate_auth_status(AuthMode::Subscription, &console(), true).is_err());
    assert!(validate_auth_status(AuthMode::Api, &subscription(), true).is_err());
    assert!(validate_auth_status(AuthMode::Api, &console(), false).is_err());
    for (field, value) in [
        ("loggedIn", json!(false)),
        ("authMethod", json!("api_key_helper")),
        ("authMethod", json!("oauth_token")),
        ("authMethod", json!("third_party")),
        ("authMethod", json!("unknown")),
        ("forcedLoginMethod", Value::Null),
        ("forcedLoginMethod", json!("claudeai")),
        ("subscriptionType", json!("max")),
        ("apiProvider", json!("bedrock")),
    ] {
        let mut status = console();
        status[field] = value;
        assert!(
            validate_auth_status(AuthMode::Api, &status, true).is_err(),
            "{field}"
        );
    }
    assert!(validate_auth_status(AuthMode::Api, &json!({}), true).is_err());
    let mut missing_plan = console();
    missing_plan
        .as_object_mut()
        .unwrap()
        .remove("subscriptionType");
    assert!(validate_auth_status(AuthMode::Api, &missing_plan, true).is_err());
    for plan in ["pro", "max", "ultra", "team", "enterprise"] {
        let mut status = subscription();
        status["subscriptionType"] = json!(plan);
        assert!(validate_auth_status(AuthMode::Subscription, &status, true).is_ok());
    }
}

#[test]
fn saved_runtime_identity_selects_billing_and_prompt_commands_enforce_it() {
    for mode in [AuthMode::Subscription, AuthMode::Api] {
        assert_eq!(AuthMode::from_runtime(mode.runtime()).unwrap(), mode);
        let resolved = crate::deps::ResolvedCommand::direct("fake-claude".into());
        let cmd = verified_auth_command(&resolved, mode, None, &["-p".into()], None);
        let cmd = cmd.as_std();
        assert_eq!(cmd.get_program(), resolved.program());
        let args = cmd
            .get_args()
            .map(|s| s.to_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(&args[..3], &["--setting-sources", "", "--settings"]);
        let settings: Value = serde_json::from_str(args[3]).unwrap();
        assert_eq!(settings["forceLoginMethod"], mode.login_method());
        assert_eq!(settings["apiKeyHelper"], "");
        assert_eq!(settings["disableAllHooks"], true);
        assert!(!args.contains(&"--bare"));
        assert_eq!(args[4], "-p");
    }
    assert!(AuthMode::from_runtime("anthropic-api").is_err());
    assert!(AuthMode::from_runtime("unknown").is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn rechecks_the_account_even_when_the_executable_is_reused() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fake-claude");
    let command = crate::deps::ResolvedCommand::direct(path.clone());
    for (status, accepted) in [
        (subscription(), true),
        (console(), false),
        (subscription(), true),
    ] {
        std::fs::write(&path, format!("#!/bin/sh\nprintf '%s\\n' '{}'\n", status)).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            verified_resolved_auth(command.clone(), AuthMode::Subscription)
                .await
                .is_ok(),
            accepted
        );
    }
}
