use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

fn resources() -> Resources {
    // Independent admissions keep these tests isolated from other runtime tests.
    Resources {
        database: Box::leak(Box::new(Semaphore::new(4))),
        files: Box::leak(Box::new(Semaphore::new(1))),
        gate: Box::leak(Box::new(RwLock::new(()))),
    }
}

#[tokio::test]
async fn file_work_releases_database_capacity_but_keeps_protection_after_cancellation() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let resources = resources();
    let (db, files, gate) = (resources.database, resources.files, resources.gate);
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let committed = Arc::new(AtomicBool::new(false));
    let finished = committed.clone();
    let task = tokio::spawn(run_with(
        resources,
        move || Ok(store),
        |_| Ok(()),
        move |_| {
            let _ = started.send(());
            wait.recv().unwrap();
            Ok(())
        },
        move |_, _| {
            finished.store(true, Ordering::SeqCst);
            Ok(())
        },
    ));
    ready.await.unwrap();
    assert_eq!(db.available_permits(), 4);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), database(db, || Ok(7)))
            .await
            .unwrap()
            .unwrap(),
        7
    );
    task.abort();
    let _ = task.await;
    assert!(
        gate.try_write().is_err(),
        "maintenance must wait for the actual file worker"
    );
    assert_eq!(files.available_permits(), 0);
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if files.available_permits() == 1 && gate.try_write().is_ok() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!committed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn errors_and_success_release_all_admission_and_commit_under_the_gate() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let resources = resources();
    let (db, files, gate) = (resources.database, resources.files, resources.gate);
    let first = store.clone();
    let result: WorkbenchResult<()> = run_with(
        resources,
        move || Ok(first),
        |_| Ok(()),
        |_| Err(WorkbenchError::invalid("scan failed")),
        |_, _: ()| panic!("failed scan cannot commit"),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(db.available_permits(), 4);
    assert_eq!(files.available_permits(), 1);
    assert!(gate.try_write().is_ok());
    let value = run_with(
        Resources {
            database: db,
            files,
            gate,
        },
        move || Ok(store),
        |_| Ok(3),
        |value| Ok(value + 1),
        move |_, value| {
            assert!(gate.try_write().is_err());
            assert_eq!(db.available_permits(), 3);
            Ok(value)
        },
    )
    .await
    .unwrap();
    assert_eq!(value, 4);
    assert!(gate.try_write().is_ok());
}
