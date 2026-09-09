use super::*;

#[test]
fn detached_extraction_owner_keeps_maintenance_locked() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("engine.lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&lock).unwrap();
    let lease = std::sync::Arc::new(InstallGuard {
        lock_file: lock,
        installing: false,
    });
    let sidecar = lease.clone();
    let maintenance = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    assert!(fs2::FileExt::try_lock_exclusive(&maintenance).is_err());
    drop(lease); // cancellation drops the caller; the blocking worker remains.
    assert!(fs2::FileExt::try_lock_exclusive(&maintenance).is_err());
    drop(sidecar);
    fs2::FileExt::try_lock_exclusive(&maintenance).unwrap();
}
