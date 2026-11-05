//! Conversation error mapping for stateless public web transport.
//! Admission and receipts remain owned by conversation_tools.
use super::store::{WorkbenchError, WorkbenchResult};
use serde_json::Value;

pub(crate) async fn search(query: &str) -> WorkbenchResult<Value> {
    crate::public_web::search(query)
        .await
        .map_err(WorkbenchError::invalid)
}
pub(crate) async fn fetch(url: &str) -> WorkbenchResult<Value> {
    crate::public_web::fetch(url)
        .await
        .map_err(WorkbenchError::invalid)
}
pub(crate) async fn download(
    url: &str,
    maximum: usize,
) -> WorkbenchResult<crate::public_web::Download> {
    crate::public_web::download(url, maximum)
        .await
        .map_err(WorkbenchError::invalid)
}
