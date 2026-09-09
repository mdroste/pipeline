//! Immutable source and producer-file handoffs between Review runs.
use crate::pipeline::extract::ScopedSourceContext;
use std::path::Path;

/// Copy only regular files through validated handles, with one aggregate bound.
/// Never link parent and child artifacts: a later write must remain isolated.
pub(super) fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    use std::io::{Read, Write};
    let mut pending = vec![(source.to_path_buf(), destination.to_path_buf())];
    let mut budget = crate::safety::WalkBudget::new("Review artifact copy");
    let mut remaining = crate::safety::MAX_RUN_OUTPUT_BYTES as u64;
    while let Some((source, destination)) = pending.pop() {
        budget.entry()?;
        let meta = std::fs::symlink_metadata(&source).map_err(|e| e.to_string())?;
        if meta.is_symlink() {
            return Err(format!("Cannot copy symlink {}", source.display()));
        }
        if meta.is_dir() {
            budget.directory()?;
            std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
            let mut entries = std::fs::read_dir(&source)
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            entries.sort_by_key(|e| e.file_name());
            pending.extend(
                entries
                    .into_iter()
                    .map(|e| (e.path(), destination.join(e.file_name()))),
            );
        } else {
            let file = crate::safety::open_regular_file(&source)?;
            let mut bytes = Vec::new();
            file.take(remaining + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            remaining = remaining
                .checked_sub(bytes.len() as u64)
                .ok_or("Review artifact copy exceeds 256 MiB")?;
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let parent = destination.parent().ok_or("Artifact has no parent")?;
            let mut pending = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
            pending.write_all(&bytes).map_err(|e| e.to_string())?;
            pending.as_file().sync_all().map_err(|e| e.to_string())?;
            pending.persist(&destination).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            std::fs::File::open(parent)
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub(super) fn snapshot_source(
    source: &ScopedSourceContext,
    root: &Path,
) -> Result<ScopedSourceContext, String> {
    let Some(path) = source.source_path.as_ref() else {
        return Ok(ScopedSourceContext::default());
    };
    let destination = root.join("source-snapshot").join(if path.is_dir() {
        "tree".into()
    } else {
        path.file_name()
            .ok_or("Source has no filename")?
            .to_os_string()
    });
    copy_tree(path, &destination)?;
    Ok(ScopedSourceContext {
        source_path: Some(destination.clone()),
        read_root: Some(if destination.is_dir() {
            destination
        } else {
            destination.parent().unwrap().into()
        }),
    })
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SourceRecord {
    path: Option<String>,
    fingerprint: String,
}

fn source_fingerprint(root: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut hash = Sha256::new();
    let mut pending = vec![root.to_path_buf()];
    let mut walk = crate::safety::WalkBudget::new("Captured source verification");
    let mut remaining = crate::safety::MAX_RUN_OUTPUT_BYTES as u64;
    while let Some(path) = pending.pop() {
        walk.entry()?;
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
            let mut children = std::fs::read_dir(path)
                .map_err(|e| e.to_string())?
                .map(|e| e.map(|e| e.path()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            children.sort();
            pending.extend(children);
        } else {
            hash.update(b"file");
            let mut bytes = Vec::new();
            crate::safety::open_regular_file(&path)?
                .take(remaining + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            remaining = remaining
                .checked_sub(bytes.len() as u64)
                .ok_or("Captured source exceeds 256 MiB")?;
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub(super) fn retain_source(
    source: &ScopedSourceContext,
    writer: &mut crate::runs::RunWriter,
) -> Result<(), String> {
    retain_source_at(source, writer, "")
}

fn retain_source_at(
    source: &ScopedSourceContext,
    writer: &mut crate::runs::RunWriter,
    prefix: &str,
) -> Result<(), String> {
    let root = writer.dir().join(prefix);
    let retained = snapshot_source(source, &root)?;
    let relative = retained.source_path.as_ref().map(|p| {
        p.strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
    });
    let record = SourceRecord {
        path: relative,
        fingerprint: retained
            .source_path
            .as_deref()
            .map(source_fingerprint)
            .transpose()?
            .unwrap_or_default(),
    };
    let rel = |tail: &str| {
        if prefix.is_empty() {
            tail.to_string()
        } else {
            format!("{prefix}/{tail}")
        }
    };
    writer.register_unlisted(&rel("source-snapshot"), "source");
    writer.add_text(
        &rel("context/source-snapshot.json"),
        "Captured source identity",
        "context",
        &serde_json::to_string(&record).map_err(|e| e.to_string())?,
    )?;
    Ok(())
}

pub(super) fn restore_source(
    parent: &Path,
    root: &Path,
    required: bool,
) -> Result<ScopedSourceContext, String> {
    let record = parent.join("context/source-snapshot.json");
    if !record.exists() {
        if required {
            return Err("This run has no immutable source snapshot. Start a new run to use source files; rerun cannot substitute the current files for the original revision.".into());
        }
        return Ok(ScopedSourceContext::default());
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    crate::safety::open_regular_file(&record)?
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4096 {
        return Err("Invalid source snapshot record".into());
    }
    let record: SourceRecord = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let Some(relative) = record.path else {
        if required {
            return Err("The parent run did not capture source files. Start a new run.".into());
        }
        return Ok(ScopedSourceContext::default());
    };
    if !Path::new(&relative)
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_)))
        || !relative.starts_with("source-snapshot/")
    {
        return Err("Invalid source snapshot path".into());
    }
    let captured = snapshot_source(
        &ScopedSourceContext {
            source_path: Some(parent.join(relative)),
            read_root: None,
        },
        root,
    )?;
    if source_fingerprint(
        captured
            .source_path
            .as_deref()
            .ok_or("Missing restored source")?,
    )? != record.fingerprint
    {
        return Err("Captured source failed its integrity check. Start a new run from the intended source revision.".into());
    }
    Ok(captured)
}

pub(super) fn retain_named_source(
    key: &str,
    source: &ScopedSourceContext,
    writer: &mut crate::runs::RunWriter,
) -> Result<(), String> {
    retain_source_at(
        source,
        writer,
        &format!(
            "named-sources/{}",
            crate::pipeline::executor::step_slug(key)
        ),
    )
}

pub(super) fn restore_named_source(
    parent: &Path,
    key: &str,
    root: &Path,
    required: bool,
) -> Result<ScopedSourceContext, String> {
    let relative = Path::new("named-sources").join(crate::pipeline::executor::step_slug(key));
    restore_source(&parent.join(&relative), &root.join(relative), required)
}

pub(super) fn requires_primary_source(config: &crate::pipeline_config::PipelineConfig) -> bool {
    config.steps.iter().filter(|s| s.enabled).any(|s| s.context.include.iter().any(|selector|
        matches!(selector, crate::pipeline_config::ArtifactSelector::Primary { parts } if parts.contains(&crate::pipeline_config::PrimaryArtifactPart::Source))))
}

pub(super) fn requires_named_source(
    config: &crate::pipeline_config::PipelineConfig,
    key: &str,
) -> bool {
    config.steps.iter().filter(|s| s.enabled).any(|s| s.context.include.iter().any(|selector|
        matches!(selector, crate::pipeline_config::ArtifactSelector::NamedInput { key: selected, parts } if selected == key && parts.contains(&crate::pipeline_config::NamedInputArtifactPart::Source))))
}

/// Adapters may read Word XML and TeX figures; point those reads at the same
/// captured revision as extraction, then restore the user-facing provenance.
pub(super) fn build_bundle(
    extraction: &crate::models::ExtractionResult,
    source: Option<&Path>,
    destination: &Path,
) -> Result<crate::document_bundle::BundleBuild, String> {
    let mut captured = extraction.clone();
    captured.source_path = source
        .map(|path| {
            if path.is_dir() && extraction.method != "folder" {
                crate::pipeline::extract::find_main_tex(path).unwrap_or_else(|| path.into())
            } else {
                path.into()
            }
        })
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    if source.is_none() {
        captured.method = "resumed-document".into();
    }
    let mut build = crate::document_bundle::build(&captured, destination)?;
    build
        .bundle
        .extraction
        .source_path
        .clone_from(&extraction.source_path);
    if let Some(origin) = build.bundle.origins.first_mut() {
        origin.path.clone_from(&extraction.source_path);
    }
    Ok(build)
}

pub(super) fn restore_producers(
    parent: &Path,
    destination: &Path,
    ids: impl Iterator<Item = String>,
) -> Result<(), String> {
    for id in ids {
        let slug = crate::pipeline::executor::step_slug(&id);
        let source = parent.join("artifacts/by-step").join(&slug);
        if source.exists() {
            copy_tree(&source, &destination.join("by-step").join(slug))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restored_sources_and_producers_are_independent_of_originals() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("original.tex");
        std::fs::write(&original, "original revision").unwrap();
        let parent = temp.path().join("parent");
        let staged = snapshot_source(
            &ScopedSourceContext {
                source_path: Some(original.clone()),
                read_root: None,
            },
            &parent,
        )
        .unwrap();
        std::fs::create_dir_all(parent.join("context")).unwrap();
        let relative = staged
            .source_path
            .as_ref()
            .unwrap()
            .strip_prefix(&parent)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        std::fs::write(
            parent.join("context/source-snapshot.json"),
            serde_json::to_vec(&SourceRecord {
                path: Some(relative),
                fingerprint: source_fingerprint(staged.source_path.as_ref().unwrap()).unwrap(),
            })
            .unwrap(),
        )
        .unwrap();
        std::fs::write(&original, "changed live revision").unwrap();
        let child = temp.path().join("child");
        let restored = restore_source(&parent, &child, true).unwrap();
        assert_eq!(
            std::fs::read_to_string(restored.source_path.unwrap()).unwrap(),
            "original revision"
        );
        let named_parent = temp.path().join("named-parent");
        let named_root = named_parent
            .join("named-sources")
            .join(crate::pipeline::executor::step_slug("appendix"));
        copy_tree(&parent, &named_root).unwrap();
        let named = restore_named_source(&named_parent, "appendix", &child, true).unwrap();
        assert_eq!(
            std::fs::read_to_string(named.source_path.unwrap()).unwrap(),
            "original revision"
        );
        let slug = crate::pipeline::executor::step_slug("producer");
        let files = parent
            .join("artifacts/by-step")
            .join(&slug)
            .join("unit/files");
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join("results.csv"), "1,2").unwrap();
        restore_producers(
            &parent,
            &child.join("artifacts"),
            std::iter::once("producer".into()),
        )
        .unwrap();
        std::fs::write(
            child
                .join("artifacts/by-step")
                .join(slug)
                .join("unit/files/results.csv"),
            "3,4",
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(files.join("results.csv")).unwrap(),
            "1,2"
        );
        assert!(restore_source(temp.path(), &child, true).is_err());
        std::fs::write(
            staged.source_path.as_ref().unwrap(),
            "tampered retained revision",
        )
        .unwrap();
        assert!(
            restore_source(&parent, &temp.path().join("tampered-child"), true)
                .unwrap_err()
                .contains("integrity check")
        );
    }
    #[test]
    fn legacy_bundle_rebuild_never_opens_the_live_document() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("changed.docx");
        std::fs::write(&original, "a changed and invalid Word document").unwrap();
        let extraction = crate::models::ExtractionResult {
            text: "captured text".into(),
            method: "resumed-document".into(),
            source_path: original.to_string_lossy().into_owned(),
            paper_hash: "captured-hash".into(),
            quality_notes: Vec::new(),
        };
        let build = build_bundle(&extraction, None, temp.path()).unwrap();
        assert_eq!(build.bundle.source_kind, "text");
        assert_eq!(build.bundle.extraction.source_path, extraction.source_path);
    }
}
