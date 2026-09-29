use super::*;

#[tokio::test]
async fn browser_login_holds_one_global_lease_and_requires_matching_cancel_identity() {
    use serde_json::{json, Value};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let temp = tempfile::tempdir().unwrap();
    let (client_io, server_io) = tokio::io::duplex(4096);
    let (read, write) = tokio::io::split(client_io);
    let client = AppServerClient::from_io(9, read, write);
    let owner = Arc::new(ChatgptAccount {
        native: Mutex::new(NativeSession::simulated(client)),
        root: temp.path().to_path_buf(),
        user_home: temp.path().to_path_buf(),
        activity: Arc::new(RwLock::new(())),
        serial: Mutex::new(()),
        clients: Mutex::new(Vec::new()),
        pending: Mutex::new(None),
        choices: Mutex::new(Vec::new()),
        login_error: Mutex::new(None),
        login_active: std::sync::atomic::AtomicBool::new(false),
        events: tokio::sync::broadcast::channel(16).0,
        _lock: std::fs::File::create(temp.path().join("lock")).unwrap(),
    });
    let server = tokio::spawn(async move {
        let (read, mut write) = tokio::io::split(server_io);
        let mut read = BufReader::new(read);
        for method in ["account/login/start", "account/login/cancel"] {
            let mut line = String::new();
            read.read_line(&mut line).await.unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], method);
            let result = if method == "account/login/start" {
                json!({"type":"chatgpt","loginId":"login-1","authUrl":"https://auth.openai.com/authorize"})
            } else {
                assert_eq!(request["params"]["loginId"], "login-1");
                json!({"status":"canceled"})
            };
            write
                .write_all(format!("{}\n", json!({"id":request["id"],"result":result})).as_bytes())
                .await
                .unwrap();
        }
    });
    let (login, _) = owner.begin_login().await.unwrap();
    assert!(owner.activity.try_read().is_err());
    assert!(owner.begin_login().await.is_err());
    assert!(owner.logout().await.is_err());
    assert!(owner.cancel_login(&login.login_id, Some(8)).await.is_err());
    assert!(owner.cancel_login("another-login", Some(9)).await.is_err());
    assert!(owner.cancel_login(&login.login_id, Some(9)).await.unwrap());
    server.await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if owner.activity.try_write().is_ok() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(owner.pending.lock().await.is_none());
}

#[tokio::test]
#[ignore = "no-model probe requires a locally installed compatible Codex runtime"]
async fn native_shared_account_installs_ephemeral_tokens_and_logs_out_both_runtimes() {
    use base64::Engine;
    use serde_json::json;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("account");
    std::fs::create_dir_all(root.join("codex")).unwrap();
    let now = chrono::Utc::now();
    let claims = json!({
        "sub":"fixture-user", "email":"fixture@example.invalid",
        "exp":now.timestamp()+3600, "iat":now.timestamp(),
        "https://api.openai.com/auth":{"chatgpt_account_id":"fixture-org","chatgpt_user_id":"fixture-user","chatgpt_plan_type":"plus"}
    });
    let token = format!(
        "e30.{}.fixture",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    private_write(&root.join("codex/auth.json"), &serde_json::to_vec(&json!({
        "auth_mode":"chatgpt", "OPENAI_API_KEY":null,
        "tokens":{"access_token":token,"id_token":token,"refresh_token":"fixture-refresh","account_id":"fixture-org"},
        "last_refresh":now.to_rfc3339()
    })).unwrap()).unwrap();
    let account = ChatgptAccount::launch(root, temp.path().to_path_buf())
        .await
        .unwrap();
    let resolved = crate::deps::resolve_command("codex").unwrap();
    let mut runtimes = Vec::new();
    for name in ["conversations", "reviews"] {
        let home = temp.path().join(name);
        private_directory(&home).unwrap();
        private_write(
            &home.join("config.toml"),
            b"cli_auth_credentials_store = \"ephemeral\"\nmodel_provider = \"openai\"\n",
        )
        .unwrap();
        let native = NativeSession::launch_resolved(
            &resolved,
            &home,
            temp.path(),
            7,
            name,
            "No-model account fixture",
        )
        .await
        .unwrap();
        assert_eq!(
            native.client.account_state(false).await.unwrap().status,
            AccountStatus::SignedOut
        );
        account.attach(&native.client).await.unwrap();
        assert_eq!(
            native.client.account_state(false).await.unwrap().status,
            AccountStatus::Chatgpt
        );
        assert!(
            !home.join("auth.json").exists(),
            "Execution runtime must not persist tokens"
        );
        runtimes.push(native);
    }
    let conversation = account.lease(&runtimes[0].client).await.unwrap();
    let review = account.lease(&runtimes[1].client).await.unwrap();
    assert!(account.logout().await.unwrap_err().contains("active"));
    drop(conversation);
    assert!(account.logout().await.is_err());
    drop(review);
    account.logout().await.unwrap();
    assert_eq!(
        account.status(false).await.unwrap().account.status,
        AccountStatus::SignedOut
    );
    for runtime in &runtimes {
        assert_eq!(
            runtime.client.account_state(false).await.unwrap().status,
            AccountStatus::SignedOut
        );
        runtime.shutdown().await;
    }
    account.native.lock().await.shutdown().await;
}
