use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogManifest {
    #[serde(rename = "schemaVersion")]
    _schema_version: u32,
    kind: String,
    id: String,
    label: String,
    order: usize,
    #[serde(default)]
    group: Option<CatalogGroup>,
    #[serde(default)]
    level: Option<String>,
    routing: CatalogRouting,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogGroup {
    id: String,
    label: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogRouting {
    description: String,
    exclusions: String,
}

fn rust_string(value: &str) -> String {
    serde_json::to_string(value).expect("catalog strings must serialize")
}

fn catalog_path(manifest_dir: &Path, suffix: impl AsRef<Path>) -> PathBuf {
    manifest_dir
        .join("../..")
        .join("prompts/auto_review")
        .join(suffix)
}

fn relative_prompt(suffix: &str) -> String {
    format!(
        "include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../../prompts/auto_review/{suffix}\"))"
    )
}

fn load_catalog_kind(
    manifest_dir: &Path,
    directory: &str,
    expected_kind: &str,
    manifest_validator: &jsonschema::Validator,
) -> Vec<CatalogManifest> {
    let root = catalog_path(manifest_dir, format!("catalog/{directory}"));
    println!("cargo:rerun-if-changed={}", root.display());
    let entries = std::fs::read_dir(&root).unwrap_or_else(|error| {
        panic!(
            "failed to read specialist catalog '{}': {error}",
            root.display()
        )
    });
    let mut manifests = Vec::new();
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("failed to read catalog entry: {error}"));
        let path = entry.path();
        if !path.is_dir() {
            panic!(
                "catalog '{}' may contain only role directories",
                root.display()
            );
        }
        let manifest_path = path.join("specialist.json");
        println!("cargo:rerun-if-changed={}", manifest_path.display());
        let bytes = std::fs::read(&manifest_path).unwrap_or_else(|error| {
            panic!(
                "failed to read catalog manifest '{}': {error}",
                manifest_path.display()
            )
        });
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            panic!(
                "catalog manifest '{}' is not valid JSON: {error}",
                manifest_path.display()
            )
        });
        if let Err(error) = manifest_validator.validate(&value) {
            panic!(
                "catalog manifest '{}' does not satisfy specialist.schema.json: {error}",
                manifest_path.display(),
            );
        }
        let manifest: CatalogManifest = serde_json::from_value(value).unwrap_or_else(|error| {
            panic!(
                "catalog manifest '{}' could not be decoded after schema validation: {error}",
                manifest_path.display()
            )
        });
        if manifest.kind != expected_kind {
            panic!(
                "catalog manifest '{}' declares kind '{}'; expected '{}'",
                manifest_path.display(),
                manifest.kind,
                expected_kind
            );
        }
        let directory_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if manifest.id != directory_name {
            panic!(
                "catalog manifest '{}' declares id '{}'; directory name must match",
                manifest_path.display(),
                manifest.id
            );
        }
        if manifest.label.trim().is_empty()
            || manifest.routing.description.trim().is_empty()
            || manifest.routing.exclusions.trim().is_empty()
        {
            panic!("catalog role '{}' has blank routing metadata", manifest.id);
        }
        manifests.push(manifest);
    }
    manifests.sort_by_key(|manifest| manifest.order);
    for (expected, manifest) in manifests.iter().enumerate() {
        if manifest.order != expected {
            panic!(
                "catalog '{directory}' orders must be contiguous from zero; expected {expected}, found {} on '{}'",
                manifest.order,
                manifest.id
            );
        }
    }
    manifests
}

fn validate_group_order(manifests: &[CatalogManifest], directory: &str) {
    let mut closed = HashSet::new();
    let mut current = None::<&str>;
    for manifest in manifests {
        let group = manifest
            .group
            .as_ref()
            .unwrap_or_else(|| panic!("catalog role '{}' needs a group", manifest.id));
        if group.id.trim().is_empty() || group.label.trim().is_empty() {
            panic!("catalog role '{}' has a blank group", manifest.id);
        }
        if current != Some(group.id.as_str()) {
            if closed.contains(group.id.as_str()) {
                panic!(
                    "catalog '{directory}' group '{}' is not contiguous",
                    group.id
                );
            }
            if let Some(previous) = current {
                closed.insert(previous.to_string());
            }
            current = Some(group.id.as_str());
        }
    }
}

