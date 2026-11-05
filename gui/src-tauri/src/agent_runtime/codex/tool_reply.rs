//! Answers to dynamic tool calls. App Server waits for an answer to every call,
//! so one that can't be sent becomes a failed result the model can read.
use super::transport::{AppServerClient, RequestError};
use serde_json::{json, Value};

const UNDELIVERED: &str =
    "This tool's reply could not be sent, usually because it is too large. Ask for less text or a single image.";

impl AppServerClient {
    /// Returns whether App Server received `response` itself, rather than the
    /// failed result that stands in for it.
    pub async fn respond_to_tool_call(
        &self,
        request_id: Value,
        response: Value,
    ) -> Result<bool, RequestError> {
        if self
            .respond_to_server_request(request_id.clone(), Ok(response))
            .await
            .is_ok()
        {
            return Ok(true);
        }
        let failed = json!({
            "success": false,
            "contentItems": [{"type": "inputText", "text": UNDELIVERED}],
        });
        self.respond_to_server_request(request_id, Ok(failed))
            .await
            .map(|()| false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::research::TOOL_PAGE_IMAGE_BYTES;
    use tokio::io::{AsyncBufReadExt, BufReader};

    fn page(bytes: u64) -> Value {
        use base64::Engine as _;
        let image = vec![0u8; bytes as usize];
        let url = format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(image)
        );
        json!({"success":true,"contentItems":[
            {"type":"inputText","text":"{\"revisionId\":\"r\",\"page\":1}"},
            {"type":"inputImage","imageUrl":url},
        ]})
    }

    /// What App Server reads for one tool reply.
    async fn delivered(response: Value) -> (bool, Value) {
        let (client_io, server_io) = tokio::io::duplex(1024 * 1024);
        let (client_read, client_write) = tokio::io::split(client_io);
        let client = AppServerClient::from_io(1, client_read, client_write);
        let answered = client.respond_to_tool_call(json!(9), response);
        let mut line = String::new();
        let mut reader = BufReader::new(server_io);
        let (answered, read) = tokio::join!(answered, reader.read_line(&mut line));
        read.unwrap();
        (answered.unwrap(), serde_json::from_str(&line).unwrap())
    }

    #[tokio::test]
    async fn a_page_at_the_tool_limit_reaches_app_server() {
        let (own, frame) = delivered(page(TOOL_PAGE_IMAGE_BYTES)).await;
        assert!(own);
        assert_eq!(frame["id"], 9);
        assert_eq!(frame["result"]["success"], true);
        assert_eq!(frame["result"]["contentItems"][1]["type"], "inputImage");
    }

    #[tokio::test]
    async fn a_reply_too_large_for_a_frame_is_answered_as_a_failed_tool_call() {
        let (own, frame) = delivered(page(8 * 1024 * 1024)).await;
        assert!(!own);
        assert_eq!(frame["id"], 9);
        assert_eq!(frame["result"]["success"], false);
        assert_eq!(frame["result"]["contentItems"][0]["type"], "inputText");
    }
}
