//! The app's record of what the assistant is asking. App Server's approval and
//! question shapes stop here: the request card reads these fields and decides
//! nothing from raw params. A grant Pipeline can't describe is reported as such,
//! so the card can withhold it.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RequestDetails {
    Command(Approval),
    FileChange(Approval),
    Permission(PermissionRequest),
    Question(QuestionRequest),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub reason: Option<String>,
    pub command: Option<String>,
    pub cwd: Option<String>,
    /// For example `example.org over HTTPS`.
    pub network: Option<String>,
    /// Permissions Allow once would add, as readable JSON.
    pub extra_permissions: Option<String>,
    /// Not added by Allow once.
    pub suggested_rule: Vec<String>,
    pub suggested_network_rules: Vec<String>,
    pub grant_root: Option<String>,
    pub unknown_grant: bool,
    pub allow_offered: bool,
    /// The decision that answers Decline.
    pub decline_decision: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    pub reason: Option<String>,
    pub cwd: Option<String>,
    /// What is granted, returned unchanged when the researcher allows it.
    pub permissions: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuestionRequest {
    pub questions: Vec<Question>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub id: String,
    pub header: Option<String>,
    pub question: String,
    pub options: Vec<QuestionOption>,
    pub allows_other: bool,
    pub secret: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

/// The grants a command approval may carry, by the pinned protocol.
const PERMISSION_FIELDS: [(&str, &[&str]); 2] = [
    (
        "fileSystem",
        &["entries", "globScanMaxDepth", "read", "write"],
    ),
    ("network", &["enabled"]),
];

/// `None` for a method no card answers. The bridge refuses those first.
pub fn request_details(method: &str, params: &Value) -> Option<RequestDetails> {
    match method {
        "item/commandExecution/requestApproval" => Some(RequestDetails::Command(approval(params))),
        "item/fileChange/requestApproval" => Some(RequestDetails::FileChange(approval(params))),
        "item/permissions/requestApproval" => Some(RequestDetails::Permission(PermissionRequest {
            reason: text(&params["reason"]),
            cwd: text(&params["cwd"]),
            permissions: params.get("permissions").cloned().unwrap_or(Value::Null),
        })),
        "item/tool/requestUserInput" => Some(RequestDetails::Question(QuestionRequest {
            questions: params["questions"]
                .as_array()
                .map(|questions| questions.iter().map(question).collect())
                .unwrap_or_default(),
        })),
        _ => None,
    }
}

fn approval(params: &Value) -> Approval {
    let offers = |decision: &str| {
        params["availableDecisions"]
            .as_array()
            .is_none_or(|decisions| decisions.iter().any(|offered| offered == decision))
    };
    let grants = &params["additionalPermissions"];
    Approval {
        reason: text(&params["reason"]),
        command: text(&params["command"]),
        cwd: text(&params["cwd"]),
        network: network_access(&params["networkApprovalContext"]),
        extra_permissions: (!grants.is_null())
            .then(|| serde_json::to_string_pretty(grants).unwrap_or_else(|_| grants.to_string())),
        suggested_rule: strings(&params["proposedExecpolicyAmendment"]),
        suggested_network_rules: network_rules(&params["proposedNetworkPolicyAmendments"]),
        grant_root: text(&params["grantRoot"]),
        unknown_grant: has_unknown_permission(grants),
        allow_offered: offers("accept"),
        decline_decision: if !offers("decline") && offers("cancel") {
            "cancel"
        } else {
            "decline"
        },
    }
}

fn question(value: &Value) -> Question {
    let options = value["options"].as_array();
    Question {
        id: value["id"].as_str().unwrap_or("question").to_owned(),
        header: text(&value["header"]),
        question: value["question"].as_str().unwrap_or("Response").to_owned(),
        options: options
            .into_iter()
            .flatten()
            .filter_map(|option| {
                Some(QuestionOption {
                    label: option["label"].as_str()?.to_owned(),
                    description: text(&option["description"]),
                })
            })
            .collect(),
        allows_other: value["isOther"] == true,
        secret: value["isSecret"] == true,
    }
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .map(str::to_owned)
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| part.as_str().map(str::to_owned))
        .collect()
}

fn network_access(value: &Value) -> Option<String> {
    let host = value["host"].as_str().filter(|host| !host.is_empty())?;
    let protocol = match value["protocol"].as_str() {
        Some("http") => "HTTP",
        Some("https") => "HTTPS",
        Some("socks5Tcp") => "SOCKS5 (TCP)",
        Some("socks5Udp") => "SOCKS5 (UDP)",
        Some(other) if !other.is_empty() => other,
        _ => return Some(host.to_owned()),
    };
    Some(format!("{host} over {protocol}"))
}

fn network_rules(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|rule| {
            Some(format!(
                "{} {}",
                rule["action"].as_str()?,
                rule["host"].as_str()?
            ))
        })
        .collect()
}

fn has_unknown_permission(permissions: &Value) -> bool {
    if permissions.is_null() {
        return false;
    }
    let Some(permissions) = permissions.as_object() else {
        return true;
    };
    permissions.iter().any(|(field, value)| {
        let Some((_, known)) = PERMISSION_FIELDS.iter().find(|(name, _)| name == field) else {
            return true;
        };
        match value {
            Value::Null => false,
            Value::Object(names) => names.keys().any(|name| !known.contains(&name.as_str())),
            _ => true,
        }
    })
}

#[cfg(test)]
mod tests;
