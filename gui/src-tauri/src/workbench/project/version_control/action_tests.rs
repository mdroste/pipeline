//! Which remote an address change reaches, Pipeline's own folders in a commit, and
//! what Discard reports, with real Git.
use super::actions::{Action, ConflictChoice};
use super::tests::{act, actions_outcome, clones, commit_paths, file_paths, identity, ready, sh};
use super::*;
use std::cell::Cell;

const ADDRESS: &str = "https://git.overleaf.com/64f0c0ffee";

fn set_remote(root: &Path) {
    act(
        root,
        Action::SetRemote {
            url: ADDRESS.into(),
        },
    )
    .unwrap();
}

#[test]
fn change_remote_reaches_the_remote_the_branch_tracks() {
    let c = clones();
    let origin = sh(&c.mine, &["remote", "get-url", "origin"]);
    sh(c.mine.parent().unwrap(), &["init", "--bare", "second.git"]);
    let second = c.mine.parent().unwrap().join("second.git");
    sh(
        &c.mine,
        &["remote", "add", "overleaf", second.to_str().unwrap()],
    );
    sh(&c.mine, &["push", "--set-upstream", "overleaf", "main"]);
    assert_eq!(ready(&c.mine).status.remote.unwrap().name, "overleaf");

    set_remote(&c.mine);

    let shown = ready(&c.mine).status.remote.unwrap();
    assert_eq!(
        (shown.name.as_str(), shown.host.as_str()),
        ("overleaf", "git.overleaf.com")
    );
    assert_eq!(sh(&c.mine, &["remote", "get-url", "overleaf"]), ADDRESS);
    assert_eq!(sh(&c.mine, &["remote", "get-url", "origin"]), origin);
}

#[test]
fn change_remote_adds_origin_only_when_there_is_no_remote() {
    let c = clones();
    sh(&c.mine, &["remote", "rename", "origin", "overleaf"]);
    set_remote(&c.mine);
    assert_eq!(sh(&c.mine, &["remote"]), "overleaf");
    assert_eq!(sh(&c.mine, &["remote", "get-url", "overleaf"]), ADDRESS);

    sh(&c.mine, &["remote", "remove", "overleaf"]);
    set_remote(&c.mine);
    assert_eq!(sh(&c.mine, &["remote"]), "origin");
    assert_eq!(sh(&c.mine, &["remote", "get-url", "origin"]), ADDRESS);
}

/// An earlier build wrote this list, and stopped looking once it was there.
fn exclude_as_earlier_builds_wrote_it(root: &Path) {
    fs::write(
        root.join(".git/info/exclude"),
        ".pipeline-tasks/\n.pipeline-apply-*\n",
    )
    .unwrap();
}

fn plant_generated_input(folder: &Path) {
    let generated = folder.join(".pipeline-workbench/generated/a");
    fs::create_dir_all(&generated).unwrap();
    fs::write(generated.join("plot.py"), "print('plot')\n").unwrap();
}

#[test]
fn generated_inputs_stay_out_of_changes_and_out_of_a_new_parent_folder() {
    let c = clones();
    exclude_as_earlier_builds_wrote_it(&c.mine);
    plant_generated_input(&c.mine);
    assert!(ready(&c.mine).files.is_empty());

    // Git reports the whole new folder as one row, so only its own exclude list
    // can keep a nested generated folder out of the commit.
    plant_generated_input(&c.mine.join("papers/one"));
    fs::write(c.mine.join("papers/one/paper.tex"), "draft\n").unwrap();
    assert_eq!(file_paths(&ready(&c.mine)), ["papers"]);
    act(&c.mine, commit_paths(&["papers"], "New paper")).unwrap();

    assert_eq!(
        sh(&c.mine, &["show", "--name-only", "--format=", "HEAD"]),
        "papers/one/paper.tex"
    );
    let exclude = fs::read_to_string(c.mine.join(".git/info/exclude")).unwrap();
    assert_eq!(
        exclude,
        ".pipeline-tasks/\n.pipeline-apply-*\n.pipeline-workbench/\n"
    );
    assert!(act(&c.mine, commit_paths(&["papers"], "Nothing left")).is_err());
    assert_eq!(
        fs::read_to_string(c.mine.join(".git/info/exclude")).unwrap(),
        exclude,
        "the list is added to once"
    );
}

