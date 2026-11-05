//! The window's life after a turn, with a stand-in for its driver process.
use super::*;
use tokio::sync::Mutex as TestLock;

/// The window is one process-wide slot, so these tests take turns.
static WINDOW: TestLock<()> = TestLock::const_new(());

async fn open_window(session: &str) {
    let mut child = tokio::process::Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    crate::commands::lifecycle::register_independent_child_pid(pid);
    let stdin = child.stdin.take().unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    *process().lock().await = Some(Process {
        session: session.into(),
        child,
        stdin,
        stdout,
        pid,
    });
}

async fn window_is_open() -> bool {
    process().lock().await.is_some()
}

#[tokio::test]
async fn the_window_outlives_its_turn_and_then_closes() {
    let _window = WINDOW.lock().await;
    open_window("signing-in").await;
    close_after(Duration::from_millis(150)).await;
    assert!(
        window_is_open().await,
        "the researcher still needs the window"
    );
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(!window_is_open().await, "an idle window closes by itself");
}

#[tokio::test]
async fn a_later_turn_keeps_the_window_it_is_using() {
    let _window = WINDOW.lock().await;
    open_window("signing-in").await;
    close_after(Duration::from_millis(100)).await;
    // What a browser call does on arrival.
    CLOSE_GENERATION.fetch_add(1, Ordering::AcqRel);
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(window_is_open().await, "the next turn is using it");
    stop().await;
}

#[tokio::test]
async fn only_the_latest_turn_end_decides_when_the_window_closes() {
    let _window = WINDOW.lock().await;
    open_window("signing-in").await;
    close_after(Duration::from_millis(100)).await;
    close_after(Duration::from_secs(30)).await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(window_is_open().await);
    stop().await;
    assert!(!window_is_open().await);
}

#[test]
fn a_snapshot_tells_the_model_how_to_hand_over_a_sign_in() {
    let snapshot = with_sign_in_note(json!({"url":"https://example.org/login"}));
    assert_eq!(snapshot["url"], "https://example.org/login");
    let note = snapshot["signIn"].as_str().unwrap();
    assert!(note.contains("10 minutes"), "{note}");
}

/// A driver that answers each command only after `delay` seconds, echoing
/// which command it answered.
async fn slow_window(delay: &str) {
    let script = format!(
        "while read -r line; do sleep {delay}; printf '%s\\n' \"{{\\\"ok\\\":true,\\\"result\\\":$line}}\"; done"
    );
    let mut child = tokio::process::Command::new("/bin/sh")
        .args(["-c", &script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    crate::commands::lifecycle::register_independent_child_pid(pid);
    let stdin = child.stdin.take().unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    *process().lock().await = Some(Process {
        session: "signing-in".into(),
        child,
        stdin,
        stdout,
        pid,
    });
}

#[tokio::test]
async fn a_dropped_call_can_not_leave_the_driver_a_reply_behind() {
    let _window = WINDOW.lock().await;
    slow_window("0.2").await;
    // Stop lands after the command is written and before its late reply.
    {
        let mut owner = process().lock().await;
        let dropped = tokio::time::timeout(
            Duration::from_millis(50),
            request(&mut owner, "signing-in", json!({"id":"stopped"})),
        )
        .await;
        assert!(dropped.is_err());
    }
    assert!(
        !window_is_open().await,
        "a process owing a reply must not be reused"
    );

    // Nothing the next command sees can be the stopped command's reply.
    slow_window("0").await;
    let mut owner = process().lock().await;
    for id in ["first", "second"] {
        let reply = request(&mut owner, "signing-in", json!({ "id": id }))
            .await
            .unwrap();
        assert_eq!(reply["result"]["id"], id);
    }
    drop(owner);
    stop().await;
}

#[tokio::test]
async fn a_finished_exchange_keeps_the_window() {
    let _window = WINDOW.lock().await;
    slow_window("0").await;
    let mut owner = process().lock().await;
    request(&mut owner, "signing-in", json!({"id":"one"}))
        .await
        .unwrap();
    assert!(owner.is_some());
    drop(owner);
    stop().await;
}

#[tokio::test]
async fn the_end_of_a_turn_tells_a_window_that_answers_and_closes_one_that_does_not() {
    let _window = WINDOW.lock().await;
    open_window("signing-in").await;
    release_window(Duration::from_secs(5)).await;
    assert!(window_is_open().await, "an answering driver keeps its page");
    stop().await;

    slow_window("30").await;
    release_window(Duration::from_millis(50)).await;
    assert!(!window_is_open().await, "a silent driver can't be trusted");
}

#[tokio::test]
async fn a_model_can_not_lift_the_turn_limit_itself() {
    let error = call("signing-in", json!({"action":RELEASE}))
        .await
        .unwrap_err();
    assert!(error.message.contains("Unsupported"), "{}", error.message);
}

#[test]
fn only_a_closed_window_starts_another() {
    // A closed popup reaches the model as an ordinary failure.
    let popup = "page.goto: Target page, context or browser has been closed";
    assert!(!refused_by_closed_window(
        &json!({"ok":false,"error":popup,"windowClosed":false})
    ));
    assert!(refused_by_closed_window(
        &json!({"ok":false,"error":"The browser window was closed","windowClosed":true})
    ));
    assert!(!refused_by_closed_window(&json!({"ok":true,"result":{}})));
}

#[test]
fn the_note_states_the_real_wait() {
    assert_eq!(OPEN_AFTER_TURN.as_secs(), 10 * 60);
}
