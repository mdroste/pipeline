use super::*;
#[tokio::test]
async fn side_cleanup_never_closes_foreground_or_releases_account_early() {
    for completed in [false, true] {
        let (foreground, _foreground_peer) = tokio::io::duplex(4096);
        let (read, write) = tokio::io::split(foreground);
        let foreground = AppServerClient::from_io(1, read, write);
        let (side, _side_peer) = tokio::io::duplex(4096);
        let (read, write) = tokio::io::split(side);
        let side = Arc::new(NativeSession::simulated(AppServerClient::from_io(
            2, read, write,
        )));
        let account = Arc::new(tokio::sync::RwLock::new(()));
        let home = tempfile::tempdir().unwrap();
        let home_path = home.path().to_path_buf();
        let guard = SideAccountUse {
            permit: Some(account.clone().read_owned().await),
            native: side.clone(),
            completed,
            submitted: true,
            home: Some(home),
        };
        assert!(account.try_write().is_err());
        drop(guard);
        let _write = tokio::time::timeout(Duration::from_secs(2), account.write())
            .await
            .unwrap();
        assert!(side.client.is_closed());
        assert!(!foreground.is_closed());
        assert!(!home_path.exists());
    }
}