fn generate_catalog(manifest_dir: &Path) {
    let schema_path = catalog_path(manifest_dir, "catalog/specialist.schema.json");
    println!("cargo:rerun-if-changed={}", schema_path.display());
    let schema_bytes = std::fs::read(&schema_path).unwrap_or_else(|error| {
        panic!(
            "failed to read catalog schema '{}': {error}",
            schema_path.display()
        )
    });
    let schema: serde_json::Value = serde_json::from_slice(&schema_bytes).unwrap_or_else(|error| {
        panic!(
            "catalog schema '{}' is not valid JSON: {error}",
            schema_path.display()
        )
    });
    let manifest_validator = jsonschema::draft202012::options()
        .build(&schema)
        .unwrap_or_else(|error| {
            panic!(
                "catalog schema '{}' is invalid: {error}",
                schema_path.display()
            )
        });

    let subjects = load_catalog_kind(manifest_dir, "subjects", "subject", &manifest_validator);
    let methods = load_catalog_kind(manifest_dir, "methods", "method", &manifest_validator);
    let genres = load_catalog_kind(manifest_dir, "genres", "genre", &manifest_validator);
    validate_group_order(&subjects, "subjects");
    validate_group_order(&methods, "methods");

    let mut all_ids = HashSet::new();
    for manifest in subjects.iter().chain(methods.iter()).chain(genres.iter()) {
        if !all_ids.insert(manifest.id.as_str()) {
            panic!("duplicate catalog id '{}'", manifest.id);
        }
    }

    let mut generated =
        String::from("// Generated by build.rs from prompts/auto_review/catalog.\n");
    generated.push_str("pub const SUBJECTS: &[SubjectSpec] = &[\n");
    let mut subject_fallbacks = BTreeMap::<&str, usize>::new();
    for manifest in &subjects {
        if !manifest.id.starts_with("subject_") {
            panic!(
                "subject catalog id '{}' must start with 'subject_'",
                manifest.id
            );
        }
        let group = manifest.group.as_ref().expect("validated subject group");
        let level = match manifest.level.as_deref() {
            Some("discipline") => {
                *subject_fallbacks.entry(group.id.as_str()).or_default() += 1;
                "SubjectLevel::Discipline"
            }
            Some("subfield") => "SubjectLevel::Subfield",
            other => panic!("subject '{}' has invalid level {other:?}", manifest.id),
        };
        let discipline_prompt = format!("subjects/{}.md", group.id);
        let discipline_path = catalog_path(manifest_dir, &discipline_prompt);
        if !discipline_path.is_file() {
            panic!(
                "subject '{}' needs discipline prompt '{}'",
                manifest.id,
                discipline_path.display()
            );
        }
        let focus_suffix = format!("catalog/subjects/{}/focus.md", manifest.id);
        let focus_path = catalog_path(manifest_dir, &focus_suffix);
        if !focus_path.is_file() {
            panic!(
                "subject '{}' needs focus prompt '{}'",
                manifest.id,
                focus_path.display()
            );
        }
        println!("cargo:rerun-if-changed={}", discipline_path.display());
        println!("cargo:rerun-if-changed={}", focus_path.display());
        writeln!(
            generated,
            "    SubjectSpec {{ id: {}, label: {}, discipline_id: {}, discipline_label: {}, level: {level}, routing_description: {}, routing_exclusions: {}, discipline_prompt: {}, review_focus: {} }},",
            rust_string(&manifest.id),
            rust_string(&manifest.label),
            rust_string(&group.id),
            rust_string(&group.label),
            rust_string(&manifest.routing.description),
            rust_string(&manifest.routing.exclusions),
            relative_prompt(&discipline_prompt),
            relative_prompt(&focus_suffix),
        )
        .unwrap();
    }
    for (group, count) in subject_fallbacks {
        if count > 1 {
            panic!("subject group '{group}' declares more than one discipline fallback");
        }
    }
    generated.push_str("];\n\n");

    generated.push_str("pub const METHODS: &[MethodSpec] = &[\n");
    let mut method_fallbacks = BTreeMap::<&str, usize>::new();
    for manifest in &methods {
        let group = manifest.group.as_ref().expect("validated method group");
        let level = match manifest.level.as_deref() {
            Some("family") => {
                *method_fallbacks.entry(group.id.as_str()).or_default() += 1;
                "MethodLevel::Family"
            }
            Some("method") => "MethodLevel::Specific",
            other => panic!("method '{}' has invalid level {other:?}", manifest.id),
        };
        let prompt_suffix = format!("catalog/methods/{}/prompt.md", manifest.id);
        let prompt_path = catalog_path(manifest_dir, &prompt_suffix);
        if !prompt_path.is_file() {
            panic!(
                "method '{}' needs prompt '{}'",
                manifest.id,
                prompt_path.display()
            );
        }
        println!("cargo:rerun-if-changed={}", prompt_path.display());
        writeln!(
            generated,
            "    MethodSpec {{ id: {}, label: {}, family_id: {}, family_label: {}, level: {level}, routing_description: {}, routing_exclusions: {}, prompt: {} }},",
            rust_string(&manifest.id),
            rust_string(&manifest.label),
            rust_string(&group.id),
            rust_string(&group.label),
            rust_string(&manifest.routing.description),
            rust_string(&manifest.routing.exclusions),
            relative_prompt(&prompt_suffix),
        )
        .unwrap();
    }
    for (group, count) in method_fallbacks {
        if count > 1 {
            panic!("method group '{group}' declares more than one family fallback");
        }
    }
    generated.push_str("];\n\n");

    generated.push_str("pub const GENRES: &[GenreSpec] = &[\n");
    for manifest in &genres {
        if !manifest.id.starts_with("genre_") {
            panic!(
                "genre catalog id '{}' must start with 'genre_'",
                manifest.id
            );
        }
        if manifest.group.is_some() || manifest.level.is_some() {
            panic!("genre '{}' may not declare a group or level", manifest.id);
        }
        let prompt_suffix = format!("catalog/genres/{}/prompt.md", manifest.id);
        let prompt_path = catalog_path(manifest_dir, &prompt_suffix);
        if !prompt_path.is_file() {
            panic!(
                "genre '{}' needs prompt '{}'",
                manifest.id,
                prompt_path.display()
            );
        }
        println!("cargo:rerun-if-changed={}", prompt_path.display());
        writeln!(
            generated,
            "    GenreSpec {{ id: {}, label: {}, routing_description: {}, routing_exclusions: {}, prompt: {} }},",
            rust_string(&manifest.id),
            rust_string(&manifest.label),
            rust_string(&manifest.routing.description),
            rust_string(&manifest.routing.exclusions),
            relative_prompt(&prompt_suffix),
        )
        .unwrap();
    }
    generated.push_str("];\n");

    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set"))
        .join("auto_review_catalog.rs");
    std::fs::write(&output, generated)
        .unwrap_or_else(|error| panic!("failed to write '{}': {error}", output.display()));
}

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));
    generate_catalog(&manifest_dir);

    // Match Tauri's default Windows manifest (the Common-Controls dependency
    // that native dialogs need) and additionally opt the exe into long-path
    // awareness — still gated on the OS LongPathsEnabled policy — so deep
    // per-step artifact directories under %USERPROFILE%\.pipeline\runs\…
    // cannot hit MAX_PATH. Ignored on other platforms.
    const WINDOWS_APP_MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">
      <longPathAware>true</longPathAware>
    </windowsSettings>
  </application>
</assembly>
"#;
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new().app_manifest(WINDOWS_APP_MANIFEST),
    ))
    .expect("failed to run tauri-build");
}
