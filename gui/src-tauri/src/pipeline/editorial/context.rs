//! Per-call immutable source and producer records. All file I/O is bounded and
//! runs off the async worker. The combined preview lives in the caller's private
//! read view; no model receives write access to the source or preview.
use super::{Candidates, Verification};
use crate::models::StepOutput;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;

pub(crate) struct Inputs {
    pub paper_path: String,
    pub orientation_path: String,
    pub prior_outputs: Vec<StepOutput>,
    pub view_dir: std::path::PathBuf,
}

pub(crate) struct Context {
    pub(super) source: String,
    pub(super) hash: String,
    pub(super) namespace: String,
    pub(super) stage: String,
    pub(super) referees: Vec<String>,
    pub(super) upstream: Vec<Candidates>,
    pub(super) warnings: Vec<String>,
    pub(super) brief: Value,
    pub(super) preview: Option<String>,
}

fn read(path: &str) -> Result<String, String> {
    let file = crate::safety::open_regular_file(std::path::Path::new(path))
        .map_err(|error| format!("Cannot read editorial source: {error}"))?;
    let mut bytes = Vec::new();
    let limit = crate::safety::MAX_EXPANDED_PROMPT_BYTES;
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > limit {
        return Err("Editorial source exceeds the text safety limit".into());
    }
    String::from_utf8(bytes).map_err(|error| error.to_string())
}

impl Context {
    pub(crate) async fn load(stage: &str, namespace: &str, inputs: Inputs) -> Result<Self, String> {
        let stage = stage.to_string();
        let namespace = namespace.to_string();
        crate::run_scope::spawn_blocking(move || Self::load_blocking(stage, namespace, inputs))
            .await
            .map_err(|error| error.to_string())?
    }

    fn load_blocking(stage: String, namespace: String, inputs: Inputs) -> Result<Self, String> {
        let source = read(&inputs.paper_path)?;
        let hash = format!("{:x}", Sha256::digest(source.as_bytes()));
        let brief: Value = serde_json::from_str(&read(&inputs.orientation_path)?)
            .map_err(|error| error.to_string())?;
        let referees: Vec<String> = serde_json::from_value(brief["referees"].clone())
            .map_err(|_| "Editorial brief has no selected subject editors")?;
        if !(1..=3).contains(&referees.len())
            || referees
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != referees.len()
        {
            return Err("Editorial brief needs one to three distinct subject editors".into());
        }
        let mut upstream = Vec::new();
        let mut warnings = Vec::new();
        for output in inputs.prior_outputs.iter().filter(|output| !output.skipped) {
            let value: Candidates = serde_json::from_str(&output.raw_text).map_err(|error| {
                format!("Invalid editorial producer '{}': {error}", output.step_id)
            })?;
            if !super::STAGES[..2].contains(&value.stage.as_str()) || value.source_hash != hash {
                return Err(
                    "Editorial producer uses a different contract or source snapshot".into(),
                );
            }
            crate::pipeline::structured::validate(
                &super::schemas::candidates(value.stage == super::STAGES[1]),
                &serde_json::to_value(&value).map_err(|error| error.to_string())?,
            )?;
            super::validate::candidates(
                &source,
                &hash,
                &output.step_id,
                &referees,
                &upstream,
                &value,
            )?;
            if value.coverage.incomplete {
                warnings.push(format!(
                    "{} reported incomplete coverage.",
                    output.step_label
                ));
            }
            warnings.extend(value.coverage.limitations.clone());
            upstream.push(value);
        }
        if stage != super::STAGES[0] {
            for (index, subject) in referees.iter().enumerate() {
                let id = format!("declaude_referee_{}", index + 1);
                if !inputs.prior_outputs.iter().any(|output| {
                    !output.skipped && output.step_id.split('/').next() == Some(id.as_str())
                }) {
                    warnings.push(format!(
                        "Selected subject editor {subject} was unavailable; coverage is partial."
                    ));
                }
            }
        }
        if let Some(notes) = brief["extraction_quality_notes"].as_array() {
            warnings.extend(
                notes
                    .iter()
                    .filter_map(|note| note["description"].as_str().map(str::to_string)),
            );
        }
        let mut result = Self {
            source,
            hash,
            namespace,
            stage,
            referees,
            upstream,
            warnings,
            brief,
            preview: None,
        };
        if result.stage == super::STAGES[2] {
            let candidates = result.consolidated()?;
            let combined = super::validate::apply(&result.source, &candidates.edits)?;
            let path = inputs.view_dir.join("editorial-proposed-document.txt");
            std::fs::write(&path, combined).map_err(|error| error.to_string())?;
            result.preview = Some(path.to_string_lossy().into_owned());
        }
        Ok(result)
    }

    pub(super) fn consolidated(&self) -> Result<&Candidates, String> {
        let mut sets = self
            .upstream
            .iter()
            .filter(|set| set.stage == super::STAGES[1]);
        let set = sets
            .next()
            .ok_or("Verification needs the consolidated edit record")?;
        if sets.next().is_some() {
            return Err("Verification needs exactly one consolidated edit record".into());
        }
        Ok(set)
    }

    pub(crate) fn instruction(&self) -> String {
        let mut text = format!(
            "\n\nHOST EDITORIAL CONTRACT\nReturn stage {} and source_hash {} exactly. Use edit IDs beginning with '{}:'. Anchors refer to the exact readable document file, before presentation cleanup. Read that file to verify exact anchors when the shared context has been cleaned; this required source check overrides no-reread efficiency notes. Copy original/prefix/suffix literally; do not normalize whitespace, Unicode, HTML, or LaTeX. A match establishes readable-text identity, not OCR correctness.\n",
            self.stage, self.hash, self.namespace
        );
        if let Some(preview) = &self.preview {
            text.push_str(&format!("The host applied all consolidated candidates to a private copy at {preview}. Read the affected passages in this combined preview and compare with the original. If rejecting edits, also assess the surviving subset in context. You may accept or reject candidates, never rewrite them in verification.\n"));
        }
        if !self.warnings.is_empty() {
            text.push_str("Known limitations (retained by the host in the report):\n");
            text.push_str(&self.warnings.join("\n"));
        }
        text
    }

    /// Called inside the supervised response acceptance/retry loop. Native
    /// provider schemas are not enough: imported profiles cannot weaken these
    /// host-owned constraints by editing a marked schema's fields.
    pub(crate) fn check(&self, canonical: &str) -> Result<Option<String>, String> {
        let value: Value = serde_json::from_str(canonical).map_err(|error| error.to_string())?;
        let schema = if self.stage == super::STAGES[2] {
            super::schemas::verification()
        } else {
            super::schemas::candidates(self.stage == super::STAGES[1])
        };
        crate::pipeline::structured::validate(&schema, &value)?;
        if self.stage == super::STAGES[2] {
            let verification: Verification =
                serde_json::from_value(value).map_err(|error| error.to_string())?;
            let candidates = self.consolidated()?;
            let accepted =
                super::validate::verified(&self.source, &self.hash, candidates, &verification)?;
            // Also check the size of the final accepted combination, which may
            // differ from the preview when the verifier rejects a candidate.
            super::validate::apply(&self.source, &accepted)?;
            super::render::report(self, &verification, &accepted).map(Some)
        } else {
            let candidates: Candidates =
                serde_json::from_value(value).map_err(|error| error.to_string())?;
            super::validate::candidates(
                &self.source,
                &self.hash,
                &self.namespace,
                &self.referees,
                &self.upstream,
                &candidates,
            )?;
            Ok(None)
        }
    }
}
