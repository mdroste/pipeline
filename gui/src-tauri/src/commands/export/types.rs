//! App-owned export command records; serialization remains unchanged.

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRunArtifactsResult {
    pub exported_path: String,
    pub file_count: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportMode {
    Shareable,
    Forensic,
    Custom,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportSelection {
    pub report: bool,
    pub verified_findings: bool,
    pub provenance: bool,
    pub workflow: bool,
    pub source_documents: bool,
    pub raw_responses: bool,
    pub logs: bool,
    pub supporting_artifacts: bool,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPackageResult {
    pub exported_path: String,
    pub file_count: u64,
    pub bytes: u64,
    /// SHA-256 of `checksums.sha256`, which commits to every exported payload
    /// file and the export manifest without creating a circular checksum.
    pub checksum: String,
    pub mode: String,
    pub sensitivity: String,
}
