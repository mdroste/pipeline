//! Researcher-directed Git for a project folder.
//!
//! Everything here runs because the researcher chose it on the Version control
//! page. No assistant, Automation, or Review reaches this module, and it keeps
//! no credentials: remotes use the researcher's own Git setup (credential
//! helpers and SSH keys). Hosts are recognized from the remote address only to
//! build links, so GitHub and Bitbucket need no account connection here.
//!
//! Git runs directly from the resolved binary, not through a login shell: a page
//! needs about a dozen calls per refresh. Reads take no optional locks and start
//! nothing the folder's own Git config names (see `guard`), because they happen
//! with no click. Fetches run under the same guard, because a look at a remote is
//! not a choice to trust the folder. Other actions run the repository's hooks and
//! filters, as a commit or push from a terminal would, because Git LFS and project
//! checks depend on them.
//!
//! Paths crossing this boundary are relative to the project folder, which may
//! be a subfolder of the repository, and are always literal pathspecs. Files a
//! discard or a chosen conflict side would destroy go to the system Trash first.
//! The command layer holds the turn permit and the project lock around any action
//! that changes files, as it does for every other file change, and `claim_project`
//! refuses one that could reach a local job's files.
use super::repository::{self, RepositoryStatus};
use super::tasks::{capture_git, GitLimits};
use super::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

mod actions;
mod errors;
mod guard;
#[cfg(all(test, unix))]
mod guard_tests;
mod inspect;
mod status;
#[cfg(all(test, unix))]
mod tests;

pub use actions::{claim_project, perform, Action, ActionResult};
pub(super) use guard::ReadGuard;
pub use inspect::{commit_detail, diff, history, CommitDetail, DiffRequest, FileDiff, HistoryPage};
use status::{BranchInfo, ChangedFile, LargeFile};

/// How long a local action (commit, switch, merge) may run, hooks included.
const LOCAL_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// How long a push, pull, or fetch may run. Large data repositories are slow.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryView {
    pub status: RepositoryStatus,
    /// Changes in the project folder, conflicts first.
    pub files: Vec<ChangedFile>,
    /// More files changed than `files` holds.
    pub files_truncated: bool,
    /// New files over the size hosts accept, anywhere in `files`.
    pub large_files: Vec<LargeFile>,
    pub branches: Vec<BranchInfo>,
    /// `merge`, `rebase`, `cherry-pick`, or `revert` while one is unfinished.
    pub operation: Option<String>,
    /// The commit message Git prepared for the unfinished operation.
    pub operation_message: Option<String>,
    /// Conflicted files elsewhere in the repository, outside the project folder.
    pub outside_conflicts: usize,
    /// None until both a name and an email are configured.
    pub identity: Option<GitIdentity>,
    /// The project folder's path inside the repository; empty at its root.
    pub folder_prefix: String,
    /// The latest commit exists only on this computer and has a parent.
    pub can_undo_commit: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum VersionControl {
    NoFolder,
    GitMissing,
    NotRepository {
        /// Set when Git refused the folder rather than finding no repository.
        problem: Option<String>,
    },
    Ready(Box<RepositoryView>),
}

/// Runs Git in one project folder.
pub(super) struct Git {
    binary: PathBuf,
    root: PathBuf,
    /// Listed once per handle: one refresh makes about a dozen reads.
    guard: Mutex<Option<Arc<guard::ReadGuard>>>,
}

impl Git {
    pub(super) fn open(root: &Path) -> Option<Self> {
        Some(Self {
            binary: crate::deps::find_working_git()?,
            root: root.to_path_buf(),
            guard: Mutex::new(None),
        })
    }

    fn command(&self) -> std::process::Command {
        let mut command = std::process::Command::new(&self.binary);
        command
            .env("PATH", crate::env::full_path())
            .env("GIT_LITERAL_PATHSPECS", "1")
            // A failed credential lookup reports an error instead of waiting
            // for a terminal that does not exist. Credential managers may still
            // show their own sign-in window.
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .env("GIT_MERGE_AUTOEDIT", "no")
            // Error messages are matched in `errors`.
            .env("LC_ALL", "C");
        command
    }

    /// What makes a call safe in this folder, listed through this handle's direct
    /// path even for a caller that goes on to use the login shell.
    pub(super) fn guard(&self) -> WorkbenchResult<Arc<guard::ReadGuard>> {
        let mut held = self.guard.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(guard) = held.as_ref() {
            return Ok(guard.clone());
        }
        let guard = Arc::new(guard::ReadGuard::of(self)?);
        *held = Some(guard.clone());
        Ok(guard)
    }

    /// A read. It cannot change the repository, reach a remote, or start any
    /// program the folder's own Git config names, including hooks and filters.
    /// When that config can't be listed, the read fails instead of running.
    pub(super) fn read(&self, args: &[&str]) -> WorkbenchResult<Vec<u8>> {
        let guard = self.guard()?;
        let mut command = self.command();
        guard.read(&mut command, args);
        capture_git(command, &self.root, GitLimits::default())
    }

    pub(super) fn read_text(&self, args: &[&str]) -> WorkbenchResult<String> {
        Ok(String::from_utf8_lossy(&self.read(args)?).into_owned())
    }

