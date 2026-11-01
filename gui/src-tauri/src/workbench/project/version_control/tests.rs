//! Real Git against a bare local remote: every action the page offers.
use super::actions::{run, Action, ConflictChoice};
use super::*;
use std::cell::RefCell;
use std::process::Command;
use tempfile::TempDir;

pub(super) fn sh(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
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
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub(super) fn identity(dir: &Path) {
    sh(dir, &["config", "user.name", "Ada Author"]);
    sh(dir, &["config", "user.email", "ada@example.org"]);
    sh(dir, &["config", "commit.gpgsign", "false"]);
}

struct Clones {
    _temp: TempDir,
    mine: PathBuf,
    theirs: PathBuf,
}

/// A bare remote with one commit, cloned twice. Local-path remotes bypass the
/// address check, which applies only to addresses the page enters.
fn clones() -> Clones {
    let temp = tempfile::tempdir().unwrap();
    let remote = temp.path().join("remote.git");
    fs::create_dir(&remote).unwrap();
    sh(&remote, &["init", "--bare", "--initial-branch=main"]);
    let url = remote.to_str().unwrap();
    sh(temp.path(), &["clone", url, "mine"]);
    sh(temp.path(), &["clone", url, "theirs"]);
    let (mine, theirs) = (temp.path().join("mine"), temp.path().join("theirs"));
    identity(&mine);
    identity(&theirs);
    fs::write(mine.join("paper.tex"), "line one\nline two\n").unwrap();
    sh(&mine, &["add", "."]);
    sh(&mine, &["commit", "-m", "First draft"]);
    sh(&mine, &["push", "-u", "origin", "HEAD:main"]);
    sh(&theirs, &["pull", "origin", "main"]);
    sh(
        &theirs,
        &["branch", "--set-upstream-to=origin/main", "main"],
    );
    Clones {
        _temp: temp,
        mine,
        theirs,
    }
}

fn act(root: &Path, action: Action) -> WorkbenchResult<actions_outcome::Seen> {
    actions_outcome::run_action(root, action, &|path| {
        fs::remove_file(path)
            .or_else(|_| fs::remove_dir_all(path))
            .map_err(err)
    })
}

mod actions_outcome {
    use super::*;
    /// What an action left behind, read fresh from the repository.
    pub(super) struct Seen {
        pub(super) notice: Option<String>,
    }
    pub(super) fn run_action(
        root: &Path,
        action: Action,
        trash: &dyn Fn(&Path) -> WorkbenchResult<()>,
    ) -> WorkbenchResult<Seen> {
        let git = Git::open(root).expect("git is installed");
        let outcome = run(&git, action, &AtomicBool::new(false), trash)?;
        Ok(Seen {
            notice: outcome.notice,
        })
    }
}

fn ready(root: &Path) -> RepositoryView {
    match state(Some(root)).unwrap() {
        VersionControl::Ready(view) => *view,
        other => panic!("not ready: {other:?}"),
    }
}

fn commit_paths(paths: &[&str], summary: &str) -> Action {
    Action::Commit {
        paths: paths.iter().map(|p| p.to_string()).collect(),
        summary: summary.to_string(),
        description: None,
    }
}

fn file_paths(view: &RepositoryView) -> Vec<&str> {
    view.files.iter().map(|file| file.path.as_str()).collect()
}

#[test]
fn a_folder_starts_tracking_and_keeps_scratch_out_of_the_first_commit() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    assert!(matches!(
        state(Some(&root)).unwrap(),
        VersionControl::NotRepository { problem: None }
    ));
    assert!(matches!(state(None).unwrap(), VersionControl::NoFolder));
    fs::write(root.join("paper.tex"), "draft").unwrap();
    fs::create_dir_all(root.join(".pipeline-tasks/copy")).unwrap();
    fs::write(root.join(".pipeline-tasks/copy/paper.tex"), "scratch").unwrap();

    act(&root, Action::Initialize).unwrap();
    assert!(fs::read_to_string(root.join(".gitignore"))
        .unwrap()
        .contains(".pipeline-tasks/"));
    let view = ready(&root);
    assert_eq!(view.status.branch.as_deref(), Some("main"));
    assert_eq!(file_paths(&view), [".gitignore", "paper.tex"]);

    act(
        &root,
        Action::SetIdentity {
            name: "Ada Author".into(),
            email: "ada@example.org".into(),
        },
    )
    .unwrap();
    sh(&root, &["config", "commit.gpgsign", "false"]);
    assert!(ready(&root).identity.is_some());
    act(
        &root,
        commit_paths(&[".gitignore", "paper.tex"], "Start tracking"),
    )
    .unwrap();

    let view = ready(&root);
    assert!(view.files.is_empty());
    assert_eq!(view.status.commits[0].subject, "Start tracking");
    assert!(
        !view.can_undo_commit,
        "a first commit has nothing to go back to"
    );
    assert_eq!(
        sh(&root, &["ls-files"]),
        ".gitignore\npaper.tex",
        "scratch folders stay out"
    );
}

