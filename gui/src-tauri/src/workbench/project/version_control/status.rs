//! Reads what the Version control page lists: changed files, branches, and any
//! unfinished merge.
use super::*;

/// GitHub refuses files over 100 MiB; other hosts are at least as strict in
/// practice for a repository of research data.
const LARGE_FILE: u64 = 100 * 1024 * 1024;
const MAX_LARGE_FILES: usize = 50;
/// Bounds the walk through new folders on each refresh.
const MAX_SIZED_FILES: usize = 5000;
const MAX_BRANCHES: &str = "--count=300";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    /// Where a renamed file came from, when that is inside the project folder.
    pub old_path: Option<String>,
    /// `modified`, `added`, `deleted`, `renamed`, `copied`, `untracked`, or
    /// `conflicted`.
    pub status: String,
    pub staged: bool,
    pub unstaged: bool,
    /// A new folder Git reports as one entry.
    pub is_directory: bool,
    /// A submodule, whose contents belong to its own repository.
    pub is_submodule: bool,
    /// How the two sides of a conflict differ, such as `both modified`.
    pub conflict: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFile {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub current: bool,
    pub upstream: Option<String>,
    /// The remote branch to start from when this name has no local branch yet.
    pub remote_ref: Option<String>,
}

pub(super) struct Changes {
    pub(super) files: Vec<ChangedFile>,
    pub(super) truncated: bool,
}

pub(super) fn changes(git: &Git, prefix: &str) -> WorkbenchResult<Changes> {
    let output = git.read(&[
        "status",
        "--porcelain=v2",
        "-z",
        "--untracked-files=normal",
        "--",
        ".",
    ])?;
    Ok(parse_changes(&String::from_utf8_lossy(&output), prefix))
}

fn conflict_label(xy: &str) -> &'static str {
    match xy {
        "DD" => "deleted on both sides",
        "AU" => "added here",
        "UD" => "deleted on the other side",
        "UA" => "added on the other side",
        "DU" => "deleted here",
        "AA" => "added on both sides",
        _ => "changed on both sides",
    }
}

/// Reduces a repository-relative path to one inside the project folder, and
/// drops Pipeline's own scratch folders, which are never project content.
fn project_path(path: &str, prefix: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    let inside = if prefix.is_empty() {
        trimmed
    } else if trimmed == prefix.trim_end_matches('/') {
        "."
    } else {
        trimmed.strip_prefix(prefix)?
    };
    let first = inside.split('/').next().unwrap_or(inside);
    (!files::reserved_component(first)).then(|| inside.to_string())
}

/// Parses `git status --porcelain=v2 -z`.
pub(super) fn parse_changes(output: &str, prefix: &str) -> Changes {
    let mut files = Vec::new();
    let mut total = 0;
    let mut entries = output.split('\0');
    while let Some(entry) = entries.next() {
        let tracked = |fields: usize| entry.splitn(fields, ' ').last().unwrap_or(entry);
        let xy = entry.get(2..4).unwrap_or("..");
        let (path, old_path, status) = match entry.as_bytes().first() {
            Some(b'1') => (tracked(9), None, tracked_status(xy)),
            Some(b'2') => {
                let status = if xy.contains('C') {
                    "copied"
                } else {
                    "renamed"
                };
                (tracked(10), entries.next(), status)
            }
            Some(b'u') => (tracked(11), None, "conflicted"),
            Some(b'?') => (tracked(2), None, "untracked"),
            _ => continue,
        };
        let Some(project) = project_path(path, prefix) else {
            continue;
        };
        total += 1;
        // A conflict always stays listed: it must be seen before any commit.
        if files.len() >= MAX_FILES && status != "conflicted" {
            continue;
        }
        let bytes = xy.as_bytes();
        let conflicted = status == "conflicted";
        files.push(ChangedFile {
            is_directory: status == "untracked" && path.ends_with('/'),
            // Only a tracked row has this field. An untracked row is `? <path>`,
            // so its later words belong to the file's name.
            is_submodule: status != "untracked"
                && entry
                    .split(' ')
                    .nth(2)
                    .is_some_and(|sub| sub.starts_with('S')),
            old_path: old_path.and_then(|old| project_path(old, prefix)),
            path: project,
            status: status.to_string(),
            staged: !conflicted && status != "untracked" && bytes[0] != b'.',
            unstaged: !conflicted && (status == "untracked" || bytes[1] != b'.'),
            conflict: conflicted.then(|| conflict_label(xy).to_string()),
        });
    }
    // Stable: Git's order is kept within conflicts and within the rest.
    files.sort_by_key(|file| file.status != "conflicted");
    Changes {
        truncated: total > files.len(),
        files,
    }
}

fn tracked_status(xy: &str) -> &'static str {
    if xy.starts_with('A') {
        "added"
    } else if xy.contains('D') {
        "deleted"
    } else {
        "modified"
    }
}

