//! Provider-portable schemas for Pipeline's stock orientation prompts.
//!
//! The prompts describe the analysis to perform; these schemas are the single
//! source of truth for the resulting artifact shape. Keep every declared
//! object property required so providers with strict structured output can use
//! constrained decoding. Empty strings and zero page values represent source
//! metadata that is unavailable.

use serde_json::{json, Value};

pub fn schema_for_prompt_name(name: &str) -> Option<Value> {
    match name.trim_end_matches(".md") {
        "orientation" => Some(paper_schema()),
        "orientation_generic" => Some(generic_schema()),
        "orientation_grant" => Some(generic_schema()),
        "orientation_folder" => Some(folder_schema()),
        _ => None,
    }
}

pub fn paper_schema() -> Value {
    json!({
        "type": "object",
        "title": "Academic paper orientation map",
        "description": "A factual inventory of the paper for downstream review steps.",
        "required": [
            "metadata", "sections", "formal_results", "tables_figures", "notation",
            "stated_contribution", "key_references", "extraction_quality_notes"
        ],
        "properties": {
            "metadata": {
                "type": "object",
                "description": "Bibliographic and structural facts reported by the paper or extraction.",
                "required": [
                    "title", "authors", "date", "paper_type", "page_count",
                    "has_appendix", "has_online_appendix"
                ],
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Paper title; empty if unavailable."
                    },
                    "authors": {
                        "type": "array",
                        "items": {"type": "string"}
                    },
                    "date": {
                        "type": "string",
                        "description": "Date as shown; empty if unavailable."
                    },
                    "paper_type": {
                        "type": "string",
                        "description": "Theory when formal results dominate, empirical when data analysis dominates, and mixed otherwise.",
                        "enum": ["theory", "empirical", "mixed"]
                    },
                    "page_count": {
                        "type": "integer",
                        "description": "Page count; 0 if unavailable."
                    },
                    "has_appendix": {"type": "boolean"},
                    "has_online_appendix": {"type": "boolean"}
                }
            },
            "sections": {
                "type": "array",
                "maxItems": 100,
                "description": "Every major section in document order.",
                "items": {
                    "type": "object",
                    "required": ["number", "title", "page_start", "page_end"],
                    "properties": {
                        "number": {"type": "string", "description": "Empty if unnumbered."},
                        "title": {"type": "string"},
                        "page_start": {"type": "integer", "description": "First page; 0 if unavailable."},
                        "page_end": {"type": "integer", "description": "Last page; 0 if unavailable."}
                    }
                }
            },
            "formal_results": {
                "type": "array",
                "maxItems": 150,
                "description": "Every explicit proposition, theorem, lemma, corollary, definition, and assumption.",
                "items": {
                    "type": "object",
                    "required": ["kind", "number", "page", "summary", "proof_location"],
                    "properties": {
                        "kind": {"type": "string"},
                        "number": {"type": "string"},
                        "page": {"type": "integer", "description": "Statement page; 0 if unavailable."},
                        "summary": {"type": "string"},
                        "proof_location": {"type": "string", "description": "Empty if no proof location is given."}
                    }
                }
            },
            "tables_figures": {
                "type": "array",
                "maxItems": 100,
                "description": "Every numbered or substantively important table and figure.",
                "items": {
                    "type": "object",
                    "required": ["kind", "number", "page", "caption_summary", "what_it_shows"],
                    "properties": {
                        "kind": {"type": "string"},
                        "number": {"type": "string"},
                        "page": {"type": "integer", "description": "Item page; 0 if unavailable."},
                        "caption_summary": {"type": "string"},
                        "what_it_shows": {"type": "string", "description": "Neutral description of the result or relationship displayed."}
                    }
                }
            },
            "notation": {
                "type": "array",
                "maxItems": 200,
                "description": "Every symbol explicitly defined by the paper.",
                "items": {
                    "type": "object",
                    "required": ["symbol", "definition", "page_introduced"],
                    "properties": {
                        "symbol": {"type": "string"},
                        "definition": {"type": "string", "description": "Definition given by the paper; never infer one."},
                        "page_introduced": {"type": "integer", "description": "First definition page; 0 if unavailable."}
                    }
                }
            },
            "stated_contribution": {
                "type": "string",
                "description": "The paper's own contribution claim, quoted or closely paraphrased from the introduction."
            },
            "key_references": {
                "type": "array",
                "maxItems": 50,
                "description": "References central to the paper's stated positioning.",
                "items": {"type": "string"}
            },
            "extraction_quality_notes": {
                "type": "array",
                "maxItems": 50,
                "description": "Garbled, incomplete, truncated, or otherwise unreliable portions of the extracted input.",
                "items": {
                    "type": "object",
                    "required": ["page_range", "description"],
                    "properties": {
                        "page_range": {"type": "string", "description": "Affected page or page range, or global."},
                        "description": {"type": "string", "description": "Specific extraction limitation without treating it as a paper error."}
                    }
                }
            }
        }
    })
}

