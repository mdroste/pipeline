use super::*;

#[tokio::test]
async fn overlapping_reads_share_work_even_if_the_first_caller_is_cancelled() {
    let flight = Arc::new(ReadFlight::new());
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let first_flight = flight.clone();
    let first = tokio::spawn(async move {
        first_flight
            .get(move || {
                let _ = started.send(());
                wait.recv().unwrap();
                Ok(41)
            })
            .await
    });
    ready.await.unwrap();
    let joined = flight.get(|| panic!("overlapping read must share the existing scan"));
    tokio::pin!(joined);
    tokio::select! { biased;
        _ = &mut joined => panic!("scan has not finished"),
        _ = tokio::task::yield_now() => (),
    }
    first.abort();
    release.send(()).unwrap();
    assert_eq!(*joined.await.unwrap(), 41);
    assert_eq!(
        *flight.get(|| Ok(42)).await.unwrap(),
        42,
        "later reads must not reuse stale results"
    );
}

#[tokio::test]
async fn refresh_after_a_write_waits_for_a_new_scan_without_overlapping_the_old_one() {
    let flight = Arc::new(ReadFlight::new());
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let first_flight = flight.clone();
    let first = tokio::spawn(async move {
        first_flight
            .get(move || {
                let _ = started.send(());
                wait.recv().unwrap();
                Ok(1)
            })
            .await
    });
    ready.await.unwrap();
    flight.invalidate();
    let refreshed = flight.get(|| Ok(2));
    tokio::pin!(refreshed);
    tokio::select! { biased;
        _ = &mut refreshed => panic!("new scan must wait for the old worker"),
        _ = tokio::task::yield_now() => (),
    }
    release.send(()).unwrap();
    assert_eq!(*first.await.unwrap().unwrap(), 1);
    assert_eq!(*refreshed.await.unwrap(), 2);
}

#[tokio::test]
async fn failures_and_panics_release_the_scan_slot() {
    let flight = ReadFlight::<usize>::new();
    assert_eq!(
        flight
            .get(|| Err("disk unavailable".into()))
            .await
            .unwrap_err(),
        "disk unavailable"
    );
    assert!(flight.get(|| panic!("fixture worker panic")).await.is_err());
    assert_eq!(*flight.get(|| Ok(7)).await.unwrap(), 7);
}
