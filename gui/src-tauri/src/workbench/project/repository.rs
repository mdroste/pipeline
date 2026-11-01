//! Read-only view of the Git repository a project folder lives in.
//!
//! Status is computed from the local clone only. The single network operation
//! is `fetch`, which runs only when the researcher asks for it and uses their
//! own Git credentials. Nothing here commits, pushes, or changes the working
//! tree, and remote URLs are reduced to host/owner/name so embedded
//! credentials never leave this module. A folder's own Git config has no effect
//! here, the fetch included: it may be a folder the researcher only unpacked, and
//! this page offers the fetch as a look at the remote. So the fetch can fail where
//! a terminal succeeds, when a repository needs its own connection settings, and
//! its error says so. Changes the researcher asks for live in `version_control`.
use super::tasks::git_fetch;
use super::version_control::Git;
use super::*;

const RECENT_COMMITS: usize = 30;
const INCOMING_COMMITS: usize = 20;
const CHANGED_PATHS: usize = 5;

/// Runs Git with the given arguments in the repository being inspected.
pub(super) type GitRunner<'a> = dyn Fn(&[&str]) -> WorkbenchResult<Vec<u8>> + 'a;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryRemote {
    pub name: String,
    /// Empty, like `owner` and `repo`, for an address that can't be reduced to
    /// them, such as a shared-drive path. Such a remote still works.
    pub host: String,
    pub owner: String,
    pub repo: String,
    /// `github` or `bitbucket`; None for any other host.
    pub provider: Option<String>,
    /// Browsable address, present only for hosts whose URL layout is known.
    pub web_url: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryCommit {
    pub sha: String,
    pub author: String,
    pub date: String,
    pub subject: String,
}
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryStatus {
    /// None while HEAD is detached.
    pub branch: Option<String>,
    /// None before the first commit.
    pub head: Option<String>,
    pub upstream: Option<String>,
    /// Relative to `upstream` as of the last fetch.
    pub ahead: u32,
    pub behind: u32,
    pub fetched_at: Option<String>,
    pub changed: u32,
    pub untracked: u32,
    pub conflicts: u32,
    pub changed_paths: Vec<String>,
    pub remote: Option<RepositoryRemote>,
    /// Recent commits on HEAD that touch the project folder, newest first.
    pub commits: Vec<RepositoryCommit>,
    /// Commits on `upstream` that HEAD does not have, as of the last fetch.
    pub incoming: Vec<RepositoryCommit>,
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).trim().to_string()
}

/// Reduces a remote URL to its host, owner, and repository name.
pub(super) fn parse_remote(name: &str, url: &str) -> Option<RepositoryRemote> {
    let url = url.trim();
    let (host, path) = if let Some((_, rest)) = url.split_once("://") {
        let (authority, path) = rest.split_once('/')?;
        // Drop any user[:password]@ prefix and a port.
        let host = authority.rsplit('@').next()?;
        (host.split(':').next()?, path)
    } else {
        // scp-like syntax: [user@]host:path
        let (authority, path) = url.split_once(':')?;
        (authority.rsplit('@').next()?, path)
    };
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repo) = match path.rsplit_once('/') {
        Some((owner, repo)) => (owner, repo),
        None => ("", path),
    };
    let plain = |value: &str, extra: &str| {
        !value.is_empty()
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
    };
    if !plain(host, ".-") || !plain(repo, "._-") {
        return None;
    }
    let host = host.to_ascii_lowercase();
    let provider = match host.as_str() {
        "github.com" if plain(owner, "-") => Some("github"),
        "bitbucket.org" if plain(owner, "._-") => Some("bitbucket"),
        _ => None,
    };
    Some(RepositoryRemote {
        name: name.to_string(),
        web_url: provider.map(|_| format!("https://{host}/{owner}/{repo}")),
        provider: provider.map(str::to_string),
        host,
        owner: owner.to_string(),
        repo: repo.to_string(),
    })
}

/// The one status read of a refresh. Its NUL-separated entries feed both the
/// branch summary here and the file list in `version_control::status`.
pub(super) const STATUS_ARGS: [&str; 7] = [
    "status",
    "--porcelain=v2",
    "--branch",
    "-z",
    "--untracked-files=normal",
    "--",
    ".",
];

