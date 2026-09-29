//! Auth refresh stays off the conversation/review event buses and journals.
use super::{credentials, ChatgptAccount};
use crate::agent_runtime::codex::transport::{AuthRefresh, RequestError};
use serde_json::Value;
use std::sync::Weak;

pub(super) struct Refresh {
    pub owner: Weak<ChatgptAccount>,
    pub identity: String,
    pub account_id: String,
}
impl AuthRefresh for Refresh {
    fn refresh(
        &self,
        params: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, RequestError>> + Send + '_>>
    {
        Box::pin(async move {
            let failed = || {
                RequestError::unavailable("ChatGPT sign-in needs attention; reconnect in Settings")
            };
            if params["reason"] != "unauthorized"
                || params["previousAccountId"]
                    .as_str()
                    .is_some_and(|id| id != self.account_id)
            {
                return Err(failed());
            }
            let owner = self.owner.upgrade().ok_or_else(failed)?;
            let expected = self.identity.clone();
            // This task retains the refresh serialization and account lease even
            // if the requesting runtime's 8-second response deadline expires.
            tokio::spawn(async move {
                let _lease = owner
                    .activity
                    .clone()
                    .try_read_owned()
                    .map_err(|_| failed())?;
                let _serial = owner.serial.lock().await;
                let before = credentials::read(&owner.root.join("codex/auth.json"))
                    .map_err(|_| failed())?
                    .ok_or_else(failed)?;
                if before.identity != expected {
                    return Err(failed());
                }
                let client = owner.client().await.map_err(|_| failed())?;
                if client.account_state(true).await.is_err() {
                    owner.native.lock().await.terminate().await;
                    return Err(failed());
                }
                let auth = credentials::read(&owner.root.join("codex/auth.json"))
                    .map_err(|_| failed())?
                    .ok_or_else(failed)?;
                if auth.identity != expected {
                    return Err(failed());
                }
                Ok(auth.external_tokens())
            })
            .await
            .map_err(|_| failed())?
        })
    }
}
