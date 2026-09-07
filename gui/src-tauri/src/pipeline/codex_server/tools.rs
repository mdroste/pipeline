use crate::pipeline::api_common::{self, ToolAccess, ToolBudget, ToolResult};
use serde_json::{json, Value};
use std::collections::HashSet;

pub(super) struct HostTools {
    enabled: HashSet<String>,
    budget: ToolBudget,
    access: ToolAccess,
}

impl HostTools {
    pub fn new(tools: &[&str], roots: &[&str], write: Option<&str>) -> Result<Self, String> {
        let mut enabled = HashSet::new();
        for tool in tools {
            match *tool {
                "Read" => {
                    enabled.extend(
                        [
                            "Read",
                            "ReadTextBatch",
                            "ReadDocumentAsset",
                            "ReadDocumentAssetsBatch",
                            "ReadPdfPage",
                        ]
                        .map(str::to_string),
                    );
                }
                "ReadTextBatch" | "ReadDocumentAsset" | "ReadDocumentAssetsBatch" => {
                    enabled.insert(tool.to_string());
                }
                "Write" | "Edit" if write.is_some() => {
                    enabled.insert("Write".into());
                }
                "WebSearch" => {}
                _ => {
                    return Err(format!(
                        "[codex-capability] Unsupported Workflow tool {tool}"
                    ))
                }
            }
        }
        Ok(Self {
            enabled,
            budget: ToolBudget::default(),
            access: ToolAccess::new(roots, write),
        })
    }

    pub fn declarations(&self) -> Vec<Value> {
        let mut definitions = vec![
            serde_json::to_value(api_common::ReadToolDef::default()).unwrap(),
            serde_json::to_value(api_common::ReadTextBatchToolDef::default()).unwrap(),
            serde_json::to_value(api_common::DocumentAssetToolDef::default()).unwrap(),
            serde_json::to_value(api_common::DocumentAssetsBatchToolDef::default()).unwrap(),
            serde_json::to_value(api_common::WriteToolDef::default()).unwrap(),
            json!({"name":"ReadPdfPage","description":"Read one numbered PDF page as an image. Start at page 1; the result says whether another page exists.","input_schema":{"type":"object","properties":{"file_path":{"type":"string"},"page":{"type":"integer","minimum":1}},"required":["file_path","page"],"additionalProperties":false}}),
        ];
        definitions.retain(|d| {
            self.enabled
                .contains(d["name"].as_str().unwrap_or_default())
        });
        definitions.into_iter().map(|d| json!({"type":"function","name":d["name"],"description":d["description"],"inputSchema":d["input_schema"]})).collect()
    }

    pub async fn execute(
        &mut self,
        app: &crate::emit::EventBus,
        name: &str,
        arguments: &Value,
    ) -> Value {
        if !self.enabled.contains(name) || !arguments.is_object() {
            return error("Tool is not enabled for this Workflow invocation");
        }
        convert(
            api_common::execute_native_tool(app, name, arguments, &mut self.budget, &self.access)
                .await,
        )
    }
}

pub(super) fn error(message: &str) -> Value {
    json!({"success":false,"contentItems":[{"type":"inputText","text":message}]})
}

fn convert(result: ToolResult) -> Value {
    let text = |text: String| json!({"type":"inputText","text":text});
    let image = |data: String, media_type: String| json!({"type":"inputImage","imageUrl":format!("data:{media_type};base64,{data}")});
    let content = match result {
        ToolResult::Error(message) => return error(&message),
        ToolResult::Text(value) => vec![text(value)],
        ToolResult::ImageBase64 { data, media_type } => vec![image(data, media_type)],
        ToolResult::ImageBatch { metadata, images } => {
            let mut items = vec![text(metadata)];
            items.extend(images.into_iter().map(|i| image(i.data, i.media_type)));
            items
        }
        ToolResult::PdfBase64(_) => return error("Use ReadPdfPage for PDF visual input"),
    };
    let value = json!({"success":true,"contentItems":content});
    if serde_json::to_vec(&value).map_or(true, |b| b.len() > 7 * 1024 * 1024) {
        error("Tool result exceeds the App Server frame budget; request a smaller text range or one image")
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_tool_surface_and_multimodal_results() {
        assert!(HostTools::new(&[], &[], None)
            .unwrap()
            .declarations()
            .is_empty());
        assert!(HostTools::new(&["Bash"], &[], None).is_err());
        assert!(HostTools::new(&["Write"], &[], None).is_err());
        let result = convert(ToolResult::ImageBase64 {
            data: "AA==".into(),
            media_type: "image/png".into(),
        });
        assert_eq!(result["contentItems"][0]["type"], "inputImage");
    }
    #[tokio::test]
    async fn host_reads_and_writes_cannot_cross_producers() {
        let own = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let secret = other.path().join("secret.txt");
        std::fs::write(&secret, "sibling answer").unwrap();
        let mut tools = HostTools::new(&["Read", "Write"], &[], own.path().to_str()).unwrap();
        let app: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        let result = tools
            .execute(&app, "Read", &json!({"file_path":secret}))
            .await;
        assert_eq!(result["success"], false);
        let result = tools
            .execute(
                &app,
                "Write",
                &json!({"file_path":secret,"content":"replace"}),
            )
            .await;
        assert_eq!(result["success"], false);
        assert_eq!(std::fs::read_to_string(secret).unwrap(), "sibling answer");
    }
}