    /// A fetch the researcher chose. It reaches the remote, but nothing the
    /// folder's own Git config names applies.
    fn fetch(
        &self,
        options: &[&str],
        remote: &str,
        timeout: Duration,
        cancel: Option<&AtomicBool>,
    ) -> WorkbenchResult<Vec<u8>> {
        let guard = self.guard()?;
        let mut command = self.command();
        guard.fetch(&mut command, options, remote)?;
        capture_git(command, &self.root, GitLimits { timeout, cancel })
    }

    /// An action the researcher chose. Hooks run, as they would in a terminal.
    fn act(
        &self,
        args: &[&str],
        timeout: Duration,
        cancel: Option<&AtomicBool>,
    ) -> WorkbenchResult<Vec<u8>> {
        let mut command = self.command();
        command.args(["-c", "core.fsmonitor=false"]).args(args);
        let result = capture_git(command, &self.root, GitLimits { timeout, cancel });
        // Anything an action ran may have changed the config.
        *self.guard.lock().unwrap_or_else(|e| e.into_inner()) = None;
        result
    }
}

/// The folder's Git state, loaded for the Version control page.
pub fn state(root: Option<&Path>) -> WorkbenchResult<VersionControl> {
    let Some(root) = root else {
        return Ok(VersionControl::NoFolder);
    };
    let Some(git) = Git::open(root) else {
        return Ok(VersionControl::GitMissing);
    };
    view(&git)
}

pub(super) fn view(git: &Git) -> WorkbenchResult<VersionControl> {
    let inside = match git.read_text(&["rev-parse", "--is-inside-work-tree"]) {
        Ok(inside) => inside,
        Err(error) if errors::needs_developer_tools(&error.message) => {
            return Ok(VersionControl::GitMissing)
        }
        Err(error) => {
            return Ok(VersionControl::NotRepository {
                problem: errors::repository_problem(&error.message),
            })
        }
    };
    if inside.trim() != "true" {
        return Ok(VersionControl::NotRepository { problem: None });
    }
    // Inside a repository, a failed read must never look like "not tracked":
    // that would offer to start a second repository here.
    let output = git.read(&repository::STATUS_ARGS).map_err(|_| {
        WorkbenchError::invalid(
            "Git could not read this repository. It may have too many changed files to show here.",
        )
    })?;
    let output = String::from_utf8_lossy(&output);
    let git_dir = git.read_text(&["rev-parse", "--absolute-git-dir"]).ok();
    let git_dir = git_dir.as_deref().map(str::trim);
    let status = repository::status_from(&git.root, &output, git_dir, &|args| git.read(args));
    let folder_prefix = git
        .read_text(&["rev-parse", "--show-prefix"])?
        .trim_end_matches(['\n', '\r'])
        .to_string();
    let changes = status::parse_changes(&output, &folder_prefix);
    let (operation, operation_message) =
        git_dir.map_or((None, None), |dir| status::operation_in(Path::new(dir)));
    let identity = status::identity(git);
    let listed = changes
        .files
        .iter()
        .filter(|file| file.status == "conflicted")
        .count();
    let outside_conflicts = status::unmerged_count(git).saturating_sub(listed);
    let can_undo_commit = operation.is_none() && status::latest_commit_is_local(git, &status);
    Ok(VersionControl::Ready(Box::new(RepositoryView {
        large_files: status::large_files(git, &changes.files),
        branches: status::branches(git),
        files: changes.files,
        files_truncated: changes.truncated,
        status,
        operation,
        operation_message,
        outside_conflicts,
        identity,
        folder_prefix,
        can_undo_commit,
    })))
}

fn running() -> &'static Mutex<HashMap<PathBuf, Arc<AtomicBool>>> {
    static RUNNING: OnceLock<Mutex<HashMap<PathBuf, Arc<AtomicBool>>>> = OnceLock::new();
    RUNNING.get_or_init(Default::default)
}

const ACTION_RUNNING: &str = "Another Git action is still running in this project.";

/// Refuses early when an action is running, so the next one hears that rather
/// than a message about the project guards the first one holds.
pub fn check_idle(root: &Path) -> WorkbenchResult<()> {
    if running()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains_key(root)
    {
        return Err(WorkbenchError::conflict(ACTION_RUNNING));
    }
    Ok(())
}

/// One action at a time per folder, so two actions never race for Git's locks.
struct ActionGuard {
    root: PathBuf,
    cancel: Arc<AtomicBool>,
}

impl ActionGuard {
    fn claim(root: &Path) -> WorkbenchResult<Self> {
        let mut running = running().lock().unwrap_or_else(|e| e.into_inner());
        if running.contains_key(root) {
            return Err(WorkbenchError::conflict(ACTION_RUNNING));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        running.insert(root.to_path_buf(), cancel.clone());
        Ok(Self {
            root: root.to_path_buf(),
            cancel,
        })
    }
}

impl Drop for ActionGuard {
    fn drop(&mut self) {
        running()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.root);
    }
}

/// Stops the running push, pull, or fetch in this folder, if there is one.
pub fn cancel(root: &Path) {
    if let Some(flag) = running()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(root)
    {
        flag.store(true, Ordering::Relaxed);
    }
}

/// A project-relative path from the page, checked before it reaches Git or the
/// file system. `.` is the whole folder, which a new repository can show.
fn checked_path(path: &str) -> WorkbenchResult<&str> {
    let invalid = || WorkbenchError::invalid("Invalid file path");
    if path.is_empty() || path.contains('\0') {
        return Err(invalid());
    }
    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) if !files::reserved_component(&name.to_string_lossy()) => {}
            _ => return Err(invalid()),
        }
    }
    Ok(path)
}
