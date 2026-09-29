//! Capture exact evidence and manuscript bytes through the owning adapters.
use super::*;

pub(super) async fn consume(
    c: &Coordinator,
    p: &mut Portfolio,
    child: &TaskRun,
    output: &Value,
) -> Result<()> {
    let mut evidence = BTreeMap::new();
    let mut artifacts = BTreeMap::new();
    if let Phase::Research { paper } = p.run.phase {
        let v: Investigation = validate::response(output)?;
        validate::investigation(&v)?;
        let ws = p.run.workspace_id.clone();
        let refs = v.sources.clone();
        let sources = run_store(move |s| research::sources(&s, &ws, &refs))
            .await
            .map_err(|e| e.message)?;
        let round = p.papers[paper].rounds.len() + 1;
        for (i, source) in sources.into_iter().enumerate() {
            evidence.insert(format!("r{round}-source{i}"), source);
        }
        let scope = child.scope.clone();
        let memo = serde_json::to_string_pretty(&v).map_err(store::err)?;
        let files = v.files;
        let captured = c
            .db(move |s| {
                let mut out = BTreeMap::new();
                out.insert(
                    format!("r{round}-dossier"),
                    adapters::snapshot(&s, &scope, json!(memo), "dossier.json")?,
                );
                for (i, path) in files.into_iter().enumerate() {
                    let filename = std::path::Path::new(&path)
                        .file_name()
                        .and_then(|p| p.to_str())
                        .ok_or("Artifact filename missing")?;
                    out.insert(
                        format!("r{round}-file{i}"),
                        adapters::snapshot(&s, &scope, json!({"path":path}), filename)?,
                    );
                }
                check_artifacts(&out)?;
                Ok(out)
            })
            .await?;
        evidence.extend(captured);
    }
    if let Phase::Draft { paper } = p.run.phase {
        let v: Manuscript = validate::response(output)?;
        validate::manuscript(&v, &p.papers[paper])?;
        let scope = child.scope.clone();
        artifacts = c
            .db(move |s| {
                let mut out = BTreeMap::new();
                for (name, text) in [
                    ("paper.md", v.markdown),
                    ("paper.tex", v.latex),
                    ("references.bib", v.bibliography),
                ] {
                    out.insert(
                        name.into(),
                        adapters::snapshot(&s, &scope, json!(text), name)?,
                    );
                }
                for (i, path) in v.files.into_iter().enumerate() {
                    let name = std::path::Path::new(&path)
                        .file_name()
                        .and_then(|p| p.to_str())
                        .ok_or("Artifact name missing")?;
                    let artifact = adapters::snapshot(&s, &scope, json!({"path":path}), name)?;
                    if let Some(canonical) = out.get(name) {
                        if canonical["hash"] != artifact["hash"] {
                            return Err(format!(
                                "Listed {name} differs from the returned manuscript source"
                            ));
                        }
                    }
                    out.insert(format!("attachment-{i}-{name}"), artifact);
                }
                if out.keys().any(|k| k.ends_with(".pdf"))
                    && !out
                        .keys()
                        .any(|k| k.starts_with("attachment-") && k.ends_with("-paper.tex"))
                {
                    return Err(
                        "A PDF attachment must include its matching paper.tex source".into(),
                    );
                }
                check_artifacts(&out)?;
                Ok(out)
            })
            .await?;
    }
    engine::consume(p, output, evidence, artifacts)
}
pub(super) fn check_artifacts(a: &BTreeMap<String, Value>) -> Result<()> {
    if a.values().any(|v| v["kind"] != "artifact") {
        return Err("List individual research files rather than directories".into());
    }
    if a.values()
        .map(|v| v["bytes"].as_u64().unwrap_or(u64::MAX))
        .try_fold(0u64, |a, b| a.checked_add(b))
        .is_none_or(|n| n > 64 * 1024 * 1024)
    {
        return Err("Research artifacts exceed 64 MiB per action".into());
    }
    Ok(())
}

pub(super) async fn materialize(
    c: &Coordinator,
    scope: &store::Scope,
    artifacts: &BTreeMap<String, Value>,
) -> Result<()> {
    let artifacts = artifacts.clone();
    let entries = c
        .db(move |s| {
            check_artifacts(&artifacts)?;
            let mut entries = Vec::new();
            for (name, a) in artifacts {
                if a["kind"] != "artifact" {
                    continue;
                }
                let path = adapters::artifact_path(&s, &a)?;
                use std::io::Read;
                let mut bytes = Vec::new();
                crate::safety::open_regular_file(std::path::Path::new(&path))?
                    .take(64 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(store::err)?;
                entries.push((name, bytes));
            }
            Ok(entries)
        })
        .await?;
    let session = scope
        .session_id
        .clone()
        .ok_or("Research role has no session")?;
    run_store(move |s| {
        let root = workspace::runtime_root(&s, &session)?.ok_or_else(|| {
            crate::workbench::store::WorkbenchError::invalid(
                "Research input destination is not an isolated role",
            )
        })?;
        crate::workbench::project::materialize_research_files(&root, entries)
    })
    .await
    .map_err(|e| e.message)
}
