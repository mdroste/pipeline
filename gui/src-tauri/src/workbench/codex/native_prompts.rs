//! Read-only inspection of model defaults. Cache text is reference material,
//! never imported into the runtime configuration or a conversation implicitly.
use crate::workbench::research::InstructionSection;
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_CACHE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePromptCatalog {
    pub installed_version: Option<String>,
    pub sources: Vec<NativePromptSource>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePromptSource {
    pub path: String,
    pub origin: String,
    pub client_version: Option<String>,
    pub fetched_at: Option<String>,
    pub models: Vec<NativeModelPrompt>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeModelPrompt {
    pub model: String,
    pub template: String,
    pub template_field: String,
    pub sections: Vec<InstructionSection>,
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn collect_sections(value: &Value, path: &str, sections: &mut Vec<InstructionSection>) {
    match value {
        Value::String(text) if !text.is_empty() => sections.push(InstructionSection {
            id: path.into(),
            label: path.into(),
            text: text.clone(),
        }),
        Value::Object(fields) => {
            for (key, child) in fields {
                collect_sections(child, &format!("{path}.{key}"), sections);
            }
        }
        _ => {}
    }
}

fn read_source(path: &Path, origin: &str) -> Result<Option<NativePromptSource>, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(format!("Could not read {}", path.display())),
    };
    if !file
        .metadata()
        .map_err(|_| "Cannot inspect model cache")?
        .is_file()
    {
        return Err(format!(
            "Model cache is not a regular file: {}",
            path.display()
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_CACHE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| format!("Could not read {}", path.display()))?;
    if bytes.len() as u64 > MAX_CACHE_BYTES {
        return Err(format!("Model cache exceeds 16 MiB: {}", path.display()));
    }
    let cache: Value = serde_json::from_slice(&bytes)
        .map_err(|_| format!("Model cache is not valid JSON: {}", path.display()))?;
    let records = cache
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Unrecognized model cache format: {}", path.display()))?;
    let mut models = Vec::new();
    for record in records {
        let Some(model) = optional_string(record, "slug") else {
            continue;
        };
        let messages = &record["model_messages"];
        let (template, template_field) = if let Some(text) = messages["instructions_template"]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            (text, "model_messages.instructions_template")
        } else if let Some(text) = record["base_instructions"]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            (text, "base_instructions")
        } else {
            continue;
        };
        let mut sections = Vec::new();
        collect_sections(messages, "model_messages", &mut sections);
        // Legacy metadata can carry both a base fallback and a newer template.
        if template_field != "base_instructions" {
            collect_sections(
                &record["base_instructions"],
                "base_instructions",
                &mut sections,
            );
        }
        sections.retain(|section| section.id != template_field);
        models.push(NativeModelPrompt {
            model,
            template: template.into(),
            template_field: template_field.into(),
            sections,
        });
    }
    if models.is_empty() {
        return Err(format!(
            "No supported base prompt templates found in {}",
            path.display()
        ));
    }
    Ok(Some(NativePromptSource {
        path: path.display().to_string(),
        origin: origin.into(),
        client_version: optional_string(&cache, "client_version"),
        fetched_at: optional_string(&cache, "fetched_at"),
        models,
    }))
}

fn read_catalog(pipeline_home: &Path, standard_home: Option<&Path>) -> NativePromptCatalog {
    let mut catalog = NativePromptCatalog {
        installed_version: None,
        sources: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut candidates = vec![(pipeline_home.join("models_cache.json"), "pipeline")];
    if let Some(home) = standard_home.filter(|home| *home != pipeline_home) {
        candidates.push((home.join("models_cache.json"), "codex"));
    }
    for (path, origin) in candidates {
        match read_source(&path, origin) {
            Ok(Some(source)) => catalog.sources.push(source),
            Ok(None) => {}
            Err(message) => catalog.diagnostics.push(message),
        }
    }
    catalog
}

pub async fn catalog(pipeline_home: PathBuf) -> Result<NativePromptCatalog, String> {
    let standard_home = dirs::home_dir().map(|home| home.join(".codex"));
    let mut catalog =
        tokio::task::spawn_blocking(move || read_catalog(&pipeline_home, standard_home.as_deref()))
            .await
            .map_err(|error| format!("Could not inspect native prompt cache: {error}"))?;
    if let Some(resolved) = crate::deps::resolve_command("codex") {
        match super::probe::installed_version(&resolved).await {
            Ok(version) => catalog.installed_version = Some(version.version),
            Err(_) => catalog
                .diagnostics
                .push("Installed Codex version could not be determined.".into()),
        }
    }
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn preserves_templates_and_provenance_without_resolving_variables() {
        let dir = tempfile::tempdir().unwrap();
        let pipeline = dir.path().join("pipeline");
        let standard = dir.path().join("codex");
        fs::create_dir(&pipeline).unwrap();
        fs::create_dir(&standard).unwrap();
        fs::write(pipeline.join("models_cache.json"), json!({"client_version":"0.153.4", "fetched_at":"today", "models":[{"slug":"modern", "model_messages":{"instructions_template":"Base {{personality}}", "persistent_instructions":"Persistent", "instructions_variables":{"personality":"friendly"}, "approvals":{"on_request":"Approval guidance"}}}]}).to_string()).unwrap();
        fs::write(standard.join("models_cache.json"), json!({"client_version":"0.147.0", "models":[{"slug":"legacy", "base_instructions":"Legacy base"}]}).to_string()).unwrap();
        let result = read_catalog(&pipeline, Some(&standard));
        assert!(result.diagnostics.is_empty());
        assert_eq!(result.sources.len(), 2);
        assert_eq!(result.sources[0].origin, "pipeline");
        assert_eq!(result.sources[0].client_version.as_deref(), Some("0.153.4"));
        assert_eq!(result.sources[0].models[0].template, "Base {{personality}}");
        assert_eq!(result.sources[0].models[0].sections.len(), 3);
        assert_eq!(result.sources[1].origin, "codex");
        assert_eq!(result.sources[1].models[0].template, "Legacy base");
        assert!(result.sources[1].models[0].sections.is_empty());
    }

    #[test]
    fn absent_corrupt_unknown_and_oversized_caches_are_explicit() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_catalog(dir.path(), None).sources.is_empty());
        let path = dir.path().join("models_cache.json");
        for text in ["{broken", "{}", r#"{"models":[{"slug":"unknown"}]}"#] {
            fs::write(&path, text).unwrap();
            let result = read_catalog(dir.path(), None);
            assert!(result.sources.is_empty());
            assert_eq!(result.diagnostics.len(), 1);
        }
        File::create(&path)
            .unwrap()
            .set_len(MAX_CACHE_BYTES + 1)
            .unwrap();
        assert!(read_catalog(dir.path(), None).diagnostics[0].contains("16 MiB"));
    }
}
