//! Exact direct-provider host-tool schemas.
use super::{
    MAX_BATCH_ASSET_RAW_BYTES, MAX_BATCH_ASSET_REQUESTS, MAX_BATCH_TEXT_ITEM_BYTES,
    MAX_BATCH_TEXT_REQUESTS,
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ReadToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl Default for ReadToolDef {
    fn default() -> Self {
        Self {
            name: "Read".to_string(),
            description: "Read the contents of a file at the given absolute path. Returns the file's text content.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute file path to read"
                    }
                },
                "required": ["file_path"]
            }),
        }
    }
}

/// Batch text reader for collecting several bounded evidence ranges in one
/// model round trip. `Read` remains available for compatibility and unusual
/// single-file cases.
#[derive(Debug, Clone, Serialize)]
pub struct ReadTextBatchToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl Default for ReadTextBatchToolDef {
    fn default() -> Self {
        Self {
            name: "ReadTextBatch".to_string(),
            description: format!(
                "Read up to {MAX_BATCH_TEXT_REQUESTS} bounded UTF-8 text ranges in one call. \
                 Prefer this over several sequential Read calls. Exact duplicate requests are \
                 returned once. Select ranges with 1-based start_line/end_line, or with offset \
                 (never both); use next_offset from a truncated result to continue."
            ),
            input_schema: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "requests": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": MAX_BATCH_TEXT_REQUESTS,
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "file_path": {
                                    "type": "string",
                                    "description": "Absolute path to a UTF-8 text file"
                                },
                                "offset": {
                                    "type": "integer",
                                    "minimum": 0,
                                    "description": "Optional UTF-8 byte offset. Mutually exclusive with start_line/end_line. Use a prior result's next_offset."
                                },
                                "start_line": {
                                    "type": "integer",
                                    "minimum": 1,
                                    "description": "Optional 1-based first line. Defaults to line 1 when only end_line is given."
                                },
                                "end_line": {
                                    "type": "integer",
                                    "minimum": 1,
                                    "description": "Optional inclusive 1-based last line. Requires line-range mode and must not precede start_line."
                                },
                                "max_bytes": {
                                    "type": "integer",
                                    "minimum": 1,
                                    "maximum": MAX_BATCH_TEXT_ITEM_BYTES,
                                    "description": "Maximum bytes to return for this range"
                                }
                            },
                            "required": ["file_path"]
                        }
                    }
                },
                "required": ["requests"]
            }),
        }
    }
}

/// Multimodal asset reader. Kept separate from `Read` so providers receive
/// image bytes as image content rather than accidentally decoding them as
/// UTF-8 text.
#[derive(Debug, Clone, Serialize)]
pub struct DocumentAssetToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl Default for DocumentAssetToolDef {
    fn default() -> Self {
        Self {
            name: "ReadDocumentAsset".to_string(),
            description: "Read an image referenced by a DocumentBundle and return it as visual input. Use the absolute path formed from the bundle's run root and the asset rel_path.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute path of a PNG, JPEG, GIF, or WebP document asset"
                    }
                },
                "required": ["file_path"]
            }),
        }
    }
}

/// Batch multimodal reader. Providers receive every unique image in one tool
/// result so the model can compare pages without one round trip per asset.
#[derive(Debug, Clone, Serialize)]
pub struct DocumentAssetsBatchToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl Default for DocumentAssetsBatchToolDef {
    fn default() -> Self {
        Self {
            name: "ReadDocumentAssetsBatch".to_string(),
            description: format!(
                "Read up to {MAX_BATCH_ASSET_REQUESTS} document images in one call. Prefer this \
                 for comparing tables, figures, or rendered pages. Exact duplicate files are \
                 attached once. Only PNG, JPEG, GIF, and WebP are accepted; aggregate raw image \
                 data is capped at {} MiB.",
                MAX_BATCH_ASSET_RAW_BYTES / 1024 / 1024
            ),
            input_schema: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "file_paths": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": MAX_BATCH_ASSET_REQUESTS,
                        "items": {
                            "type": "string",
                            "description": "Absolute path to a document image"
                        }
                    }
                },
                "required": ["file_paths"]
            }),
        }
    }
}

/// The Write tool schema sent to APIs. Confined to the run's artifact
/// directory by `write_file_for_tool`.
#[derive(Debug, Clone, Serialize)]
pub struct WriteToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl Default for WriteToolDef {
    fn default() -> Self {
        Self {
            name: "Write".to_string(),
            description: "Write a UTF-8 text file inside the run's artifact directory. Paths outside that directory are rejected.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Destination path. Either absolute (must be inside the artifact directory) or relative to it."
                    },
                    "content": {
                        "type": "string",
                        "description": "Full file content to write"
                    }
                },
                "required": ["file_path", "content"]
            }),
        }
    }
}