/// New files too large for a host, including those inside new folders.
pub(super) fn large_files(git: &Git, files: &[ChangedFile]) -> Vec<LargeFile> {
    let mut sized = 0;
    let mut found = Vec::new();
    let mut check = |path: &str, found: &mut Vec<LargeFile>| {
        sized += 1;
        if let Ok(meta) = fs::symlink_metadata(git.root.join(path)) {
            if meta.is_file() && meta.len() > LARGE_FILE && found.len() < MAX_LARGE_FILES {
                found.push(LargeFile {
                    path: path.to_string(),
                    size: meta.len(),
                });
            }
        }
    };
    for file in files {
        if matches!(file.status.as_str(), "deleted" | "conflicted") {
            continue;
        }
        if !file.is_directory {
            check(&file.path, &mut found);
            continue;
        }
        let Ok(listed) = git.read(&[
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            &file.path,
        ]) else {
            continue;
        };
        for path in String::from_utf8_lossy(&listed)
            .split('\0')
            .filter(|path| !path.is_empty())
            .take(MAX_SIZED_FILES)
        {
            check(path, &mut found);
        }
    }
    found
}

/// Local branches, plus remote branches with no local branch of the same name.
pub(super) fn branches(git: &Git) -> Vec<BranchInfo> {
    git.read_text(&[
        "for-each-ref",
        MAX_BRANCHES,
        "--sort=-committerdate",
        "--format=%(refname)%1f%(symref)%1f%(upstream:short)%1f%(HEAD)",
        "refs/heads",
        "refs/remotes",
    ])
    .map(|output| parse_branches(&output))
    .unwrap_or_default()
}

fn parse_branches(output: &str) -> Vec<BranchInfo> {
    let mut local = Vec::new();
    let mut remote = Vec::new();
    for line in output.lines() {
        let fields: Vec<&str> = line.split('\u{1f}').collect();
        let [refname, symref, upstream, head] = fields[..] else {
            continue;
        };
        if let Some(name) = refname.strip_prefix("refs/heads/") {
            local.push(BranchInfo {
                name: name.to_string(),
                current: head == "*",
                upstream: (!upstream.is_empty()).then(|| upstream.to_string()),
                remote_ref: None,
            });
        } else if let Some(short) = refname.strip_prefix("refs/remotes/") {
            if symref.is_empty() {
                remote.push(short.to_string());
            }
        }
    }
    let mut seen: BTreeSet<String> = local.iter().map(|branch| branch.name.clone()).collect();
    for short in remote {
        if let Some((_, name)) = short.split_once('/') {
            if seen.insert(name.to_string()) {
                local.push(BranchInfo {
                    name: name.to_string(),
                    current: false,
                    upstream: None,
                    remote_ref: Some(short),
                });
            }
        }
    }
    local
}

/// The unfinished operation, with the commit message Git prepared for it.
pub(super) fn operation(git: &Git) -> (Option<String>, Option<String>) {
    let Ok(dir) = git.read_text(&["rev-parse", "--absolute-git-dir"]) else {
        return (None, None);
    };
    operation_in(Path::new(dir.trim()))
}

/// `operation`, for a caller that already knows the Git directory.
pub(super) fn operation_in(dir: &Path) -> (Option<String>, Option<String>) {
    let operation = if dir.join("rebase-merge").exists() || dir.join("rebase-apply").exists() {
        "rebase"
    } else if dir.join("MERGE_HEAD").exists() {
        "merge"
    } else if dir.join("CHERRY_PICK_HEAD").exists() {
        "cherry-pick"
    } else if dir.join("REVERT_HEAD").exists() {
        "revert"
    } else {
        return (None, None);
    };
    let message = fs::read_to_string(dir.join("MERGE_MSG"))
        .ok()
        .and_then(|text| {
            let line = text.lines().next()?.trim().to_string();
            (!line.is_empty()).then_some(line)
        });
    (Some(operation.to_string()), message)
}

/// The author Git would record, when the researcher has set one up.
pub(super) fn identity(git: &Git) -> Option<GitIdentity> {
    let value = |key: &str| {
        let value = git.read_text(&["config", "--get", key]).ok()?;
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    };
    Some(GitIdentity {
        name: value("user.name")?,
        email: value("user.email")?,
    })
}