#[test]
fn a_commit_takes_only_the_selected_files_and_leaves_other_staged_work() {
    let c = clones();
    fs::write(c.mine.join("paper.tex"), "changed\n").unwrap();
    fs::write(c.mine.join("notes.txt"), "new\n").unwrap();
    fs::write(c.mine.join("staged.txt"), "someone staged this\n").unwrap();
    sh(&c.mine, &["add", "staged.txt"]);
    fs::create_dir(c.mine.join("data")).unwrap();
    fs::write(c.mine.join("data/a b.csv"), "1,2\n").unwrap();

    act(
        &c.mine,
        Action::Commit {
            paths: vec!["paper.tex".into(), "data".into()],
            summary: "Revise the draft".into(),
            description: Some("Second paragraph.".into()),
        },
    )
    .unwrap();

    assert_eq!(
        sh(&c.mine, &["show", "--name-only", "--format=", "HEAD"]),
        "data/a b.csv\npaper.tex"
    );
    assert_eq!(
        sh(&c.mine, &["log", "-1", "--format=%B"]),
        "Revise the draft\n\nSecond paragraph."
    );
    let view = ready(&c.mine);
    assert_eq!(file_paths(&view), ["staged.txt", "notes.txt"]);
    assert!(
        view.files[0].staged,
        "the unrelated staged file stays staged"
    );
}

#[test]
fn a_rename_commits_as_one_change() {
    let c = clones();
    sh(&c.mine, &["mv", "paper.tex", "article.tex"]);
    let view = ready(&c.mine);
    assert_eq!(view.files[0].status, "renamed");
    assert_eq!(view.files[0].old_path.as_deref(), Some("paper.tex"));
    act(&c.mine, commit_paths(&["article.tex"], "Rename")).unwrap();
    assert!(ready(&c.mine).files.is_empty());
    assert_eq!(sh(&c.mine, &["ls-files"]), "article.tex");
}

#[test]
fn hooks_run_for_a_commit_but_not_for_reads() {
    let c = clones();
    let hook = c.mine.join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\necho 'tests must pass' >&2\nexit 1\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(c.mine.join("paper.tex"), "changed\n").unwrap();
    ready(&c.mine);
    let error = act(&c.mine, commit_paths(&["paper.tex"], "Blocked"))
        .err()
        .unwrap();
    assert!(
        error.message.contains("tests must pass"),
        "{}",
        error.message
    );
}

#[test]
fn discard_keeps_a_copy_and_restores_each_kind_of_change() {
    let c = clones();
    sh(&c.mine, &["mv", "paper.tex", "renamed.tex"]);
    sh(&c.mine, &["commit", "-m", "Rename"]);
    fs::write(c.mine.join("renamed.tex"), "edited\n").unwrap();
    fs::write(c.mine.join("new.txt"), "untracked\n").unwrap();
    fs::write(c.mine.join("added.txt"), "staged new\n").unwrap();
    sh(&c.mine, &["add", "added.txt"]);

    let kept = RefCell::new(Vec::new());
    let trash = |path: &Path| {
        kept.borrow_mut()
            .push(fs::read_to_string(path).map_err(err)?);
        fs::remove_file(path).map_err(err)
    };
    let seen = actions_outcome::run_action(
        &c.mine,
        Action::Discard {
            paths: vec!["renamed.tex".into(), "new.txt".into(), "added.txt".into()],
        },
        &trash,
    )
    .unwrap();

    assert_eq!(seen.notice.as_deref(), Some("Moved to the Trash."));
    assert!(ready(&c.mine).files.is_empty());
    assert_eq!(
        fs::read_to_string(c.mine.join("renamed.tex")).unwrap(),
        "line one\nline two\n"
    );
    assert!(!c.mine.join("new.txt").exists());
    let mut kept = kept.into_inner();
    kept.sort();
    assert_eq!(kept, ["edited\n", "staged new\n", "untracked\n"]);
}

