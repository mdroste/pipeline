//! Waiting for a place to run, and the log pages a running job is read through.
use super::*;

async fn stop_after_a_moment(id: &str) {
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancelled_executions().lock().unwrap().insert(id.into());
}

#[tokio::test]
async fn a_stopped_job_stops_waiting_for_a_place_to_run() {
    let id = "execution_stopped_while_full";
    let full = Semaphore::new(0);
    let (slot, ()) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(wait_for_slot(id, &full), stop_after_a_moment(id))
    })
    .await
    .expect("a stopped job does not wait for a slot");
    assert!(slot.is_none());
    cancelled_executions().lock().unwrap().remove(id);
}

#[tokio::test]
async fn a_job_that_was_not_stopped_takes_a_free_place() {
    let open = Semaphore::new(1);
    assert!(wait_for_slot("execution_not_stopped", &open)
        .await
        .is_some());
    assert_eq!(open.available_permits(), 1);
}

#[test]
fn a_page_never_ends_inside_a_character() {
    let cases: [(&str, usize); 5] = [("ab", 2), ("aé", 3), ("a€", 4), ("a😀", 5), ("€€", 6)];
    for (text, whole) in cases {
        let bytes = text.as_bytes();
        assert_eq!(page_end(bytes, 0, whole), whole, "{text}");
        for limit in 1..whole {
            let end = page_end(bytes, 0, limit);
            assert!(text.is_char_boundary(end), "{text} at {limit}");
            assert!(limit - end < 4);
        }
    }
}

#[test]
fn output_that_is_not_text_is_paged_where_it_falls() {
    assert_eq!(page_end(&[b'a', 0xFF], 0, 2), 2);
    assert_eq!(page_end(&[0x80, 0x80, 0x80, 0x80], 0, 4), 4);
    assert_eq!(page_end(&[0xC0, 0xAF], 0, 2), 2);
}
