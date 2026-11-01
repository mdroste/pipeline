//! Changes the researcher asks for: commits, branches, sync, and conflict
//! resolution. Each action refreshes and returns the page's state.
use super::*;

const STARTER_IGNORE: &str = "\
# Files Git should leave alone. Add your own below.
.DS_Store
Thumbs.db
.pipeline-tasks/
.pipeline-apply-*

# LaTeX build files
*.aux
*.fdb_latexmk
*.fls
*.synctex.gz
*.toc

# R and Python
.Rhistory
.RData
__pycache__/
.ipynb_checkpoints/
";

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictChoice {
    /// Keep the version on this branch.
    Ours,
    /// Keep the incoming version.
    Theirs,
    /// The researcher fixed the file by hand.
    Edited,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Action {
    Initialize,
    SetRemote {
        url: String,
    },
    SetIdentity {
        name: String,
        email: String,
    },
    Commit {
        paths: Vec<String>,
        summary: String,
        description: Option<String>,
    },
    Discard {
        paths: Vec<String>,
    },
    Ignore {
        path: String,
    },
    UndoCommit,
    SwitchBranch {
        name: String,
    },
    CreateBranch {
        name: String,
    },
    Fetch,
    Pull,
    Push,
    Resolve {
        path: String,
        choice: ConflictChoice,
    },
    AbortOperation,
}

impl Action {
    /// Whether this changes the project's files or Git's record of them, so it
    /// waits like any other file change. A fetch, a push, and a setting leave the
    /// folder as it is.
    pub fn changes_files(&self) -> bool {
        !matches!(
            self,
            Self::SetRemote { .. } | Self::SetIdentity { .. } | Self::Fetch | Self::Push
        )
    }

    /// The only files this can touch or record, when those can be named. None
    /// means any file in the project. A commit rewrites nothing in the folder, so
    /// it needs only its own files to be out of a local job's hands. A new branch
    /// starts from the same files.
    fn named_paths(&self) -> Option<Vec<&str>> {
        match self {
            Self::Commit { paths, .. } => Some(paths.iter().map(String::as_str).collect()),
            Self::Ignore { path } => Some(vec![path, ".gitignore"]),
            Self::Initialize => Some(vec![".gitignore"]),
            Self::CreateBranch { .. } => Some(Vec::new()),
            _ => None,
        }
    }
}

/// Takes the project lock for an action that changes files, and refuses one that
/// could reach a local job's files. A running job holds the lock only while it is
/// prepared, so the lock alone would let a branch switch rewrite what the job is
/// reading, or leave another branch's bytes to be recorded as the job's output.
pub fn claim_project(
    store: &Store,
    workspace_id: &str,
    root: &Path,
    action: &Action,
) -> WorkbenchResult<fs::File> {
    let lock = lock(store, workspace_id)?;
    match action.named_paths() {
        Some(paths) => research::jobs::ensure_paths_free(workspace_id, root, paths)?,
        None => research::jobs::ensure_project_free(workspace_id)?,
    }
    Ok(lock)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub state: VersionControl,
    /// One line on what happened, when the page itself would not show it.
    pub notice: Option<String>,
    /// The message of an undone commit, to put back in the commit box.
    pub undone_message: Option<String>,
}

#[derive(Default)]
pub(super) struct Outcome {
    pub(super) notice: Option<String>,
    pub(super) undone_message: Option<String>,
}

impl Outcome {
    fn notice(text: impl Into<String>) -> Self {
        Self {
            notice: Some(text.into()),
            undone_message: None,
        }
    }
}

pub fn perform(root: &Path, action: Action) -> WorkbenchResult<ActionResult> {
    let git = Git::open(root).ok_or_else(|| WorkbenchError::invalid("Git is not installed"))?;
    let guard = ActionGuard::claim(root)?;
    let outcome = run(&git, action, &guard.cancel, &|path| {
        file_browser::system_trash(path)
    })?;
    drop(guard);
    Ok(ActionResult {
        state: view(&git)?,
        notice: outcome.notice,
        undone_message: outcome.undone_message,
    })
}

struct Context<'a> {
    git: &'a Git,
    status: Option<RepositoryStatus>,
    cancel: &'a AtomicBool,
}