#[test]
fn discard_and_ignore_refuse_paths_outside_the_list() {
    let c = clones();
    let refused = |path: &str| {
        act(
            &c.mine,
            Action::Discard {
                paths: vec![path.to_string()],
            },
        )
        .is_err()
    };
    assert!(refused("../escape"));
    assert!(refused(".git/config"));
    assert!(refused("/etc/hosts"));
    assert!(refused("paper.tex"), "an unchanged file is not discardable");
    assert!(c.mine.join("paper.tex").exists());
    assert!(act(
        &c.mine,
        Action::Ignore {
            path: "paper.tex".into()
        }
    )
    .is_err());
}

#[test]
fn ignoring_a_new_file_removes_it_from_the_list() {
    let c = clones();
    fs::create_dir(c.mine.join("data")).unwrap();
    fs::write(c.mine.join("data/raw[1].dta"), "bytes").unwrap();
    fs::write(c.mine.join("data/keep.dta"), "bytes").unwrap();
    sh(&c.mine, &["add", "data/raw[1].dta"]);
    act(
        &c.mine,
        Action::Ignore {
            path: "data/raw[1].dta".into(),
        },
    )
    .unwrap();
    assert_eq!(
        file_paths(&ready(&c.mine)),
        [".gitignore", "data"],
        "the folder row now holds only keep.dta"
    );
    assert_eq!(
        sh(
            &c.mine,
            &["status", "--porcelain", "--ignored", "--", "data"]
        ),
        "?? data/\n!! data/raw[1].dta"
    );
}

#[test]
fn large_new_files_are_reported_even_inside_a_new_folder() {
    let c = clones();
    fs::create_dir(c.mine.join("data")).unwrap();
    let big = fs::File::create(c.mine.join("data/panel.dta")).unwrap();
    big.set_len(101 * 1024 * 1024).unwrap();
    let view = ready(&c.mine);
    assert_eq!(view.large_files[0].path, "data/panel.dta");
}

#[test]
fn push_publishes_a_branch_and_pull_brings_commits_back() {
    let c = clones();
    act(
        &c.mine,
        Action::CreateBranch {
            name: "referee/round-2".into(),
        },
    )
    .unwrap();
    assert!(act(
        &c.mine,
        Action::CreateBranch {
            name: "bad name..".into()
        }
    )
    .is_err());
    fs::write(c.mine.join("reply.md"), "Dear editor\n").unwrap();
    act(&c.mine, commit_paths(&["reply.md"], "Reply to the referee")).unwrap();
    assert!(ready(&c.mine).can_undo_commit);
    act(&c.mine, Action::Push).unwrap();
    let view = ready(&c.mine);
    assert_eq!(
        view.status.upstream.as_deref(),
        Some("origin/referee/round-2")
    );
    assert!(!view.can_undo_commit, "a pushed commit is shared");

    act(&c.theirs, Action::Fetch).unwrap();
    let view = ready(&c.theirs);
    let remote = view
        .branches
        .iter()
        .find(|b| b.name == "referee/round-2")
        .unwrap();
    assert_eq!(remote.remote_ref.as_deref(), Some("origin/referee/round-2"));
    act(
        &c.theirs,
        Action::SwitchBranch {
            name: "referee/round-2".into(),
        },
    )
    .unwrap();
    assert!(c.theirs.join("reply.md").exists());

    fs::write(c.theirs.join("reply.md"), "Dear editor, thanks\n").unwrap();
    act(&c.theirs, commit_paths(&["reply.md"], "Thank the editor")).unwrap();
    act(&c.theirs, Action::Push).unwrap();
    act(&c.mine, Action::Fetch).unwrap();
    assert_eq!(ready(&c.mine).status.behind, 1);
    act(&c.mine, Action::Pull).unwrap();
    assert_eq!(ready(&c.mine).status.behind, 0);
    assert_eq!(
        fs::read_to_string(c.mine.join("reply.md")).unwrap(),
        "Dear editor, thanks\n"
    );
}

#[test]
fn a_rejected_push_asks_for_a_pull_and_undo_returns_the_message() {
    let c = clones();
    fs::write(c.theirs.join("theirs.txt"), "x").unwrap();
    act(&c.theirs, commit_paths(&["theirs.txt"], "Theirs")).unwrap();
    act(&c.theirs, Action::Push).unwrap();
    fs::write(c.mine.join("mine.txt"), "y").unwrap();
    act(&c.mine, commit_paths(&["mine.txt"], "Mine")).unwrap();
    let error = act(&c.mine, Action::Push).err().unwrap();
    assert!(
        error.message.starts_with("The remote has commits"),
        "{}",
        error.message
    );

    let git = Git::open(&c.mine).unwrap();
    let outcome = run(&git, Action::UndoCommit, &AtomicBool::new(false), &|_| {
        Ok(())
    })
    .unwrap();
    assert_eq!(outcome.undone_message.as_deref(), Some("Mine"));
    assert!(ready(&c.mine).files.iter().any(|f| f.path == "mine.txt"));
}

