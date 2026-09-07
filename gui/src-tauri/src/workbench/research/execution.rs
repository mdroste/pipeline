//! Explicitly authorized host execution profiles, receipts, and validation.

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionProfile {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub adapter: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub environment: Value,
    pub inputs: Vec<String>,
    pub timeout_seconds: u64,
    pub outputs: Vec<String>,
    pub tested_at: Option<String>,
    pub test_status: Option<String>,
    pub revision: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveExecutionProfileRequest {
    pub profile_id: Option<String>,
    pub workspace_id: String,
    pub name: String,
    pub adapter: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub environment: Value,
    pub inputs: Vec<String>,
    pub timeout_seconds: u64,
    pub outputs: Vec<String>,
    pub expected_revision: Option<i64>,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResearchExecution {
    pub id: String,
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub profile_id: Option<String>,
    pub adapter: String,
    pub command: Vec<String>,
    pub cwd: String,
    pub input_manifest: Value,
    pub dependency_hash: String,
    pub outcome: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub exit_status: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub output_manifest: Value,
    pub validation: Value,
    pub snapshot_consistency: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunExecutionRequest {
    pub profile_id: String,
    pub session_id: Option<String>,
    pub test_only: bool,
    pub operation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelExecutionRequest {
    pub execution_id: String,
}

static EXECUTION_PIDS: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
static CANCELLED_EXECUTIONS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn execution_pids() -> &'static Mutex<HashMap<String, u32>> {
    EXECUTION_PIDS.get_or_init(|| Mutex::new(HashMap::new()))
}
pub(crate) fn cancelled_executions() -> &'static Mutex<HashSet<String>> {
    CANCELLED_EXECUTIONS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn safe_relative_path(label: &str, value: &str) -> WorkbenchResult<()> {
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > 4_096
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(WorkbenchError::invalid(format!(
            "{label} must be a normalized relative path"
        )));
    }
    Ok(())
}

fn validate_profile_request(
    store: &Store,
    request: &SaveExecutionProfileRequest,
) -> WorkbenchResult<(PathBuf, Value, String, String)> {
    validate_id("workspace id", &request.workspace_id)?;
    let name = validate_text("Execution profile name", &request.name, 300)?;
    if !matches!(request.adapter.as_str(), "latex" | "stata" | "command") {
        return Err(WorkbenchError::invalid(
            "Execution adapter must be latex, stata, or command",
        ));
    }
    if request.argv.is_empty()
        || request.argv.len() > 64
        || request.argv.iter().any(|argument| {
            argument.is_empty() || argument.len() > 8_192 || argument.contains('\0')
        })
    {
        return Err(WorkbenchError::invalid(
            "Execution argv must contain 1 to 64 bounded arguments",
        ));
    }
    if !(1..=7_200).contains(&request.timeout_seconds) {
        return Err(WorkbenchError::invalid(
            "Execution timeout must be between 1 and 7200 seconds",
        ));
    }
    if request.inputs.len() > 1_000 || request.outputs.len() > 100 {
        return Err(WorkbenchError::invalid(
            "Execution input or output declaration is too large",
        ));
    }
    for path in request.inputs.iter().chain(request.outputs.iter()) {
        safe_relative_path("Execution input/output", path)?;
    }
    let (environment, environment_json) = json_object(
        "Execution environment",
        request.environment.clone(),
        64 * 1024,
    )?;
    let secret_names = [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GOOGLE_API_KEY",
        "AWS_SECRET_ACCESS_KEY",
        "GITHUB_TOKEN",
    ];
    if environment.as_object().is_some_and(|values| {
        values.iter().any(|(key, value)| {
            key.is_empty()
                || key.contains('=')
                || key.chars().any(char::is_control)
                || secret_names.contains(&key.as_str())
                || !value.is_string()
        })
    }) {
        return Err(WorkbenchError::invalid("Execution environment must contain string values and cannot persist credential variables"));
    }
    let program_name = Path::new(&request.argv[0])
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if [
        "stata", "stata-mp", "stata-se", "statamp", "statase", "stataic",
    ]
    .contains(&program_name.as_str())
    {
        return Err(WorkbenchError::invalid("Direct Stata executables are not permitted. Configure the Stata adapter with /bin/zsh -lic and oldstata."));
    }
    if request.adapter != "stata"
        && request
            .argv
            .iter()
            .any(|a| a.trim_start().starts_with("oldstata "))
    {
        return Err(WorkbenchError::invalid("oldstata commands require the Stata adapter so log validation and cleanup handling remain active."));
    }
    if request.adapter == "stata"
        && (request.argv.len() != 3
            || request.argv[0] != "/bin/zsh"
            || request.argv[1] != "-lic"
            || !request.argv[2].trim_start().starts_with("oldstata ")
            || [";", "&&", "||", "|", "\n", "`", "$("]
                .iter()
                .any(|token| request.argv[2].contains(token)))
    {
        return Err(WorkbenchError::invalid(
            "Stata profiles on this Mac must invoke /bin/zsh -lic with an oldstata command",
        ));
    }
    if request.adapter == "latex" {
        let executable = Path::new(&request.argv[0])
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !matches!(
            executable,
            "latexmk" | "tectonic" | "pdflatex" | "xelatex" | "lualatex"
        ) || request.argv.iter().any(|argument| {
            (argument.contains("shell-escape") && argument != "-no-shell-escape")
                || argument.contains("write18")
        }) {
            return Err(WorkbenchError::invalid(
                "LaTeX profiles require a supported engine and cannot enable shell escape",
            ));
        }
    }
    let connection = open_connection(store)?;
    let root: String = connection.query_row("SELECT root FROM workspaces WHERE id = ?1 AND root IS NOT NULL AND archived_at IS NULL", [&request.workspace_id], |row| row.get(0)).optional().map_err(|error| WorkbenchError::storage("Failed to inspect execution workspace", error))?.ok_or_else(|| WorkbenchError::invalid("Execution requires an active Workspace with a registered folder"))?;
    let root = fs::canonicalize(root)
        .map_err(|error| WorkbenchError::storage("Failed to resolve execution workspace", error))?;
    let cwd = fs::canonicalize(&request.cwd)
        .map_err(|error| WorkbenchError::storage("Failed to resolve execution directory", error))?;
    if !cwd.is_dir() || !cwd.starts_with(&root) {
        return Err(WorkbenchError::invalid(
            "Execution cwd must be inside the registered Workspace folder",
        ));
    }
    Ok((cwd, environment, environment_json, name))
}

fn profile_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExecutionProfile> {
    let argv: String = row.get(4)?;
    let environment: String = row.get(6)?;
    let inputs: String = row.get(7)?;
    let outputs: String = row.get(9)?;
    let adapter: String = row.get(3)?;
    let mut argv: Vec<String> = serde_json::from_str(&argv).unwrap_or_default();
    if adapter == "latex" && !argv.first().is_some_and(|s| s.ends_with("tectonic")) {
        if !argv.iter().any(|s| s == "-no-shell-escape") {
            argv.insert(1.min(argv.len()), "-no-shell-escape".into());
        }
        if argv
            .first()
            .is_some_and(|s| Path::new(s).file_name().and_then(|s| s.to_str()) == Some("latexmk"))
            && !argv.iter().any(|s| s == "-norc")
        {
            argv.insert(1, "-norc".into());
        }
    }
    Ok(ExecutionProfile {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        name: row.get(2)?,
        adapter,
        argv,
        cwd: row.get(5)?,
        environment: serde_json::from_str(&environment).unwrap_or(Value::Null),
        inputs: serde_json::from_str(&inputs).unwrap_or_default(),
        timeout_seconds: row.get::<_, i64>(8)? as u64,
        outputs: serde_json::from_str(&outputs).unwrap_or_default(),
        tested_at: row.get(10)?,
        test_status: row.get(11)?,
        revision: row.get(12)?,
    })
}

pub fn save_execution_profile(
    store: &Store,
    request: SaveExecutionProfileRequest,
) -> WorkbenchResult<ExecutionProfile> {
    let (cwd, _environment, environment_json, name) = validate_profile_request(store, &request)?;
    let id = match request.profile_id.as_deref() {
        Some(id) => {
            validate_id("execution profile id", id)?;
            id.to_string()
        }
        None => new_id("execprofile")?,
    };
    let argv = serde_json::to_string(&request.argv)
        .map_err(|error| WorkbenchError::storage("Failed to encode execution argv", error))?;
    let inputs = serde_json::to_string(&request.inputs)
        .map_err(|error| WorkbenchError::storage("Failed to encode execution inputs", error))?;
    let outputs = serde_json::to_string(&request.outputs)
        .map_err(|error| WorkbenchError::storage("Failed to encode execution outputs", error))?;
    let mut connection = open_connection(store)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            WorkbenchError::storage("Failed to start execution profile update", error)
        })?;
    let existing_workspace: Option<String> = transaction
        .query_row(
            "SELECT workspace_id FROM execution_profiles WHERE id=?1",
            [&id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to scope execution profile", error))?;
    if existing_workspace
        .as_deref()
        .is_some_and(|workspace| workspace != request.workspace_id)
    {
        return Err(WorkbenchError::invalid(
            "Execution profile belongs to another workspace",
        ));
    }
    let current: Option<i64> = transaction
        .query_row(
            "SELECT revision FROM execution_profiles WHERE id = ?1 AND workspace_id = ?2",
            params![id, request.workspace_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to inspect execution profile", error))?;
    if request.expected_revision.unwrap_or(current.unwrap_or(0)) != current.unwrap_or(0) {
        return Err(WorkbenchError::invalid(
            "Execution profile changed; refresh before saving",
        ));
    }
    let revision = current.unwrap_or(0) + 1;
    let timestamp = now();
    transaction.execute("INSERT INTO execution_profiles (id, workspace_id, name, adapter, argv_json, cwd, environment_json, inputs_json, timeout_seconds, outputs_json, revision, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12) ON CONFLICT(id) DO UPDATE SET name=excluded.name, adapter=excluded.adapter, argv_json=excluded.argv_json, cwd=excluded.cwd, environment_json=excluded.environment_json, inputs_json=excluded.inputs_json, timeout_seconds=excluded.timeout_seconds, outputs_json=excluded.outputs_json, tested_at=NULL, test_status=NULL, revision=excluded.revision, updated_at=excluded.updated_at", params![id, request.workspace_id, name, request.adapter, argv, cwd.to_string_lossy(), environment_json, inputs, request.timeout_seconds as i64, outputs, revision, timestamp]).map_err(|error| WorkbenchError::storage("Failed to save execution profile", error))?;
    append_change(
        &transaction,
        &request.operation_id,
        "execution_profile",
        &id,
        (Some(&request.workspace_id), None),
        "saved",
        &json!({"revision":revision,"adapter":request.adapter}),
    )?;
    transaction
        .commit()
        .map_err(|error| WorkbenchError::storage("Failed to commit execution profile", error))?;
    get_execution_profile(store, &id)
}

pub(crate) fn get_execution_profile(
    store: &Store,
    profile_id: &str,
) -> WorkbenchResult<ExecutionProfile> {
    open_connection(store)?.query_row("SELECT id, workspace_id, name, adapter, argv_json, cwd, environment_json, inputs_json, timeout_seconds, outputs_json, tested_at, test_status, revision FROM execution_profiles WHERE id = ?1", [profile_id], profile_from_row).optional().map_err(|error| WorkbenchError::storage("Failed to read execution profile", error))?.ok_or_else(|| WorkbenchError::invalid("Execution profile was not found"))
}

pub fn list_execution_profiles(
    store: &Store,
    workspace_id: &str,
) -> WorkbenchResult<Vec<ExecutionProfile>> {
    validate_id("workspace id", workspace_id)?;
    let connection = open_connection(store)?;
    let mut statement = connection.prepare("SELECT id, workspace_id, name, adapter, argv_json, cwd, environment_json, inputs_json, timeout_seconds, outputs_json, tested_at, test_status, revision FROM execution_profiles WHERE workspace_id = ?1 ORDER BY name, id LIMIT 200").map_err(|error| WorkbenchError::storage("Failed to prepare execution profile list", error))?;
    let profiles = statement
        .query_map([workspace_id], profile_from_row)
        .map_err(|error| WorkbenchError::storage("Failed to list execution profiles", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode execution profiles", error))?;
    Ok(profiles)
}

pub(crate) fn input_manifest(
    profile: &ExecutionProfile,
) -> WorkbenchResult<(Value, String, String)> {
    let cwd = Path::new(&profile.cwd);
    let mut files = Vec::new();
    let mut complete = true;
    for relative in &profile.inputs {
        let path = cwd.join(relative);
        match hash_file(&path) {
            Ok((hash, size)) => {
                files.push(json!({"path":relative,"contentHash":hash,"sizeBytes":size}))
            }
            Err(_) => {
                complete = false;
                files.push(json!({"path":relative,"missing":true}));
            }
        }
    }
    let manifest = json!({"complete":complete,"declarationComplete":complete,"dependencyCoverage":"declared_only","snapshotConsistency":"unverified_live_files","files":files});
    let encoded =
        serde_json::to_vec(&json!({"profile":profile,"manifest":manifest})).map_err(|error| {
            WorkbenchError::storage("Failed to encode execution dependency manifest", error)
        })?;
    Ok((
        manifest,
        hash_bytes(&encoded),
        if complete { "uncertain" } else { "partial" }.to_string(),
    ))
}

fn drain_capped<R: Read>(mut reader: R, limit: usize, job: &str, stream: &str) -> (Vec<u8>, bool) {
    let mut kept = Vec::new();
    let mut truncated = false;
    let mut buffer = [0u8; 16 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                super::jobs::append_log(job, stream, &buffer[..count]);
                let take = count.min(limit.saturating_sub(kept.len()));
                kept.extend_from_slice(&buffer[..take]);
                truncated |= take < count;
            }
        }
    }
    (kept, truncated)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExecutionOutput {
    outcome: String,
    status: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_truncated: bool,
    stderr_truncated: bool,
    wrapper_completed_after_stop: bool,
    process_started: bool,
}

pub(crate) fn run_profile_process(
    execution_id: &str,
    profile: &ExecutionProfile,
    authorized_launch: &Value,
) -> Result<ExecutionOutput, String> {
    let launch = launch_manifest(profile).map_err(|e| e.message)?;
    if &launch != authorized_launch {
        return Err("The launch program changed after authorization".into());
    }
    let program = launch["executable"]
        .as_str()
        .ok_or("Execution program was not resolved")?;
    let mut command = Command::new(program);
    // Multicall tools such as pdflatex -> pdftex select behavior from argv[0].
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.arg0(&profile.argv[0]);
    }
    command
        .args(&profile.argv[1..])
        .current_dir(&profile.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(environment) = profile.environment.as_object() {
        for (key, value) in environment {
            if let Some(value) = value.as_str() {
                command.env(key, value);
            }
        }
    }
    for secret in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GOOGLE_API_KEY",
        "AWS_SECRET_ACCESS_KEY",
        "GITHUB_TOKEN",
    ] {
        command.env_remove(secret);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start research execution: {error}"))?;
    let pid = child.id();
    crate::commands::lifecycle::register_independent_child_pid(pid);
    execution_pids()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(execution_id.to_string(), pid);
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_id = execution_id.to_string();
    let stderr_id = execution_id.to_string();
    let stdout_thread = std::thread::spawn(move || {
        stdout
            .map(|pipe| drain_capped(pipe, 4 * 1024 * 1024, &stdout_id, "stdout"))
            .unwrap_or_default()
    });
    let stderr_thread = std::thread::spawn(move || {
        stderr
            .map(|pipe| drain_capped(pipe, 4 * 1024 * 1024, &stderr_id, "stderr"))
            .unwrap_or_default()
    });
    let started = Instant::now();
    let mut wait_error = None;
    let mut stop_requested: Option<(Instant, &str)> = None;
    let mut wrapper_completed_after_stop = false;
    let (outcome, status) = loop {
        if cancelled_executions()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains(execution_id)
        {
            stop_requested.get_or_insert((Instant::now(), "interrupted"));
        } else if started.elapsed() >= Duration::from_secs(profile.timeout_seconds) {
            stop_requested.get_or_insert((Instant::now(), "timed_out"));
        }
        if let Some((requested_at, reason)) = stop_requested {
            // oldstata has no signal trap. Let it return normally through Legacy Time Off.
            // Forced termination is explicitly recorded as requiring cleanup verification.
            if profile.adapter != "stata" || requested_at.elapsed() >= Duration::from_secs(30) {
                crate::commands::lifecycle::kill_independent_process(pid);
                let _ = child.wait();
                break (reason.to_string(), None);
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if let Some((_, reason)) = stop_requested {
                    wrapper_completed_after_stop = profile.adapter == "stata";
                    break (reason.to_string(), status.code());
                }
                break (
                    (if status.success() {
                        "completed"
                    } else {
                        "failed"
                    })
                    .to_string(),
                    status.code(),
                );
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                crate::commands::lifecycle::kill_independent_process(pid);
                let _ = child.wait();
                wait_error = Some(format!(
                    "Failed while waiting for research execution: {error}"
                ));
                break ("outcome_unknown".to_string(), None);
            }
        }
    };
    execution_pids()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(execution_id);
    cancelled_executions()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(execution_id);
    // A completed launcher may leave descendants holding inherited output pipes.
    // Reap this execution's independent group before joining capture threads.
    crate::commands::lifecycle::kill_independent_process(pid);
    crate::commands::lifecycle::unregister_independent_child_pid(pid);
    let (stdout, stdout_truncated) = stdout_thread.join().unwrap_or_default();
    let (mut stderr, stderr_truncated) = stderr_thread.join().unwrap_or_default();
    if let Some(error) = wait_error {
        stderr.extend_from_slice(error.as_bytes());
    }
    Ok(ExecutionOutput {
        outcome,
        status,
        stdout,
        stderr,
        stdout_truncated,
        stderr_truncated,
        wrapper_completed_after_stop,
        process_started: true,
    })
}

fn execution_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchExecution> {
    let command: String = row.get(5)?;
    let input: String = row.get(7)?;
    let output: String = row.get(15)?;
    let validation: String = row.get(16)?;
    Ok(ResearchExecution {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        session_id: row.get(2)?,
        profile_id: row.get(3)?,
        adapter: row.get(4)?,
        command: serde_json::from_str(&command).unwrap_or_default(),
        cwd: row.get(6)?,
        input_manifest: serde_json::from_str(&input).unwrap_or(Value::Null),
        dependency_hash: row.get(8)?,
        outcome: row.get(9)?,
        started_at: row.get(10)?,
        ended_at: row.get(11)?,
        exit_status: row.get(12)?,
        stdout: row.get(13)?,
        stderr: row.get(14)?,
        output_manifest: serde_json::from_str(&output).unwrap_or(Value::Null),
        validation: serde_json::from_str(&validation).unwrap_or(Value::Null),
        snapshot_consistency: row.get(17)?,
        created_at: row.get(18)?,
    })
}

const EXECUTION_SELECT: &str = "SELECT id, workspace_id, session_id, profile_id, adapter, command_json, cwd, input_manifest_json, dependency_hash, outcome, started_at, ended_at, exit_status, stdout_text, stderr_text, output_manifest_json, validation_json, snapshot_consistency, created_at FROM research_executions";

pub(crate) fn get_execution(
    store: &Store,
    execution_id: &str,
) -> WorkbenchResult<ResearchExecution> {
    validate_id("execution id", execution_id)?;
    open_connection(store)?
        .query_row(
            &format!("{EXECUTION_SELECT} WHERE id = ?1"),
            [execution_id],
            execution_from_row,
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to read research execution", error))?
        .ok_or_else(|| WorkbenchError::invalid("Research execution was not found"))
}

pub fn list_executions(
    store: &Store,
    workspace_id: &str,
) -> WorkbenchResult<Vec<ResearchExecution>> {
    validate_id("workspace id", workspace_id)?;
    let connection = open_connection(store)?;
    let mut statement = connection
        .prepare(&format!(
            "{EXECUTION_SELECT} WHERE workspace_id = ?1 ORDER BY created_at DESC LIMIT 200"
        ))
        .map_err(|error| WorkbenchError::storage("Failed to prepare execution list", error))?;
    let values = statement
        .query_map([workspace_id], execution_from_row)
        .map_err(|error| WorkbenchError::storage("Failed to list executions", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode executions", error))?;
    Ok(values)
}

pub(super) fn validate_execution_outputs(
    profile: &ExecutionProfile,
    outcome: &str,
) -> (Value, bool) {
    let mut checks = Vec::new();
    let mut valid = outcome == "completed";
    for relative in &profile.outputs {
        let path = Path::new(&profile.cwd).join(relative);
        let present = path.is_file();
        checks.push(json!({"path":relative,"present":present}));
        valid &= present;
    }
    if profile.adapter == "latex" {
        let pdf = profile.outputs.iter().any(|path| {
            path.to_ascii_lowercase().ends_with(".pdf")
                && Path::new(&profile.cwd).join(path).is_file()
        });
        checks.push(json!({"check":"compiledPdf","passed":pdf}));
        valid &= pdf;
    }
    if profile.adapter == "stata" {
        static STATA_ERROR_CODE: OnceLock<regex::Regex> = OnceLock::new();
        let error_code = STATA_ERROR_CODE.get_or_init(|| {
            regex::Regex::new(r"(?m)^r\([0-9]+\);\s*$").expect("static Stata error-code regex")
        });
        let mut found_log = false;
        let mut clean = true;
        for relative in profile
            .outputs
            .iter()
            .filter(|path| path.to_ascii_lowercase().ends_with(".log"))
        {
            found_log = true;
            match fs::read_to_string(Path::new(&profile.cwd).join(relative)) {
                Ok(log) => clean &= !error_code.is_match(&log),
                Err(_) => clean = false,
            }
        }
        clean &= found_log;
        checks.push(json!({"check":"stataLogHasNoErrorCode","passed":clean}));
        valid &= clean;
    }
    (json!({"passed":valid,"checks":checks}), valid)
}

fn adopt_execution_outputs(
    store: &Store,
    profile: &ExecutionProfile,
    execution_id: &str,
) -> WorkbenchResult<Value> {
    let mut adopted = Vec::new();
    for relative in &profile.outputs {
        let source = Path::new(&profile.cwd).join(relative);
        if !source.is_file() {
            continue;
        }
        let (hash, size) = hash_file(&source)?;
        let suffix = source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("bin");
        let destination = store
            .root_path()
            .join("blobs")
            .join(format!("{hash}.{suffix}"));
        copy_immutable(&source, &destination, &hash)?;
        let artifact_id = new_id("artifact")?;
        open_connection(store)?.execute("INSERT OR IGNORE INTO artifacts (id, workspace_id, content_hash, media_kind, size_bytes, origin, storage_reference, original_path, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'research_execution', ?6, ?7, ?8)", params![artifact_id, profile.workspace_id, hash, suffix, size as i64, destination.to_string_lossy(), source.to_string_lossy(), now()]).map_err(|error| WorkbenchError::storage("Failed to adopt execution artifact", error))?;
        let artifact_id: String = open_connection(store)?.query_row("SELECT id FROM artifacts WHERE workspace_id=?1 AND content_hash=?2 AND media_kind=?3", params![profile.workspace_id,hash,suffix], |row|row.get(0)).map_err(|e|WorkbenchError::storage("Failed to resolve adopted artifact",e))?;
        adopted.push(json!({"path":relative,"contentHash":hash,"sizeBytes":size,"artifactId":artifact_id,"sourceExecutionId":execution_id}));
    }
    Ok(json!({"artifacts":adopted}))
}

pub(crate) struct PreparedExecution {
    pub store: Store,
    pub request: RunExecutionRequest,
    pub profile: ExecutionProfile,
    pub authorization: HostExecutionPreview,
    pub manifest: Value,
    pub before_outputs: HashMap<String, Option<std::time::SystemTime>>,
    pub execution_id: String,
    pub _project_guard: fs::File,
}
pub(crate) enum PreparedOrExisting {
    Prepared(Box<PreparedExecution>),
    Existing(Box<ResearchExecution>),
}

pub(crate) fn prepare_execution(
    store: &Store,
    request: RunExecutionRequest,
    tool_origin: Option<(&str, &str, &str)>,
) -> WorkbenchResult<PreparedOrExisting> {
    validate_id("execution profile id", &request.profile_id)?;
    let profile = get_execution_profile(store, &request.profile_id)?;
    let project_guard = super::super::project::execution_lock(store, &profile.workspace_id)?;
    let authorization = preview_host_execution(store, &profile.id)?;
    if !authorization.authorized {
        return Err(WorkbenchError::invalid("Review and authorize this exact host command and its current inputs before testing or running it"));
    }
    if let Some(session_id) = &request.session_id {
        if store
            .session_snapshot(session_id)?
            .session
            .overrides
            .get("projectCheckpointId")
            .is_some()
        {
            return Err(WorkbenchError::invalid("Workspace host profiles are unavailable in isolated task sessions; use the task sandbox"));
        }
    }
    if !request.test_only && profile.test_status.as_deref() != Some("passed") {
        return Err(WorkbenchError::invalid(
            "Run the profile test successfully before using it for research execution",
        ));
    }
    if let Some(session_id) = request.session_id.as_deref() {
        validate_id("session id", session_id)?;
        let belongs: bool = open_connection(store)?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE id=?1 AND workspace_id=?2)",
                params![session_id, profile.workspace_id],
                |row| row.get(0),
            )
            .map_err(|error| WorkbenchError::storage("Failed to scope execution session", error))?;
        if !belongs {
            return Err(WorkbenchError::invalid(
                "Execution session belongs to another workspace",
            ));
        }
    }
    let (mut manifest, dependency_hash, consistency) = input_manifest(&profile)?;
    let previous_operation: Option<(String, String)> = open_connection(store)?.query_row("SELECT entity_id,details_json FROM change_log WHERE operation_id=?1 AND entity_type='research_execution'", [&request.operation_id], |r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|WorkbenchError::storage("Failed to inspect prior execution",e))?;
    if let Some((id, details)) = previous_operation {
        let details: Value = serde_json::from_str(&details)
            .map_err(|e| WorkbenchError::storage("Invalid prior execution receipt", e))?;
        if details["request"]
            != serde_json::to_value(&request)
                .map_err(|e| WorkbenchError::storage("Failed to encode execution request", e))?
        {
            return Err(WorkbenchError::conflict("Execution operation ID was reused with different arguments or has an older incomplete receipt"));
        }
        return Ok(PreparedOrExisting::Existing(Box::new(get_execution(
            store, &id,
        )?)));
    }
    // Existing outputs cannot count as newly produced artifacts. Record their
    // metadata before launch; unchanged outputs remain explicitly unverified.
    let before_outputs = profile
        .outputs
        .iter()
        .map(|p| {
            let path = Path::new(&profile.cwd).join(p);
            (
                p.clone(),
                fs::metadata(path).ok().and_then(|m| m.modified().ok()),
            )
        })
        .collect::<HashMap<_, _>>();
    let execution_id = new_id("execution")?;
    let input_size = manifest["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| f["sizeBytes"].as_u64().unwrap_or(0))
        .sum::<u64>();
    if input_size <= 256 * 1024 * 1024 {
        let mut input_profile = profile.clone();
        input_profile.outputs = profile.inputs.clone();
        manifest["snapshots"] = adopt_execution_outputs(store, &input_profile, &execution_id)?;
    } else {
        manifest["snapshotCapture"] =
            json!("Declared inputs exceed the 256 MiB snapshot budget; hashes only");
    }
    let created = now();
    let command_json = serde_json::to_string(&profile.argv)
        .map_err(|error| WorkbenchError::storage("Failed to encode execution command", error))?;
    let (binding_id, provider_turn_id, tool_call_id) = tool_origin
        .map(|(binding, turn, call)| (Some(binding), Some(turn), Some(call)))
        .unwrap_or((None, None, None));
    let mut queue_connection = open_connection(store)?;
    let queue = queue_connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            WorkbenchError::storage("Failed to start research execution receipt", error)
        })?;
    queue.execute("INSERT INTO research_executions (id, workspace_id, session_id, binding_id, provider_turn_id, tool_call_id, profile_id, adapter, command_json, cwd, input_manifest_json, dependency_hash, outcome, output_manifest_json, validation_json, snapshot_consistency, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'queued', '{}', ?15, ?13, ?14)", params![execution_id, profile.workspace_id, request.session_id, binding_id, provider_turn_id, tool_call_id, profile.id, profile.adapter, command_json, profile.cwd, serde_json::to_string(&manifest).unwrap_or_default(), dependency_hash, consistency, created, json!({"processInstance":super::jobs::process_instance()}).to_string()]).map_err(|error| WorkbenchError::storage("Failed to queue research execution", error))?;
    append_change(
        &queue,
        &request.operation_id,
        "research_execution",
        &execution_id,
        (Some(&profile.workspace_id), request.session_id.as_deref()),
        "queued",
        &json!({"profileId":profile.id,"testOnly":request.test_only,"request":request}),
    )?;
    queue.commit().map_err(|error| {
        WorkbenchError::storage("Failed to commit research execution receipt", error)
    })?;
    Ok(PreparedOrExisting::Prepared(Box::new(PreparedExecution {
        store: store.clone(),
        request,
        profile,
        authorization,
        manifest,
        before_outputs,
        execution_id,
        _project_guard: project_guard,
    })))
}

pub(crate) fn start_execution(store: &Store, execution_id: &str) -> WorkbenchResult<()> {
    open_connection(store)?.execute("UPDATE research_executions SET outcome='running',started_at=?2 WHERE id=?1 AND outcome='queued'", params![execution_id,now()]).map_err(|e|WorkbenchError::storage("Failed to start execution",e))?;
    Ok(())
}
pub(crate) fn wait_execution(prepared: &PreparedExecution) -> ExecutionOutput {
    if cancelled_executions()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&prepared.execution_id)
    {
        return ExecutionOutput {
            outcome: "interrupted".into(),
            status: None,
            stdout: vec![],
            stderr: vec![],
            stdout_truncated: false,
            stderr_truncated: false,
            wrapper_completed_after_stop: true,
            process_started: false,
        };
    }
    if input_manifest(&prepared.profile)
        .map(|(m, _, _)| m["files"] != prepared.manifest["files"])
        .unwrap_or(true)
    {
        return ExecutionOutput { outcome:"failed".into(),status:None,stdout:vec![],stderr:b"Declared inputs changed while queued; no command was launched. Review and authorize current inputs.".to_vec(),stdout_truncated:false,stderr_truncated:false,wrapper_completed_after_stop:false,process_started:false };
    }
    run_profile_process(
        &prepared.execution_id,
        &prepared.profile,
        &prepared.authorization.launch,
    )
    .unwrap_or_else(|error| ExecutionOutput {
        outcome: "outcome_unknown".into(),
        status: None,
        stdout: Vec::new(),
        stderr: error.into_bytes(),
        stdout_truncated: false,
        stderr_truncated: false,
        wrapper_completed_after_stop: false,
        process_started: false,
    })
}
pub(crate) fn finalize_execution(
    store: &Store,
    prepared: PreparedExecution,
    output: ExecutionOutput,
) -> WorkbenchResult<ResearchExecution> {
    let PreparedExecution {
        request,
        profile,
        authorization,
        manifest,
        before_outputs,
        execution_id,
        _project_guard,
        store: _,
    } = prepared;
    let (validation, valid) = validate_execution_outputs(&profile, &output.outcome);
    let (after_manifest, _, _) = input_manifest(&profile)?;
    let inputs_changed = manifest["files"] != after_manifest["files"];
    let stale_outputs = profile
        .outputs
        .iter()
        .filter(|p| {
            let before = before_outputs.get(*p).and_then(|v| *v);
            before.is_some()
                && before
                    == fs::metadata(Path::new(&profile.cwd).join(p))
                        .ok()
                        .and_then(|m| m.modified().ok())
        })
        .cloned()
        .collect::<Vec<_>>();
    let valid = valid && stale_outputs.is_empty() && !inputs_changed;
    let outcome = if output.outcome == "completed" && !valid {
        "failed".to_string()
    } else {
        output.outcome
    };
    let outputs = match adopt_execution_outputs(store, &profile, &execution_id) {
        Ok(outputs) => outputs,
        Err(error) => {
            open_connection(store)?.execute("UPDATE research_executions SET outcome='outcome_unknown',ended_at=?2,validation_json=?3 WHERE id=?1",params![execution_id,now(),json!({"adoptionError":error.message}).to_string()]).map_err(|e|WorkbenchError::storage("Failed to record adoption failure",e))?;
            return Err(error);
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let validation = json!({"adapter":validation,"stdoutTruncated":output.stdout_truncated,"stderrTruncated":output.stderr_truncated,"executionBoundary":"explicitly_authorized_host","authorizationFingerprint":authorization.fingerprint,"authorizedLaunch":authorization.launch,"dependencyCoverage":"declared_only","inputsChanged":inputs_changed,"unchangedPreexistingOutputs":stale_outputs,"stataWrapperReturnedAfterStop":output.wrapper_completed_after_stop,"processStarted":output.process_started,"stataCleanupRequired":profile.adapter=="stata" && output.process_started && outcome!="completed" && !output.wrapper_completed_after_stop});
    let ended = now();
    open_connection(store)?.execute("UPDATE execution_jobs SET finalization_json=?2 WHERE execution_id=?1", params![execution_id,json!({"outcome":outcome,"endedAt":ended,"exitStatus":output.status,"stdout":stdout,"stderr":stderr,"outputs":outputs,"validation":validation,"testOnly":request.test_only,"profileId":profile.id,"profileRevision":profile.revision}).to_string()]).map_err(|e|WorkbenchError::storage("Failed to journal adopted execution",e))?;
    open_connection(store)?.execute("UPDATE research_executions SET outcome=?2, ended_at=?3, exit_status=?4, stdout_text=?5, stderr_text=?6, output_manifest_json=?7, validation_json=?8 WHERE id=?1", params![execution_id, outcome, ended, output.status, stdout, stderr, serde_json::to_string(&outputs).unwrap_or_default(), serde_json::to_string(&validation).unwrap_or_default()]).map_err(|error| WorkbenchError::storage("Failed to complete research execution receipt", error))?;
    if request.test_only {
        open_connection(store)?
            .execute(
                "UPDATE execution_profiles SET tested_at=?2, test_status=?3 WHERE id=?1 AND revision=?4",
                params![
                    profile.id,
                    ended,
                    if outcome == "completed" {
                        "passed"
                    } else {
                        "failed"
                    },
                    profile.revision
                ],
            )
            .map_err(|error| {
                WorkbenchError::storage("Failed to record execution profile test", error)
            })?;
    }
    get_execution(store, &execution_id)
}

#[cfg(test)]
pub fn run_execution(
    store: &Store,
    request: RunExecutionRequest,
) -> WorkbenchResult<ResearchExecution> {
    match prepare_execution(store, request, None)? {
        PreparedOrExisting::Existing(e) => Ok(*e),
        PreparedOrExisting::Prepared(p) => {
            start_execution(store, &p.execution_id)?;
            let output = wait_execution(&p);
            finalize_execution(store, *p, output)
        }
    }
}

pub fn cancel_executions_for_provider_turn(
    store: &Store,
    provider_thread_id: &str,
    provider_turn_id: &str,
) -> WorkbenchResult<usize> {
    provider_identifier("provider thread id", provider_thread_id)?;
    provider_identifier("provider turn id", provider_turn_id)?;
    let connection = open_connection(store)?;
    let mut statement = connection.prepare("SELECT e.id FROM research_executions e JOIN session_bindings b ON b.id=e.binding_id WHERE b.provider_thread_id=?1 AND e.provider_turn_id=?2 AND e.outcome IN ('queued','running')").map_err(|error| WorkbenchError::storage("Failed to prepare turn execution cancellation", error))?;
    let ids = statement
        .query_map(params![provider_thread_id, provider_turn_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| WorkbenchError::storage("Failed to find turn executions", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode turn executions", error))?;
    drop(statement);
    drop(connection);
    let mut cancelled = 0;
    for id in ids {
        if cancel_execution(store, CancelExecutionRequest { execution_id: id }).is_ok() {
            cancelled += 1;
        }
    }
    Ok(cancelled)
}

pub fn cancel_execution(store: &Store, request: CancelExecutionRequest) -> WorkbenchResult<()> {
    validate_id("execution id", &request.execution_id)?;
    let active = get_execution(store, &request.execution_id)?;
    let changed = usize::from(matches!(active.outcome.as_str(), "queued" | "running"));
    open_connection(store)?
        .execute(
            "UPDATE execution_jobs SET cancel_requested=1 WHERE execution_id=?1",
            [&request.execution_id],
        )
        .map_err(|e| WorkbenchError::storage("Failed to request cancellation", e))?;
    if changed == 0 {
        return Err(WorkbenchError::invalid("Research execution is not active"));
    }
    cancelled_executions()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(request.execution_id.clone());
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResearchResultV1 {
    pub result_id: String,
    pub estimand: String,
    pub specification_id: String,
    pub sample_id: String,
    pub estimate: f64,
    pub standard_error: Option<f64>,
    pub confidence_interval: Option<[f64; 2]>,
    pub n: Option<u64>,
    pub units: String,
    pub transformation: Option<String>,
    pub uncertainty_method: Option<String>,
    pub source_execution_id: String,
    pub artifact_locator: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareResultsRequest {
    pub left: ResearchResultV1,
    pub right: ResearchResultV1,
    pub absolute_tolerance: f64,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResultComparison {
    pub comparable: bool,
    pub passed: bool,
    pub absolute_difference: Option<f64>,
    pub tolerance: f64,
    pub incompatibilities: Vec<String>,
    pub limitations: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordStructuredResultRequest {
    pub workspace_id: String,
    pub result: ResearchResultV1,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VerificationRecord {
    pub id: String,
    pub evidence_link_id: String,
    pub method: String,
    pub checker_identity: String,
    pub input_hashes: Value,
    pub observed_result: Value,
    pub limitations: String,
    pub passed: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyEvidenceResultsRequest {
    pub evidence_id: String,
    pub left_result_id: String,
    pub right_result_id: String,
    pub absolute_tolerance: f64,
    pub rationale: Option<String>,
    pub operation_id: String,
}

pub(crate) fn validate_result(result: &ResearchResultV1) -> WorkbenchResult<()> {
    for (label, value) in [
        ("result id", &result.result_id),
        ("specification id", &result.specification_id),
        ("sample id", &result.sample_id),
        ("source execution id", &result.source_execution_id),
    ] {
        validate_id(label, value)?;
    }
    for value in [result.estimate]
        .into_iter()
        .chain(result.standard_error)
        .chain(result.confidence_interval.into_iter().flatten())
    {
        if !value.is_finite() {
            return Err(WorkbenchError::invalid(
                "Structured results require finite numeric values",
            ));
        }
    }
    if result.standard_error.is_some_and(|s| s < 0.)
        || result.confidence_interval.is_some_and(|v| v[0] > v[1])
    {
        return Err(WorkbenchError::invalid(
            "Invalid uncertainty: standard errors must be nonnegative and intervals ordered",
        ));
    }
    validate_text("Estimand", &result.estimand, 1_000)?;
    validate_text("Units", &result.units, 300)?;
    validate_text("Artifact locator", &result.artifact_locator, 4_096)?;
    Ok(())
}

pub fn record_structured_result(
    store: &Store,
    request: RecordStructuredResultRequest,
) -> WorkbenchResult<ResearchResultV1> {
    validate_id("workspace id", &request.workspace_id)?;
    validate_result(&request.result)?;
    let execution = get_execution(store, &request.result.source_execution_id)?;
    if execution.workspace_id != request.workspace_id || execution.outcome != "completed" {
        return Err(WorkbenchError::invalid(
            "Structured results require a completed execution in this workspace",
        ));
    }
    let locator_path = request
        .result
        .artifact_locator
        .split('#')
        .next()
        .unwrap_or_default();
    let locator_valid = execution
        .output_manifest
        .get("artifacts")
        .and_then(Value::as_array)
        .is_some_and(|artifacts| {
            artifacts.iter().any(|artifact| {
                artifact.get("path").and_then(Value::as_str) == Some(locator_path)
                    || artifact.get("artifactId").and_then(Value::as_str) == Some(locator_path)
            })
        });
    if !locator_valid {
        return Err(WorkbenchError::invalid(
            "Structured result artifact locator is not present in the execution output manifest",
        ));
    }
    let id = new_id("structured")?;
    let body = serde_json::to_string(&request.result)
        .map_err(|error| WorkbenchError::storage("Failed to encode structured result", error))?;
    let mut connection = open_connection(store)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            WorkbenchError::storage("Failed to start structured result recording", error)
        })?;
    transaction.execute("INSERT INTO structured_results (id, execution_id, result_id, body_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)", params![id, request.result.source_execution_id, request.result.result_id, body, now()]).map_err(|error| WorkbenchError::storage("Failed to record structured result", error))?;
    append_change(
        &transaction,
        &request.operation_id,
        "structured_result",
        &id,
        (Some(&request.workspace_id), None),
        "recorded",
        &json!({"resultId":request.result.result_id,"executionId":request.result.source_execution_id}),
    )?;
    transaction
        .commit()
        .map_err(|error| WorkbenchError::storage("Failed to commit structured result", error))?;
    Ok(request.result)
}

pub fn list_structured_results(
    store: &Store,
    workspace_id: &str,
) -> WorkbenchResult<Vec<ResearchResultV1>> {
    validate_id("workspace id", workspace_id)?;
    let connection = open_connection(store)?;
    let mut statement = connection.prepare("SELECT sr.body_json FROM structured_results sr JOIN research_executions e ON e.id=sr.execution_id WHERE e.workspace_id=?1 ORDER BY sr.created_at DESC LIMIT 1000").map_err(|error| WorkbenchError::storage("Failed to prepare structured result list", error))?;
    let bodies = statement
        .query_map([workspace_id], |row| row.get::<_, String>(0))
        .map_err(|error| WorkbenchError::storage("Failed to list structured results", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            WorkbenchError::storage("Failed to decode structured result rows", error)
        })?;
    bodies
        .into_iter()
        .map(|body| {
            serde_json::from_str(&body).map_err(|error| {
                WorkbenchError::storage("Stored structured result is invalid", error)
            })
        })
        .collect()
}

fn verification_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<VerificationRecord> {
    let hashes: String = row.get(4)?;
    let observed: String = row.get(5)?;
    Ok(VerificationRecord {
        id: row.get(0)?,
        evidence_link_id: row.get(1)?,
        method: row.get(2)?,
        checker_identity: row.get(3)?,
        input_hashes: serde_json::from_str(&hashes).unwrap_or(Value::Null),
        observed_result: serde_json::from_str(&observed).unwrap_or(Value::Null),
        limitations: row.get(6)?,
        passed: row.get(7)?,
        created_at: row.get(8)?,
    })
}

pub fn verify_evidence_results(
    store: &Store,
    request: VerifyEvidenceResultsRequest,
) -> WorkbenchResult<VerificationRecord> {
    validate_id("evidence id", &request.evidence_id)?;
    let connection = open_connection(store)?;
    let workspace_id: String = connection
        .query_row(
            "SELECT workspace_id FROM evidence_links WHERE id=?1",
            [&request.evidence_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            WorkbenchError::storage("Failed to inspect evidence for verification", error)
        })?
        .ok_or_else(|| WorkbenchError::invalid("Evidence link was not found"))?;
    let load_result = |result_id: &str| -> WorkbenchResult<(ResearchResultV1, String)> {
        validate_id("result id", result_id)?;
        let (body, dependency): (String, String) = connection.query_row("SELECT sr.body_json, e.dependency_hash FROM structured_results sr JOIN research_executions e ON e.id=sr.execution_id WHERE sr.result_id=?1 AND e.workspace_id=?2 ORDER BY sr.created_at DESC LIMIT 1", params![result_id, workspace_id], |row| Ok((row.get(0)?,row.get(1)?))).optional().map_err(|error| WorkbenchError::storage("Failed to load verification input", error))?.ok_or_else(|| WorkbenchError::invalid("Verification result was not found in this workspace"))?;
        Ok((
            serde_json::from_str(&body).map_err(|error| {
                WorkbenchError::storage("Stored verification result is invalid", error)
            })?,
            dependency,
        ))
    };
    let (left, left_hash) = load_result(&request.left_result_id)?;
    let (right, right_hash) = load_result(&request.right_result_id)?;
    let comparison = compare_results(CompareResultsRequest {
        left,
        right,
        absolute_tolerance: request.absolute_tolerance,
        rationale: request.rationale,
    })?;
    let id = new_id("verification")?;
    let timestamp = now();
    let hashes = json!({"leftDependencyHash":left_hash,"rightDependencyHash":right_hash});
    let observed = serde_json::to_value(&comparison)
        .map_err(|error| WorkbenchError::storage("Failed to encode verification result", error))?;
    let mut write_connection = open_connection(store)?;
    let transaction = write_connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start evidence verification", error))?;
    transaction.execute("INSERT INTO verification_records (id, evidence_link_id, method, checker_identity, input_hashes_json, observed_result_json, limitations, passed, created_at) VALUES (?1, ?2, 'research-results-v1 comparison', 'workspace-host-v1', ?3, ?4, ?5, ?6, ?7)", params![id, request.evidence_id, serde_json::to_string(&hashes).unwrap_or_default(), serde_json::to_string(&observed).unwrap_or_default(), comparison.limitations, comparison.passed, timestamp]).map_err(|error| WorkbenchError::storage("Failed to record evidence verification", error))?;
    transaction.execute("UPDATE evidence_links SET assessment=?2, assessor='workspace-host-v1', freshness='current', stale_reason=NULL, updated_at=?3 WHERE id=?1", params![request.evidence_id, if comparison.passed {"check_passed"} else {"check_failed"}, timestamp]).map_err(|error| WorkbenchError::storage("Failed to apply evidence verification", error))?;
    append_change(
        &transaction,
        &request.operation_id,
        "verification",
        &id,
        (Some(&workspace_id), None),
        "recorded",
        &json!({"evidenceId":request.evidence_id,"passed":comparison.passed}),
    )?;
    transaction.commit().map_err(|error| {
        WorkbenchError::storage("Failed to commit evidence verification", error)
    })?;
    open_connection(store)?.query_row("SELECT id, evidence_link_id, method, checker_identity, input_hashes_json, observed_result_json, limitations, passed, created_at FROM verification_records WHERE id=?1", [&id], verification_from_row).map_err(|error| WorkbenchError::storage("Failed to read verification record", error))
}

pub fn compare_results(request: CompareResultsRequest) -> WorkbenchResult<ResultComparison> {
    validate_result(&request.left)?;
    validate_result(&request.right)?;
    if !request.absolute_tolerance.is_finite() || request.absolute_tolerance < 0.0 {
        return Err(WorkbenchError::invalid(
            "Result comparison tolerance must be finite and nonnegative",
        ));
    }
    let mut incompatibilities = Vec::new();
    for (label, left, right) in [
        (
            "estimand",
            request.left.estimand.as_str(),
            request.right.estimand.as_str(),
        ),
        (
            "specification",
            request.left.specification_id.as_str(),
            request.right.specification_id.as_str(),
        ),
        (
            "sample",
            request.left.sample_id.as_str(),
            request.right.sample_id.as_str(),
        ),
        (
            "units",
            request.left.units.as_str(),
            request.right.units.as_str(),
        ),
        (
            "transformation",
            request.left.transformation.as_deref().unwrap_or(""),
            request.right.transformation.as_deref().unwrap_or(""),
        ),
    ] {
        if left != right {
            incompatibilities.push(label.to_string());
        }
    }
    let rationale = request
        .rationale
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let quantity_compatible = request.left.units == request.right.units
        && request.left.transformation == request.right.transformation
        && request.left.estimand == request.right.estimand;
    let comparable = quantity_compatible && (incompatibilities.is_empty() || rationale.is_some());
    let difference = comparable.then_some((request.left.estimate - request.right.estimate).abs());
    Ok(ResultComparison { comparable, passed: difference.is_some_and(|value| value <= request.absolute_tolerance), absolute_difference:difference, tolerance:request.absolute_tolerance, incompatibilities, limitations:"This check compares declared numeric results only; it does not establish identification, causal interpretation, or theoretical validity.".to_string() })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostExecutionPreview {
    pub profile_id: String,
    pub fingerprint: String,
    pub command: Vec<String>,
    pub cwd: String,
    pub inputs: Value,
    pub launch: Value,
    pub outputs: Vec<String>,
    pub authorized: bool,
    pub boundary: String,
}

fn launch_manifest(profile: &ExecutionProfile) -> WorkbenchResult<Value> {
    let requested = Path::new(&profile.argv[0]);
    let executable = if requested.is_absolute() {
        requested.to_path_buf()
    } else if requested.components().count() > 1 {
        Path::new(&profile.cwd).join(requested)
    } else {
        let path = profile
            .environment
            .get("PATH")
            .and_then(Value::as_str)
            .map(std::ffi::OsString::from)
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_default();
        std::env::split_paths(&path)
            .map(|p| p.join(requested))
            .find(|p| p.is_file())
            .ok_or_else(|| {
                WorkbenchError::invalid(
                    "Execution program is missing. Install it or configure its absolute path.",
                )
            })?
    };
    let executable = fs::canonicalize(executable)
        .map_err(|e| WorkbenchError::storage("Failed to resolve execution program", e))?;
    let (hash, size) = hash_file(&executable)?;
    let mut launch_files = Vec::new();
    for argument in profile.argv.iter().skip(1) {
        let path = Path::new(argument);
        if !path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| {
                [
                    "sh", "bash", "zsh", "py", "r", "jl", "do", "pl", "rb", "tex",
                ]
                .contains(&ext.to_ascii_lowercase().as_str())
            })
        {
            continue;
        }
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            Path::new(&profile.cwd).join(path)
        };
        if path.is_file() {
            let path = fs::canonicalize(path)
                .map_err(|e| WorkbenchError::storage("Failed to resolve launch script", e))?;
            let (hash, size) = hash_file(&path)?;
            launch_files.push(json!({"path":path,"hash":hash,"size":size}));
        }
    }
    let mut startup = Vec::new();
    if profile.argv[0] == "/bin/zsh" {
        if let Some(home) = profile
            .environment
            .get("ZDOTDIR")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("ZDOTDIR").map(PathBuf::from))
            .or_else(dirs::home_dir)
        {
            for name in [".zshenv", ".zprofile", ".zshrc", ".zlogin"] {
                let path = home.join(name);
                startup.push(if path.exists() {
                    let (hash, size) = hash_file(&path)?;
                    json!({"path":path,"hash":hash,"size":size})
                } else {
                    json!({"path":path,"missing":true})
                });
            }
        }
    }
    Ok(
        json!({"executable":executable,"hash":hash,"size":size,"shellStartupFiles":startup,"launchFiles":launch_files,"coverage":"executable, directly referenced launch scripts, known shell startup files and declared inputs; shell-string and transitive dependencies require explicit input declarations"}),
    )
}

pub fn preview_host_execution(
    store: &Store,
    profile_id: &str,
) -> WorkbenchResult<HostExecutionPreview> {
    let profile = get_execution_profile(store, profile_id)?;
    let root = store
        .workspace(&profile.workspace_id)?
        .root
        .ok_or_else(|| WorkbenchError::invalid("The execution root is unavailable"))?;
    let root = fs::canonicalize(root)
        .map_err(|e| WorkbenchError::storage("Failed to resolve execution root", e))?;
    let cwd = fs::canonicalize(&profile.cwd)
        .map_err(|e| WorkbenchError::storage("Failed to resolve execution directory", e))?;
    if !cwd.starts_with(&root) {
        return Err(WorkbenchError::invalid(
            "Execution directory no longer belongs to this Workspace",
        ));
    }
    let (inputs, _, _) = input_manifest(&profile)?;
    if inputs["complete"] != true {
        return Err(WorkbenchError::invalid(
            "All declared inputs must be available before authorizing execution",
        ));
    }
    let launch = launch_manifest(&profile)?;
    let fingerprint=hash_bytes(&serde_json::to_vec(&json!({"profileId":profile.id,"revision":profile.revision,"argv":profile.argv,"cwd":cwd,"rootIdentity":super::super::store::root_identity(&root)?,"environment":profile.environment,"launch":launch,"inputs":inputs,"outputs":profile.outputs,"timeout":profile.timeout_seconds})).map_err(|e|WorkbenchError::storage("Failed to fingerprint host execution",e))?);
    let authorized=open_connection(store)?.query_row("SELECT EXISTS(SELECT 1 FROM execution_authorizations WHERE profile_id=?1 AND fingerprint=?2)",params![profile.id,fingerprint],|r|r.get(0)).map_err(|e|WorkbenchError::storage("Failed to read execution authorization",e))?;
    Ok(HostExecutionPreview{profile_id:profile.id,fingerprint,command:profile.argv,cwd:profile.cwd,inputs,launch,outputs:profile.outputs,authorized,boundary:"Host execution: this command runs with your operating-system access, outside the conversation sandbox. It may read/write other files or use the network. Authorization covers this exact profile, resolved executable, known shell startup files, and current declared input bytes. Changes require a new review. Other sourced scripts and transitive dependencies remain unverified. Dependency coverage remains declared-only.".into()})
}

// UI-only capability grant. This operation is deliberately not a dynamic tool.
pub fn authorize_host_execution(
    store: &Store,
    profile_id: &str,
    fingerprint: &str,
) -> WorkbenchResult<HostExecutionPreview> {
    let preview = preview_host_execution(store, profile_id)?;
    if preview.fingerprint != fingerprint {
        return Err(WorkbenchError::conflict(
            "The command or its inputs changed during review; preview it again",
        ));
    }
    open_connection(store)?.execute("INSERT INTO execution_authorizations(profile_id,fingerprint,authorized_at) VALUES(?1,?2,?3) ON CONFLICT(profile_id) DO UPDATE SET fingerprint=excluded.fingerprint,authorized_at=excluded.authorized_at",params![profile_id,fingerprint,now()]).map_err(|e|WorkbenchError::storage("Failed to authorize host execution",e))?;
    preview_host_execution(store, profile_id)
}
