//! The host analyzers a workflow step may run, with their declared inputs,
//! network use, and options. Ids and versions are pinned in saved workflows;
//! a changed behavior gets a new version rather than altering an old one.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzerInfo {
    pub id: &'static str,
    pub version: u32,
    pub label: &'static str,
    pub description: &'static str,
    /// Run inputs the analyzer reads.
    pub reads: &'static [&'static str],
    /// Services contacted when the step's `network` option is true.
    pub network_services: &'static [&'static str],
    /// What leaves the computer when the network is used.
    pub network_payload: &'static str,
}

pub const REFERENCE_AUDIT: &str = "reference_audit";
pub const REPLICATION_STATIC: &str = "replication_static";

pub const ANALYZERS: &[AnalyzerInfo] = &[
    AnalyzerInfo {
        id: REFERENCE_AUDIT,
        version: 1,
        label: "Reference check",
        description: "Finds the bibliography and, when network lookups are enabled, checks each reference against Crossref: whether it can be found, whether its details match, whether another version exists, and whether it was retracted or corrected.",
        reads: &["Extracted document", "Bibliography files (.bib) beside a LaTeX source"],
        network_services: &["Crossref"],
        network_payload: "Each reference's text as written in the bibliography (titles, authors, years, venues, DOIs). No other manuscript text.",
    },
    AnalyzerInfo {
        id: REPLICATION_STATIC,
        version: 1,
        label: "Replication package check",
        description: "Checks a replication package folder against common data-editor requirements: README sections, a main script, hard-coded or backslash paths, package installation, random seeds, files referenced by code but missing, exported tables and figures, and very large files.",
        reads: &["Source folder"],
        network_services: &[],
        network_payload: "",
    },
];

pub fn find(id: &str, version: u32) -> Option<&'static AnalyzerInfo> {
    ANALYZERS
        .iter()
        .find(|info| info.id == id && info.version == version)
}

/// Whether a step's options turn on the analyzer's declared network use.
pub fn uses_network(info: &AnalyzerInfo, options: &Value) -> bool {
    !info.network_services.is_empty()
        && options
            .get("network")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn check_bool(options: &serde_json::Map<String, Value>, key: &str) -> Result<(), String> {
    match options.get(key) {
        None | Some(Value::Bool(_)) => Ok(()),
        Some(_) => Err(format!("option '{key}' must be true or false")),
    }
}

fn check_count(
    options: &serde_json::Map<String, Value>,
    key: &str,
    range: std::ops::RangeInclusive<u64>,
) -> Result<(), String> {
    match options.get(key) {
        None => Ok(()),
        Some(value) => match value.as_u64() {
            Some(count) if range.contains(&count) => Ok(()),
            _ => Err(format!(
                "option '{key}' must be a whole number from {} to {}",
                range.start(),
                range.end()
            )),
        },
    }
}

pub fn validate_options(info: &AnalyzerInfo, options: &Value) -> Result<(), String> {
    let empty = serde_json::Map::new();
    let map = match options {
        Value::Null => &empty,
        Value::Object(map) => map,
        _ => return Err("options must be an object".to_string()),
    };
    let allowed: &[&str] = match info.id {
        REFERENCE_AUDIT => &["network", "check_retractions", "max_references"],
        REPLICATION_STATIC => &["max_files"],
        _ => &[],
    };
    if let Some(unknown) = map.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("unknown option '{unknown}'"));
    }
    match info.id {
        REFERENCE_AUDIT => {
            check_bool(map, "network")?;
            check_bool(map, "check_retractions")?;
            check_count(map, "max_references", 1..=400)
        }
        REPLICATION_STATIC => check_count(map, "max_files", 1..=20_000),
        _ => Ok(()),
    }
}

/// What one enabled analyzer step of a workflow reads and sends, for the
/// run preview.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HostCheck {
    pub step_id: String,
    pub step_label: String,
    pub analyzer: &'static str,
    pub reads: &'static [&'static str],
    pub network_services: &'static [&'static str],
    pub network_payload: &'static str,
}

pub fn host_checks(config: &crate::pipeline_config::PipelineConfig) -> Vec<HostCheck> {
    config
        .steps
        .iter()
        .filter(|step| step.enabled)
        .filter_map(|step| {
            let spec = step.analyzer.as_ref()?;
            let info = find(&spec.id, spec.version)?;
            let network = uses_network(info, &spec.options);
            Some(HostCheck {
                step_id: step.id.clone(),
                step_label: step.label.clone(),
                analyzer: info.label,
                reads: info.reads,
                network_services: if network { info.network_services } else { &[] },
                network_payload: if network { info.network_payload } else { "" },
            })
        })
        .collect()
}

/// The analyzers a workflow may use, for the designer.
#[tauri::command]
pub fn list_analyzers() -> Vec<AnalyzerInfo> {
    ANALYZERS.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn analyzers_are_found_only_by_exact_version() {
        assert!(find(REFERENCE_AUDIT, 1).is_some());
        assert!(find(REFERENCE_AUDIT, 2).is_none());
        assert!(find("unknown", 1).is_none());
    }

    #[test]
    fn options_are_closed_and_typed() {
        let audit = find(REFERENCE_AUDIT, 1).unwrap();
        assert!(validate_options(audit, &Value::Null).is_ok());
        assert!(validate_options(audit, &json!({"network": true, "max_references": 50})).is_ok());
        assert!(validate_options(audit, &json!({"network": "yes"})).is_err());
        assert!(validate_options(audit, &json!({"max_references": 0})).is_err());
        assert!(validate_options(audit, &json!({"shell": true})).is_err());
        assert!(validate_options(audit, &json!([1])).is_err());
    }

    #[test]
    fn network_requires_both_a_service_and_an_opt_in() {
        let audit = find(REFERENCE_AUDIT, 1).unwrap();
        let package = find(REPLICATION_STATIC, 1).unwrap();
        assert!(!uses_network(audit, &Value::Null));
        assert!(uses_network(audit, &json!({"network": true})));
        assert!(!uses_network(package, &json!({"network": true})));
    }
}
