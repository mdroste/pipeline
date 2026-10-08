use super::*;
use std::future::Future;
use std::task::{Context, Poll, Waker};
#[tokio::test]
async fn direct_write_settles_in_one_poll_before_finalization_or_retry() {
    let root = tempfile::tempdir().unwrap();
    let access = ToolAccess::new(&[], Some(&root.path().to_string_lossy()));
    let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::CliEvents);
    let input = serde_json::json!({"file_path":"result.md","content":"first attempt"});
    let mut budget = ToolBudget::default();
    {
        let mut future = Box::pin(execute_tool(
            &bus,
            "Write",
            &input,
            "test",
            0,
            &mut budget,
            &access,
            &OPENAI_MEDIA_POLICY,
        ));
        let result = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()));
        assert!(matches!(result, Poll::Ready(ToolResult::Text(_))));
    }
    let path = root.path().join("result.md");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "first attempt");
    std::fs::write(&path, "next attempt").unwrap();
    tokio::task::yield_now().await;
    assert_eq!(std::fs::read_to_string(path).unwrap(), "next attempt");
    let input = serde_json::json!({"file_path":"cancelled.md","content":"never run"});
    drop(execute_tool(
        &bus,
        "Write",
        &input,
        "test",
        0,
        &mut budget,
        &access,
        &OPENAI_MEDIA_POLICY,
    ));
    tokio::task::yield_now().await;
    assert!(!root.path().join("cancelled.md").exists());
}