impl Context<'_> {
    fn remote(&self) -> Option<&repository::RepositoryRemote> {
        self.status.as_ref()?.remote.as_ref()
    }

    fn explained(&self, error: WorkbenchError) -> WorkbenchError {
        WorkbenchError::invalid(errors::explain(&error.message, self.remote()))
    }

    fn local(&self, args: &[&str]) -> WorkbenchResult<Vec<u8>> {
        self.git
            .act(args, LOCAL_TIMEOUT, None)
            .map_err(|error| self.explained(error))
    }

    fn network(&self, args: &[&str]) -> WorkbenchResult<Vec<u8>> {
        self.git
            .act(args, NETWORK_TIMEOUT, Some(self.cancel))
            .map_err(|error| self.explained(error))
    }

    fn fetching(&self, options: &[&str], remote: &str) -> WorkbenchResult<Vec<u8>> {
        self.git
            .fetch(options, remote, NETWORK_TIMEOUT, Some(self.cancel))
            .map_err(|error| {
                let error = self.explained(error);
                let advice = self.git.guard().ok().and_then(|guard| guard.fetch_advice());
                match advice {
                    Some(advice) if !self.cancel.load(Ordering::Relaxed) => {
                        WorkbenchError::invalid(format!("{} {advice}", error.message))
                    }
                    _ => error,
                }
            })
    }

    fn remote_name(&self) -> WorkbenchResult<String> {
        self.status
            .as_ref()
            .and_then(|status| repository::remote_name(status, &|args| self.git.read(args)))
            .ok_or_else(|| WorkbenchError::invalid("Connect a remote first."))
    }

    fn branch(&self) -> WorkbenchResult<String> {
        self.status
            .as_ref()
            .and_then(|status| status.branch.clone())
            .ok_or_else(|| WorkbenchError::invalid("Switch to a branch first."))
    }
}

pub(super) fn run(
    git: &Git,
    action: Action,
    cancel: &AtomicBool,
    trash: &dyn Fn(&Path) -> WorkbenchResult<()>,
) -> WorkbenchResult<Outcome> {
    let status = repository::inspect_with(&git.root, &|args| git.read(args));
    let context = Context {
        git,
        status,
        cancel,
    };
    match action {
        Action::Initialize => initialize(&context),
        Action::SetRemote { url } => set_remote(&context, &url),
        Action::SetIdentity { name, email } => set_identity(&context, &name, &email),
        Action::Commit {
            paths,
            summary,
            description,
        } => commit(&context, &paths, &summary, description.as_deref()),
        Action::Discard { paths } => discard(&context, &paths, trash),
        Action::Ignore { path } => ignore(&context, &path),
        Action::UndoCommit => undo_commit(&context),
        Action::SwitchBranch { name } => switch_branch(&context, &name),
        Action::CreateBranch { name } => create_branch(&context, &name),
        Action::Fetch => fetch(&context),
        Action::Pull => pull(&context),
        Action::Push => push(&context),
        Action::Resolve { path, choice } => resolve(&context, &path, choice, trash),
        Action::AbortOperation => abort(&context),
    }
}

/// A NUL-separated list of literal paths. A file instead of arguments keeps a
/// first commit of thousands of files under the command-line limit.
fn path_list(paths: &[&str]) -> WorkbenchResult<tempfile::NamedTempFile> {
    let mut list = tempfile::NamedTempFile::new().map_err(err)?;
    list.write_all(paths.join("\0").as_bytes()).map_err(err)?;
    list.flush().map_err(err)?;
    Ok(list)
}

