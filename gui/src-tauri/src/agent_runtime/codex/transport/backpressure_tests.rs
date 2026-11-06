use super::*;
#[tokio::test]
async fn oversized_decision_leaves_claim_available_for_a_shorter_answer() {
    let (io, _peer) = tokio::io::duplex(4096);
    let (read, write) = tokio::io::split(io);
    let client = AppServerClient::from_io(1, read, write);
    let mut pending = true;
    assert!(client
        .respond_after_claim(json!(1), Ok(json!("x".repeat(FRAME_LIMIT))), || {
            pending = false;
            Ok(())
        })
        .await
        .is_err());
    assert!(pending);
    client
        .respond_after_claim(json!(1), Ok(json!("short")), || {
            pending = false;
            Ok(())
        })
        .await
        .unwrap();
    assert!(!pending);
}
#[tokio::test]
async fn full_queue_bounds_close_and_response_and_cleans_requests() {
    let (io, _stalled_peer) = tokio::io::duplex(1);
    let (read, write) = tokio::io::split(io);
    let client = AppServerClient::from_io(1, read, write);
    client
        .outbound
        .send(Outbound::Frame(vec![0; 8]))
        .await
        .unwrap();
    tokio::task::yield_now().await;
    while client
        .outbound
        .try_send(Outbound::Frame(vec![0; 8]))
        .is_ok()
    {}
    let close = client.close_writer();
    let mut pending = true;
    let response = client.respond_after_claim(json!(1), Ok(json!({})), || {
        pending = false;
        Ok(())
    });
    let request = client.request("fixture", json!({}), Duration::from_secs(1));
    let (close, response, request) = tokio::time::timeout(Duration::from_secs(4), async {
        tokio::join!(close, response, request)
    })
    .await
    .expect("All writes must be bounded");
    assert!(close.is_err() && response.is_err() && request.is_err());
    assert!(pending);
    assert!(client.shared.pending.lock().unwrap().is_empty());
    let request = client.request("fixture", json!({}), Duration::from_secs(60));
    assert!(tokio::time::timeout(Duration::from_millis(10), request)
        .await
        .is_err());
    assert!(client.shared.pending.lock().unwrap().is_empty());
}
