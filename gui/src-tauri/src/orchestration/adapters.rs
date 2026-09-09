use super::{
    definition::{Action, Binding, Chain, Step},
    state::Ready,
    store::*,
};
use crate::workbench::{commands::run_store, tasks};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    path::Path,
};

pub async fn prepare(chain: &Chain, session: Option<String>, inputs: &Value) -> Result<Scope> {
    super::definition::validate(chain)?;
    let mut scope = Scope::default();
    if let Some(session) = session {
        let binding = run_store(move |s| tasks::binding(&s, &session))
            .await
            .map_err(|e| e.message)?;
        scope.root_identity = Some(binding.root_identity);
        scope.session_id = Some(binding.session_id);
        scope.workspace_id = binding.workspace_id;
        scope.session_cursor = Some(binding.cursor);
        scope.harness_fingerprint = Some(binding.harness_fingerprint);
        scope.runtime_root = Some(binding.runtime_root);
    }
    let mut all = Vec::new();
    flatten(&chain.steps, &mut all);
    for step in all {
        match &step.action {
            Action::LiteratureLookup { .. } => {
                let ws = scope
                    .workspace_id
                    .clone()
                    .ok_or("Literature lookup requires a project")?;
                if !run_store(move |s| crate::workbench::acquisition::network_enabled(&s, &ws))
                    .await
                    .map_err(|e| e.message)?
                {
                    return Err(
                        "Enable the project's literature acquisition before preparing this lookup"
                            .into(),
                    );
                }
            }
            Action::Workspace { .. } | Action::Deliver { .. } if scope.session_id.is_none() => {
                return Err("Choose a Workspace conversation for this chain".into())
            }
            Action::Review { profile_id, .. } => {
                let key = profile_id.clone();
                let pin =
                    tokio::task::spawn_blocking(move || crate::commands::orchestration::pin(&key))
                        .await
                        .map_err(err)??;
                if let Action::Review { variables, .. } = &step.action {
                    let document =
                        crate::pipeline_config::parse_workflow_document_strict(&pin.workflow_json)?;
                    if document
                        .config
                        .variables
                        .iter()
                        .any(|v| v.secret && variables.contains_key(&v.key))
                    {
                        return Err("Secret Review variables cannot be stored in task definitions. Configure them in Review settings.".into());
                    }
                }
                scope.profiles.insert(profile_id.clone(), pin);
            }
            Action::Check { profile_id } => {
                let workspace = scope
                    .workspace_id
                    .clone()
                    .ok_or("Checks require a Workspace project")?;
                let profile = profile_id.clone();
                let preview = run_store(move |s| {
                    if !crate::workbench::research::list_execution_profiles(&s, &workspace)?
                        .iter()
                        .any(|p| p.id == profile)
                    {
                        return Err(crate::workbench::store::WorkbenchError::invalid(
                            "Check belongs to another project",
                        ));
                    }
                    crate::workbench::research::preview_host_execution(&s, &profile)
                })
                .await
                .map_err(|e| e.message)?;
                if !preview.authorized {
                    return Err(format!(
                        "Authorize the selected check '{}' in Research tools first",
                        profile_id
                    ));
                }
                scope.checks.insert(profile_id.clone(), preview.fingerprint);
            }
            Action::CapturedCheck { plan_id } => {
                let workspace = scope
                    .workspace_id
                    .clone()
                    .ok_or("Captured checks require a Workspace project")?;
                let plan = plan_id.clone();
                let status = run_store(move |s| {
                    crate::workbench::research::execution_plan::status(&s, &workspace, &plan)
                })
                .await
                .map_err(|e| e.message)?;
                if !status.authorized {
                    return Err("Authorize this exact captured plan in Research tools first".into());
                }
                scope
                    .captured_checks
                    .insert(plan_id.clone(), status.record.content_hash);
            }
            _ => {}
        }
    }
    if !inputs.is_object() {
        return Err("Task inputs must be an object".into());
    }
    Ok(scope)
}
fn flatten<'a>(steps: &'a [Step], out: &mut Vec<&'a Step>) {
    for step in steps {
        out.push(step);
        match &step.action {
            Action::If {
                then_steps,
                else_steps,
                ..
            } => {
                flatten(then_steps, out);
                flatten(else_steps, out);
            }
            Action::Repeat { steps, .. }
            | Action::While { steps, .. }
            | Action::ForEach { steps, .. } => flatten(steps, out),
            Action::Parallel { branches } => {
                for branch in branches {
                    flatten(branch, out);
                }
            }
            Action::Chain { chain } => flatten(&chain.steps, out),
            _ => {}
        }
    }
}
pub fn task_binding(scope: &Scope) -> Result<tasks::TaskBinding> {
    Ok(tasks::TaskBinding {
        session_id: scope.session_id.clone().ok_or("No conversation selected")?,
        workspace_id: scope.workspace_id.clone(),
        cursor: scope.session_cursor.clone().unwrap_or_default(),
        harness_fingerprint: scope
            .harness_fingerprint
            .clone()
            .ok_or("No research settings snapshot")?,
        root_identity: scope
            .root_identity
            .clone()
            .ok_or("Task root identity is missing; refresh context")?,
        runtime_root: scope
            .runtime_root
            .clone()
            .ok_or("No Workspace runtime root")?,
    })
}
pub fn available(action: &Action, scope: &Scope) -> bool {
    match action {
        Action::Workspace { .. } | Action::Check { .. } | Action::CapturedCheck { .. } => {
            crate::workbench::commands::task_turn_available()
                && !scope
                    .workspace_id
                    .as_deref()
                    .is_some_and(crate::workbench::research::jobs::workspace_active)
        }
        Action::Review { .. } => crate::commands::orchestration::available(),
        _ => true,
    }
}
fn resolve(binding: &Binding, run: &TaskRun, ready: &Ready) -> Result<Value> {
    binding
        .resolve(&run.progress.outputs, &ready.inputs)
        .ok_or_else(|| format!("{}: required output is unavailable", ready.step.label))
}
pub fn atomic_json(path: &Path, value: &Value) -> Result<()> {
    let parent = path.parent().ok_or("Invalid artifact path")?;
    std::fs::create_dir_all(parent).map_err(err)?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(err)?;
    serde_json::to_writer(&mut file, value).map_err(err)?;
    file.flush().map_err(err)?;
    file.as_file().sync_all().map_err(err)?;
    file.persist(path).map_err(err)?;
    Ok(())
}
pub fn snapshot(store: &Store, scope: &Scope, input: Value, filename: &str) -> Result<Value> {
    if filename.is_empty() || filename.contains(['/', '\\']) || filename == "." || filename == ".."
    {
        return Err("Snapshot requires a plain filename".into());
    }
    let bytes = if let Some(path) = input.get("path").and_then(Value::as_str) {
        let root = scope
            .runtime_root
            .as_ref()
            .ok_or("File snapshots need a bound Workspace folder")?;
        if scope.root_identity.as_deref()
            != Some(
                &crate::workbench::store::root_identity(Path::new(root)).map_err(|e| e.message)?,
            )
        {
            return Err("Task folder identity changed".into());
        }
        let selected = Path::new(root).join(path);
        let files = crate::workbench::project::task_capture(Path::new(root), &selected)
            .map_err(|e| e.message)?;
        if selected.is_dir() {
            return snapshot_tree(store, files);
        }
        files.into_iter().next().ok_or("Snapshot is empty")?.1
    } else if let Some(text) = input.as_str() {
        text.as_bytes().to_vec()
    } else {
        return Err("Snapshot input must be text or a scoped file path".into());
    };
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("Snapshot exceeds 64 MiB".into());
    }
    use sha2::{Digest, Sha256};
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let dir = store.root.join("blobs").join(&digest);
    std::fs::create_dir_all(&dir).map_err(err)?;
    let path = dir.join(filename);
    if !path.exists() {
        let mut temp = tempfile::NamedTempFile::new_in(&dir).map_err(err)?;
        temp.write_all(&bytes).map_err(err)?;
        temp.as_file().sync_all().map_err(err)?;
        temp.persist(&path).map_err(err)?;
    }
    Ok(json!({"kind":"artifact","path":path,"hash":digest,"filename":filename,"bytes":bytes.len()}))
}
pub fn artifact_path(store: &Store, value: &Value) -> Result<String> {
    let path = value
        .get("path")
        .and_then(Value::as_str)
        .ok_or("Review needs an immutable snapshot output")?;
    let digest = value
        .get("hash")
        .and_then(Value::as_str)
        .ok_or("Artifact hash is missing")?;
    if ![Some("artifact"), Some("artifactTree")]
        .contains(&value.get("kind").and_then(Value::as_str))
    {
        return Err("Review input is not an artifact".into());
    }
    let canonical = Path::new(path).canonicalize().map_err(err)?;
    let root = store.root.join("blobs").canonicalize().map_err(err)?;
    if !canonical.starts_with(&root) {
        return Err("Review input is outside task artifact storage".into());
    }
    if value["kind"] == "artifactTree" {
        let files = crate::workbench::project::task_capture(&canonical, &canonical)
            .map_err(|e| e.message)?;
        let manifest = tree_manifest(&files);
        if hash(&manifest)? != digest {
            return Err("Snapshot tree changed after capture".into());
        }
        return Ok(canonical.to_string_lossy().into_owned());
    }
    let mut file = crate::safety::open_regular_file(&canonical)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    use sha2::{Digest, Sha256};
    if bytes.len() > 64 * 1024 * 1024 || format!("{:x}", Sha256::digest(bytes)) != digest {
        return Err("Review input changed after it was captured".into());
    }
    Ok(canonical.to_string_lossy().into_owned())
}
/// Completed adapter results are journaled before the coordinator consumes them.
pub async fn execute(
    app: Option<tauri::AppHandle>,
    store: Store,
    run: TaskRun,
    ready: Ready,
    operation: String,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> Result<Value> {
    if *stop.borrow() {
        return Err("Task stopped before dispatch".into());
    }
    let journal = store
        .root
        .join("actions")
        .join(&operation)
        .join("result.json");
    if journal.exists() {
        let bytes = std::fs::read(&journal).map_err(err)?;
        return serde_json::from_slice(&bytes).map_err(err);
    }
    #[cfg(all(feature = "e2e", debug_assertions))]
    if matches!(ready.step.action, Action::Workspace { .. }) {
        if let Some(result) = super::missions::qualification::scripted(&store, &run)? {
            atomic_json(&journal, &result)?;
            return Ok(result);
        }
    }
    let result = match &ready.step.action {
        Action::LiteratureLookup { query } => {
            crate::workbench::discovery::acquire(
                run.scope
                    .workspace_id
                    .clone()
                    .ok_or("Literature lookup needs a project")?,
                query.clone(),
                operation.clone(),
            )
            .await?
        }
        Action::Snapshot {
            input,
            filename,
            require_change,
        } => {
            let value = resolve(input, &run, &ready)?;
            let local = store.clone();
            let scope = run.scope.clone();
            let filename = filename.clone();
            let result =
                tokio::task::spawn_blocking(move || snapshot(&local, &scope, value, &filename))
                    .await
                    .map_err(err)??;
            if *require_change
                && run.progress.outputs.as_object().is_some_and(|o| {
                    o.values()
                        .any(|v| v.get("hash").is_some() && v["hash"] == result["hash"])
                })
            {
                return Err(
                    "The revised artifact is unchanged. Inspect the revision before retrying."
                        .into(),
                );
            }
            result
        }
        Action::Workspace {
            prompt,
            model,
            effort,
        } => {
            let text =
                super::definition::render_prompt(prompt, &run.progress.outputs, &ready.inputs)?;
            let binding = task_binding(&run.scope)?;
            let session = binding.session_id.clone();
            let submission = format!("task-{operation}");
            let previous_session = session.clone();
            let previous_op = submission.clone();
            let previous = run_store(move |s| tasks::outcome(&s, &previous_session, &previous_op))
                .await
                .map_err(|e| e.message)?;
            let (thread, turn) = if let Some(previous) = previous {
                if previous["state"] == "completed" {
                    atomic_json(&journal, &previous)?;
                    return Ok(previous);
                }
                (
                    previous["threadId"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    previous["turnId"].as_str().unwrap_or_default().to_string(),
                )
            } else {
                let sent = crate::workbench::commands::submit_turn(
                    app.clone()
                        .ok_or("Workspace execution requires the desktop runtime")?,
                    crate::workbench::commands::SendTurnRequest {
                        session_id: session.clone(),
                        text,
                        client_submission_id: submission.clone(),
                        model: model.clone(),
                        effort: effort.clone(),
                    },
                    Some(binding),
                )
                .await
                .map_err(|e| {
                    serde_json::to_value(e)
                        .ok()
                        .and_then(|v| v["message"].as_str().map(str::to_string))
                        .unwrap_or_else(|| "Workspace submission outcome is unknown".into())
                })?;
                (sent.thread_id, sent.turn_id)
            };
            atomic_json(
                &store
                    .root
                    .join("actions")
                    .join(&operation)
                    .join("child.json"),
                &json!({"kind":"workspace","sessionId":session,"submissionId":submission,"threadId":thread,"turnId":turn}),
            )?;
            loop {
                if *stop.borrow() {
                    let _ = crate::workbench::commands::workbench_codex_interrupt_turn(
                        thread.clone(),
                        turn.clone(),
                    )
                    .await;
                    return Err("Task stopped; Workspace turn interrupted".into());
                }
                let ss = session.clone();
                let op = submission.clone();
                if let Some(value) = run_store(move |s| tasks::outcome(&s, &ss, &op))
                    .await
                    .map_err(|e| e.message)?
                {
                    match value["state"].as_str() {
                        Some("completed") => break value,
                        Some("failed" | "interrupted" | "cancelled") => {
                            return Err(format!("Workspace turn {}", value["state"]))
                        }
                        _ => {}
                    }
                }
                if let Some(question) =
                    crate::workbench::commands::task_pending_request(&thread, &turn)
                {
                    let _ = crate::workbench::commands::workbench_codex_interrupt_turn(
                        thread.clone(),
                        turn.clone(),
                    )
                    .await;
                    return Err(format!("Workspace needs your input: {question}. The turn was interrupted; answer in the conversation, then refresh task context."));
                }
                tokio::select! {_=tokio::time::sleep(std::time::Duration::from_millis(500))=>{},_=stop.changed()=>{}}
            }
        }
        Action::Review {
            profile_id,
            input,
            interpretation,
            variables,
        } => {
            let artifact = resolve(input, &run, &ready)?;
            let path = artifact_path(&store, &artifact)?;
            atomic_json(
                &store
                    .root
                    .join("actions")
                    .join(&operation)
                    .join("review-input.json"),
                &artifact,
            )?;
            let pinned = run
                .scope
                .profiles
                .get(profile_id)
                .cloned()
                .ok_or("Review profile was not prepared")?;
            let future = crate::commands::orchestration::execute(
                operation.clone(),
                pinned,
                path,
                interpretation.clone(),
                variables.clone(),
                crate::emit::background(&crate::emit::from_app(
                    app.clone()
                        .ok_or("Review execution requires the desktop runtime")?,
                )),
                stop.clone(),
            );
            tokio::pin!(future);
            let value = tokio::select! {v=&mut future=>v?,_=stop.changed()=>{crate::commands::orchestration::cancel(&operation);let _=future.await;return Err("Task review stopped".into());}};
            atomic_json(
                &store
                    .root
                    .join("actions")
                    .join(&operation)
                    .join("review-raw.json"),
                &value,
            )?;
            normalize_review(value, artifact)?
        }
        Action::Check { .. } | Action::CapturedCheck { .. } => {
            let workspace = run
                .scope
                .workspace_id
                .clone()
                .ok_or("Checks require a Workspace project")?;
            let (profile, plan, expected) = match &ready.step.action {
                Action::Check { profile_id } => (
                    profile_id.clone(),
                    None,
                    run.scope
                        .checks
                        .get(profile_id)
                        .cloned()
                        .ok_or("Check was not prepared")?,
                ),
                Action::CapturedCheck { plan_id } => (
                    String::new(),
                    Some(plan_id.clone()),
                    run.scope
                        .captured_checks
                        .get(plan_id)
                        .cloned()
                        .ok_or("Captured check was not prepared")?,
                ),
                _ => unreachable!(),
            };
            let session = run.scope.session_id.clone();
            let op = operation.clone();
            let receipt=run_store(move|s| {
                if let Some(plan_id) = plan {
                    let status = crate::workbench::research::execution_plan::status(&s,&workspace,&plan_id)?;
                    if !status.authorized || status.record.content_hash != expected { return Err(crate::workbench::store::WorkbenchError::conflict("Captured check inputs or authorization changed")); }
                    let captured: crate::workbench::research::execution_plan::ExecutionPlan = serde_json::from_value(status.record.body).map_err(|e|crate::workbench::store::WorkbenchError::invalid(e.to_string()))?;
                    return crate::workbench::research::jobs::queue(&s,crate::workbench::research::RunExecutionRequest{plan_id:Some(plan_id),profile_id:captured.profile.id,session_id:session,test_only:true,operation_id:op},"detached",None);
                }
                if !crate::workbench::research::list_execution_profiles(&s,&workspace)?.iter().any(|p|p.id==profile) {return Err(crate::workbench::store::WorkbenchError::invalid("Check belongs to another project"));}
                let preview=crate::workbench::research::preview_host_execution(&s,&profile)?;
                if preview.fingerprint!=expected||!preview.authorized{return Err(crate::workbench::store::WorkbenchError::conflict("Check inputs or authorization changed. Reauthorize this check before continuing."));}
                crate::workbench::research::jobs::queue(&s,crate::workbench::research::RunExecutionRequest{plan_id:None,profile_id:profile,session_id:session,test_only:false,operation_id:op},"detached",None)
            }).await.map_err(|e|e.message)?;
            crate::workbench::research::jobs::launch_pending();
            let execution = receipt.id.clone();
            atomic_json(
                &store
                    .root
                    .join("actions")
                    .join(&operation)
                    .join("child.json"),
                &json!({"kind":"check","executionId":execution}),
            )?;
            loop {
                if *stop.borrow() {
                    let ex = execution.clone();
                    let _ = run_store(move |s| {
                        crate::workbench::research::cancel_execution(
                            &s,
                            crate::workbench::research::CancelExecutionRequest { execution_id: ex },
                        )
                    })
                    .await;
                    return Err("Task check stopped".into());
                }
                let ex = execution.clone();
                let value = run_store(move |s| crate::workbench::research::get_execution(&s, &ex))
                    .await
                    .map_err(|e| e.message)?;
                if !["queued", "running"].contains(&value.outcome.as_str()) {
                    break json!({"passed":value.outcome=="completed","receipt":value});
                }
                tokio::select! {_=tokio::time::sleep(std::time::Duration::from_millis(500))=>{},_=stop.changed()=>{}}
            }
        }
        Action::Deliver { input } => {
            let value = resolve(input, &run, &ready)?;
            crate::workbench::commands::deliver_task(
                task_binding(&run.scope)?,
                operation.clone(),
                value,
            )
            .await?
        }
        _ => return Err("Control steps must execute in the coordinator".into()),
    };
    if result.to_string().len() > 1024 * 1024 {
        return Err("Task action output exceeds 1 MiB; select a smaller result".into());
    }
    atomic_json(&journal, &result)?;
    Ok(result)
}
pub(crate) fn normalize_review(value: Value, artifact: Value) -> Result<Value> {
    let report: crate::models::PipelineReport =
        serde_json::from_value(value["report"].clone()).map_err(err)?;
    let findings = report.products.findings.as_ref();
    let high = findings
        .map(|f| f.findings.iter().filter(|f| f.priority == "high").count())
        .unwrap_or(0);
    let unknown = findings
        .map(|f| {
            f.findings
                .iter()
                .filter(|f| !["high", "medium", "low"].contains(&f.priority.as_str()))
                .count()
        })
        .unwrap_or(1);
    let complete =
        report.quality.status == "done" && report.failed_steps.is_empty() && findings.is_some();
    let mut markdown = value["markdown"].as_str().unwrap_or_default().to_string();
    if markdown.len() > 128 * 1024 {
        let mut n = 128 * 1024;
        while !markdown.is_char_boundary(n) {
            n -= 1;
        }
        markdown.truncate(n);
        markdown.push_str(
            "\n[Report excerpt truncated; open the retained Review run for the full report.]",
        );
    }
    let text = value["extracted_text"]
        .as_str()
        .filter(|v| v.len() <= 128 * 1024);
    let mut selected = Vec::new();
    let mut bytes = 0usize;
    if let Some(product) = findings {
        for finding in &product.findings {
            let value = serde_json::to_value(finding).map_err(err)?;
            let size = value.to_string().len();
            if bytes + size > 128 * 1024 || selected.len() >= 200 {
                break;
            }
            bytes += size;
            selected.push(value);
        }
    }
    Ok(
        json!({"revisionTextAvailable":text.is_some(),"findingsTruncated":findings.is_some_and(|f|f.findings.len()>selected.len()),"runId":value["run_id"],"complete":complete,"highPriorityCount":high,"unknownPriorityCount":unknown,"report":markdown,"paperText":text,"reviewedArtifact":artifact,"quality":report.quality,"findings":selected}),
    )
}

fn tree_manifest(files: &[(String, Vec<u8>)]) -> Value {
    use sha2::{Digest, Sha256};
    json!(files.iter().map(|(path,bytes)|json!({"path":path,"hash":format!("{:x}",Sha256::digest(bytes)),"bytes":bytes.len()})).collect::<Vec<_>>())
}
fn snapshot_tree(store: &Store, files: Vec<(String, Vec<u8>)>) -> Result<Value> {
    if files.is_empty() {
        return Err("Snapshot folder is empty".into());
    }
    let manifest = tree_manifest(&files);
    let digest = hash(&manifest)?;
    let root = store.root.join("blobs");
    std::fs::create_dir_all(&root).map_err(err)?;
    let destination = root.join(format!("tree-{digest}"));
    if !destination.exists() {
        let temp = tempfile::tempdir_in(&root).map_err(err)?;
        for (name, bytes) in &files {
            let path = temp.path().join(name);
            std::fs::create_dir_all(path.parent().ok_or("Invalid snapshot name")?).map_err(err)?;
            let mut file = std::fs::File::create(path).map_err(err)?;
            file.write_all(bytes).map_err(err)?;
            file.sync_all().map_err(err)?;
        }
        std::fs::rename(temp.path(), &destination).map_err(err)?;
    }
    Ok(
        json!({"kind":"artifactTree","path":destination,"hash":digest,"manifest":manifest,"filename":"paper-project","bytes":files.iter().map(|(_,b)|b.len()).sum::<usize>()}),
    )
}
pub fn recover_review(store: &Store, operation: &str) -> Result<Option<Value>> {
    let root = store.root.join("actions").join(operation);
    let input = root.join("review-input.json");
    if !input.is_file() {
        return Ok(None);
    }
    let read = |path: &Path| -> Result<Value> {
        serde_json::from_slice(&std::fs::read(path).map_err(err)?).map_err(err)
    };
    let artifact = read(&input)?;
    let raw = root.join("review-raw.json");
    let result = if raw.is_file() {
        Some(read(&raw)?)
    } else if root.join("review-run.json").is_file() {
        let metadata = read(&root.join("review-run.json"))?;
        crate::commands::orchestration::recovered_result(
            metadata["runId"]
                .as_str()
                .ok_or("Review receipt omitted run identity")?,
        )?
    } else {
        None
    };
    if let Some(value) = result {
        atomic_json(&raw, &value)?;
        let result = normalize_review(value, artifact)?;
        atomic_json(&root.join("result.json"), &result)?;
        Ok(Some(result))
    } else {
        Ok(None)
    }
}

/// History rows keep small previews. The full immutable response is loaded only
/// when requested; bindings use the latest full value for each step.
pub fn output_preview(value: &Value) -> Value {
    fn compact(v: &Value, depth: usize) -> Value {
        if depth > 5 {
            return json!("[Open output for nested content]");
        }
        match v {
            Value::String(s) if s.len() > 2048 => {
                let mut n = 2048;
                while !s.is_char_boundary(n) {
                    n -= 1;
                }
                json!(format!("{}… [Open output for the full response]", &s[..n]))
            }
            Value::Array(a) => {
                Value::Array(a.iter().take(12).map(|v| compact(v, depth + 1)).collect())
            }
            Value::Object(o) => Value::Object(
                o.iter()
                    .take(32)
                    .map(|(k, v)| (k.clone(), compact(v, depth + 1)))
                    .collect(),
            ),
            _ => v.clone(),
        }
    }
    compact(value, 0)
}