#[test]
fn a_new_repository_ignores_generated_inputs_from_the_start() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    plant_generated_input(&root);
    act(&root, Action::Initialize).unwrap();
    identity(&root);
    assert_eq!(file_paths(&ready(&root)), [".gitignore"]);
    assert!(fs::read_to_string(root.join(".gitignore"))
        .unwrap()
        .contains(".pipeline-workbench/\n"));
    sh(&root, &["check-ignore", ".pipeline-workbench/generated"]);
}

#[test]
fn discard_mentions_the_trash_only_when_a_copy_went_there() {
    let c = clones();
    fs::write(c.mine.join("second.tex"), "second\n").unwrap();
    sh(&c.mine, &["add", "second.tex"]);
    sh(&c.mine, &["commit", "-m", "Second"]);
    let moved = Cell::new(0);
    let trash = |path: &Path| {
        moved.set(moved.get() + 1);
        fs::remove_file(path).map_err(err)
    };

    fs::remove_file(c.mine.join("paper.tex")).unwrap();
    let restored = actions_outcome::run_action(
        &c.mine,
        Action::Discard {
            paths: vec!["paper.tex".into()],
        },
        &trash,
    )
    .unwrap();
    assert_eq!(restored.notice, None);
    assert_eq!(moved.get(), 0);
    assert!(c.mine.join("paper.tex").exists());

    fs::remove_file(c.mine.join("paper.tex")).unwrap();
    fs::write(c.mine.join("second.tex"), "edited\n").unwrap();
    let mixed = actions_outcome::run_action(
        &c.mine,
        Action::Discard {
            paths: vec!["paper.tex".into(), "second.tex".into()],
        },
        &trash,
    )
    .unwrap();
    assert_eq!(mixed.notice.as_deref(), Some("Moved to the Trash."));
    assert_eq!(moved.get(), 1);
}

/// An earlier build's commit: figure outputs land in the program's own folder.
fn commit_generated_output(c: &super::tests::Clones) -> &'static str {
    const FIGURE: &str = ".pipeline-workbench/generated/a/figure.svg";
    plant_generated_input(&c.mine);
    fs::write(c.mine.join(FIGURE), "<svg>first</svg>\n").unwrap();
    sh(&c.mine, &["add", "--force", ".pipeline-workbench"]);
    sh(&c.mine, &["commit", "-m", "Figure"]);
    sh(&c.mine, &["push", "origin", "HEAD:main"]);
    FIGURE
}

/// Something untracked in the folder, with the list a commit writes. Git then
/// treats the folder as ignored, though it holds tracked files.
fn plant_new_input(root: &Path) {
    fs::write(root.join(".git/info/exclude"), ".pipeline-workbench/\n").unwrap();
    fs::write(root.join(".pipeline-workbench/generated/a/new.json"), "{}").unwrap();
}

#[test]
fn a_tracked_generated_file_shows_and_commits_like_any_other() {
    let c = clones();
    let figure = commit_generated_output(&c);
    fs::write(c.mine.join(figure), "<svg>rerun</svg>\n").unwrap();
    // A new input beside it is still Pipeline's own.
    plant_new_input(&c.mine);

    assert_eq!(file_paths(&ready(&c.mine)), [figure]);
    act(&c.mine, commit_paths(&[figure], "Rerun the figure")).unwrap();
    assert!(ready(&c.mine).files.is_empty());
    assert_eq!(
        sh(&c.mine, &["show", "--name-only", "--format=", "HEAD"]),
        figure
    );

    fs::write(c.mine.join(figure), "<svg>again</svg>\n").unwrap();
    let diff = inspect::diff(
        &c.mine,
        DiffRequest::Working {
            path: figure.into(),
            old_path: None,
        },
    )
    .unwrap();
    assert!(diff.text.contains("again"), "{}", diff.text);
    act(
        &c.mine,
        Action::Discard {
            paths: vec![figure.into()],
        },
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(c.mine.join(figure)).unwrap(),
        "<svg>rerun</svg>\n"
    );
}

