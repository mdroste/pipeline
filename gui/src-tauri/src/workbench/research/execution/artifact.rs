//! Explicit runs of saved conversation scripts. The existing local process
//! runner owns the child tree and pipes. This view keeps only recent output
//! for the current app session and never creates a reusable command profile.
use super::types::ExecutionProfile;
use super::{cancelled_executions, launch::launch_manifest, run_profile_process, toolchains};
use crate::workbench::project;
use crate::workbench::research::{hash_bytes, jobs, new_id};
use crate::workbench::store::{RuntimeFileOwner, Store, WorkbenchError, WorkbenchResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

const OUTPUT_LIMIT: usize = 128 * 1024;
const RETAINED_RUNS: usize = 12;
const TIMEOUT_SECONDS: u64 = 600;
const STATA_WRAPPER: &str = r#"set more off
local root : environment PIPELINE_ARTIFACT_ROOT
local script : environment PIPELINE_ARTIFACT_FILE
capture noisily cd `"`root'"'
local status = _rc
if !`status' {
    capture noisily do `"`script'"'
    local status = _rc
}
exit `status', clear
"#;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactTarget {
    pub session_id: String,
    pub scope_root: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactRunRequest {
    pub target: ArtifactTarget,
    pub expected_hash: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRun {
    id: String,
    state: String,
    exit_status: Option<i32>,
    stdout: String,
    stderr: String,
    truncated: bool,
}

struct Run {
    target: ArtifactTarget,
    workspace_id: Option<String>,
    stata_log: Option<PathBuf>,
    view: ArtifactRun,
}

impl Run {
    fn active(&self) -> bool {
        matches!(self.view.state.as_str(), "running" | "stopping")
    }
}

#[derive(Default)]
struct Registry {
    closing: bool,
    recent: Vec<Run>,
}

fn registry() -> MutexGuard<'static, Registry> {
    static RUNS: OnceLock<Mutex<Registry>> = OnceLock::new();
    RUNS.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn start(store: &Store, request: ArtifactRunRequest) -> WorkbenchResult<ArtifactRun> {
    let ArtifactRunRequest {
        target,
        expected_hash,
    } = request;
    let (root, owner) = store.runtime_file_scope(&target.session_id)?;
    if root.to_str() != Some(&target.scope_root) {
        return Err(WorkbenchError::conflict(
            "The working folder changed. Reopen the file before running it.",
        ));
    }
    let workspace_id = match owner {
        RuntimeFileOwner::Private => None,
        RuntimeFileOwner::Project { workspace_id, .. } => Some(workspace_id),
        RuntimeFileOwner::Discovery => {
            return Err(WorkbenchError::invalid(
                "Discovery files cannot start local jobs from this pane.",
            ))
        }
    };
    let lock_key = workspace_id.as_deref().unwrap_or(&target.session_id);
    let _scope_lock = project::execution_lock(store, lock_key)?;
    if let Some(ws) = &workspace_id {
        jobs::ensure_project_free(ws)?;
    }
    let (relative, bytes) =
        project::conversation_file_bytes(store, &target.session_id, &target.path)?;
    if relative != target.path || hash_bytes(&bytes) != expected_hash {
        return Err(WorkbenchError::conflict(
            "The script changed. Reload the saved file before running it.",
        ));
    }
    if bytes.len() > 2 * 1024 * 1024
        || !std::str::from_utf8(&bytes).is_ok_and(|text| !text.contains('\0'))
    {
        return Err(WorkbenchError::invalid(
            "Choose a UTF-8 script up to 2 MiB.",
        ));
    }
    let kind = match Path::new(&relative)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("py") => "python",
        Some("do") => "stata",
        _ => {
            return Err(WorkbenchError::invalid(
                "Choose a Python script or Stata do-file.",
            ))
        }
    };
    let capacity = jobs::reserve_artifact_slot()?;
    let folder_slot = project::job_slot(store, lock_key)?;
    let id = new_id("artifact-run")?;
    let (profile, scratch) = launch_profile(&id, lock_key, kind, &root, &relative, &bytes)?;
    let launch = launch_manifest(&profile)?;
    if kind == "python" && launch["launchFiles"][0]["hash"].as_str() != Some(&expected_hash) {
        return Err(WorkbenchError::conflict(
            "The script changed. Reload the saved file before running it.",
        ));
    }
    let stata_log = scratch
        .as_ref()
        .map(|folder| folder.path().join("artifact.log"));
    let view = ArtifactRun {
        id: id.clone(),
        state: "running".into(),
        exit_status: None,
        stdout: String::new(),
        stderr: String::new(),
        truncated: false,
    };
    {
        let mut runs = registry();
        if runs.closing
            || runs
                .recent
                .iter()
                .any(|run| run.active() && run.target.scope_root == target.scope_root)
        {
            return Err(WorkbenchError::conflict(
                "The working folder is busy. Wait for the current job to finish.",
            ));
        }
        runs.recent
            .retain(|run| run.target != target || run.active());
        if runs.recent.len() >= RETAINED_RUNS {
            if let Some(index) = runs.recent.iter().position(|run| !run.active()) {
                runs.recent.remove(index);
            }
        }
        runs.recent.push(Run {
            target,
            workspace_id,
            stata_log: stata_log.clone(),
            view: view.clone(),
        });
    }
    tauri::async_runtime::spawn(async move {
        let worker_id = id.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _slot = (capacity, folder_slot);
            let output = run_profile_process(&worker_id, &profile, &launch);
            // The batch log is still in the private temporary directory.
            let log = stata_log.as_deref().and_then(read_batch_log);
            drop(scratch);
            (output, log, kind == "stata")
        })
        .await;
        let mut runs = registry();
        if let Some(run) = runs.recent.iter_mut().find(|run| run.view.id == id) {
            run.stata_log = None;
            match result {
                Ok((Ok(output), log, stata)) => {
                    let (stdout, a) = output_tail(&output.stdout);
                    let (stderr, b) = output_tail(&output.stderr);
                    run.view.stdout = stdout;
                    run.view.stderr = stderr;
                    run.view.state = output.outcome;
                    run.view.exit_status = output.status;
                    run.view.truncated =
                        output.stdout_truncated || output.stderr_truncated || a || b;
                    if let Some((log, truncated)) = log {
                        if run.view.state == "completed"
                            && !super::outputs::stata_log_errors(&log).is_empty()
                        {
                            run.view.state = "failed".into();
                            run.view.exit_status = None;
                        }
                        run.view.stdout = log;
                        run.view.truncated |= truncated;
                    } else if stata && run.view.state == "completed" {
                        run.view.state = "failed".into();
                        run.view.exit_status = None;
                        run.view
                            .stderr
                            .push_str("\nStata did not produce a batch log.");
                    }
                }
                Ok((Err(error), _, _)) => {
                    run.view.state = "failed".into();
                    run.view.stderr = error;
                }
                Err(error) => {
                    run.view.state = "failed".into();
                    run.view.stderr = format!("The local job stopped unexpectedly: {error}");
                }
            }
        }
        jobs::forget_artifact_log(&id);
        cancelled_executions()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
    });
    Ok(view)
}

pub(crate) fn status(target: &ArtifactTarget) -> Option<ArtifactRun> {
    let (mut view, live, log) = {
        let runs = registry();
        let run = runs.recent.iter().find(|run| &run.target == target)?;
        (run.view.clone(), run.active(), run.stata_log.clone())
    };
    if live {
        let (stdout, stderr, truncated) = jobs::artifact_log(&view.id);
        view.stdout = stdout;
        view.stderr = stderr;
        view.truncated = truncated;
        if let Some((log, truncated)) = log.as_deref().and_then(read_batch_log) {
            view.stdout = log;
            view.truncated |= truncated;
        }
    }
    Some(view)
}

pub(crate) fn stop(target: &ArtifactTarget, id: &str) {
    let mut runs = registry();
    if let Some(run) = runs
        .recent
        .iter_mut()
        .find(|run| &run.target == target && run.view.id == id && run.active())
    {
        run.view.state = "stopping".into();
        cancelled_executions()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.into());
    }
}

pub(crate) fn cancel_all() {
    let mut runs = registry();
    runs.closing = true;
    cancelled_executions()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .extend(
            runs.recent
                .iter()
                .filter(|run| run.active())
                .map(|run| run.view.id.clone()),
        );
}

pub(crate) fn has_active() -> bool {
    registry().recent.iter().any(Run::active)
}

pub(crate) fn session_active(session: &str) -> bool {
    registry()
        .recent
        .iter()
        .any(|run| run.active() && run.target.session_id == session)
}

pub(crate) fn active_in_project(ws: &str) -> Option<(PathBuf, String)> {
    registry()
        .recent
        .iter()
        .find(|run| run.active() && run.workspace_id.as_deref() == Some(ws))
        .map(|run| {
            (
                PathBuf::from(&run.target.scope_root),
                run.target.path.clone(),
            )
        })
}

pub(crate) fn ensure_folder_free(root: &Path) -> WorkbenchResult<()> {
    if registry()
        .recent
        .iter()
        .any(|run| run.active() && Path::new(&run.target.scope_root) == root)
    {
        return Err(WorkbenchError::conflict(
            "Stop or finish the local script before changing its working folder.",
        ));
    }
    Ok(())
}

pub(crate) fn output_tail(bytes: &[u8]) -> (String, bool) {
    let mut start = bytes.len().saturating_sub(OUTPUT_LIMIT);
    while start > 0 && start < bytes.len() && bytes[start] & 0xc0 == 0x80 {
        start += 1;
    }
    (
        String::from_utf8_lossy(&bytes[start..]).into_owned(),
        start > 0,
    )
}

fn read_batch_log(path: &Path) -> Option<(String, bool)> {
    let mut file = crate::safety::open_regular_file(path).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(OUTPUT_LIMIT as u64)))
        .ok()?;
    let mut bytes = Vec::new();
    file.take(OUTPUT_LIMIT as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    Some((output_tail(&bytes).0, len > OUTPUT_LIMIT as u64))
}

fn launch_profile(
    id: &str,
    workspace_id: &str,
    kind: &str,
    root: &Path,
    relative: &str,
    source: &[u8],
) -> WorkbenchResult<(ExecutionProfile, Option<tempfile::TempDir>)> {
    let fail = |error| WorkbenchError::storage("Could not prepare the local script", error);
    let program = toolchains::artifact_program(kind);
    let mut environment = json!({});
    let (argv, cwd, scratch) = if kind == "python" {
        let program = program
            .ok_or_else(|| WorkbenchError::invalid("Install Python 3 to run this script."))?;
        (
            vec![
                program.to_string_lossy().into_owned(),
                "-u".into(),
                root.join(relative).to_string_lossy().into_owned(),
            ],
            root.to_path_buf(),
            None,
        )
    } else {
        let mut builder = tempfile::Builder::new();
        builder.prefix("pipeline-stata-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            builder.permissions(fs::Permissions::from_mode(0o700));
        }
        let scratch = builder.tempdir().map_err(fail)?;
        let cwd = scratch.path().canonicalize().map_err(fail)?;
        let input = crate::file_viewer::system_path(&cwd.join("input.do"));
        let root = crate::file_viewer::system_path(root);
        if [&root, &input]
            .iter()
            .any(|p| p.to_string_lossy().contains(['"', '`', '$', '\n', '\r']))
        {
            return Err(WorkbenchError::invalid("Stata cannot open this working folder name. Move it to a path without quotes, backticks, or dollar signs."));
        }
        fs::write(cwd.join("artifact.do"), STATA_WRAPPER).map_err(fail)?;
        fs::write(&input, source).map_err(fail)?;
        let installed = program.as_deref().and_then(Path::to_str).unwrap_or("");
        environment = json!({"PIPELINE_ARTIFACT_ROOT":root,"PIPELINE_ARTIFACT_FILE":input,"PIPELINE_STATA":installed});
        let argv = if cfg!(target_os = "macos") {
            // The script is fixed; filesystem paths travel in environment
            // values, and the researcher’s interactive login alias wins.
            vec!["/bin/zsh".into(), "-lic".into(),
                r#"if (( $+aliases[stata] || $+functions[stata] || $+commands[stata] )); then stata -q -b do artifact.do; elif [[ -n $PIPELINE_STATA ]]; then "$PIPELINE_STATA" -q -b do artifact.do; else print -u2 'Install Stata or define its stata alias.'; exit 127; fi"#.into()]
        } else {
            let program = environment["PIPELINE_STATA"]
                .as_str()
                .filter(|program| !program.is_empty())
                .ok_or_else(|| WorkbenchError::invalid("Install Stata to run this do-file."))?;
            vec![
                program.into(),
                if cfg!(windows) {
                    "/e".into()
                } else {
                    "-b".into()
                },
                "do".into(),
                "artifact.do".into(),
            ]
        };
        (argv, cwd, Some(scratch))
    };
    Ok((
        ExecutionProfile {
            id: id.into(),
            workspace_id: workspace_id.into(),
            name: relative.into(),
            adapter: kind.into(),
            argv,
            cwd: cwd.to_string_lossy().into_owned(),
            environment,
            inputs: vec![],
            timeout_seconds: TIMEOUT_SECONDS,
            outputs: vec![],
            tested_at: None,
            test_status: None,
            revision: 0,
        },
        scratch,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::store::CreateSessionRequest;
    use std::time::{Duration, Instant};

    fn fixture() -> (tempfile::TempDir, Store, ArtifactTarget) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open_at(directory.path()).unwrap();
        let session = store
            .create_session(CreateSessionRequest {
                workspace_id: None,
                title: "Script run".into(),
                operation_id: new_id("fixture").unwrap(),
            })
            .unwrap()
            .record;
        let target = ArtifactTarget {
            session_id: session.id.clone(),
            scope_root: store
                .runtime_root(&session.id)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            path: String::new(),
        };
        (directory, store, target)
    }

    fn run(store: &Store, target: &ArtifactTarget, source: &str) -> ArtifactRun {
        fs::write(Path::new(&target.scope_root).join(&target.path), source).unwrap();
        start(
            store,
            ArtifactRunRequest {
                target: target.clone(),
                expected_hash: hash_bytes(source.as_bytes()),
            },
        )
        .unwrap()
    }

    fn finished(target: &ArtifactTarget) -> ArtifactRun {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let result = status(target).unwrap();
            if !matches!(result.state.as_str(), "running" | "stopping") {
                return result;
            }
            assert!(Instant::now() < deadline, "The script took too long.");
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    #[test]
    #[ignore = "Needs the host's installed Python 3"]
    fn live_artifact_python_output_and_stop() {
        let (_directory, store, mut target) = fixture();
        target.path = "analysis with spaces.py".into();
        fs::write(
            Path::new(&target.scope_root).join("input.txt"),
            "observations",
        )
        .unwrap();
        run(
            &store,
            &target,
            "import sys\nprint(open('input.txt').read())\nprint('diagnostic', file=sys.stderr)\n",
        );
        let output = finished(&target);
        assert_eq!(output.state, "completed", "{}", output.stderr);
        assert_eq!(output.stdout.trim(), "observations");
        assert_eq!(output.stderr.trim(), "diagnostic");
        let sleeping = run(
            &store,
            &target,
            "import time\nprint('ready', flush=True)\ntime.sleep(60)\n",
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while !status(&target).unwrap().stdout.contains("ready") {
            assert!(Instant::now() < deadline, "Python never began.");
            std::thread::sleep(Duration::from_millis(25));
        }
        stop(&target, &sleeping.id);
        assert_eq!(finished(&target).state, "interrupted");
    }

    #[test]
    #[ignore = "Needs a licensed Stata installation"]
    fn live_artifact_stata_batch_log_and_error() {
        let (_directory, store, mut target) = fixture();
        target.path = "analysis with spaces.do".into();
        fs::write(
            Path::new(&target.scope_root).join("input.txt"),
            "observations",
        )
        .unwrap();
        run(&store, &target, "type input.txt\ndisplay 42\n");
        let result = finished(&target);
        assert_eq!(
            result.state, "completed",
            "{}\n{}",
            result.stdout, result.stderr
        );
        assert!(result.stdout.contains("observations"));
        run(&store, &target, "error 198\n");
        let result = finished(&target);
        assert_eq!(result.state, "failed");
        assert!(result.stdout.contains("r(198);"));
    }
}