fn with_paths(context: &Context<'_>, args: &[&str], paths: &[&str]) -> WorkbenchResult<Vec<u8>> {
    let list = path_list(paths)?;
    let flag = format!("--pathspec-from-file={}", list.path().display());
    let mut args = args.to_vec();
    args.extend([flag.as_str(), "--pathspec-file-nul"]);
    context.local(&args)
}

fn rows(context: &Context<'_>) -> WorkbenchResult<Vec<status::ChangedFile>> {
    let prefix = context
        .git
        .read_text(&["rev-parse", "--show-prefix"])?
        .trim_end_matches(['\n', '\r'])
        .to_string();
    Ok(status::changes(context.git, &prefix)?.files)
}

fn selected(context: &Context<'_>, paths: &[String]) -> WorkbenchResult<Vec<status::ChangedFile>> {
    let chosen: BTreeSet<&str> = paths
        .iter()
        .map(|path| checked_path(path))
        .collect::<WorkbenchResult<_>>()?;
    let rows: Vec<_> = rows(context)?
        .into_iter()
        .filter(|row| chosen.contains(row.path.as_str()))
        .collect();
    if rows.is_empty() {
        return Err(WorkbenchError::invalid("Choose at least one changed file."));
    }
    Ok(rows)
}

fn initialize(context: &Context<'_>) -> WorkbenchResult<Outcome> {
    context.local(&["init", "--quiet"])?;
    // Set here rather than with `--initial-branch`, which needs a newer Git.
    if context
        .git
        .read(&["config", "--get", "init.defaultBranch"])
        .is_err()
    {
        context.local(&["symbolic-ref", "HEAD", "refs/heads/main"])?;
    }
    let starter = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(context.git.root.join(".gitignore"));
    if let Ok(mut file) = starter {
        file.write_all(STARTER_IGNORE.as_bytes()).map_err(err)?;
    }
    Ok(Outcome::default())
}

/// A remote address Git will contact without running anything locally. HTTPS
/// sign-in details are dropped: `.git/config` is plain text, and Bitbucket's
/// clone addresses carry a username the credential helper doesn't need.
fn remote_address(url: &str) -> WorkbenchResult<String> {
    let url = url.trim();
    let invalid = || {
        WorkbenchError::invalid(
            "Enter the repository's HTTPS or SSH address, such as https://github.com/owner/name.git",
        )
    };
    if url.starts_with('-')
        || url.contains("::")
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(invalid());
    }
    let mut address = url.to_string();
    if let Some((scheme, rest)) = url.split_once("://") {
        let scheme = scheme.to_ascii_lowercase();
        if !matches!(scheme.as_str(), "https" | "ssh") {
            return Err(invalid());
        }
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        if let Some((user, host)) = authority.rsplit_once('@') {
            if user.contains(':') {
                return Err(WorkbenchError::invalid(
                    "Remove the password from this address. Git would store it as plain text.",
                ));
            }
            if scheme == "https" {
                address = format!("https://{host}/{path}");
            }
        }
    }
    repository::parse_remote("origin", &address).ok_or_else(invalid)?;
    Ok(address)
}

fn set_remote(context: &Context<'_>, url: &str) -> WorkbenchResult<Outcome> {
    let url = remote_address(url)?;
    let exists = context.git.read(&["remote", "get-url", "origin"]).is_ok();
    let verb = if exists { "set-url" } else { "add" };
    context.local(&["remote", verb, "origin", &url])?;
    Ok(Outcome::default())
}

fn set_identity(context: &Context<'_>, name: &str, email: &str) -> WorkbenchResult<Outcome> {
    let (name, email) = (name.trim(), email.trim());
    if name.is_empty() || email.is_empty() || !email.contains('@') {
        return Err(WorkbenchError::invalid(
            "Enter the name and email to record on your commits.",
        ));
    }
    // Stored for this repository only, so other projects keep their settings.
    context.local(&["config", "user.name", name])?;
    context.local(&["config", "user.email", email])?;
    Ok(Outcome::default())
}

