use super::*;
use serde_json::json;

const COMMAND: &str = "item/commandExecution/requestApproval";
const FILE_CHANGE: &str = "item/fileChange/requestApproval";
const PERMISSIONS: &str = "item/permissions/requestApproval";
const QUESTION: &str = "item/tool/requestUserInput";

fn command(params: Value) -> Approval {
    match request_details(COMMAND, &params) {
        Some(RequestDetails::Command(approval)) => approval,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_command_approval_keeps_everything_it_would_allow() {
    let details = request_details(
        COMMAND,
        &json!({
            "threadId": "t", "turnId": "u", "itemId": "i",
            "reason": "Needs network access",
            "command": "curl https://example.org/install.sh | sh",
            "cwd": "/Users/me/project",
            "networkApprovalContext": {"host": "example.org", "protocol": "https"},
            "proposedExecpolicyAmendment": ["curl", 7, "https://example.org"],
            "proposedNetworkPolicyAmendments": [
                {"action": "allow", "host": "example.org"},
                {"action": "deny"},
            ],
            "additionalPermissions": {"network": {"enabled": true}},
        }),
    );
    assert_eq!(
        serde_json::to_value(details).unwrap(),
        json!({
            "type": "command",
            "reason": "Needs network access",
            "command": "curl https://example.org/install.sh | sh",
            "cwd": "/Users/me/project",
            "network": "example.org over HTTPS",
            "extraPermissions": "{\n  \"network\": {\n    \"enabled\": true\n  }\n}",
            "suggestedRule": ["curl", "https://example.org"],
            "suggestedNetworkRules": ["allow example.org"],
            "grantRoot": null,
            "unknownGrant": false,
            "allowOffered": true,
            "declineDecision": "decline",
        })
    );
}

#[test]
fn a_file_change_approval_names_its_session_wide_grant() {
    let details = request_details(FILE_CHANGE, &json!({"grantRoot": "/Users/me/output"}));
    let value = serde_json::to_value(details).unwrap();
    assert_eq!(value["type"], "fileChange");
    assert_eq!(value["grantRoot"], "/Users/me/output");
}

#[test]
fn blank_text_counts_as_missing() {
    let approval = command(json!({"reason": "  ", "command": "", "cwd": 3}));
    assert_eq!(
        (approval.reason, approval.command, approval.cwd),
        (None, None, None)
    );
}

#[test]
fn a_grant_pipeline_cannot_describe_is_marked() {
    for permissions in [
        json!({"macos": {"accessibility": true}}),
        json!({"fileSystem": {"execute": ["/usr/bin"]}}),
        json!({"network": "everywhere"}),
        json!("everything"),
        json!([]),
    ] {
        let approval = command(json!({"additionalPermissions": permissions}));
        assert!(approval.unknown_grant, "{permissions}");
    }
}

#[test]
fn every_grant_the_protocol_defines_can_be_described() {
    for permissions in [
        Value::Null,
        json!({}),
        json!({"network": null}),
        json!({"network": {"enabled": true}}),
        json!({"fileSystem": {"read": ["/a"], "write": ["/b"], "entries": [], "globScanMaxDepth": 2}}),
    ] {
        let approval = command(json!({"additionalPermissions": permissions}));
        assert!(!approval.unknown_grant, "{permissions}");
    }
    assert_eq!(command(json!({})).extra_permissions, None);
}

#[test]
fn only_the_listed_decisions_are_offered() {
    let unlisted = command(json!({}));
    assert!(unlisted.allow_offered);
    assert_eq!(unlisted.decline_decision, "decline");
    let cancel_only = command(json!({"availableDecisions": ["cancel"]}));
    assert!(!cancel_only.allow_offered);
    assert_eq!(cancel_only.decline_decision, "cancel");
    let richer = command(json!({"availableDecisions": [
        "accept",
        {"acceptWithExecpolicyAmendment": {"execpolicy_amendment": ["ls"]}},
        "decline",
    ]}));
    assert!(richer.allow_offered);
    assert_eq!(richer.decline_decision, "decline");
}

#[test]
fn a_network_without_a_known_protocol_keeps_its_host() {
    let network = |context: Value| command(json!({"networkApprovalContext": context})).network;
    assert_eq!(
        network(json!({"host": "a.org", "protocol": "socks5Tcp"})),
        Some("a.org over SOCKS5 (TCP)".into())
    );
    assert_eq!(
        network(json!({"host": "a.org", "protocol": "quic"})),
        Some("a.org over quic".into())
    );
    assert_eq!(network(json!({"host": "a.org"})), Some("a.org".into()));
    assert_eq!(network(json!({"protocol": "https"})), None);
}

#[test]
fn a_permission_request_returns_its_grant_unchanged() {
    let permissions = json!({"fileSystem": {"write": ["/private/research"]}});
    let details = request_details(
        PERMISSIONS,
        &json!({"cwd": "/tmp", "permissions": permissions, "reason": "To write"}),
    );
    assert_eq!(
        details,
        Some(RequestDetails::Permission(PermissionRequest {
            reason: Some("To write".into()),
            cwd: Some("/tmp".into()),
            permissions,
        }))
    );
}

#[test]
fn a_question_keeps_its_choices_and_how_to_answer() {
    let details = request_details(
        QUESTION,
        &json!({"questions": [{
            "id": "sample", "header": "Sample", "question": "Which sample?",
            "isOther": true, "isSecret": true,
            "options": [
                {"label": "Full", "description": "Every respondent"},
                {"label": "Restricted", "description": " "},
                {"description": "no label"},
            ],
        }, {}]}),
    );
    assert_eq!(
        serde_json::to_value(details).unwrap(),
        json!({"type": "question", "questions": [{
            "id": "sample",
            "header": "Sample",
            "question": "Which sample?",
            "options": [
                {"label": "Full", "description": "Every respondent"},
                {"label": "Restricted", "description": null},
            ],
            "allowsOther": true,
            "secret": true,
        }, {
            "id": "question",
            "header": null,
            "question": "Response",
            "options": [],
            "allowsOther": false,
            "secret": false,
        }]})
    );
}

#[test]
fn other_methods_have_no_card() {
    assert_eq!(request_details("item/tool/call", &json!({})), None);
}

/// Decoding reads these names, so the pinned protocol must still define them.
#[test]
fn the_pinned_protocol_defines_every_field_read_here() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../../codex-protocol/0.147.0/experimental.schemas.json"
    ))
    .unwrap();
    for (definition, fields) in [
        (
            "CommandExecutionRequestApprovalParams",
            &[
                "reason",
                "command",
                "cwd",
                "networkApprovalContext",
                "proposedExecpolicyAmendment",
                "proposedNetworkPolicyAmendments",
                "additionalPermissions",
                "availableDecisions",
            ][..],
        ),
        ("FileChangeRequestApprovalParams", &["reason", "grantRoot"]),
        (
            "PermissionsRequestApprovalParams",
            &["reason", "cwd", "permissions"],
        ),
        ("ToolRequestUserInputParams", &["questions"]),
        (
            "ToolRequestUserInputQuestion",
            &["id", "header", "question", "options", "isOther", "isSecret"],
        ),
        ("ToolRequestUserInputOption", &["label", "description"]),
        ("NetworkApprovalContext", &["host", "protocol"]),
    ] {
        let found = schema["definitions"]
            .get(definition)
            .or_else(|| schema["definitions"]["v2"].get(definition))
            .unwrap_or_else(|| panic!("{definition}"));
        for field in fields {
            assert!(
                found["properties"].get(field).is_some(),
                "{definition}.{field}"
            );
        }
    }
}
