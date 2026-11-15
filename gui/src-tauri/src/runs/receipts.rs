//! An accepted logical step owns the digest of the products its consumers may
//! read. A receipt lives alongside wave checkpoints but survives their cleanup.
//! Missing or damaged receipts disable reuse without hiding the parent report.
use crate::{models::StepOutput, pipeline::executor::step_slug, safety::WalkBudget};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Serialize, Deserialize, PartialEq)]
struct StepReceipt {
    output: String,
    files: Option<String>,
}

fn receipt(root: &Path, step: &str, outputs: &[StepOutput]) -> Result<StepReceipt, String> {
    let mut digest = Sha256::new();
    for output in outputs
        .iter()
        .filter(|o| o.step_id.split('/').next() == Some(step))
    {
        // Bind the accepted text, item identity, coverage and call provenance.
        // StepOutput contains only ordered records, scalars and vectors.
        serde_json::to_writer(&mut digest, output).map_err(|e| e.to_string())?;
    }
    let tree = root.join("by-step").join(step_slug(step));
    let files = match std::fs::symlink_metadata(&tree) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(String::new()),
        Ok(_) => source_fingerprint(&tree).ok(),
        Err(_) => None,
    };
    Ok(StepReceipt {
        output: format!("{:x}", digest.finalize()),
        files,
    })
}

pub(crate) fn record(root: &Path, steps: &[String], outputs: &[StepOutput]) -> Result<(), String> {
    let dir = root.join("step-receipts");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for step in steps {
        let bytes =
            serde_json::to_vec(&receipt(root, step, outputs)?).map_err(|e| e.to_string())?;
        super::write_text_atomic(&dir, &format!("{}.json", step_slug(step)), &bytes)?;
    }
    Ok(())
}

pub(crate) fn matches(parent: &Path, actual: &Path, step: &str, outputs: &[StepOutput]) -> bool {
    let path = parent
        .join("step-receipts")
        .join(format!("{}.json", step_slug(step)));
    let saved = super::read_utf8_at_most(&path, 4096, "Step receipt")
        .ok()
        .and_then(|s| serde_json::from_str::<StepReceipt>(&s).ok());
    saved.is_some_and(|s| {
        s.files.is_some() && receipt(actual, step, outputs).ok().as_ref() == Some(&s)
    })
}

pub(crate) fn source_fingerprint(root: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut hash = Sha256::new();
    let mut pending = vec![root.to_path_buf()];
    let mut walk = WalkBudget::new("Captured source verification").without_time_limit();
    let mut remaining = crate::safety::MAX_RUN_OUTPUT_BYTES as u64;
    walk.entry()?;
    while let Some(path) = pending.pop() {
        let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.is_symlink() {
            return Err("Captured source contains a symlink".into());
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        hash.update((relative.len() as u64).to_le_bytes());
        hash.update(relative.as_bytes());
        if metadata.is_dir() {
            walk.directory()?;
            hash.update(b"directory");
            let mut children = Vec::new();
            for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
                walk.entry()?;
                children.push(entry.map_err(|e| e.to_string())?.path());
            }
            children.sort();
            pending.extend(children);
        } else {
            hash.update(b"file");
            let mut file = crate::safety::open_regular_file(&path)?;
            let len = file.metadata().map_err(|e| e.to_string())?.len();
            remaining = remaining
                .checked_sub(len)
                .ok_or("Captured source exceeds 256 MiB")?;
            hash.update(len.to_le_bytes());
            if std::io::copy(&mut file.by_ref().take(len + 1), &mut hash)
                .map_err(|e| e.to_string())?
                != len
            {
                return Err("Captured file changed while it was being verified".into());
            }
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_changed_upstream_report_or_file_is_not_reusable() {
        let root = tempfile::tempdir().unwrap();
        let step = "review".to_string();
        let tree = root.path().join("by-step").join(step_slug(&step));
        std::fs::create_dir_all(&tree).unwrap();
        let file = tree.join("attachment");
        std::fs::write(&file, "original").unwrap();
        let mut outputs = [StepOutput {
            step_id: step.clone(),
            raw_text: "accepted".into(),
            ..Default::default()
        }];
        record(root.path(), std::slice::from_ref(&step), &outputs).unwrap();
        assert!(matches(root.path(), root.path(), &step, &outputs));
        std::fs::write(&file, "changed").unwrap();
        assert!(!matches(root.path(), root.path(), &step, &outputs));
        std::fs::write(&file, "original").unwrap();
        outputs[0].raw_text = "replacement".into();
        assert!(!matches(root.path(), root.path(), &step, &outputs));
    }
}
