//! Read-only diffs and history. Diffs are bounded before they are read: a
//! regenerated data file must not freeze the page.
use super::*;

const MAX_DIFF_LINES: u64 = 20_000;
const MAX_NEW_FILE: u64 = 1024 * 1024;
const HISTORY_PAGE: usize = 50;
const MAX_COMMIT_FILES: usize = 2000;

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DiffRequest {
    Working {
        path: String,
        old_path: Option<String>,
    },
    Commit {
        sha: String,
        path: String,
        old_path: Option<String>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    /// `diff` is unified diff text, `new` a new file's text, `conflict` a file
    /// with conflict markers; `binary`, `tooLarge`, `folder`, and `empty` carry
    /// no text.
    pub kind: String,
    pub text: String,
}

impl FileDiff {
    fn of(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            text: String::new(),
        }
    }
}

fn numstat_is_viewable(output: &str) -> Result<(), &'static str> {
    let mut total = 0u64;
    for line in output.lines() {
        let mut fields = line.split('\t');
        match (fields.next(), fields.next()) {
            (Some("-"), Some("-")) => return Err("binary"),
            (Some(added), Some(removed)) => {
                total += added.parse::<u64>().unwrap_or(0) + removed.parse::<u64>().unwrap_or(0);
            }
            _ => {}
        }
    }
    if total > MAX_DIFF_LINES {
        return Err("tooLarge");
    }
    Ok(())
}

/// Runs `base` with `--numstat`, then the patch when that shows it is viewable.
fn bounded_patch(git: &Git, base: &[&str], paths: &[&str]) -> WorkbenchResult<FileDiff> {
    let run = |mode: &str| {
        let mut args = base.to_vec();
        args.extend(["--relative", "--no-ext-diff", "--no-textconv", "--no-color"]);
        args.push(mode);
        args.push("--");
        args.extend_from_slice(paths);
        git.read(&args)
    };
    let stats = String::from_utf8_lossy(&run("--numstat")?).into_owned();
    if stats.trim().is_empty() {
        return Ok(FileDiff::of("empty"));
    }
    if let Err(kind) = numstat_is_viewable(&stats) {
        return Ok(FileDiff::of(kind));
    }
    match run("--patch") {
        Ok(patch) => Ok(FileDiff {
            kind: "diff".to_string(),
            text: String::from_utf8_lossy(&patch).into_owned(),
        }),
        Err(error) if error.message.contains("capture limit") => Ok(FileDiff::of("tooLarge")),
        Err(error) => Err(error),
    }
}

fn file_text(git: &Git, path: &str, kind: &str) -> WorkbenchResult<FileDiff> {
    let target = git.root.join(path);
    let meta = fs::symlink_metadata(&target).map_err(err)?;
    if meta.is_dir() {
        return Ok(FileDiff::of("folder"));
    }
    if !meta.is_file() {
        return Ok(FileDiff::of("binary"));
    }
    if meta.len() > MAX_NEW_FILE {
        return Ok(FileDiff::of("tooLarge"));
    }
    let bytes = fs::read(&target).map_err(err)?;
    if bytes.iter().take(8192).any(|byte| *byte == 0) {
        return Ok(FileDiff::of("binary"));
    }
    Ok(FileDiff {
        kind: kind.to_string(),
        text: String::from_utf8_lossy(&bytes).into_owned(),
    })
}

fn has_head(git: &Git) -> bool {
    git.read(&["rev-parse", "--quiet", "--verify", "HEAD"])
        .is_ok()
}

fn valid_sha(sha: &str) -> WorkbenchResult<&str> {
    if matches!(sha.len(), 40 | 64) && sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(sha)
    } else {
        Err(WorkbenchError::invalid("Invalid commit"))
    }
}