/// Adds lines to a text file without rewriting what is already there, which
/// may not be valid UTF-8.
fn append_lines(path: &Path, lines: &str) -> WorkbenchResult<()> {
    let current = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(err(error)),
    };
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .map_err(err)?;
    let separator = if current.is_empty() || current.ends_with(b"\n") {
        ""
    } else {
        "\n"
    };
    file.write_all(format!("{separator}{lines}").as_bytes())
        .map_err(err)
}

/// Keeps Pipeline's scratch folders out of a commit even when a whole new parent
/// folder is selected. Git never shares this file with other people.
fn exclude_scratch(context: &Context<'_>) -> WorkbenchResult<()> {
    let path = context
        .git
        .read_text(&["rev-parse", "--git-path", "info/exclude"])?;
    let path = context.git.root.join(path.trim());
    let current = fs::read(&path).unwrap_or_default();
    if String::from_utf8_lossy(&current)
        .lines()
        .any(|line| line == ".pipeline-tasks/")
    {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(err)?;
    }
    append_lines(&path, ".pipeline-tasks/\n.pipeline-apply-*\n")
}

fn commit(
    context: &Context<'_>,
    paths: &[String],
    summary: &str,
    description: Option<&str>,
) -> WorkbenchResult<Outcome> {
    let summary = summary.trim();
    if summary.is_empty() {
        return Err(WorkbenchError::invalid("Write a short summary first."));
    }
    let (operation, _) = status::operation(context.git);
    if status::unmerged_count(context.git) > 0 {
        return Err(WorkbenchError::invalid(
            "Resolve the conflicted files before you commit.",
        ));
    }
    let mut message = vec!["commit", "--quiet", "-m", summary];
    if let Some(description) = description.map(str::trim).filter(|text| !text.is_empty()) {
        message.extend(["-m", description]);
    }
    exclude_scratch(context)?;
    match operation.as_deref() {
        None => {}
        // Git allows no partial commit here. Resolved files are already staged;
        // anything else in the folder stays out of the merge.
        Some("merge") | Some("cherry-pick") | Some("revert") => {
            context.local(&message)?;
            return Ok(Outcome::default());
        }
        Some(_) => {
            return Err(WorkbenchError::invalid(
                "Finish the rebase from a terminal before you commit.",
            ))
        }
    }
    let chosen = selected(context, paths)?;
    let add: Vec<&str> = chosen
        .iter()
        .filter(|row| row.unstaged)
        .map(|row| row.path.as_str())
        .collect();
    if !add.is_empty() {
        with_paths(context, &["add", "-A"], &add)?;
    }
    // A file staged as new and then deleted is gone once added, so `--only`
    // would not find it. A rename needs its old name too, or the file would be
    // committed twice. `--only` leaves staged work outside the selection alone.
    let mut commit_paths: Vec<&str> = chosen
        .iter()
        .filter(|row| {
            row.status != "added" || fs::symlink_metadata(context.git.root.join(&row.path)).is_ok()
        })
        .map(|row| row.path.as_str())
        .collect();
    commit_paths.extend(chosen.iter().filter_map(|row| row.old_path.as_deref()));
    if commit_paths.is_empty() {
        return Err(WorkbenchError::invalid(
            "Git found nothing to commit in those files.",
        ));
    }
    message.push("--only");
    with_paths(context, &message, &commit_paths)?;
    Ok(Outcome::default())
}

/// Moves a working copy to the Trash, and says whether there was one.
fn keep_copy(
    context: &Context<'_>,
    path: &str,
    trash: &dyn Fn(&Path) -> WorkbenchResult<()>,
) -> WorkbenchResult<bool> {
    let target = context.git.root.join(path);
    if fs::symlink_metadata(&target).is_err() {
        return Ok(false);
    }
    let root = context.git.root.canonicalize().map_err(err)?;
    let parent = target.parent().unwrap_or(&target);
    if !parent.canonicalize().map_err(err)?.starts_with(&root) {
        return Err(WorkbenchError::invalid("This file left the project folder"));
    }
    trash(&target)?;
    Ok(true)
}