/// Commits on HEAD that no remote-tracking branch has, newest first.
pub(super) fn unpushed(git: &Git) -> BTreeSet<String> {
    git.read_text(&["rev-list", "--max-count=200", "HEAD", "--not", "--remotes"])
        .map(|output| output.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Unmerged files anywhere in the repository, not only the project folder.
pub(super) fn unmerged_count(git: &Git) -> usize {
    git.read(&["diff", "--name-only", "--diff-filter=U", "-z"])
        .map(|output| {
            output
                .split(|byte| *byte == 0)
                .filter(|p| !p.is_empty())
                .count()
        })
        .unwrap_or(0)
}

/// Undoing a commit is offered only when nobody else can already have it. A
/// merge is left out because recommitting would lose its second parent.
pub(super) fn latest_commit_is_local(git: &Git, status: &RepositoryStatus) -> bool {
    let Some(head) = &status.head else {
        return false;
    };
    let has = |name: &str| {
        git.read(&["rev-parse", "--quiet", "--verify", name])
            .is_ok()
    };
    has("HEAD^") && !has("HEAD^2") && unpushed(git).contains(head)
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn status_rows_keep_odd_names_and_list_conflicts_first() {
        let changes = parse_changes(
            "# branch.oid abc\0# branch.head main\0\
             1 .M N... 100644 100644 100644 aaa bbb paper/my main.tex\0\
             1 M. N... 100644 100644 100644 aaa bbb staged.do\0\
             1 AM N... 000000 100644 100644 000 bbb new.do\0\
             1 D. N... 100644 000000 000000 aaa 000 gone.do\0\
             2 R. N... 100644 100644 100644 aaa bbb R100 new name.do\0old name.do\0\
             u UD N... 100644 100644 000000 100644 aaa bbb ccc table.tex\0\
             ? data/\0\
             ? .pipeline-tasks/\0\
             ? notes.txt\0",
            "",
        );
        let rows: Vec<(&str, &str, bool, bool)> = changes
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.status.as_str(), f.staged, f.unstaged))
            .collect();
        assert_eq!(
            rows,
            [
                ("table.tex", "conflicted", false, false),
                ("paper/my main.tex", "modified", false, true),
                ("staged.do", "modified", true, false),
                ("new.do", "added", true, true),
                ("gone.do", "deleted", true, false),
                ("new name.do", "renamed", true, false),
                ("data", "untracked", false, true),
                ("notes.txt", "untracked", false, true),
            ]
        );
        assert_eq!(
            changes.files[0].conflict.as_deref(),
            Some("deleted on the other side")
        );
        assert_eq!(changes.files[5].old_path.as_deref(), Some("old name.do"));
        assert!(changes.files[6].is_directory);
        assert!(!changes.truncated);
    }

    #[test]
    fn paths_become_relative_to_a_project_inside_a_repository() {
        let changes = parse_changes(
            "1 .M N... 100644 100644 100644 aaa bbb papers/one/main.tex\0\
             ? papers/one/\0",
            "papers/one/",
        );
        assert_eq!(changes.files[0].path, "main.tex");
        assert_eq!(changes.files[1].path, ".");
    }

    #[test]
    fn branches_merge_remote_names_and_skip_the_remote_head() {
        let sep = '\u{1f}';
        let branches = parse_branches(&format!(
            "refs/heads/main{sep}{sep}origin/main{sep}*\n\
             refs/heads/draft{sep}{sep}{sep} \n\
             refs/remotes/origin/HEAD{sep}refs/remotes/origin/main{sep}{sep} \n\
             refs/remotes/origin/main{sep}{sep}{sep} \n\
             refs/remotes/origin/referee/round-2{sep}{sep}{sep} \n"
        ));
        let names: Vec<&str> = branches.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["main", "draft", "referee/round-2"]);
        assert!(branches[0].current);
        assert_eq!(branches[0].upstream.as_deref(), Some("origin/main"));
        assert_eq!(
            branches[2].remote_ref.as_deref(),
            Some("origin/referee/round-2")
        );
    }

    #[test]
    fn a_conflict_past_the_row_limit_still_comes_first() {
        let mut output = String::new();
        for index in 0..MAX_FILES + 10 {
            output.push_str(&format!("? file{index}.txt\0"));
        }
        output.push_str("u UU N... 100644 100644 100644 100644 a b c zz.tex\0");
        let changes = parse_changes(&output, "");
        assert_eq!(changes.files[0].path, "zz.tex");
        assert!(changes.truncated);
    }

    #[test]
    fn submodules_and_a_deleted_new_file_are_labelled() {
        let changes = parse_changes(
            "1 .M SCM. 160000 160000 160000 aaa bbb vendor/tool\0\
             1 AD N... 000000 100644 000000 000 bbb gone.do\0",
            "",
        );
        assert!(changes.files[0].is_submodule);
        assert_eq!(changes.files[1].status, "added");
    }

    #[test]
    fn an_untracked_file_whose_name_has_a_word_starting_with_s_is_not_a_submodule() {
        let changes = parse_changes(
            "? Final Summary.docx\0? Table S1.tex\0? papers/one/Main Sample.dta\0",
            "",
        );
        assert_eq!(changes.files.len(), 3);
        assert!(changes.files.iter().all(|file| !file.is_submodule));
    }
}