/// Parses `git status --porcelain=v2 --branch -z` output.
pub(super) fn parse_status(output: &str) -> RepositoryStatus {
    let mut status = RepositoryStatus::default();
    let mut path = |line: &str, fields: usize| {
        if status.changed_paths.len() < CHANGED_PATHS {
            if let Some(path) = line.splitn(fields, ' ').last() {
                status.changed_paths.push(path.to_string());
            }
        }
    };
    let mut changed = 0;
    let mut untracked = 0;
    let mut conflicts = 0;
    let mut header = Vec::new();
    let mut entries = output.split('\0');
    while let Some(line) = entries.next() {
        if let Some(rest) = line.strip_prefix("# ") {
            header.push(rest.to_string());
        } else if line.starts_with("1 ") {
            changed += 1;
            path(line, 9);
        } else if line.starts_with("2 ") {
            changed += 1;
            path(line, 10);
            // A rename's original path is its own entry, and may look like one.
            entries.next();
        } else if line.starts_with("u ") {
            conflicts += 1;
            path(line, 11);
        } else if line.starts_with("? ") {
            untracked += 1;
            path(line, 2);
        }
    }
    status.changed = changed;
    status.untracked = untracked;
    status.conflicts = conflicts;
    for line in header {
        if let Some(oid) = line.strip_prefix("branch.oid ") {
            status.head = (oid != "(initial)").then(|| oid.chars().take(40).collect());
        } else if let Some(head) = line.strip_prefix("branch.head ") {
            status.branch = (head != "(detached)").then(|| head.to_string());
        } else if let Some(upstream) = line.strip_prefix("branch.upstream ") {
            status.upstream = Some(upstream.to_string());
        } else if let Some(counts) = line.strip_prefix("branch.ab ") {
            let mut parts = counts.split(' ');
            let number = |part: Option<&str>| {
                part.and_then(|p| p.get(1..))
                    .and_then(|p| p.parse::<u32>().ok())
                    .unwrap_or(0)
            };
            status.ahead = number(parts.next());
            status.behind = number(parts.next());
        }
    }
    status
}

const LOG_FORMAT: &str = "--pretty=format:%H%x1f%an%x1f%aI%x1f%s%x1e";
pub(super) fn parse_log(output: &str) -> Vec<RepositoryCommit> {
    output
        .split('\u{1e}')
        .filter_map(|record| {
            let mut fields = record.trim_start_matches(['\n', '\r']).split('\u{1f}');
            Some(RepositoryCommit {
                sha: fields.next().filter(|sha| !sha.is_empty())?.to_string(),
                author: fields.next()?.to_string(),
                date: fields.next()?.to_string(),
                subject: fields.next()?.chars().take(200).collect(),
            })
        })
        .collect()
}

/// The repository state of `root`, or None when it is not inside a Git work tree
/// (or Git is not installed).
pub fn inspect(root: &Path) -> Option<RepositoryStatus> {
    // This makes about seven calls for each Overview load.
    let direct = Git::open(root)?;
    inspect_with(root, &|args| direct.read(args))
}

/// `inspect` over a caller's way of running Git in `root`.
pub(super) fn inspect_with(root: &Path, run: &GitRunner<'_>) -> Option<RepositoryStatus> {
    let inside = run(&["rev-parse", "--is-inside-work-tree"]).ok()?;
    if text(inside) != "true" {
        return None;
    }
    let output = run(&STATUS_ARGS).ok()?;
    let git_dir = run(&["rev-parse", "--absolute-git-dir"]).ok().map(text);
    Some(status_from(
        root,
        &String::from_utf8_lossy(&output),
        git_dir.as_deref(),
        run,
    ))
}

