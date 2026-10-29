//! Execution-profile validation, persistence, and adapter flags.

use super::types::{ExecutionProfile, SaveExecutionProfileRequest};
use crate::workbench::research::{
    append_change, json_object, new_id, now, open_connection, validate_id, validate_text,
};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension as _, TransactionBehavior};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn latex_engine(program: &str, windows: bool) -> Option<String> {
    let name = if windows {
        program.rsplit(['/', '\\']).next()?.to_ascii_lowercase()
    } else {
        Path::new(program).file_name()?.to_str()?.to_owned()
    };
    let name = if windows {
        name.strip_suffix(".exe").unwrap_or(&name)
    } else {
        &name
    };
    matches!(
        name,
        "latexmk" | "tectonic" | "pdflatex" | "xelatex" | "lualatex"
    )
    .then(|| name.to_owned())
}

pub(in crate::workbench::research) fn safe_relative_path(
    label: &str,
    value: &str,
) -> WorkbenchResult<()> {
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
    if request.adapter == "latex"
        && (latex_engine(&request.argv[0], cfg!(windows)).is_none()
            || request.argv.iter().any(|argument| {
                (argument.contains("shell-escape") && argument != "-no-shell-escape")
                    || argument.contains("write18")
            }))
    {
        return Err(WorkbenchError::invalid(
            "LaTeX profiles require a supported engine and cannot enable shell escape",
        ));
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
    let engine = argv.first().and_then(|s| latex_engine(s, cfg!(windows)));
    if adapter == "latex" && engine.as_deref() != Some("tectonic") {
        if !argv.iter().any(|s| s == "-no-shell-escape") {
            argv.insert(1.min(argv.len()), "-no-shell-escape".into());
        }
        if engine.as_deref() == Some("latexmk") && !argv.iter().any(|s| s == "-norc") {
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