#[test]
fn a_rejected_first_publish_names_only_what_the_page_offers() {
    let c = clones();
    // The remote's branch has a commit of its own, as a README would make.
    sh(&c.theirs, &["switch", "-c", "draft"]);
    fs::write(c.theirs.join("README.md"), "x").unwrap();
    sh(&c.theirs, &["add", "."]);
    sh(&c.theirs, &["commit", "-m", "Add a README"]);
    sh(&c.theirs, &["push", "origin", "draft"]);
    sh(&c.mine, &["switch", "-c", "draft", "--no-track", "main"]);
    fs::write(c.mine.join("mine.txt"), "y").unwrap();
    act(&c.mine, commit_paths(&["mine.txt"], "Mine")).unwrap();
    assert_eq!(ready(&c.mine).status.upstream, None);

    let error = act(&c.mine, Action::Push).err().unwrap();
    assert!(
        error
            .message
            .contains("already has its own commits on draft"),
        "{}",
        error.message
    );
    assert!(error.message.contains("Change remote"), "{}", error.message);
    assert!(!error.message.contains("Pull"), "{}", error.message);
}

#[test]
fn a_remote_with_an_address_pipeline_cannot_reduce_keeps_its_sync_buttons() {
    let c = clones();
    let status = ready(&c.mine).status;
    let remote = status
        .remote
        .expect("the local-path origin is still a remote");
    assert_eq!(remote.name, "origin");
    assert_eq!(
        (
            remote.host.as_str(),
            remote.owner.as_str(),
            remote.repo.as_str()
        ),
        ("", "", "")
    );
    assert_eq!(remote.web_url, None);
    let json = serde_json::to_string(&remote).unwrap();
    assert!(!json.contains("remote.git"), "{json}");
    assert_eq!(status.upstream.as_deref(), Some("origin/main"));
}