#[test]
fn a_tracked_generated_change_that_stops_a_pull_can_be_seen() {
    let c = clones();
    let figure = commit_generated_output(&c);
    sh(&c.theirs, &["pull", "origin", "main"]);
    fs::write(c.theirs.join(figure), "<svg>theirs</svg>\n").unwrap();
    sh(&c.theirs, &["commit", "-am", "Their figure"]);
    sh(&c.theirs, &["push", "origin", "HEAD:main"]);

    fs::write(c.mine.join(figure), "<svg>mine</svg>\n").unwrap();
    sh(
        &c.mine,
        &["branch", "--set-upstream-to=origin/main", "main"],
    );
    assert!(act(&c.mine, Action::Pull).is_err());
    assert_eq!(file_paths(&ready(&c.mine)), [figure]);
}

#[test]
fn a_conflict_in_a_tracked_generated_file_can_be_resolved() {
    let c = clones();
    let figure = commit_generated_output(&c);
    sh(&c.theirs, &["pull", "origin", "main"]);
    fs::write(c.theirs.join(figure), "<svg>theirs</svg>\n").unwrap();
    sh(&c.theirs, &["commit", "-am", "Their figure"]);
    sh(&c.theirs, &["push", "origin", "HEAD:main"]);

    fs::write(c.mine.join(figure), "<svg>mine</svg>\n").unwrap();
    sh(&c.mine, &["commit", "-am", "My figure"]);
    sh(
        &c.mine,
        &["branch", "--set-upstream-to=origin/main", "main"],
    );
    act(&c.mine, Action::Pull).unwrap();
    plant_new_input(&c.mine);

    let view = ready(&c.mine);
    assert_eq!(view.operation.as_deref(), Some("merge"));
    assert_eq!(view.outside_conflicts, 0);
    assert_eq!(file_paths(&view), [figure]);
    assert_eq!(view.files[0].status, "conflicted");

    act(
        &c.mine,
        Action::Resolve {
            path: figure.into(),
            choice: ConflictChoice::Theirs,
        },
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(c.mine.join(figure)).unwrap(),
        "<svg>theirs</svg>\n"
    );
    act(&c.mine, commit_paths(&[], "Merge the figure")).unwrap();
    assert_eq!(ready(&c.mine).operation, None);
}

#[test]
fn a_hand_fix_of_a_tracked_generated_file_can_be_marked_resolved() {
    let c = clones();
    let figure = commit_generated_output(&c);
    sh(&c.theirs, &["pull", "origin", "main"]);
    fs::write(c.theirs.join(figure), "<svg>theirs</svg>\n").unwrap();
    sh(&c.theirs, &["commit", "-am", "Their figure"]);
    sh(&c.theirs, &["push", "origin", "HEAD:main"]);
    fs::write(c.mine.join(figure), "<svg>mine</svg>\n").unwrap();
    sh(&c.mine, &["commit", "-am", "My figure"]);
    sh(
        &c.mine,
        &["branch", "--set-upstream-to=origin/main", "main"],
    );
    act(&c.mine, Action::Pull).unwrap();
    plant_new_input(&c.mine);

    fs::write(c.mine.join(figure), "<svg>both</svg>\n").unwrap();
    act(
        &c.mine,
        Action::Resolve {
            path: figure.into(),
            choice: ConflictChoice::Edited,
        },
    )
    .unwrap();
    assert!(ready(&c.mine)
        .files
        .iter()
        .all(|f| f.status != "conflicted"));
}

#[test]
fn pipelines_other_folders_stay_out_of_reach_of_the_page() {
    assert!(checked_path(".pipeline-tasks/copy/paper.tex").is_err());
    assert!(checked_path(".pipeline-apply-claim").is_err());
    assert!(checked_path(".git/config").is_err());
    assert!(checked_path(".pipeline-workbench/generated/a/figure.svg").is_ok());
}
