//! Revision-aware sent-draft acknowledgement.
use crate::workbench::store::{Store, UpdateSessionRequest, WorkbenchResult};
/// Clears a sent message from the saved draft once its turn is accepted. A draft
/// that has moved on is the researcher's newer text and stays. A title or
/// settings write can win the revision, so a conflict gets one more attempt:
/// a sent message left as the draft could come back and be sent twice.
pub(crate) fn clear_sent_draft(
    store: &Store,
    session_id: &str,
    sent: &str,
    submission_id: &str,
) -> WorkbenchResult<bool> {
    clear_sent_draft_with(store, session_id, sent, submission_id, |request| {
        store.update_session(request).map(|_| ())
    })
}

pub(crate) fn clear_sent_draft_with(
    store: &Store,
    session_id: &str,
    sent: &str,
    submission_id: &str,
    mut write: impl FnMut(UpdateSessionRequest) -> WorkbenchResult<()>,
) -> WorkbenchResult<bool> {
    for attempt in 0..2 {
        let snapshot = store.session_snapshot(session_id)?;
        if snapshot.session.draft.trim() != sent.trim() {
            return Ok(false);
        }
        match write(UpdateSessionRequest {
            session_id: session_id.to_string(),
            expected_revision: snapshot.session.revision,
            operation_id: format!("send-ack-{submission_id}"),
            title: None,
            draft: Some(String::new()),
            overrides: None,
            archived: None,
            preset_id: None,
            paper_id: None,
            clear_paper: None,
        }) {
            Ok(()) => return Ok(true),
            Err(error) if error.code == "storage_conflict" && attempt == 0 => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}
