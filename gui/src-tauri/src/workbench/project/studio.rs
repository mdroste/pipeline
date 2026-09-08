//! Typed, optional manuscript, review, result and source work surfaces.
use super::*;
mod literature;
mod manuscript;
mod results;
mod review;
mod theory;
pub use literature::*;
pub use manuscript::*;
pub use results::*;
pub use review::*;
pub use theory::*;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StudioAction {
    SaveText {
        checkpoint_id: Option<String>,
        path: String,
        expected_hash: String,
        content: String,
    },
    SaveBuild {
        id: Option<String>,
        expected_revision: i64,
        config: BuildConfig,
    },
    InspectBuild {
        execution_id: String,
    },
    InspectPages {
        execution_id: String,
        pages: Vec<u32>,
        note: String,
    },
    ImportFindings {
        package: FindingPackage,
        selected: Vec<String>,
    },
    ImportReport {
        revision_id: String,
        comments: Vec<ReportComment>,
    },
    SaveResponse {
        id: String,
        expected_revision: i64,
        response: ResponseDecision,
    },
    SaveExperiment {
        id: Option<String>,
        expected_revision: i64,
        experiment: Experiment,
    },
    SaveSpecification {
        id: Option<String>,
        expected_revision: i64,
        specification: Specification,
    },
    ImportResults {
        execution_id: String,
        artifact_id: String,
    },
    BindNumber {
        id: Option<String>,
        expected_revision: i64,
        binding: NumericBinding,
    },
    GenerateValues {
        checkpoint_id: String,
        path: String,
        values: Vec<ValueMacro>,
    },
    ImportBibliography {
        revision_id: String,
        selected: Vec<String>,
    },
    SaveLiterature {
        id: Option<String>,
        expected_revision: i64,
        note: LiteratureNote,
    },
    SaveTheory {
        id: Option<String>,
        expected_revision: i64,
        note: TheoryNote,
    },
    SaveCheck {
        id: Option<String>,
        expected_revision: i64,
        check: TheoryCheck,
    },
    SaveDirection {
        id: Option<String>,
        expected_revision: i64,
        direction: ResearchDirection,
    },
    ConvertDirection {
        id: String,
        expected_revision: i64,
    },
    PromoteTheory {
        checkpoint_id: String,
        path: String,
        theory_id: String,
        after_line: Option<usize>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioMutation {
    pub workspace_id: String,
    pub operation_id: String,
    #[serde(flatten)]
    pub action: StudioAction,
}

pub fn studio_mutate(store: &Store, request: StudioMutation) -> WorkbenchResult<Value> {
    scope(store, &request.workspace_id)?;
    valid_id(&request.operation_id)?;
    let _guard = lock(store, &request.workspace_id)?;
    let digest = hash(&serde_json::to_vec(&request).map_err(err)?);
    let previous: Option<(String, String)> = store
        .connection()?
        .query_row(
            "SELECT request_hash,response_json FROM project_operations WHERE operation_id=?1",
            [&request.operation_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(err)?;
    if let Some((old, result)) = previous {
        if old != digest {
            return Err(WorkbenchError::conflict(
                "Operation ID reused with different arguments",
            ));
        }
        return serde_json::from_str(&result).map_err(err);
    }
    store.connection()?.execute("INSERT INTO project_operations VALUES(?1,?2,?3,?4)",params![request.operation_id,digest,json!({"outcome":"unknown","recovery":"Refresh and inspect records; no automatic retry"}).to_string(),now()]).map_err(err)?;
    let ws = &request.workspace_id;
    let value = match request.action {
        StudioAction::SaveText {
            checkpoint_id,
            path,
            expected_hash,
            content,
        } => save_text(
            store,
            ws,
            checkpoint_id.as_deref(),
            &path,
            &expected_hash,
            &content,
        )?,
        StudioAction::SaveBuild {
            id,
            expected_revision,
            config,
        } => serde_json::to_value(save_build(
            store,
            ws,
            id.as_deref(),
            expected_revision,
            config,
        )?)
        .map_err(err)?,
        StudioAction::InspectBuild { execution_id } => inspect_build(store, ws, &execution_id)?,
        StudioAction::InspectPages {
            execution_id,
            pages,
            note,
        } => inspect_pages(store, ws, &execution_id, pages, &note)?,
        StudioAction::ImportFindings { package, selected } => {
            import_findings(store, ws, package, &selected)?
        }
        StudioAction::ImportReport {
            revision_id,
            comments,
        } => import_report(store, ws, &revision_id, comments)?,
        StudioAction::SaveResponse {
            id,
            expected_revision,
            response,
        } => serde_json::to_value(save_response(store, ws, &id, expected_revision, response)?)
            .map_err(err)?,
        StudioAction::SaveExperiment {
            id,
            expected_revision,
            experiment,
        } => serde_json::to_value(save_experiment(
            store,
            ws,
            id.as_deref(),
            expected_revision,
            experiment,
        )?)
        .map_err(err)?,
        StudioAction::SaveSpecification {
            id,
            expected_revision,
            specification,
        } => serde_json::to_value(save_specification(
            store,
            ws,
            id.as_deref(),
            expected_revision,
            specification,
        )?)
        .map_err(err)?,
        StudioAction::ImportResults {
            execution_id,
            artifact_id,
        } => import_results(store, ws, &execution_id, &artifact_id)?,
        StudioAction::BindNumber {
            id,
            expected_revision,
            binding,
        } => serde_json::to_value(bind_number(
            store,
            ws,
            id.as_deref(),
            expected_revision,
            binding,
        )?)
        .map_err(err)?,
        StudioAction::GenerateValues {
            checkpoint_id,
            path,
            values,
        } => generate_values(store, ws, &checkpoint_id, &path, values)?,
        StudioAction::ImportBibliography {
            revision_id,
            selected,
        } => import_bibliography(store, ws, &revision_id, &selected)?,
        StudioAction::SaveLiterature {
            id,
            expected_revision,
            note,
        } => serde_json::to_value(save_literature(
            store,
            ws,
            id.as_deref(),
            expected_revision,
            note,
        )?)
        .map_err(err)?,
        StudioAction::SaveTheory {
            id,
            expected_revision,
            note,
        } => serde_json::to_value(save_theory(
            store,
            ws,
            id.as_deref(),
            expected_revision,
            note,
        )?)
        .map_err(err)?,
        StudioAction::SaveCheck {
            id,
            expected_revision,
            check,
        } => serde_json::to_value(save_check(
            store,
            ws,
            id.as_deref(),
            expected_revision,
            check,
        )?)
        .map_err(err)?,
        StudioAction::SaveDirection {
            id,
            expected_revision,
            direction,
        } => serde_json::to_value(save_direction(
            store,
            ws,
            id.as_deref(),
            expected_revision,
            direction,
        )?)
        .map_err(err)?,
        StudioAction::ConvertDirection {
            id,
            expected_revision,
        } => convert_direction(store, ws, &id, expected_revision)?,
        StudioAction::PromoteTheory {
            checkpoint_id,
            path,
            theory_id,
            after_line,
        } => promote_theory(store, ws, &checkpoint_id, &path, &theory_id, after_line)?,
    };
    store
        .connection()?
        .execute(
            "UPDATE project_operations SET response_json=?2 WHERE operation_id=?1",
            params![request.operation_id, value.to_string()],
        )
        .map_err(err)?;
    Ok(value)
}

pub fn studio_records(store: &Store, ws: &str, kind: &str) -> WorkbenchResult<Vec<ProjectRecord>> {
    if !matches!(
        kind,
        "build"
            | "response"
            | "experiment"
            | "specification"
            | "series"
            | "binding"
            | "bibliography"
            | "literature"
            | "theory"
            | "check"
            | "direction"
    ) {
        return Err(WorkbenchError::invalid("Unknown research surface"));
    }
    records(store, ws, kind)
}
pub fn studio_history(store: &Store, ws: &str, object_id: &str) -> WorkbenchResult<Vec<Value>> {
    scope(store, ws)?;
    valid_id(object_id)?;
    let conn = store.connection()?;
    let mut stmt=conn.prepare("SELECT revision,body_json,updated_at FROM project_record_history WHERE object_id=?1 AND workspace_id=?2 ORDER BY revision DESC LIMIT 100").map_err(err)?;
    let rows = stmt
        .query_map(params![object_id, ws], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    rows.into_iter().map(|(revision,body,at)|Ok(json!({"revision":revision,"body":serde_json::from_str::<Value>(&body).map_err(err)?,"updatedAt":at}))).collect()
}
fn execution(store: &Store, ws: &str, id: &str) -> WorkbenchResult<research::ResearchExecution> {
    let e = research::get_execution(store, id)?;
    if e.workspace_id != ws {
        return Err(WorkbenchError::invalid(
            "Execution belongs to another project",
        ));
    }
    Ok(e)
}
fn artifact_bytes(
    store: &Store,
    ws: &str,
    e: &research::ResearchExecution,
    artifact_id: &str,
) -> WorkbenchResult<(Vec<u8>, Value)> {
    let artifact = e.output_manifest["artifacts"]
        .as_array()
        .and_then(|a| {
            a.iter()
                .find(|a| a["artifactId"].as_str() == Some(artifact_id))
        })
        .ok_or_else(|| {
            WorkbenchError::invalid("Artifact is not an adopted output of this execution")
        })?
        .clone();
    let (path, digest): (String, String) = store
        .connection()?
        .query_row(
            "SELECT storage_reference,content_hash FROM artifacts WHERE id=?1 AND workspace_id=?2",
            params![artifact_id, ws],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(err)?;
    let path = fs::canonicalize(path).map_err(err)?;
    if !path.starts_with(fs::canonicalize(store.root_path().join("blobs")).map_err(err)?) {
        return Err(WorkbenchError::invalid("Artifact escaped private storage"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(err)?
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > MAX_FILE || hash(&bytes) != digest || artifact["contentHash"] != digest
    {
        return Err(WorkbenchError::invalid(
            "Artifact identity failed validation",
        ));
    }
    Ok((bytes, artifact))
}
fn new_or_id(object_id: Option<&str>) -> WorkbenchResult<String> {
    match object_id {
        Some(s) => {
            valid_id(s)?;
            Ok(s.into())
        }
        None => id("research"),
    }
}
fn exact_text(store: &Store, ws: &str, revision: &str) -> WorkbenchResult<String> {
    let view = read_document(store, ws, revision, 0, None)?;
    if view.total_bytes > 2 * 1024 * 1024 {
        return Err(WorkbenchError::invalid(
            "This operation supports documents through 2 MiB",
        ));
    }
    let mut text = view.text;
    let mut offset = view.end;
    while offset < view.total_bytes {
        let part = read_document(store, ws, revision, offset, None)?;
        if part.end <= offset {
            break;
        }
        offset = part.end;
        text.push_str(&part.text);
    }
    Ok(text)
}

pub fn stage_publication(
    store: &Store,
    ws: &str,
    checkpoint: &str,
    path: &str,
    expected: Option<&str>,
    bytes: &[u8],
) -> WorkbenchResult<Value> {
    stage_publication_files(
        store,
        ws,
        checkpoint,
        vec![(path.into(), expected.map(str::to_owned), bytes.to_vec())],
    )
}
pub fn stage_publication_files(
    store: &Store,
    ws: &str,
    checkpoint: &str,
    files_to_write: Vec<(String, Option<String>, Vec<u8>)>,
) -> WorkbenchResult<Value> {
    scope(store, ws)?;
    let _guard = lock(store, ws)?;
    if files_to_write.len() > 100
        || files_to_write.iter().map(|f| f.2.len()).sum::<usize>() > 64 * 1024 * 1024
    {
        return Err(WorkbenchError::invalid(
            "Staged publication exceeds its file or byte limit",
        ));
    }
    let root = files::SafeRoot::open(&session_task_root(store, ws, checkpoint)?)?;
    for (path, expected, bytes) in &files_to_write {
        files::relative(path)?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(WorkbenchError::invalid(
                "Publication artifact exceeds 8 MiB",
            ));
        }
        let current = root.optional_read(path)?;
        if current.as_deref() != Some(bytes.as_slice()) && current.as_deref().map(hash) != *expected
        {
            return Err(WorkbenchError::conflict(format!(
                "Destination {path} changed; refresh the task copy before staging"
            )));
        }
        if root.executable(path)? {
            return Err(WorkbenchError::invalid(
                "Publication files cannot overwrite an executable file",
            ));
        }
    }
    let mut staged = Vec::new();
    for (path, expected, bytes) in files_to_write {
        if root.optional_read(&path)?.as_deref() != Some(bytes.as_slice()) {
            let claim = id("publication")?;
            root.replace(
                &path,
                expected.as_deref(),
                false,
                Some(&bytes),
                false,
                &claim,
            )?;
            root.clear_claim(&path, &claim)?;
        }
        staged.push(json!({"path":path,"hash":hash(&bytes)}));
    }
    let checkpoint = tasks::capture_changes(store, ws, checkpoint)?;
    Ok(
        json!({"checkpointId":checkpoint.id,"checkpointRevision":checkpoint.revision,"files":staged,"accepted":false,"checksValid":false}),
    )
}