/// A stand-in for `/usr/bin/git` on a Mac with no developer tools.
fn missing_tools_git(root: &Path) -> Git {
    use std::os::unix::fs::PermissionsExt;
    let shim = root.join("git-shim");
    fs::write(
        &shim,
        "#!/bin/sh\necho 'xcode-select: note: No developer tools were found, requesting install.' >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
    Git {
        binary: shim,
        root: root.to_path_buf(),
        guard: Mutex::new(None),
    }
}

#[test]
fn a_mac_with_no_developer_tools_reports_git_as_missing() {
    let temp = tempfile::tempdir().unwrap();
    let state = view(&missing_tools_git(temp.path())).unwrap();
    assert!(matches!(state, VersionControl::GitMissing), "{state:?}");
}

fn conflict(c: &Clones) {
    fs::write(c.theirs.join("paper.tex"), "their line\nline two\n").unwrap();
    fs::write(c.theirs.join("gone.txt"), "x").unwrap();
    act(
        &c.theirs,
        commit_paths(&["paper.tex", "gone.txt"], "Their edit"),
    )
    .unwrap();
    act(&c.theirs, Action::Push).unwrap();
    fs::write(c.mine.join("paper.tex"), "my line\nline two\n").unwrap();
    act(&c.mine, commit_paths(&["paper.tex"], "My edit")).unwrap();
    let seen = act(&c.mine, Action::Pull).unwrap();
    assert!(seen.notice.unwrap().contains("both sides"));
}

#[test]
fn a_conflicted_pull_resolves_by_choosing_a_side_and_commits_the_merge() {
    let c = clones();
    conflict(&c);
    let view = ready(&c.mine);
    assert_eq!(view.operation.as_deref(), Some("merge"));
    assert!(view.operation_message.unwrap().starts_with("Merge"));
    assert_eq!(view.files[0].path, "paper.tex");
    assert_eq!(view.files[0].status, "conflicted");
    assert_eq!(
        view.files[0].conflict.as_deref(),
        Some("changed on both sides")
    );
    assert!(act(&c.mine, commit_paths(&["paper.tex"], "Too soon")).is_err());
    assert!(act(
        &c.mine,
        Action::Resolve {
            path: "paper.tex".into(),
            choice: ConflictChoice::Edited
        }
    )
    .err()
    .unwrap()
    .message
    .contains("conflict markers"));

    act(
        &c.mine,
        Action::Resolve {
            path: "paper.tex".into(),
            choice: ConflictChoice::Theirs,
        },
    )
    .unwrap();
    assert!(fs::read_to_string(c.mine.join("paper.tex"))
        .unwrap()
        .starts_with("their line"));
    act(&c.mine, commit_paths(&[], "Merge their edit")).unwrap();
    let view = ready(&c.mine);
    assert_eq!(view.operation, None);
    assert!(view.files.is_empty());
    assert!(c.mine.join("gone.txt").exists());
}

#[test]
fn hand_edited_conflicts_and_an_abort_both_finish_cleanly() {
    let c = clones();
    conflict(&c);
    fs::write(c.mine.join("paper.tex"), "both lines\nline two\n").unwrap();
    act(
        &c.mine,
        Action::Resolve {
            path: "paper.tex".into(),
            choice: ConflictChoice::Edited,
        },
    )
    .unwrap();
    assert!(ready(&c.mine)
        .files
        .iter()
        .all(|f| f.status != "conflicted"));
    act(&c.mine, Action::AbortOperation).unwrap();
    let view = ready(&c.mine);
    assert_eq!(view.operation, None);
    assert_eq!(
        fs::read_to_string(c.mine.join("paper.tex")).unwrap(),
        "my line\nline two\n"
    );
}

#[test]
fn a_delete_conflict_resolves_to_the_side_that_removed_the_file() {
    let c = clones();
    sh(&c.theirs, &["rm", "paper.tex"]);
    fs::write(c.theirs.join("other.txt"), "x").unwrap();
    sh(&c.theirs, &["add", "other.txt"]);
    sh(&c.theirs, &["commit", "-m", "Remove the draft"]);
    act(&c.theirs, Action::Push).unwrap();
    fs::write(c.mine.join("paper.tex"), "kept\n").unwrap();
    act(&c.mine, commit_paths(&["paper.tex"], "Edit")).unwrap();
    act(&c.mine, Action::Pull).unwrap();
    assert_eq!(
        ready(&c.mine).files[0].conflict.as_deref(),
        Some("deleted on the other side")
    );
    act(
        &c.mine,
        Action::Resolve {
            path: "paper.tex".into(),
            choice: ConflictChoice::Theirs,
        },
    )
    .unwrap();
    assert!(!c.mine.join("paper.tex").exists());
    act(&c.mine, commit_paths(&[], "Merge")).unwrap();
    assert_eq!(ready(&c.mine).operation, None);
}

#[test]
fn choosing_a_side_keeps_a_hand_merge_in_the_trash() {
    for (choice, kept) in [
        (ConflictChoice::Ours, "my line"),
        (ConflictChoice::Theirs, "their line"),
    ] {
        let c = clones();
        conflict(&c);
        // The markers are gone, so only the Trash can save this work.
        fs::write(c.mine.join("paper.tex"), "merged by hand\n").unwrap();
        let trashed = RefCell::new(Vec::new());
        let seen = actions_outcome::run_action(
            &c.mine,
            Action::Resolve {
                path: "paper.tex".into(),
                choice,
            },
            &|path| {
                trashed
                    .borrow_mut()
                    .push(fs::read_to_string(path).map_err(err)?);
                fs::remove_file(path).map_err(err)
            },
        )
        .unwrap();
        assert_eq!(*trashed.borrow(), ["merged by hand\n"]);
        assert_eq!(
            seen.notice.as_deref(),
            Some("Your previous copy is in the Trash.")
        );
        let text = fs::read_to_string(c.mine.join("paper.tex")).unwrap();
        assert!(text.contains(kept), "{text}");
        assert!(ready(&c.mine).files.iter().all(|f| f.conflict.is_none()));
    }
}

#[test]
fn a_side_that_deleted_the_file_still_keeps_the_hand_merge() {
    let c = clones();
    sh(&c.theirs, &["rm", "paper.tex"]);
    fs::write(c.theirs.join("other.txt"), "x").unwrap();
    sh(&c.theirs, &["add", "other.txt"]);
    sh(&c.theirs, &["commit", "-m", "Remove the draft"]);
    act(&c.theirs, Action::Push).unwrap();
    fs::write(c.mine.join("paper.tex"), "kept\n").unwrap();
    act(&c.mine, commit_paths(&["paper.tex"], "Edit")).unwrap();
    act(&c.mine, Action::Pull).unwrap();
    let trashed = RefCell::new(Vec::new());
    actions_outcome::run_action(
        &c.mine,
        Action::Resolve {
            path: "paper.tex".into(),
            choice: ConflictChoice::Theirs,
        },
        &|path| {
            trashed
                .borrow_mut()
                .push(fs::read_to_string(path).map_err(err)?);
            fs::remove_file(path).map_err(err)
        },
    )
    .unwrap();
    assert_eq!(*trashed.borrow(), ["kept\n"]);
    assert!(!c.mine.join("paper.tex").exists());
}

#[test]
fn a_failed_trash_leaves_the_conflicted_file_alone() {
    let c = clones();
    conflict(&c);
    fs::write(c.mine.join("paper.tex"), "merged by hand\n").unwrap();
    let refused = actions_outcome::run_action(
        &c.mine,
        Action::Resolve {
            path: "paper.tex".into(),
            choice: ConflictChoice::Theirs,
        },
        &|_| Err(WorkbenchError::invalid("No Trash here")),
    );
    assert!(refused.is_err());
    assert_eq!(
        fs::read_to_string(c.mine.join("paper.tex")).unwrap(),
        "merged by hand\n"
    );
    assert_eq!(
        ready(&c.mine).files[0].conflict.as_deref(),
        Some("changed on both sides")
    );
}

#[test]
fn switching_refuses_to_overwrite_uncommitted_work() {
    let c = clones();
    act(
        &c.mine,
        Action::CreateBranch {
            name: "other".into(),
        },
    )
    .unwrap();
    fs::write(c.mine.join("paper.tex"), "on other\n").unwrap();
    act(&c.mine, commit_paths(&["paper.tex"], "Other")).unwrap();
    fs::write(c.mine.join("paper.tex"), "uncommitted\n").unwrap();
    let error = act(
        &c.mine,
        Action::SwitchBranch {
            name: "main".into(),
        },
    )
    .err()
    .unwrap();
    assert!(error.message.contains("overwritten"), "{}", error.message);
    assert_eq!(
        fs::read_to_string(c.mine.join("paper.tex")).unwrap(),
        "uncommitted\n"
    );
}

#[test]
fn a_project_inside_a_repository_sees_only_its_own_folder_with_local_paths() {
    let c = clones();
    let project = c.mine.join("papers/one");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("main.tex"), "draft\n").unwrap();
    fs::write(c.mine.join("outside.txt"), "x\n").unwrap();
    act(&project, commit_paths(&["."], "Add the paper")).unwrap();
    let view = ready(&project);
    assert_eq!(view.folder_prefix, "papers/one/");
    assert!(view.files.is_empty(), "outside.txt is not in this project");
    fs::write(project.join("main.tex"), "edited\n").unwrap();
    let view = ready(&project);
    assert_eq!(file_paths(&view), ["main.tex"]);
    let patch = diff(
        &project,
        DiffRequest::Working {
            path: "main.tex".into(),
            old_path: None,
        },
    )
    .unwrap();
    assert!(patch.text.contains("+edited"), "{patch:?}");
    let page = history(&project, 0).unwrap();
    assert_eq!(page.commits.len(), 1, "only commits touching the folder");
    let detail = commit_detail(&project, &page.commits[0].sha).unwrap();
    assert_eq!(detail.files[0].path, "main.tex");
    let shown = diff(
        &project,
        DiffRequest::Commit {
            sha: page.commits[0].sha.clone(),
            path: "main.tex".into(),
            old_path: None,
        },
    )
    .unwrap();
    assert!(shown.text.contains("+draft"), "{shown:?}");
}