/// The summary for one status read, with its recent commits and remote.
pub(super) fn status_from(
    root: &Path,
    output: &str,
    git_dir: Option<&str>,
    run: &GitRunner<'_>,
) -> RepositoryStatus {
    let mut status = parse_status(output);
    let limit = format!("-n{RECENT_COMMITS}");
    if status.head.is_some() {
        if let Ok(log) = run(&["log", &limit, "--no-color", LOG_FORMAT, "--", "."]) {
            status.commits = parse_log(&String::from_utf8_lossy(&log));
        }
    }
    if status.upstream.is_some() && status.behind > 0 {
        let limit = format!("-n{INCOMING_COMMITS}");
        if let Ok(log) = run(&["log", &limit, "--no-color", LOG_FORMAT, "HEAD..@{upstream}"]) {
            status.incoming = parse_log(&String::from_utf8_lossy(&log));
        }
    }
    let remote = remote_name(&status, run);
    if let Some(name) = &remote {
        let url = run(&["remote", "get-url", name]).ok();
        status.remote = Some(
            url.and_then(|url| parse_remote(name, &text(url)))
                .unwrap_or_else(|| RepositoryRemote {
                    name: name.clone(),
                    host: String::new(),
                    owner: String::new(),
                    repo: String::new(),
                    provider: None,
                    web_url: None,
                }),
        );
    }
    if let Some(dir) = git_dir {
        status.fetched_at = fs::metadata(root.join(dir).join("FETCH_HEAD"))
            .and_then(|meta| meta.modified())
            .ok()
            .map(|at| chrono::DateTime::<chrono::Utc>::from(at).to_rfc3339());
    }
    status
}

/// The remote the current branch tracks, else `origin` when it exists.
pub(super) fn remote_name(status: &RepositoryStatus, run: &GitRunner<'_>) -> Option<String> {
    let remotes = text(run(&["remote"]).ok()?);
    let remotes: Vec<&str> = remotes.lines().collect();
    status
        .upstream
        .as_deref()
        .and_then(|upstream| {
            remotes
                .iter()
                .filter(|name| upstream.starts_with(&format!("{name}/")))
                .max_by_key(|name| name.len())
        })
        .or_else(|| remotes.iter().find(|name| **name == "origin"))
        .or_else(|| remotes.first())
        .map(|name| name.to_string())
}

/// Updates remote-tracking refs from the project's remote. The working tree,
/// index, and local branches are left alone.
pub fn fetch(root: &Path) -> WorkbenchResult<RepositoryStatus> {
    let status = inspect(root)
        .ok_or_else(|| WorkbenchError::invalid("The project folder is not a Git repository"))?;
    let direct = Git::open(root).ok_or_else(|| WorkbenchError::invalid("Git is not installed"))?;
    let remote = remote_name(&status, &|args| direct.read(args))
        .ok_or_else(|| WorkbenchError::invalid("This repository has no remote to check"))?;
    let advice = direct
        .guard()?
        .fetch_advice()
        .unwrap_or_else(|| "Confirm you can run `git fetch` in this folder.".to_string());
    git_fetch(root, &["--quiet", "--no-tags"], &remote).map_err(|error| {
        WorkbenchError::invalid(format!(
            "The remote could not be checked. {advice} {}",
            error.message.trim_start_matches("Git task setup failed: ")
        ))
    })?;
    inspect(root)
        .ok_or_else(|| WorkbenchError::invalid("The project folder is not a Git repository"))
}

