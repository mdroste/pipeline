use super::*;

#[tauri::command]
pub(crate) async fn chatgpt_account_status(
    app: tauri::AppHandle,
    refresh_token: Option<bool>,
) -> Result<Status, String> {
    let owner = connect().await?;
    ensure_event_bridge(app, &owner);
    owner.status(refresh_token.unwrap_or(false)).await
}
#[tauri::command]
pub(crate) async fn chatgpt_login_start(app: tauri::AppHandle) -> Result<AccountLogin, String> {
    begin_browser_login(app).await.map(|(result, _)| result)
}

pub(crate) async fn begin_browser_login(
    app: tauri::AppHandle,
) -> Result<(AccountLogin, LoginStart), String> {
    let owner = connect().await?;
    ensure_event_bridge(app.clone(), &owner);
    let (result, login) = owner.begin_login().await?;
    use tauri_plugin_shell::ShellExt;
    #[allow(deprecated)]
    if app.shell().open(&login.auth_url, None).is_err() {
        let _ = owner
            .cancel_login(&result.login_id, Some(result.epoch))
            .await;
        return Err("Could not open ChatGPT sign-in in the browser".into());
    }
    Ok((result, login))
}
#[tauri::command]
pub(crate) async fn chatgpt_login_cancel(login_id: String, epoch: u64) -> Result<bool, String> {
    connect().await?.cancel_login(&login_id, Some(epoch)).await
}
#[tauri::command]
pub(crate) async fn chatgpt_logout() -> Result<(), String> {
    connect().await?.logout().await
}
#[tauri::command]
pub(crate) async fn chatgpt_select_existing_account(source: String) -> Result<(), String> {
    connect().await?.select_existing(&source).await
}
#[tauri::command]
pub(crate) async fn chatgpt_model_catalog() -> Result<super::super::ModelCatalog, String> {
    let owner = connect().await?;
    let _lease = owner.activity.try_read().map_err(|_| BUSY)?;
    owner
        .client()
        .await?
        .model_catalog()
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub(crate) async fn chatgpt_rate_limits() -> Result<super::super::RateLimits, String> {
    let owner = connect().await?;
    let _lease = owner.activity.try_read().map_err(|_| BUSY)?;
    owner
        .client()
        .await?
        .rate_limits()
        .await
        .map_err(|e| e.to_string())
}

fn ensure_event_bridge(app: tauri::AppHandle, owner: &Arc<ChatgptAccount>) {
    static BRIDGE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if BRIDGE.swap(true, Ordering::AcqRel) {
        return;
    }
    let mut events = owner.events.subscribe();
    tokio::spawn(async move {
        use tauri::Emitter;
        while let Ok(()) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) =
            events.recv().await
        {
            let _ = app.emit("chatgpt:account-changed", ());
        }
        BRIDGE.store(false, Ordering::Release);
    });
}