#[test]
fn diffs_label_new_binary_folder_and_oversized_files() {
    let c = clones();
    let kind = |path: &str| {
        diff(
            &c.mine,
            DiffRequest::Working {
                path: path.into(),
                old_path: None,
            },
        )
        .unwrap()
    };
    fs::write(c.mine.join("new.txt"), "hello\n").unwrap();
    assert_eq!(kind("new.txt").kind, "new");
    assert_eq!(kind("new.txt").text, "hello\n");
    fs::write(c.mine.join("data.dta"), [0u8, 1, 2, 3]).unwrap();
    assert_eq!(kind("data.dta").kind, "binary");
    fs::create_dir(c.mine.join("folder")).unwrap();
    fs::write(c.mine.join("folder/a.txt"), "x").unwrap();
    assert_eq!(kind("folder").kind, "folder");
    fs::write(c.mine.join("paper.tex"), "x\n".repeat(30_000)).unwrap();
    assert_eq!(kind("paper.tex").kind, "tooLarge");
}

#[test]
fn commit_detail_lists_renames_and_a_first_commit() {
    let c = clones();
    let first = sh(&c.mine, &["rev-parse", "HEAD"]);
    let detail = commit_detail(&c.mine, &first).unwrap();
    assert_eq!(detail.message, "First draft");
    assert_eq!(detail.files[0].status, "added");
    assert!(commit_detail(&c.mine, "HEAD").is_err());
    assert!(commit_detail(&c.mine, "--output=/tmp/x").is_err());
}