/// The project's folder, or None when no folder is attached.
pub fn repository_root(store: &Store, workspace_id: &str) -> WorkbenchResult<Option<PathBuf>> {
    scope(store, workspace_id)?;
    if store.workspace(workspace_id)?.root.is_none() {
        return Ok(None);
    }
    root(store, workspace_id).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::process::Command;

    #[cfg(unix)]
    fn run(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args([
                "-c",
                "user.name=Ada Author",
                "-c",
                "user.email=ada@example.org",
            ])
            .args([
                "-c",
                "init.defaultBranch=main",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }

    #[test]
    fn remote_urls_reduce_to_host_owner_and_name_without_credentials() {
        for url in [
            "git@github.com:mdroste/minwage.git",
            "https://github.com/mdroste/minwage",
            "https://someone:ghp_secret@github.com/mdroste/minwage.git",
            "ssh://git@github.com:22/mdroste/minwage.git",
        ] {
            let remote = parse_remote("origin", url).unwrap();
            assert_eq!(
                (
                    remote.host.as_str(),
                    remote.owner.as_str(),
                    remote.repo.as_str()
                ),
                ("github.com", "mdroste", "minwage"),
                "{url}"
            );
            assert_eq!(
                remote.web_url.as_deref(),
                Some("https://github.com/mdroste/minwage")
            );
            assert!(!serde_json::to_string(&remote).unwrap().contains("secret"));
        }
        let bitbucket = parse_remote("origin", "git@bitbucket.org:lab-team/minwage.git").unwrap();
        assert_eq!(bitbucket.provider.as_deref(), Some("bitbucket"));
        assert_eq!(
            bitbucket.web_url.as_deref(),
            Some("https://bitbucket.org/lab-team/minwage")
        );
        // Other hosts are named but never turned into links.
        let overleaf = parse_remote("origin", "https://git.overleaf.com/64f0c0ffee").unwrap();
        assert_eq!(overleaf.host, "git.overleaf.com");
        assert_eq!(overleaf.web_url, None);
        assert!(parse_remote("origin", "/srv/git/local.git").is_none());
        assert!(parse_remote("origin", "https://github.com/a/b?x=<script>").is_none());
    }

    #[test]
    fn porcelain_status_counts_changes_and_reads_branch_tracking() {
        let status = parse_status(
            "# branch.oid 1111111111111111111111111111111111111111\0\
             # branch.head main\0\
             # branch.upstream origin/main\0\
             # branch.ab +2 -3\0\
             1 .M N... 100644 100644 100644 aaa bbb paper/main.tex\0\
             2 R. N... 100644 100644 100644 aaa bbb R100 analysis/new name.do\0\
             1 this old name looks like an entry.do\0\
             u UU N... 100644 100644 100644 100644 aaa bbb ccc output/table4.tex\0\
             ? data/scratch.csv\0\
             ! ignored.log\0",
        );
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!((status.ahead, status.behind), (2, 3));
        assert_eq!(
            (status.changed, status.conflicts, status.untracked),
            (2, 1, 1)
        );
        assert_eq!(
            status.changed_paths,
            [
                "paper/main.tex",
                "analysis/new name.do",
                "output/table4.tex",
                "data/scratch.csv"
            ]
        );
        let detached = parse_status("# branch.oid (initial)\0# branch.head (detached)\0");
        assert_eq!((detached.head, detached.branch), (None, None));
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_outside_any_repository_has_no_status() {
        let temp = tempfile::tempdir().unwrap();
        assert!(inspect(temp.path()).is_none());
        assert!(fetch(temp.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn inspects_a_clone_and_sees_coauthor_commits_only_after_an_explicit_fetch() {
        let temp = tempfile::tempdir().unwrap();
        let remote = temp.path().join("remote.git");
        let mine = temp.path().join("mine");
        let theirs = temp.path().join("theirs");
        fs::create_dir(&remote).unwrap();
        run(&remote, &["init", "--bare", "--initial-branch=main"]);
        run(temp.path(), &["clone", remote.to_str().unwrap(), "mine"]);
        fs::write(mine.join("paper.tex"), "Draft one").unwrap();
        run(&mine, &["add", "."]);
        run(&mine, &["commit", "-m", "First draft"]);
        run(&mine, &["push", "-u", "origin", "HEAD:main"]);

        // A coauthor pushes; the local clone has not looked yet.
        run(temp.path(), &["clone", remote.to_str().unwrap(), "theirs"]);
        fs::write(theirs.join("paper.tex"), "Draft two").unwrap();
        run(&theirs, &["commit", "-am", "Tighten the introduction"]);
        run(&theirs, &["push", "origin", "HEAD:main"]);

        fs::write(mine.join("notes.txt"), "scratch").unwrap();
        fs::write(mine.join("paper.tex"), "Draft one, edited").unwrap();
        let before = inspect(&mine).unwrap();
        assert_eq!(before.branch.as_deref(), Some("main"));
        assert_eq!(before.upstream.as_deref(), Some("origin/main"));
        assert_eq!((before.changed, before.untracked), (1, 1));
        assert_eq!((before.ahead, before.behind), (0, 0));
        assert_eq!(before.commits.len(), 1);
        assert_eq!(before.commits[0].subject, "First draft");
        assert_eq!(before.commits[0].author, "Ada Author");

        let after = fetch(&mine).unwrap();
        assert_eq!(after.behind, 1);
        assert_eq!(after.incoming.len(), 1);
        assert_eq!(after.incoming[0].subject, "Tighten the introduction");
        assert!(after.fetched_at.is_some());
        // Fetching never touches the working tree or the local branch.
        assert_eq!(
            fs::read_to_string(mine.join("paper.tex")).unwrap(),
            "Draft one, edited"
        );
        assert_eq!(after.head, before.head);
    }
}
