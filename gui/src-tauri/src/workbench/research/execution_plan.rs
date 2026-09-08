//! Captured execution plans. Fresh copies provide input identity, not host containment.
use super::execution::{
    get_execution_profile, input_manifest, launch_manifest, safe_relative_path, ExecutionProfile,
    HostExecutionPreview,
};
use super::*;
use crate::workbench::desk::{self, DeskRecord};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPlan {
    pub schema_version: u32,
    pub research_inputs: Vec<desk::ResearchObjectRef>,
    pub profile: ExecutionProfile,
    pub input_manifest: Value,
    pub captured_files: Vec<CapturedFile>,
    pub parameters: Value,
    pub random_seed: Option<String>,
    pub environment: Value,
    pub launch: Value,
    pub platform: String,
    pub toolchain_version: String,
    pub containment: String,
    pub dependency_coverage: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedFile {
    pub path: String,
    pub artifact_id: String,
    pub hash: String,
    pub executable: bool,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturePlanRequest {
    #[serde(default)]
    pub research_inputs: Vec<desk::ResearchObjectRef>,
    pub profile_id: String,
    pub parameters: Value,
    pub random_seed: Option<String>,
    pub toolchain_version: String,
    pub operation_id: String,
}
fn plan(store: &Store, ws: &str, id: &str) -> WorkbenchResult<(DeskRecord, ExecutionPlan)> {
    let r = desk::record(store, ws, id)?;
    if r.kind != "execution_plan" {
        return Err(WorkbenchError::invalid("Select a captured execution plan"));
    }
    let p = serde_json::from_value(r.body.clone())
        .map_err(|e| WorkbenchError::storage("Invalid execution plan", e))?;
    Ok((r, p))
}
pub fn capture(store: &Store, r: CapturePlanRequest) -> WorkbenchResult<DeskRecord> {
    let profile = get_execution_profile(store, &r.profile_id)?;
    let _guard = super::super::project::execution_lock(store, &profile.workspace_id)?;
    super::preview_host_execution(store, &profile.id)?; // scoped root, complete inputs and executable identity
    if profile.inputs.is_empty() {
        return Err(WorkbenchError::invalid(
            "Declare the script and inputs before capturing a plan",
        ));
    }
    let parameters = serde_json::to_vec(&r.parameters)
        .map_err(|e| WorkbenchError::storage("Plan parameters", e))?;
    if parameters.len() > 16 * 1024 || !r.parameters.is_object() {
        return Err(WorkbenchError::invalid(
            "Plan parameters must be an object under 16 KiB",
        ));
    }
    let secret_key = |key: &str| {
        let k = key.to_ascii_uppercase();
        ["KEY", "TOKEN", "PASSWORD", "SECRET", "CREDENTIAL"]
            .iter()
            .any(|s| k.contains(s))
    };
    if r.parameters
        .as_object()
        .unwrap()
        .keys()
        .chain(
            profile
                .environment
                .as_object()
                .into_iter()
                .flat_map(|m| m.keys()),
        )
        .any(|k| secret_key(k))
    {
        return Err(WorkbenchError::invalid(
            "Keep credentials out of captured plan parameters and environment",
        ));
    }
    desk::check_text(&r.toolchain_version, 1000)?;
    let mut profile = profile;
    let root =
        fs::canonicalize(&profile.cwd).map_err(|e| WorkbenchError::storage("Plan root", e))?;
    // Absolute directly referenced scripts must be converted by the researcher;
    // silently rewriting arbitrary shell strings would change their meaning.
    for arg in profile.argv.iter().skip(1) {
        if arg.contains(&profile.cwd) {
            return Err(WorkbenchError::invalid("Use paths relative to the profile cwd before capture, including inside oldstata commands"));
        }
    }
    let launch = launch_manifest(&profile)?;
    for file in launch["launchFiles"].as_array().into_iter().flatten() {
        let path = Path::new(file["path"].as_str().unwrap_or(""));
        let relative = path
            .strip_prefix(&root)
            .map_err(|_| WorkbenchError::invalid("Declare launch scripts inside the project cwd"))?
            .to_string_lossy()
            .to_string();
        if !profile.inputs.contains(&relative) {
            return Err(WorkbenchError::invalid(format!(
                "Add launch script {relative} to the declared inputs"
            )));
        }
    }
    let (manifest, _, _) = input_manifest(&profile)?;
    let total = manifest["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| f["sizeBytes"].as_u64().unwrap_or(0))
        .sum::<u64>();
    if manifest["complete"] != true || total > 256 * 1024 * 1024 {
        return Err(WorkbenchError::invalid(
            "A capture requires complete declared inputs totaling at most 256 MiB",
        ));
    }
    if r.research_inputs.len() > 32 {
        return Err(WorkbenchError::invalid(
            "At most 32 research input references",
        ));
    }
    for input in &r.research_inputs {
        if ["result","record"].contains(&input.kind.as_str()) {
            crate::workbench::search::read_object(store,&profile.workspace_id,input,1)?;
            continue;
        }
        let d = desk::record(store, &profile.workspace_id, &input.id)?;
        if !["dataset", "sample"].contains(&input.kind.as_str())
            || d.kind != input.kind
            || d.content_hash != input.revision
        {
            return Err(WorkbenchError::invalid(
                "Plan inputs must be exact datasets, samples, numeric results, or project records",
            ));
        }
        if d.kind == "dataset"
            && !manifest["files"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|f| f["contentHash"] == d.body["contentHash"])
        {
            return Err(WorkbenchError::invalid(
                "The selected dataset bytes must be among the profile's declared input files",
            ));
        }
    }
    let mut captured = Vec::new();
    for file in manifest["files"].as_array().into_iter().flatten() {
        let relative = file["path"]
            .as_str()
            .ok_or_else(|| WorkbenchError::invalid("Invalid manifest path"))?;
        safe_relative_path("Captured input", relative)?;
        let source = root.join(relative);
        let canonical =
            fs::canonicalize(&source).map_err(|e| WorkbenchError::storage("Capture path", e))?;
        if !canonical.starts_with(&root)
            || fs::symlink_metadata(&source)
                .map_err(|e| WorkbenchError::storage("Capture metadata", e))?
                .file_type()
                .is_symlink()
        {
            return Err(WorkbenchError::invalid(
                "Captured inputs must be regular files within cwd",
            ));
        }
        let hash = file["contentHash"].as_str().unwrap_or("");
        let dest = store
            .root_path()
            .join("blobs")
            .join(format!("{hash}.capture"));
        copy_immutable(&source, &dest, hash)?;
        let artifact_id = new_id("capture")?;
        open_connection(store)?.execute("INSERT OR IGNORE INTO artifacts(id,workspace_id,content_hash,media_kind,size_bytes,origin,storage_reference,created_at) VALUES(?1,?2,?3,'capture',?4,'execution_plan',?5,?6)",params![artifact_id,profile.workspace_id,hash,file["sizeBytes"].as_u64().unwrap_or(0) as i64,dest.to_string_lossy(),now()]).map_err(|e|WorkbenchError::storage("Capture artifact",e))?;
        let artifact_id=open_connection(store)?.query_row("SELECT id FROM artifacts WHERE workspace_id=?1 AND content_hash=?2 AND media_kind='capture'",params![profile.workspace_id,hash],|r|r.get(0)).map_err(|e|WorkbenchError::storage("Capture artifact",e))?;
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(source)
                .map_err(|e| WorkbenchError::storage("Capture mode", e))?
                .permissions()
                .mode()
                & 0o111
                != 0
        };
        #[cfg(not(unix))]
        let executable = false;
        captured.push(CapturedFile {
            path: relative.into(),
            artifact_id,
            hash: hash.into(),
            executable,
        });
    }
    if input_manifest(&profile)?.0["files"] != manifest["files"] {
        return Err(WorkbenchError::conflict(
            "Inputs changed during capture; capture again",
        ));
    }
    // Explicit standard parameters and seed file can be consumed by all adapters.
    profile.environment["PIPELINE_PARAMETERS_FILE"] = json!("pipeline-parameters.json");
    if let Some(seed) = &r.random_seed {
        desk::check_text(seed, 100)?;
        profile.environment["PIPELINE_RANDOM_SEED"] = json!(seed);
    }
    let environment = json!({"variables":profile.environment,"locale":std::env::var("LANG").ok(),"threads":profile.environment.get("OMP_NUM_THREADS"),"lockfiles":captured.iter().filter(|f|["requirements.txt","uv.lock","poetry.lock","renv.lock","Manifest.toml","Project.toml"].iter().any(|n|f.path.ends_with(n))).map(|f|f.path.clone()).collect::<Vec<_>>(),"coverage":"declared environment plus inherited host environment; packages are not provisioned or contained"});
    let plan = ExecutionPlan {
        schema_version: 1,
        research_inputs: r.research_inputs,
        profile: profile.clone(),
        input_manifest: manifest,
        captured_files: captured,
        parameters: r.parameters,
        random_seed: r.random_seed,
        environment,
        launch,
        platform: std::env::consts::OS.into(),
        toolchain_version: r.toolchain_version,
        containment: "host_access; fresh captured cwd; no filesystem or network containment".into(),
        dependency_coverage: "declared_only".into(),
    };
    desk::insert(
        store,
        &profile.workspace_id,
        "execution_plan",
        &profile.name,
        serde_json::to_value(plan).map_err(|e| WorkbenchError::storage("Plan record", e))?,
        None,
        &r.operation_id,
    )
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStatus {
    pub record: DeskRecord,
    pub authorized: bool,
    pub test_status: Option<String>,
}
pub fn status(store: &Store, ws: &str, id: &str) -> WorkbenchResult<PlanStatus> {
    let (record, _) = plan(store, ws, id)?;
    let (authorized,test_status)=open_connection(store)?.query_row("SELECT authorized,test_status FROM execution_plan_state WHERE plan_id=?1 AND fingerprint=?2",params![id,record.content_hash],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|WorkbenchError::storage("Plan status",e))?.unwrap_or((false,None));
    Ok(PlanStatus {
        record,
        authorized,
        test_status,
    })
}
pub fn authorize(
    store: &Store,
    ws: &str,
    id: &str,
    fingerprint: &str,
) -> WorkbenchResult<PlanStatus> {
    let (r, p) = plan(store, ws, id)?;
    if r.content_hash != fingerprint {
        return Err(WorkbenchError::conflict("Plan changed during review"));
    }
    let current = launch_manifest(&p.profile)?;
    if current["hash"] != p.launch["hash"]
        || current["shellStartupFiles"] != p.launch["shellStartupFiles"]
        || current["pythonEnvironment"] != p.launch["pythonEnvironment"]
    {
        return Err(WorkbenchError::conflict(
            "Executable or shell startup changed; capture a new plan",
        ));
    }
    open_connection(store)?.execute("INSERT INTO execution_plan_state(plan_id,fingerprint,authorized) VALUES(?1,?2,1) ON CONFLICT(plan_id) DO UPDATE SET fingerprint=?2,authorized=1",params![id,fingerprint]).map_err(|e|WorkbenchError::storage("Plan authorization",e))?;
    status(store, ws, id)
}
pub(crate) fn stage(
    store: &Store,
    ws: &str,
    id: &str,
    execution_id: &str,
    test_only: bool,
) -> WorkbenchResult<(ExecutionProfile, HostExecutionPreview, Value)> {
    let state = status(store, ws, id)?;
    if !state.authorized || (!test_only && state.test_status.as_deref() != Some("passed")) {
        return Err(WorkbenchError::invalid(
            "Review, authorize and successfully test this exact captured plan before running it",
        ));
    }
    let (_, p) = plan(store, ws, id)?;
    let cwd = store
        .root_path()
        .join("jobs")
        .join(format!("captured-{execution_id}"));
    fs::create_dir(&cwd).map_err(|e| WorkbenchError::storage("Fresh captured run directory", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&cwd, fs::Permissions::from_mode(0o700))
            .map_err(|e| WorkbenchError::storage("Private run directory", e))?;
    }
    for f in &p.captured_files {
        safe_relative_path("Captured input", &f.path)?;
        let source:String=open_connection(store)?.query_row("SELECT storage_reference FROM artifacts WHERE id=?1 AND workspace_id=?2 AND content_hash=?3",params![f.artifact_id,ws,f.hash],|r|r.get(0)).map_err(|e|WorkbenchError::storage("Captured input artifact",e))?;
        let source = fs::canonicalize(source)
            .map_err(|e| WorkbenchError::storage("Captured artifact", e))?;
        if !source.starts_with(
            store
                .root_path()
                .join("blobs")
                .canonicalize()
                .map_err(|e| WorkbenchError::storage("Private blob root", e))?,
        ) {
            return Err(WorkbenchError::invalid("Capture escaped blob storage"));
        }
        let dest = cwd.join(&f.path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| WorkbenchError::storage("Captured directories", e))?;
        }
        fs::copy(source, &dest).map_err(|e| WorkbenchError::storage("Copy captured bytes", e))?;
        if hash_file(&dest)?.0 != f.hash {
            return Err(WorkbenchError::invalid("Captured input integrity failed"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &dest,
                fs::Permissions::from_mode(if f.executable { 0o700 } else { 0o600 }),
            )
            .map_err(|e| WorkbenchError::storage("Captured input permissions", e))?;
        }
    }
    if p.profile
        .inputs
        .iter()
        .any(|f| f == "pipeline-parameters.json")
    {
        return Err(WorkbenchError::invalid(
            "pipeline-parameters.json is reserved for plan parameters",
        ));
    }
    fs::write(
        cwd.join("pipeline-parameters.json"),
        serde_json::to_vec_pretty(&p.parameters)
            .map_err(|e| WorkbenchError::storage("Plan parameters", e))?,
    )
    .map_err(|e| WorkbenchError::storage("Stage parameters", e))?;
    let mut profile = p.profile;
    profile.cwd = cwd.to_string_lossy().into_owned();
    profile.inputs.push("pipeline-parameters.json".into());
    let launch = launch_manifest(&profile)?;
    if launch["hash"] != p.launch["hash"]
        || launch["shellStartupFiles"] != p.launch["shellStartupFiles"]
        || launch["pythonEnvironment"] != p.launch["pythonEnvironment"]
    {
        return Err(WorkbenchError::conflict(
            "The executable or shell startup changed after capture",
        ));
    }
    let (mut manifest, _, _) = input_manifest(&profile)?;
    manifest["snapshotConsistency"] = json!("captured_inputs_verified");
    manifest["planId"] = json!(id);
    manifest["planFingerprint"] = json!(state.record.content_hash);
    manifest["executionContainment"] = json!(p.containment);
    manifest["environment"] = p.environment;
    manifest["researchInputs"] = json!(p.research_inputs);
    let authorization = HostExecutionPreview {
        profile_id: profile.id.clone(),
        fingerprint: state.record.content_hash,
        command: profile.argv.clone(),
        cwd: profile.cwd.clone(),
        inputs: manifest.clone(),
        launch,
        outputs: profile.outputs.clone(),
        authorized: true,
        boundary: p.containment,
    };
    Ok((profile, authorization, manifest))
}
pub(crate) fn record_test(store: &Store, id: &str, outcome: &str) -> WorkbenchResult<()> {
    open_connection(store)?
        .execute(
            "UPDATE execution_plan_state SET test_status=?2 WHERE plan_id=?1",
            params![
                id,
                if outcome == "completed" {
                    "passed"
                } else {
                    "failed"
                }
            ],
        )
        .map_err(|e| WorkbenchError::storage("Plan test status", e))?;
    Ok(())
}