/// Moves the working copies to the Trash, then returns the files to their last
/// committed state. New files are simply gone from the folder.
fn discard(
    context: &Context<'_>,
    paths: &[String],
    trash: &dyn Fn(&Path) -> WorkbenchResult<()>,
) -> WorkbenchResult<Outcome> {
    let chosen = selected(context, paths)?;
    for row in &chosen {
        let refusal = if row.status == "conflicted" {
            "A conflicted file can't be discarded. Choose a version to keep instead."
        } else if row.is_submodule {
            "A submodule's changes belong to its own repository. Discard them there."
        } else if row.staged && row.unstaged {
            // The Trash can hold the working copy, but not the staged version.
            "This file has staged and unstaged changes. Sort it out from a terminal, so neither version is lost."
        } else {
            continue;
        };
        return Err(WorkbenchError::invalid(refusal));
    }
    if context.status.as_ref().is_some_and(|s| s.head.is_none())
        && chosen
            .iter()
            .any(|row| row.status != "untracked" && row.status != "added")
    {
        return Err(WorkbenchError::invalid("There is no commit to go back to."));
    }
    let mut unstage = Vec::new();
    let mut restore = Vec::new();
    for row in &chosen {
        keep_copy(context, &row.path, trash)?;
        match row.status.as_str() {
            "untracked" => {}
            "added" | "copied" => unstage.push(row.path.as_str()),
            "renamed" => {
                unstage.push(row.path.as_str());
                if let Some(old) = &row.old_path {
                    // New work may already sit at the old name.
                    keep_copy(context, old, trash)?;
                    restore.push(old.as_str());
                }
            }
            _ => restore.push(row.path.as_str()),
        }
    }
    if !unstage.is_empty() {
        with_paths(context, &["reset", "--quiet"], &unstage)?;
    }
    if !restore.is_empty() {
        with_paths(
            context,
            &["restore", "--source=HEAD", "--staged", "--worktree"],
            &restore,
        )?;
    }
    Ok(Outcome::notice("Moved to the Trash."))
}