#[test]
fn a_running_action_blocks_a_second_one_and_can_be_cancelled() {
    let temp = tempfile::tempdir().unwrap();
    let guard = ActionGuard::claim(temp.path()).unwrap();
    assert!(ActionGuard::claim(temp.path()).is_err());
    assert!(!guard.cancel.load(Ordering::Relaxed));
    cancel(temp.path());
    assert!(guard.cancel.load(Ordering::Relaxed));
    drop(guard);
    assert!(ActionGuard::claim(temp.path()).is_ok());
}

#[test]
fn the_page_and_backend_agree_on_their_json() {
    let c = clones();
    fs::write(c.mine.join("paper.tex"), "changed\n").unwrap();
    let state = serde_json::to_value(super::state(Some(&c.mine)).unwrap()).unwrap();
    assert_eq!(state["state"], "ready");
    assert_eq!(state["status"]["branch"], "main");
    assert_eq!(state["status"]["upstream"], "origin/main");
    assert_eq!(state["canUndoCommit"], false);
    assert_eq!(state["folderPrefix"], "");
    assert_eq!(state["identity"]["name"], "Ada Author");
    let row = &state["files"][0];
    assert_eq!(row["path"], "paper.tex");
    assert_eq!(row["status"], "modified");
    assert_eq!(row["oldPath"], Value::Null);
    assert_eq!(row["isDirectory"], false);
    assert_eq!(state["branches"][0]["remoteRef"], Value::Null);

    assert_eq!(
        serde_json::to_value(VersionControl::NoFolder).unwrap(),
        json!({ "state": "noFolder" })
    );
    assert_eq!(
        serde_json::to_value(VersionControl::GitMissing).unwrap(),
        json!({ "state": "gitMissing" })
    );
    assert_eq!(
        serde_json::to_value(VersionControl::NotRepository { problem: None }).unwrap(),
        json!({ "state": "notRepository", "problem": null })
    );

    for action in [
        json!({ "kind": "initialize" }),
        json!({ "kind": "setRemote", "url": "x" }),
        json!({ "kind": "setIdentity", "name": "a", "email": "b" }),
        json!({ "kind": "commit", "paths": ["a"], "summary": "s", "description": null }),
        json!({ "kind": "discard", "paths": ["a"] }),
        json!({ "kind": "ignore", "path": "a" }),
        json!({ "kind": "undoCommit" }),
        json!({ "kind": "switchBranch", "name": "a" }),
        json!({ "kind": "createBranch", "name": "a" }),
        json!({ "kind": "fetch" }),
        json!({ "kind": "pull" }),
        json!({ "kind": "push" }),
        json!({ "kind": "resolve", "path": "a", "choice": "theirs" }),
        json!({ "kind": "abortOperation" }),
    ] {
        serde_json::from_value::<Action>(action.clone())
            .unwrap_or_else(|error| panic!("{action}: {error}"));
    }
    for request in [
        json!({ "kind": "working", "path": "a", "oldPath": null }),
        json!({ "kind": "commit", "sha": "b", "path": "a", "oldPath": "c" }),
    ] {
        serde_json::from_value::<DiffRequest>(request.clone())
            .unwrap_or_else(|error| panic!("{request}: {error}"));
    }
}

#[test]
fn a_merge_commit_leaves_unrelated_new_files_out() {
    let c = clones();
    conflict(&c);
    fs::write(c.mine.join("private.csv"), "not for the merge").unwrap();
    act(
        &c.mine,
        Action::Resolve {
            path: "paper.tex".into(),
            choice: ConflictChoice::Ours,
        },
    )
    .unwrap();
    act(&c.mine, commit_paths(&[], "Merge")).unwrap();
    assert_eq!(ready(&c.mine).operation, None);
    assert!(!sh(&c.mine, &["ls-files"]).contains("private.csv"));
    assert!(ready(&c.mine).files.iter().any(|f| f.path == "private.csv"));
}

#[test]
fn choosing_a_side_refuses_a_file_that_is_no_longer_conflicted() {
    let c = clones();
    let refused = act(
        &c.mine,
        Action::Resolve {
            path: "paper.tex".into(),
            choice: ConflictChoice::Theirs,
        },
    );
    assert!(refused
        .err()
        .unwrap()
        .message
        .contains("no longer in conflict"));
    assert!(c.mine.join("paper.tex").exists());
}

#[test]
fn a_conflict_outside_the_project_folder_blocks_the_commit() {
    let c = clones();
    let project = c.mine.join("papers");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("note.txt"), "x").unwrap();
    sh(&c.mine, &["add", "."]);
    sh(&c.mine, &["commit", "-m", "Add a folder"]);
    conflict(&c);
    let view = ready(&project);
    assert_eq!(view.operation.as_deref(), Some("merge"));
    assert_eq!(view.outside_conflicts, 1);
    assert!(view.files.is_empty());
    assert!(act(&project, commit_paths(&[], "Too soon")).is_err());
}