pub fn diff(root: &Path, request: DiffRequest) -> WorkbenchResult<FileDiff> {
    let git = opened(root)?;
    match request {
        DiffRequest::Working { path, old_path } => {
            let path = checked_path(&path)?;
            let row = status::changes(&git, &prefix(&git)?)?
                .files
                .into_iter()
                .find(|file| file.path == path);
            match row.as_ref().map(|file| file.status.as_str()) {
                Some("untracked") => file_text(&git, path, "new"),
                Some("conflicted") => match file_text(&git, path, "conflict") {
                    Ok(diff) => Ok(diff),
                    // One side deleted the file, so there is nothing to show.
                    Err(_) => Ok(FileDiff::of("empty")),
                },
                _ => {
                    let mut paths = vec![path];
                    if let Some(old) = &old_path {
                        paths.push(checked_path(old)?);
                    }
                    let base: &[&str] = if has_head(&git) {
                        &["diff", "-M", "HEAD"]
                    } else {
                        &["diff", "--cached"]
                    };
                    bounded_patch(&git, base, &paths)
                }
            }
        }
        DiffRequest::Commit {
            sha,
            path,
            old_path,
        } => {
            let sha = valid_sha(&sha)?;
            let mut paths = vec![checked_path(&path)?];
            if let Some(old) = &old_path {
                paths.push(checked_path(old)?);
            }
            let base = ["show", "--format=", "-M", "--first-parent", sha];
            bounded_patch(&git, &base, &paths)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCommit {
    pub sha: String,
    pub author: String,
    pub date: String,
    pub subject: String,
    pub is_merge: bool,
    /// Branch and tag names pointing here, such as `origin/main` and `v1.0`.
    pub refs: Vec<String>,
    /// No remote branch has this commit yet.
    pub unpushed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub commits: Vec<HistoryCommit>,
    pub has_more: bool,
}

fn parse_refs(field: &str) -> Vec<String> {
    field
        .split(", ")
        .filter(|name| !name.is_empty() && *name != "HEAD")
        .map(|name| {
            let name = name.strip_prefix("HEAD -> ").unwrap_or(name);
            name.strip_prefix("tag: ").unwrap_or(name).to_string()
        })
        .collect()
}

fn parse_history(output: &str, unpushed: &BTreeSet<String>) -> Vec<HistoryCommit> {
    output
        .split('\u{1e}')
        .filter_map(|record| {
            let mut fields = record.trim_start_matches(['\n', '\r']).split('\u{1f}');
            let sha = fields.next().filter(|sha| !sha.is_empty())?.to_string();
            let parents = fields.next()?;
            Some(HistoryCommit {
                is_merge: parents.split(' ').filter(|p| !p.is_empty()).count() > 1,
                author: fields.next()?.to_string(),
                date: fields.next()?.to_string(),
                subject: fields.next()?.chars().take(200).collect(),
                refs: parse_refs(fields.next()?),
                unpushed: unpushed.contains(&sha),
                sha,
            })
        })
        .collect()
}

/// A page of commits on HEAD that touch the project folder, newest first.
pub fn history(root: &Path, skip: usize) -> WorkbenchResult<HistoryPage> {
    let git = opened(root)?;
    if !has_head(&git) {
        return Ok(HistoryPage {
            commits: Vec::new(),
            has_more: false,
        });
    }
    let output = git.read_text(&[
        "log",
        &format!("-n{}", HISTORY_PAGE + 1),
        &format!("--skip={skip}"),
        "--no-color",
        "--decorate=short",
        "--format=%H%x1f%P%x1f%an%x1f%aI%x1f%s%x1f%D%x1e",
        "--",
        ".",
    ])?;
    let mut commits = parse_history(&output, &status::unpushed(&git));
    let has_more = commits.len() > HISTORY_PAGE;
    commits.truncate(HISTORY_PAGE);
    Ok(HistoryPage { commits, has_more })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFile {
    pub path: String,
    pub old_path: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    pub sha: String,
    pub author: String,
    pub date: String,
    pub message: String,
    pub files: Vec<CommitFile>,
    pub files_truncated: bool,
}

fn file_status(code: &str) -> &'static str {
    match code.as_bytes().first() {
        Some(b'A') => "added",
        Some(b'D') => "deleted",
        Some(b'R') => "renamed",
        Some(b'C') => "copied",
        _ => "modified",
    }
}

/// Parses `git diff-tree --name-status -z`.
fn parse_commit_files(output: &str) -> Vec<CommitFile> {
    let mut entries = output.split('\0').filter(|entry| !entry.is_empty());
    let mut files = Vec::new();
    while let Some(code) = entries.next() {
        let moved = matches!(code.as_bytes().first(), Some(b'R' | b'C'));
        let first = entries.next();
        let (old_path, path) = if moved {
            (first, entries.next())
        } else {
            (None, first)
        };
        let Some(path) = path else { break };
        files.push(CommitFile {
            path: path.to_string(),
            old_path: old_path.map(str::to_string),
            status: file_status(code).to_string(),
        });
    }
    files
}

/// One commit's message and the project files it changed. A merge is compared
/// with its first parent, which is what it brought into this branch.
pub fn commit_detail(root: &Path, sha: &str) -> WorkbenchResult<CommitDetail> {
    let git = opened(root)?;
    let sha = valid_sha(sha)?;
    let header = git.read_text(&["show", "-s", "--format=%an%x1f%aI%x1f%P%x1f%B", sha])?;
    let mut fields = header.splitn(4, '\u{1f}');
    let mut field = || fields.next().unwrap_or_default().to_string();
    let (author, date, parents, message) = (field(), field(), field(), field());
    let base = [
        "diff-tree",
        "-r",
        "-M",
        "--relative",
        "--no-commit-id",
        "--name-status",
        "-z",
    ];
    let mut args = base.to_vec();
    match parents.split_whitespace().next() {
        Some(parent) => args.extend([parent, sha]),
        None => args.extend(["--root", sha]),
    }
    let mut files = parse_commit_files(&git.read_text(&args)?);
    let files_truncated = files.len() > MAX_COMMIT_FILES;
    files.truncate(MAX_COMMIT_FILES);
    Ok(CommitDetail {
        sha: sha.to_string(),
        author,
        date,
        message: message.trim().to_string(),
        files,
        files_truncated,
    })
}

fn opened(root: &Path) -> WorkbenchResult<Git> {
    Git::open(root).ok_or_else(|| WorkbenchError::invalid("Git is not installed"))
}

fn prefix(git: &Git) -> WorkbenchResult<String> {
    Ok(git
        .read_text(&["rev-parse", "--show-prefix"])?
        .trim_end_matches(['\n', '\r'])
        .to_string())
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn history_reads_merges_decorations_and_what_is_unpushed() {
        let unpushed = BTreeSet::from(["aaa".to_string()]);
        let commits = parse_history(
            "aaa\u{1f}p1 p2\u{1f}Ada\u{1f}2026-01-02T03:04:05Z\u{1f}Merge\u{1f}HEAD -> main, origin/main, tag: v1\u{1e}\n\
             bbb\u{1f}\u{1f}Sam\u{1f}2026-01-01T03:04:05Z\u{1f}First\u{1f}\u{1e}\n",
            &unpushed,
        );
        assert_eq!(commits.len(), 2);
        assert!(commits[0].is_merge && commits[0].unpushed);
        assert_eq!(commits[0].refs, ["main", "origin/main", "v1"]);
        assert!(!commits[1].is_merge && !commits[1].unpushed);
    }

    #[test]
    fn commit_files_read_renames_with_both_names() {
        let files = parse_commit_files("M\0a b.do\0R100\0old.do\0new.do\0D\0gone.do\0");
        assert_eq!(files[0].path, "a b.do");
        assert_eq!(files[1].path, "new.do");
        assert_eq!(files[1].old_path.as_deref(), Some("old.do"));
        assert_eq!(files[1].status, "renamed");
        assert_eq!(files[2].status, "deleted");
    }

    #[test]
    fn numstat_stops_binary_and_oversized_diffs() {
        assert_eq!(numstat_is_viewable("3\t1\ta.do\n"), Ok(()));
        assert_eq!(numstat_is_viewable("-\t-\ta.dta\n"), Err("binary"));
        assert_eq!(numstat_is_viewable("20001\t0\ta.csv\n"), Err("tooLarge"));
    }
}