/// A `.gitignore` line matching exactly this path from the project folder.
fn ignore_pattern(path: &str) -> String {
    let mut pattern = String::from("/");
    for character in path.chars() {
        if matches!(character, '\\' | '*' | '?' | '[' | '!' | '#') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern
}

fn ignore(context: &Context<'_>, path: &str) -> WorkbenchResult<Outcome> {
    let path = checked_path(path)?;
    let committed = context.status.as_ref().is_some_and(|s| s.head.is_some())
        && !context
            .git
            .read(&["ls-tree", "--name-only", "HEAD", "--", path])?
            .is_empty();
    if committed {
        return Err(WorkbenchError::invalid(
            "This file is already in the repository, so .gitignore won't affect it.",
        ));
    }
    if !context.git.read(&["ls-files", "--", path])?.is_empty() {
        with_paths(context, &["reset", "--quiet"], &[path])?;
    }
    let file = context.git.root.join(".gitignore");
    if fs::symlink_metadata(&file).is_ok_and(|meta| !meta.is_file()) {
        return Err(WorkbenchError::invalid(".gitignore is not a regular file."));
    }
    append_lines(&file, &format!("{}\n", ignore_pattern(path)))?;
    Ok(Outcome::notice(format!("Added {path} to .gitignore.")))
}

fn undo_commit(context: &Context<'_>) -> WorkbenchResult<Outcome> {
    let status = context
        .status
        .as_ref()
        .ok_or_else(|| WorkbenchError::invalid("The project folder is not a Git repository"))?;
    let (operation, _) = status::operation(context.git);
    if operation.is_some() || !status::latest_commit_is_local(context.git, status) {
        return Err(WorkbenchError::invalid(
            "Only a commit that hasn't been pushed can be undone.",
        ));
    }
    let message = context.git.read_text(&["log", "-1", "--format=%B"])?;
    context.local(&["reset", "--quiet", "--soft", "HEAD~1"])?;
    Ok(Outcome {
        notice: Some("Commit undone. Its changes are back in your list.".to_string()),
        undone_message: Some(message.trim().to_string()),
    })
}

fn checked_branch<'a>(context: &Context<'_>, name: &'a str) -> WorkbenchResult<&'a str> {
    let name = name.trim();
    context
        .git
        .read(&["check-ref-format", "--branch", name])
        .map_err(|_| {
            WorkbenchError::invalid(format!("\"{name}\" can't be used as a branch name."))
        })?;
    Ok(name)
}

fn switch_branch(context: &Context<'_>, name: &str) -> WorkbenchResult<Outcome> {
    let name = checked_branch(context, name)?;
    let branch = status::branches(context.git)
        .into_iter()
        .find(|branch| branch.name == name)
        .ok_or_else(|| WorkbenchError::invalid("That branch no longer exists."))?;
    match &branch.remote_ref {
        Some(remote) => context.local(&["switch", "--quiet", "--track", remote])?,
        None => context.local(&["switch", "--quiet", name])?,
    };
    Ok(Outcome::default())
}

fn create_branch(context: &Context<'_>, name: &str) -> WorkbenchResult<Outcome> {
    let name = checked_branch(context, name)?;
    context.local(&["switch", "--quiet", "-c", name])?;
    Ok(Outcome::default())
}

fn fetch(context: &Context<'_>) -> WorkbenchResult<Outcome> {
    let remote = context.remote_name()?;
    context.fetching(&["--quiet", "--prune"], &remote)?;
    Ok(Outcome::default())
}

/// Fetches, then merges as a separate step. Stop can end only the network part,
/// so it never interrupts Git while it rewrites files in the folder. The fetch
/// runs under the guard and the merge doesn't: a merge needs the repository's
/// filters to write its files correctly.
fn pull(context: &Context<'_>) -> WorkbenchResult<Outcome> {
    let has_upstream = context
        .status
        .as_ref()
        .is_some_and(|status| status.upstream.is_some());
    if !has_upstream {
        return Err(WorkbenchError::invalid("Publish this branch first."));
    }
    fetch(context)?;
    if context.cancel.load(Ordering::Relaxed) {
        return Ok(Outcome::notice("Stopped. Nothing was merged."));
    }
    let merged = context
        .git
        .act(
            &["merge", "--quiet", "--no-edit", "@{upstream}"],
            NETWORK_TIMEOUT,
            None,
        )
        .map_err(|error| context.explained(error));
    match merged {
        Ok(_) => Ok(Outcome::default()),
        // Git stops here on purpose. The page takes over with the conflict list.
        Err(_) if status::operation(context.git).0.as_deref() == Some("merge") => Ok(
            Outcome::notice("Some files changed on both sides. Choose what to keep below."),
        ),
        Err(error) => Err(error),
    }
}

fn push(context: &Context<'_>) -> WorkbenchResult<Outcome> {
    let has_upstream = context
        .status
        .as_ref()
        .is_some_and(|status| status.upstream.is_some());
    if has_upstream {
        context.network(&["push", "--quiet"])?;
    } else {
        let remote = context.remote_name()?;
        let branch = context.branch()?;
        context
            .git
            .act(
                &["push", "--quiet", "--set-upstream", "--", &remote, &branch],
                NETWORK_TIMEOUT,
                Some(context.cancel),
            )
            .map_err(|error| {
                WorkbenchError::invalid(errors::explain_publish(
                    &error.message,
                    context.remote(),
                    &branch,
                ))
            })?;
    }
    Ok(Outcome::default())
}

fn has_conflict_markers(bytes: &[u8]) -> bool {
    bytes
        .split(|byte| *byte == b'\n')
        .any(|line| line.starts_with(b"<<<<<<< "))
}

fn resolve(
    context: &Context<'_>,
    path: &str,
    choice: ConflictChoice,
    trash: &dyn Fn(&Path) -> WorkbenchResult<()>,
) -> WorkbenchResult<Outcome> {
    let path = checked_path(path)?;
    let (operation, _) = status::operation(context.git);
    let side = match choice {
        ConflictChoice::Ours => Some(("--ours", '2')),
        ConflictChoice::Theirs => Some(("--theirs", '3')),
        ConflictChoice::Edited => None,
    };
    if side.is_some() && operation.as_deref() == Some("rebase") {
        return Err(WorkbenchError::invalid(
            "Finish the rebase from a terminal.",
        ));
    }
    match side {
        Some((flag, stage)) => {
            // Each line starts `<mode> <hash> <stage>\t<path>`.
            let stages = context
                .git
                .read_text(&["ls-files", "--unmerged", "--", path])?;
            if stages.is_empty() {
                return Err(WorkbenchError::invalid(
                    "This file is no longer in conflict.",
                ));
            }
            let exists = stages.lines().any(|line| {
                line.split('\t')
                    .next()
                    .is_some_and(|fields| fields.ends_with(stage))
            });
            // The file may hold a hand merge, whatever markers are left in it. A
            // folder here is a submodule, which checkout leaves as it is.
            let folder =
                fs::symlink_metadata(context.git.root.join(path)).is_ok_and(|meta| meta.is_dir());
            let kept = !(exists && folder) && keep_copy(context, path, trash)?;
            if exists {
                context.local(&["checkout", flag, "--", path])?;
                context.local(&["add", "--", path])?;
            } else {
                context.local(&["rm", "--quiet", "--force", "--", path])?;
            }
            if kept {
                return Ok(Outcome::notice("Your previous copy is in the Trash."));
            }
        }
        None => {
            let target = context.git.root.join(path);
            if fs::symlink_metadata(&target).is_ok_and(|meta| meta.is_file())
                && fs::metadata(&target).map_err(err)?.len() <= MAX_FILE
                && has_conflict_markers(&fs::read(&target).map_err(err)?)
            {
                return Err(WorkbenchError::invalid(
                    "This file still has conflict markers. Edit it, then try again.",
                ));
            }
            context.local(&["add", "-A", "--", path])?;
        }
    }
    Ok(Outcome::default())
}

fn abort(context: &Context<'_>) -> WorkbenchResult<Outcome> {
    let (operation, _) = status::operation(context.git);
    let operation = operation.ok_or_else(|| WorkbenchError::invalid("Nothing is in progress."))?;
    context.local(&[operation.as_str(), "--abort"])?;
    Ok(Outcome::default())
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn remote_addresses_refuse_helpers_passwords_and_local_paths() {
        for good in [
            "https://github.com/owner/name.git",
            "https://ada@bitbucket.org/team/name.git",
            "git@bitbucket.org:team/name.git",
            "ssh://git@github.com/owner/name.git",
            "  https://git.overleaf.com/64f0c0ffee  ",
        ] {
            assert!(remote_address(good).is_ok(), "{good}");
        }
        // Whatever sits before the host in an HTTPS address may be a token.
        assert_eq!(
            remote_address("https://ghp_token@github.com/owner/name.git").unwrap(),
            "https://github.com/owner/name.git"
        );
        for bad in [
            "ext::sh -c touch%20x",
            "--upload-pack=touch",
            "file:///tmp/repo.git",
            "http://github.com/owner/name.git",
            "/srv/git/local.git",
            "https://user:secret@github.com/owner/name.git",
            "https://github.com/owner/na me.git",
        ] {
            assert!(remote_address(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn ignore_lines_match_only_the_literal_path() {
        assert_eq!(ignore_pattern("data/raw.csv"), "/data/raw.csv");
        assert_eq!(ignore_pattern("a[1]*.dta"), "/a\\[1]\\*.dta");
    }

    #[test]
    fn conflict_markers_are_found_only_at_line_starts() {
        assert!(has_conflict_markers(b"a\n<<<<<<< HEAD\nb"));
        assert!(!has_conflict_markers(b"a <<<<<<< HEAD\n"));
    }
}