#[test]
fn discard_refuses_staged_work_it_could_not_keep_and_guards_a_renames_old_name() {
    let c = clones();
    fs::write(c.mine.join("paper.tex"), "staged\n").unwrap();
    sh(&c.mine, &["add", "paper.tex"]);
    fs::write(c.mine.join("paper.tex"), "unstaged\n").unwrap();
    let error = act(
        &c.mine,
        Action::Discard {
            paths: vec!["paper.tex".into()],
        },
    )
    .err()
    .unwrap();
    assert!(
        error.message.contains("staged and unstaged"),
        "{}",
        error.message
    );
    assert_eq!(sh(&c.mine, &["show", ":paper.tex"]), "staged");
    sh(&c.mine, &["add", "paper.tex"]);
    sh(&c.mine, &["commit", "-m", "Edit"]);

    sh(&c.mine, &["mv", "paper.tex", "article.tex"]);
    fs::write(c.mine.join("paper.tex"), "new work\n").unwrap();
    let kept = RefCell::new(Vec::new());
    let trash = |path: &Path| {
        kept.borrow_mut()
            .push(fs::read_to_string(path).map_err(err)?);
        fs::remove_file(path).map_err(err)
    };
    actions_outcome::run_action(
        &c.mine,
        Action::Discard {
            paths: vec!["article.tex".into()],
        },
        &trash,
    )
    .unwrap();
    assert!(kept.into_inner().contains(&"new work\n".to_string()));
    assert_eq!(
        fs::read_to_string(c.mine.join("paper.tex")).unwrap(),
        "unstaged\n"
    );
}

#[test]
fn a_new_file_deleted_after_staging_does_not_fail_the_commit() {
    let c = clones();
    fs::write(c.mine.join("gone.do"), "x").unwrap();
    sh(&c.mine, &["add", "gone.do"]);
    fs::remove_file(c.mine.join("gone.do")).unwrap();
    fs::write(c.mine.join("paper.tex"), "changed\n").unwrap();
    act(&c.mine, commit_paths(&["gone.do", "paper.tex"], "Edit")).unwrap();
    assert_eq!(sh(&c.mine, &["ls-files"]), "paper.tex");
}

#[test]
fn a_local_merge_commit_is_not_offered_for_undo() {
    let c = clones();
    conflict(&c);
    act(
        &c.mine,
        Action::Resolve {
            path: "paper.tex".into(),
            choice: ConflictChoice::Ours,
        },
    )
    .unwrap();
    act(&c.mine, commit_paths(&[], "Merge")).unwrap();
    assert!(!ready(&c.mine).can_undo_commit);
}

#[test]
fn appending_to_gitignore_keeps_bytes_that_are_not_utf8() {
    let c = clones();
    fs::write(c.mine.join(".gitignore"), b"# caf\xe9").unwrap();
    fs::write(c.mine.join("new.csv"), "x").unwrap();
    act(
        &c.mine,
        Action::Ignore {
            path: "new.csv".into(),
        },
    )
    .unwrap();
    assert_eq!(
        fs::read(c.mine.join(".gitignore")).unwrap(),
        b"# caf\xe9\n/new.csv\n"
    );
}

#[test]
fn nothing_to_commit_reads_as_advice_not_a_blank_error() {
    let c = clones();
    assert_eq!(
        errors::explain("Git task setup failed: On branch main\nnothing to commit, working tree clean", None),
        "Git found nothing to commit in those files. Changes inside a submodule are committed in the submodule."
    );
    assert!(c.mine.exists());
}

#[cfg(unix)]
#[test]
fn a_failed_command_keeps_its_message_whatever_bytes_it_printed() {
    let temp = tempfile::tempdir().unwrap();
    let run = |script: &str| {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        capture_git(command, temp.path(), GitLimits::default())
            .unwrap_err()
            .message
    };
    let latin1 = run(r"printf 'hook refused caf\351 draft\n' >&2; exit 1");
    assert!(latin1.contains("hook refused caf"), "{latin1}");
    assert!(latin1.contains("draft"), "{latin1}");
    // A two-byte character cut in half by the limit on what is read.
    let split = run(
        r"i=0; while [ $i -lt 8191 ]; do printf x; i=$((i+1)); done >&2; printf '\303\251' >&2; exit 1",
    );
    assert!(split.contains("xxxx"), "{split}");
}
