//! Chat approval settings and exact, conversation-scoped command grants.
//! Only request fingerprints are stored. Each grant also binds to the
//! executing conversation's harness fingerprint and working directory.

use super::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::params;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Ask,
    Auto,
    Sandbox,
}

impl Mode {
    pub(crate) fn parse(value: &Value) -> Option<Self> {
        match value.as_str()? {
            "ask" => Some(Self::Ask),
            "auto" => Some(Self::Auto),
            "sandbox" => Some(Self::Sandbox),
            _ => None,
        }
    }

    pub(crate) fn codex_fingerprint(self, harness: &str) -> String {
        // An already loaded thread keeps its approval settings on resume.
        // Keep legacy fingerprints compatible with the default policy.
        match self {
            Self::Ask => harness.to_owned(),
            Self::Auto => hash(&format!("{harness}:auto")),
            Self::Sandbox => hash(&format!("{harness}:sandbox-only")),
        }
    }
}

fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value))
}

pub(crate) fn command_key(method: &str, p: &Value) -> Option<String> {
    let command = p["command"].as_str().filter(|s| !s.trim().is_empty())?;
    let cwd = p[if method == super::claude::approval::METHOD {
        "sandboxRoot"
    } else {
        "cwd"
    }]
    .as_str()
    .filter(|s| std::path::Path::new(s).is_absolute())?;
    let allowed = p["availableDecisions"]
        .as_array()
        .is_none_or(|values| values.iter().any(|value| value == "accept"));
    if !allowed {
        return None;
    }
    let identity = match method {
        "item/commandExecution/requestApproval" => {
            // The renderer must be able to describe the whole grant. Unknown
            // protocol fields and additional authority require a fresh decision.
            if p.as_object()?.iter().any(|(key, value)| {
                !value.is_null()
                    && !matches!(
                        key.as_str(),
                        "command"
                            | "cwd"
                            | "environmentId"
                            | "commandActions"
                            | "approvalId"
                            | "itemId"
                            | "reason"
                            | "startedAtMs"
                            | "threadId"
                            | "turnId"
                            | "proposedExecpolicyAmendment"
                            | "availableDecisions"
                    )
            }) {
                return None;
            }
            json!([
                method,
                cwd,
                command,
                p["environmentId"],
                p["commandActions"]
            ])
        }
        super::claude::approval::METHOD => json!([method, cwd, p["inputKey"].as_str()?]),
        _ => return None,
    };
    Some(hash(&identity.to_string()))
}

pub(crate) fn input_key(input: &Value) -> String {
    let mut input = input.clone();
    if let Some(fields) = input.as_object_mut() {
        fields.remove("description");
    }
    hash(&input.to_string())
}

fn scope(store: &Store, session: &str, thread: &str) -> WorkbenchResult<Option<String>> {
    Ok(store
        .active_binding(session)?
        .filter(|binding| binding.provider_thread_id == thread)
        .and_then(|binding| binding.harness_fingerprint))
}

pub(crate) fn allowed(
    store: &Store,
    session: &str,
    thread: &str,
    key: &str,
) -> WorkbenchResult<bool> {
    let Some(scope) = scope(store, session, thread)? else {
        return Ok(false);
    };
    store.connection()?.query_row(
        "SELECT EXISTS(SELECT 1 FROM chat_approvals WHERE session_id = ?1 AND scope = ?2 AND request_key = ?3)",
        params![session, scope, key], |row| row.get(0),
    ).map_err(|e| WorkbenchError::storage("Could not read saved chat approval", e))
}

pub(crate) fn remember(
    store: &Store,
    session: &str,
    thread: &str,
    key: &str,
) -> WorkbenchResult<()> {
    let scope = scope(store, session, thread)?.ok_or_else(|| {
        WorkbenchError::conflict("The command's conversation is no longer active")
    })?;
    store.connection()?.execute(
        "INSERT OR IGNORE INTO chat_approvals (session_id, scope, request_key) VALUES (?1, ?2, ?3)",
        params![session, scope, key],
    ).map_err(|e| WorkbenchError::storage("Could not save chat approval", e))?;
    Ok(())
}

pub(crate) fn clear(store: &Store, session: &str) -> WorkbenchResult<()> {
    store
        .connection()?
        .execute(
            "DELETE FROM chat_approvals WHERE session_id = ?1",
            [session],
        )
        .map_err(|e| WorkbenchError::storage("Could not clear saved chat approvals", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::tests::disposable_workspace;
    use crate::workbench::store::CreateSessionRequest;

    #[test]
    fn saved_approval_requires_an_exact_command_and_current_conversation_scope() {
        let store = disposable_workspace();
        let session = store
            .create_session(CreateSessionRequest {
                workspace_id: None,
                title: "Approvals".into(),
                operation_id: "create-approval-conversation".into(),
            })
            .unwrap()
            .record
            .id;
        let binding = store
            .bind_session(&session, "codex-test", "thread-one")
            .unwrap();
        let fingerprint = hash("project and permissions");
        store
            .set_binding_harness(&binding.id, &fingerprint, &[])
            .unwrap();
        let method = "item/commandExecution/requestApproval";
        let cwd = std::env::temp_dir();
        let mut params = json!({"command":"pwd","cwd":cwd,"itemId":"one"});
        let key = command_key(method, &params).unwrap();
        remember(&store, &session, "thread-one", &key).unwrap();
        params["itemId"] = json!("two");
        assert_eq!(command_key(method, &params).as_deref(), Some(key.as_str()));
        assert!(allowed(&store, &session, "thread-one", &key).unwrap());
        assert!(!allowed(&store, &session, "thread-other", &key).unwrap());
        params["command"] = json!("ls");
        assert!(!allowed(
            &store,
            &session,
            "thread-one",
            &command_key(method, &params).unwrap()
        )
        .unwrap());
        params["additionalPermissions"] = json!({"network":{"enabled":true}});
        assert!(command_key(method, &params).is_none());
        store
            .set_binding_harness(&binding.id, &hash("changed permissions"), &[])
            .unwrap();
        assert!(!allowed(&store, &session, "thread-one", &key).unwrap());
        clear(&store, &session).unwrap();
        store
            .set_binding_harness(&binding.id, &fingerprint, &[])
            .unwrap();
        assert!(!allowed(&store, &session, "thread-one", &key).unwrap());
    }
}
