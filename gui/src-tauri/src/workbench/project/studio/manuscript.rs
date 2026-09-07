use super::*;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildConfig {
    pub name: String,
    pub checkpoint_id: Option<String>,
    pub directory: String,
    pub root_document: String,
    pub expected_pdf: String,
    pub engine: String,
    pub timeout_seconds: u64,
    pub inputs: Vec<String>,
    pub advanced_argv: Option<Vec<String>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorFile {
    pub path: String,
    pub content: String,
    pub hash: String,
    pub checkpoint_id: Option<String>,
}
fn editor_root(store: &Store, ws: &str, checkpoint: Option<&str>) -> WorkbenchResult<PathBuf> {
    match checkpoint {
        Some(id) => session_task_root(store, ws, id),
        None => root(store, ws),
    }
}
fn editable(path: &str) -> WorkbenchResult<()> {
    files::relative(path)?;
    if !matches!(
        Path::new(path).extension().and_then(|s| s.to_str()),
        Some("tex" | "md" | "bib" | "txt")
    ) {
        return Err(WorkbenchError::invalid(
            "The manuscript editor supports TeX, Markdown, BibTeX and text",
        ));
    }
    Ok(())
}
pub fn editor_file(
    store: &Store,
    ws: &str,
    checkpoint: Option<&str>,
    path: &str,
) -> WorkbenchResult<EditorFile> {
    editable(path)?;
    let bytes = files::SafeRoot::open(&editor_root(store, ws, checkpoint)?)?.read(path)?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(WorkbenchError::invalid(
            "The editor supports files through 2 MiB",
        ));
    }
    Ok(EditorFile {
        path: path.into(),
        hash: hash(&bytes),
        content: String::from_utf8(bytes).map_err(err)?,
        checkpoint_id: checkpoint.map(str::to_owned),
    })
}
pub(super) fn save_text(
    store: &Store,
    ws: &str,
    checkpoint_id: Option<&str>,
    path: &str,
    expected_hash: &str,
    content: &str,
) -> WorkbenchResult<Value> {
    editable(path)?;
    if content.len() > 2 * 1024 * 1024 {
        return Err(WorkbenchError::invalid("Text exceeds 2 MiB"));
    }
    let current = editor_file(store, ws, checkpoint_id, path)?;
    if current.hash != expected_hash {
        return Err(WorkbenchError::conflict(
            "This file changed outside the editor. Reload or copy your draft before saving.",
        ));
    }
    if current.content == content {
        return Ok(json!({"saved":true,"hash":current.hash,"changed":false}));
    }
    let cp = if let Some(cp) = checkpoint_id {
        cp.to_string()
    } else {
        let task = tasks::create_task(
            store,
            ws,
            format!("Edit {path}"),
            None,
            vec![path.into()],
            vec!["Rebuild the accepted file combination".into()],
        )?;
        tasks::checkpoint(store, ws, &task.id, vec![path.into()], "copy")?.id
    };
    stage_text(store, ws, &cp, path, Some(expected_hash), content)?;
    let captured = tasks::capture_changes(store, ws, &cp)?;
    let application = if checkpoint_id.is_none() {
        Some(tasks::apply(
            store,
            ws,
            &cp,
            vec![path.into()],
            captured.revision,
        )?)
    } else {
        None
    };
    Ok(
        json!({"saved":true,"hash":hash(content.as_bytes()),"checkpointId":cp,"application":application,"checksValid":false}),
    )
}
pub(super) fn stage_text(
    store: &Store,
    ws: &str,
    cp: &str,
    path: &str,
    expected: Option<&str>,
    content: &str,
) -> WorkbenchResult<()> {
    files::relative(path)?;
    let safe = files::SafeRoot::open(&session_task_root(store, ws, cp)?)?;
    let mode = safe.executable(path)?;
    let claim = id("editor")?;
    safe.replace(path, expected, mode, Some(content.as_bytes()), mode, &claim)?;
    safe.clear_claim(path, &claim)?;
    Ok(())
}
pub(super) fn save_build(
    store: &Store,
    ws: &str,
    object_id: Option<&str>,
    expected: i64,
    config: BuildConfig,
) -> WorkbenchResult<ProjectRecord> {
    bounded(&config.name, 300)?;
    editable(&config.root_document)?;
    files::relative(&config.expected_pdf)?;
    if !config.root_document.ends_with(".tex") || !config.expected_pdf.ends_with(".pdf") {
        return Err(WorkbenchError::invalid(
            "Choose a TeX root document and expected PDF",
        ));
    }
    let base = editor_root(store, ws, config.checkpoint_id.as_deref())?;
    let cwd = if config.directory.is_empty() || config.directory == "." {
        base.clone()
    } else {
        files::relative(&config.directory)?;
        fs::canonicalize(base.join(&config.directory)).map_err(err)?
    };
    if !cwd.starts_with(&base) {
        return Err(WorkbenchError::invalid(
            "Build directory escapes the selected root",
        ));
    }
    let mut inputs = config.inputs.clone();
    if !inputs.contains(&config.root_document) {
        inputs.push(config.root_document.clone());
    }
    let stem = Path::new(&config.expected_pdf)
        .with_extension("")
        .to_string_lossy()
        .into_owned();
    let argv = if let Some(argv) = &config.advanced_argv {
        argv.clone()
    } else {
        match config.engine.as_str() {
            "pdflatex" | "xelatex" | "lualatex" => vec![
                config.engine.clone(),
                "-no-shell-escape".into(),
                "-interaction=nonstopmode".into(),
                "-halt-on-error".into(),
                "-file-line-error".into(),
                "-synctex=1".into(),
                "-recorder".into(),
                config.root_document.clone(),
            ],
            "latexmk" => vec![
                "latexmk".into(),
                "-norc".into(),
                "-pdf".into(),
                "-no-shell-escape".into(),
                "-interaction=nonstopmode".into(),
                "-file-line-error".into(),
                "-synctex=1".into(),
                config.root_document.clone(),
            ],
            _ => {
                return Err(WorkbenchError::invalid(
                    "Choose a supported engine or use the advanced argv editor",
                ))
            }
        }
    };
    // Even advanced TeX engine profiles retain the no-shell-escape policy.
    if argv
        .iter()
        .any(|a| a.contains("shell-escape") && a != "-no-shell-escape" || a.contains("write18"))
    {
        return Err(WorkbenchError::invalid(
            "Build profiles cannot enable shell escape",
        ));
    }
    let id = new_or_id(object_id)?;
    if object_id.is_some() {
        let old = record(store, ws, &id, "build")?;
        if old.revision != expected {
            return Err(WorkbenchError::conflict("Build settings changed; refresh"));
        }
    }
    let profile_id = format!("profile_{id}");
    let existing = research::get_execution_profile(store, &profile_id).ok();
    let profile = research::save_execution_profile(
        store,
        research::SaveExecutionProfileRequest {
            profile_id: Some(profile_id),
            workspace_id: ws.into(),
            name: config.name.clone(),
            adapter: "latex".into(),
            argv,
            cwd: cwd.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs,
            timeout_seconds: config.timeout_seconds,
            outputs: vec![
                config.expected_pdf.clone(),
                format!("{stem}.log"),
                format!("{stem}.synctex.gz"),
            ],
            expected_revision: existing.map(|p| p.revision),
            operation_id: super::super::id("profile_save")?,
        },
    )?;
    put(
        store,
        ws,
        &id,
        "build",
        expected,
        &json!({"recordType":"configuration","config":config,"profile":profile,"localAuthorizationRequired":true}),
    )
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildDiagnostic {
    pub severity: String,
    pub message: String,
    pub path: Option<String>,
    pub line: Option<u64>,
}
pub fn parse_diagnostics(text: &str) -> Vec<BuildDiagnostic> {
    let location =
        regex::Regex::new(r"^(.+?\.(?:tex|bib)):(\d+):\s*(.*)$").expect("diagnostic regex");
    text.lines()
        .filter_map(|line| {
            if let Some(c) = location.captures(line) {
                return Some(BuildDiagnostic {
                    severity: "error".into(),
                    message: c[3].into(),
                    path: Some(c[1].trim_start_matches("./").into()),
                    line: c[2].parse().ok(),
                });
            }
            if line.contains("Warning")
                || line.contains("undefined")
                || line.starts_with("! ")
                || line.contains("Overfull")
                || line.contains("Underfull")
            {
                Some(BuildDiagnostic {
                    severity: if line.starts_with('!') {
                        "error"
                    } else {
                        "warning"
                    }
                    .into(),
                    message: line.into(),
                    path: None,
                    line: None,
                })
            } else {
                None
            }
        })
        .take(500)
        .collect()
}
pub(super) fn inspect_build(store: &Store, ws: &str, execution_id: &str) -> WorkbenchResult<Value> {
    let e = execution(store, ws, execution_id)?;
    if e.adapter != "latex" {
        return Err(WorkbenchError::invalid(
            "This execution is not a manuscript build",
        ));
    }
    let record_id = format!("build_{execution_id}");
    if let Ok(prior) = record(store, ws, &record_id, "build") {
        return Ok(prior.body);
    }
    if matches!(e.outcome.as_str(), "queued" | "running") {
        return Err(WorkbenchError::invalid("The build is still active"));
    }
    let diagnostics = parse_diagnostics(&format!(
        "{}\n{}",
        e.stdout.as_deref().unwrap_or(""),
        e.stderr.as_deref().unwrap_or("")
    ));
    let mut pdf_revision = None;
    let mut mapping_artifact = None;
    if e.outcome == "completed" {
        for artifact in e.output_manifest["artifacts"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let path = artifact["path"].as_str().unwrap_or("");
            let aid = artifact["artifactId"].as_str().unwrap_or("");
            if path.ends_with(".pdf") {
                let (bytes, _) = artifact_bytes(store, ws, &e, aid)?;
                if !bytes.starts_with(b"%PDF-") {
                    return Err(WorkbenchError::invalid("Build output is not a PDF"));
                }
                let original: String = store
                    .connection()?
                    .query_row(
                        "SELECT storage_reference FROM artifacts WHERE id=?1 AND workspace_id=?2",
                        params![aid, ws],
                        |r| r.get(0),
                    )
                    .map_err(err)?;
                let paper = research::import_paper(
                    store,
                    research::ImportPaperRequest {
                        workspace_id: ws.into(),
                        paper_id: None,
                        title: format!("Build {} · {path}", e.id),
                        role: "other".into(),
                        path: original,
                        operation_id: id("build_pdf")?,
                    },
                )?;
                pdf_revision = Some(paper);
            }
            if path.ends_with(".synctex.gz") {
                mapping_artifact = Some(aid.to_string());
            }
        }
    }
    let value = json!({"recordType":"receipt","executionId":e.id,"outcome":e.outcome,"diagnostics":diagnostics,"pdf":pdf_revision,"mappingArtifactId":mapping_artifact,"pageInspection":"not_recorded","sourceManifest":e.input_manifest,"synchronization":if mapping_artifact.is_some(){"available_for_exact_build"}else{"source_and_page_fallback"}});
    put(store, ws, &record_id, "build", 0, &value)?;
    Ok(value)
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRequest {
    pub workspace_id: String,
    pub execution_id: String,
    pub source_path: Option<String>,
    pub line: Option<u32>,
    pub page: Option<u32>,
    pub x: Option<f64>,
    pub y: Option<f64>,
}
/// Read-only SyncTeX runs against an isolated pair of adopted build outputs.
pub fn synchronize(store: &Store, request: SyncRequest) -> WorkbenchResult<Value> {
    let e = execution(store, &request.workspace_id, &request.execution_id)?;
    if e.outcome != "completed" || e.adapter != "latex" {
        return Err(WorkbenchError::invalid(
            "Synchronization requires a successful exact build",
        ));
    }
    let artifacts = e.output_manifest["artifacts"]
        .as_array()
        .ok_or_else(|| WorkbenchError::invalid("Build artifacts are unavailable"))?;
    let pdf = artifacts
        .iter()
        .find(|a| a["path"].as_str().is_some_and(|p| p.ends_with(".pdf")))
        .ok_or_else(|| WorkbenchError::invalid("No adopted PDF; use page navigation"))?;
    let mapping = artifacts
        .iter()
        .find(|a| {
            a["path"]
                .as_str()
                .is_some_and(|p| p.ends_with(".synctex.gz"))
        })
        .ok_or_else(|| {
            WorkbenchError::invalid("No adopted SyncTeX mapping; use source/page navigation")
        })?;
    let dir = tempfile::tempdir().map_err(err)?;
    let output = dir.path().join("build.pdf");
    fs::write(
        &output,
        artifact_bytes(
            store,
            &request.workspace_id,
            &e,
            pdf["artifactId"].as_str().unwrap_or(""),
        )?
        .0,
    )
    .map_err(err)?;
    fs::write(
        dir.path().join("build.synctex.gz"),
        artifact_bytes(
            store,
            &request.workspace_id,
            &e,
            mapping["artifactId"].as_str().unwrap_or(""),
        )?
        .0,
    )
    .map_err(err)?;
    let mut cmd =
        std::process::Command::new(crate::deps::find_on_path("synctex").ok_or_else(|| {
            WorkbenchError::invalid("SyncTeX is unavailable; use source/page navigation")
        })?);
    if let Some(source) = request.source_path {
        files::relative(&source)?;
        if !e.input_manifest["files"]
            .as_array()
            .is_some_and(|a| a.iter().any(|f| f["path"] == source))
        {
            return Err(WorkbenchError::invalid(
                "This source was not declared in the exact build",
            ));
        }
        let line = request
            .line
            .filter(|n| *n > 0 && *n < 10_000_000)
            .ok_or_else(|| WorkbenchError::invalid("Choose a source line"))?;
        cmd.args([
            "view",
            "-i",
            &format!("{line}:0:{}", Path::new(&e.cwd).join(source).display()),
            "-o",
        ])
        .arg(&output);
    } else {
        let page = request
            .page
            .filter(|p| *p > 0 && *p < 100_000)
            .ok_or_else(|| WorkbenchError::invalid("Choose a PDF page"))?;
        let x = request.x.unwrap_or(0.);
        let y = request.y.unwrap_or(0.);
        if !x.is_finite()
            || !y.is_finite()
            || !(0. ..=20000.).contains(&x)
            || !(0. ..=20000.).contains(&y)
        {
            return Err(WorkbenchError::invalid("Invalid page coordinates"));
        }
        cmd.args([
            "edit",
            "-o",
            &format!("{page}:{x}:{y}:{}", output.display()),
        ]);
    }
    // Environment variables can otherwise make this nominally read-only tool launch an editor/viewer.
    cmd.env_remove("SYNCTEX_VIEWER")
        .env_remove("SYNCTEX_EDITOR")
        .stdin(std::process::Stdio::null());
    let stdout = tempfile::tempfile().map_err(err)?;
    let stderr = tempfile::tempfile().map_err(err)?;
    cmd.stdout(stdout.try_clone().map_err(err)?)
        .stderr(stderr.try_clone().map_err(err)?);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(err)?;
    let pid = child.id();
    crate::commands::lifecycle::register_independent_child_pid(pid);
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(err)? {
            break Some(status);
        }
        if start.elapsed() > Duration::from_secs(3) {
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    crate::commands::lifecycle::kill_independent_process(pid);
    let _ = child.wait();
    crate::commands::lifecycle::unregister_independent_child_pid(pid);
    if !status.is_some_and(|s| s.success()) {
        return Err(WorkbenchError::invalid(
            "SyncTeX failed or timed out; use source/page navigation",
        ));
    }
    use std::io::{Seek, SeekFrom};
    let mut stdout = stdout;
    stdout.seek(SeekFrom::Start(0)).map_err(err)?;
    let mut text = String::new();
    stdout
        .take(64 * 1024)
        .read_to_string(&mut text)
        .map_err(err)?;
    let mut candidates = Vec::new();
    let mut candidate = serde_json::Map::new();
    for line in text.lines() {
        if line.starts_with("Output:") && !candidate.is_empty() {
            candidates.push(Value::Object(std::mem::take(&mut candidate)));
        }
        if let Some((key, value)) = line.split_once(':') {
            match key {
                "Page" | "Line" => {
                    if let Ok(n) = value.trim().parse::<u32>() {
                        candidate.insert(key.to_ascii_lowercase(), json!(n));
                    }
                }
                "x" | "y" => {
                    if let Ok(n) = value.trim().parse::<f64>() {
                        if n.is_finite() {
                            candidate.insert(key.into(), json!(n));
                        }
                    }
                }
                "Input" => {
                    let path = Path::new(value.trim());
                    let relative = path
                        .strip_prefix(&e.cwd)
                        .ok()
                        .map(|p| p.to_string_lossy().into_owned());
                    if let Some(relative) = relative.filter(|p| {
                        e.input_manifest["files"]
                            .as_array()
                            .is_some_and(|a| a.iter().any(|f| f["path"] == *p))
                    }) {
                        candidate.insert("sourcePath".into(), json!(relative));
                    }
                }
                _ => {}
            }
        }
    }
    if !candidate.is_empty() {
        candidates.push(Value::Object(candidate));
    }
    Ok(
        json!({"executionId":e.id,"mappingHash":mapping["contentHash"],"result":text,"candidates":candidates,"scope":"Exact build only; source lines may have moved in the working copy"}),
    )
}

pub fn editor_external_path(
    store: &Store,
    ws: &str,
    checkpoint: Option<&str>,
    path: &str,
) -> WorkbenchResult<String> {
    editor_file(store, ws, checkpoint, path)?;
    Ok(editor_root(store, ws, checkpoint)?
        .join(path)
        .to_string_lossy()
        .into_owned())
}

pub(super) fn inspect_pages(
    store: &Store,
    ws: &str,
    execution_id: &str,
    pages: Vec<u32>,
    note: &str,
) -> WorkbenchResult<Value> {
    let object_id = format!("build_{execution_id}");
    let old = record(store, ws, &object_id, "build")?;
    let revision = old.body["pdf"]["revision"]["id"]
        .as_str()
        .ok_or_else(|| WorkbenchError::invalid("This build has no successful retained PDF"))?;
    let count = read_document(store, ws, revision, 0, None)?
        .page_count
        .ok_or_else(|| WorkbenchError::invalid("Page count is unavailable"))?;
    if pages.is_empty() || pages.len() > 100 || pages.iter().any(|p| *p == 0 || *p as usize > count)
    {
        return Err(WorkbenchError::invalid("Choose existing PDF pages"));
    }
    bounded(note, 4000)?;
    let mut body = old.body;
    let mut inspected = body["inspectedPages"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for page in pages {
        if !inspected.contains(&json!(page)) {
            inspected.push(json!(page));
        }
    }
    body["inspectedPages"] = json!(inspected);
    body["pageInspection"] = json!("researcher_recorded");
    body["pageInspectionNote"] = json!(note);
    body["pageInspectedAt"] = json!(now());
    put(store, ws, &object_id, "build", old.revision, &body)?;
    Ok(body)
}