pub fn generic_schema() -> Value {
    json!({
        "type": "object",
        "title": "General input survey",
        "description": "A factual, domain-neutral map of the supplied input.",
        "required": [
            "overview", "structure", "key_elements", "conventions",
            "cross_references", "quality_notes"
        ],
        "properties": {
            "overview": {
                "type": "string",
                "description": "One-paragraph summary of what the input is and what it is for."
            },
            "structure": {
                "type": "array",
                "description": "Every major part of the input in a useful reading order.",
                "items": {
                    "type": "object",
                    "required": ["part", "role", "location"],
                    "properties": {
                        "part": {"type": "string", "description": "Name of the major part."},
                        "role": {"type": "string", "description": "What the part does or covers."},
                        "location": {"type": "string", "description": "Precise file, section, or page location."}
                    }
                }
            },
            "key_elements": {
                "type": "array",
                "description": "Important definitions, functions, claims, datasets, or other domain-relevant elements.",
                "items": {
                    "type": "object",
                    "required": ["name", "what_it_is", "location"],
                    "properties": {
                        "name": {"type": "string", "description": "Element name."},
                        "what_it_is": {"type": "string", "description": "Neutral explanation of the element."},
                        "location": {"type": "string", "description": "Precise file, section, or page location."}
                    }
                }
            },
            "conventions": {
                "type": "array",
                "description": "Notable notation, naming, layout, or formatting conventions.",
                "items": {"type": "string"}
            },
            "cross_references": {
                "type": "array",
                "description": "Places where one part depends on or refers to another.",
                "items": {"type": "string"}
            },
            "quality_notes": {
                "type": "array",
                "description": "Gaps, garbled text, missing pieces, or other input-quality limitations.",
                "items": {"type": "string"}
            }
        }
    })
}

pub fn folder_schema() -> Value {
    json!({
        "type": "object",
        "title": "Folder survey",
        "description": "A factual map of a source tree based on files actually inspected.",
        "required": [
            "overview", "components", "entry_points", "key_files", "conventions",
            "cross_references", "quality_notes"
        ],
        "properties": {
            "overview": {
                "type": "string",
                "description": "One-paragraph summary of what the folder contains and what it is for."
            },
            "components": {
                "type": "array",
                "description": "Every major directory and top-level file.",
                "items": {
                    "type": "object",
                    "required": ["path", "role", "notes"],
                    "properties": {
                        "path": {"type": "string", "description": "Exact relative path from the inventory root."},
                        "role": {"type": "string", "description": "What the component does or covers."},
                        "notes": {"type": "string", "description": "Relevant languages, frameworks, or data formats."}
                    }
                }
            },
            "entry_points": {
                "type": "array",
                "description": "Where execution, the build, or reading starts.",
                "items": {
                    "type": "object",
                    "required": ["path", "purpose"],
                    "properties": {
                        "path": {"type": "string", "description": "Exact relative path."},
                        "purpose": {"type": "string", "description": "Why this is an entry point."}
                    }
                }
            },
            "key_files": {
                "type": "array",
                "description": "Central files that were opened or identified as central by project documentation.",
                "items": {
                    "type": "object",
                    "required": ["path", "what_it_is"],
                    "properties": {
                        "path": {"type": "string", "description": "Exact relative path."},
                        "what_it_is": {"type": "string", "description": "Neutral description of the file's role."}
                    }
                }
            },
            "conventions": {
                "type": "array",
                "description": "Notable naming, layout, or formatting conventions.",
                "items": {"type": "string"}
            },
            "cross_references": {
                "type": "array",
                "description": "Imports, includes, data reads, generated outputs, or other dependencies between files.",
                "items": {"type": "string"}
            },
            "quality_notes": {
                "type": "array",
                "description": "Referenced-but-missing, empty, unreadable, or truncated material.",
                "items": {"type": "string"}
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_all_object_properties_required(schema: &Value, path: &str) {
        match schema.get("type").and_then(Value::as_str) {
            Some("object") => {
                let properties = schema
                    .get("properties")
                    .and_then(Value::as_object)
                    .unwrap_or_else(|| panic!("{path} has no properties"));
                let required = schema
                    .get("required")
                    .and_then(Value::as_array)
                    .unwrap_or_else(|| panic!("{path} has no required array"));
                for (key, child) in properties {
                    assert!(
                        required.iter().any(|value| value.as_str() == Some(key)),
                        "{path}.{key} is optional"
                    );
                    assert_all_object_properties_required(child, &format!("{path}.{key}"));
                }
            }
            Some("array") => {
                let items = schema
                    .get("items")
                    .unwrap_or_else(|| panic!("{path} has no item schema"));
                assert_all_object_properties_required(items, &format!("{path}[]"));
            }
            _ => {}
        }
    }

    #[test]
    fn stock_schemas_are_portable_and_strict_capable() {
        for schema in [paper_schema(), generic_schema(), folder_schema()] {
            crate::pipeline::structured::provider_schema(&schema).unwrap();
            assert_all_object_properties_required(&schema, "$");
        }
    }

    #[test]
    fn prompt_names_resolve_to_their_matching_contracts() {
        assert_eq!(schema_for_prompt_name("orientation"), Some(paper_schema()));
        assert_eq!(
            schema_for_prompt_name("orientation_generic.md"),
            Some(generic_schema())
        );
        assert_eq!(
            schema_for_prompt_name("orientation_grant"),
            Some(generic_schema())
        );
        assert_eq!(
            schema_for_prompt_name("orientation_folder"),
            Some(folder_schema())
        );
        assert_eq!(schema_for_prompt_name("contribution"), None);
    }

    #[test]
    fn stock_prompts_leave_serialization_to_the_schema() {
        for name in [
            "orientation",
            "orientation_generic",
            "orientation_grant",
            "orientation_folder",
        ] {
            let prompt = crate::prompts::compiled_default(name).unwrap();
            assert!(!prompt.contains("JSON object"), "{name}");
            assert!(!prompt.contains("Return ONLY"), "{name}");
            assert!(!prompt.contains("markdown fences"), "{name}");
        }
        let auto = crate::auto_review::orientation_prompt();
        assert!(!auto.contains("JSON object"));
        assert!(!auto.contains("Return ONLY"));
        assert!(!auto.contains("markdown fences"));
    }
}
