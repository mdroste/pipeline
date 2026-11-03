//! Listings too long for one read, and new folders that hold a repository, with
//! real Git.
use super::tests::{act, commit_paths, identity, ready, sh};
use super::*;

/// A repository with one commit, and `count` tracked files that changed.
fn changed_repository(count: usize) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    sh(root, &["init"]);
    identity(root);
    fs::create_dir(root.join("outputs")).unwrap();
    for index in 0..count {
        fs::write(root.join(format!("outputs/table_{index:05}.csv")), "a").unwrap();
    }
    sh(root, &["add", "."]);
    sh(root, &["commit", "-m", "Outputs"]);
    for index in 0..count {
        fs::write(root.join(format!("outputs/table_{index:05}.csv")), "b").unwrap();
    }
    temp
}

#[test]
fn more_changes_than_one_read_holds_still_show_the_page() {
    let temp = changed_repository(16_000);
    let root = temp.path();
    let git = Git::open(root).unwrap();
    assert!(
        git.read(&repository::STATUS_ARGS).is_err(),
        "this many rows must be past the limit on a plain read"
    );
    let view = ready(root);
    assert!(view.files_truncated);
    assert_eq!(view.files.len(), 5000);
    let status = repository::inspect(root).expect("still a repository");
    assert_eq!(status.changed, 16_000);
    act(
        root,
        commit_paths(&["outputs/table_00000.csv"], "First table"),
    )
    .unwrap();
    assert!(ready(root).files_truncated);
}

#[test]
fn a_listing_past_its_limit_keeps_only_complete_records() {
    let temp = changed_repository(40);
    let git = Git::open(temp.path()).unwrap();
    let whole = git.read_listing(&repository::STATUS_ARGS).unwrap();
    let cut = git
        .read_up_to(&repository::STATUS_ARGS, whole.len() as u64 / 2)
        .unwrap();
    assert_eq!(cut.last(), Some(&0));
    assert!(cut.len() <= whole.len() / 2);
    assert!(whole.starts_with(&cut));
    assert!(status::parse_changes(&String::from_utf8_lossy(&cut), "")
        .files
        .iter()
        .all(|file| file.path.ends_with(".csv")));
}

#[test]
fn a_plain_read_still_fails_past_its_limit() {
    let temp = tempfile::tempdir().unwrap();
    let mut command = std::process::Command::new("sh");
    command.args(["-c", "head -c 3000000 /dev/zero"]);
    let error = capture_git(command, temp.path(), GitLimits::default()).unwrap_err();
    assert!(error.message.contains("limit"), "{}", error.message);
}

/// A project with a new `paper/` cloned from a repository of its own.
fn nested_project() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    sh(&root, &["init"]);
    identity(&root);
    fs::write(root.join("notes.txt"), "notes").unwrap();
    sh(&root, &["add", "."]);
    sh(&root, &["commit", "-m", "Notes"]);
    let paper = root.join("paper");
    fs::create_dir(&paper).unwrap();
    sh(&paper, &["init"]);
    identity(&paper);
    fs::write(paper.join("main.tex"), "text").unwrap();
    sh(&paper, &["add", "."]);
    sh(&paper, &["commit", "-m", "Draft"]);
    temp
}

#[test]
fn a_new_folder_with_its_own_repository_is_marked_and_refused() {
    let temp = nested_project();
    let root = temp.path().join("project");
    fs::write(root.join("notes.txt"), "more notes").unwrap();
    let view = ready(&root);
    let paper = view.files.iter().find(|f| f.path == "paper").unwrap();
    assert!(paper.is_directory && paper.nested_repository);
    let error = act(&root, commit_paths(&["paper", "notes.txt"], "All"))
        .err()
        .unwrap();
    assert!(error.message.contains("its own Git repository"));
    assert!(!sh(&root, &["log", "-1", "--format=%s"]).contains("All"));
    assert_eq!(sh(&root, &["ls-files", "--stage", "paper"]), "");
    act(&root, commit_paths(&["notes.txt"], "Only notes")).unwrap();
}

#[test]
fn a_repository_deeper_inside_a_new_folder_is_found_too() {
    let temp = nested_project();
    let root = temp.path().join("project");
    fs::create_dir(root.join("work")).unwrap();
    fs::rename(root.join("paper"), root.join("work/paper")).unwrap();
    fs::write(root.join("work/plan.txt"), "plan").unwrap();
    let view = ready(&root);
    let work = view.files.iter().find(|f| f.path == "work").unwrap();
    assert!(work.nested_repository);
    assert!(act(&root, commit_paths(&["work"], "Work")).is_err());
    assert_eq!(sh(&root, &["ls-files", "--stage", "work"]), "");
}

#[test]
fn an_ignored_repository_and_a_plain_new_folder_commit_as_before() {
    let temp = nested_project();
    let root = temp.path().join("project");
    fs::create_dir(root.join("work")).unwrap();
    fs::rename(root.join("paper"), root.join("work/paper")).unwrap();
    fs::write(root.join("work/plan.txt"), "plan").unwrap();
    fs::write(root.join(".gitignore"), "work/paper/\n").unwrap();
    let view = ready(&root);
    let work = view.files.iter().find(|f| f.path == "work").unwrap();
    assert!(!work.nested_repository);
    act(&root, commit_paths(&["work", ".gitignore"], "Work")).unwrap();
    assert_eq!(sh(&root, &["ls-files", "work"]), "work/plan.txt");
}

const OVER_THE_HOST_LIMIT: u64 = 101 * 1024 * 1024;

fn new_folder(root: &Path, name: &str, count: usize) {
    fs::create_dir_all(root.join(name)).unwrap();
    for index in 0..count {
        fs::write(root.join(format!("{name}/row_{index:05}.csv")), "a").unwrap();
    }
}

fn large_file(path: &Path) {
    let file = fs::File::create(path).unwrap();
    file.set_len(OVER_THE_HOST_LIMIT).unwrap();
}

fn new_files_repository() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    sh(temp.path(), &["init"]);
    identity(temp.path());
    temp
}

#[test]
fn a_large_file_past_the_checks_says_the_list_was_cut_short() {
    let temp = new_files_repository();
    let root = temp.path();
    new_folder(root, "data", 5000);
    let view = ready(root);
    assert!(!view.large_files_cut_short, "every file was checked");

    large_file(&root.join("data/zz_panel.dta"));
    let view = ready(root);
    assert!(view.large_files.is_empty());
    assert!(view.large_files_cut_short);
}

#[test]
fn the_size_checks_are_shared_by_every_new_folder() {
    let temp = new_files_repository();
    let root = temp.path();
    new_folder(root, "first", 3000);
    new_folder(root, "second", 3000);
    large_file(&root.join("second/zz_panel.dta"));
    // A row of its own, listed after both folders, still gets its check.
    large_file(&root.join("zz_alone.dta"));
    // So does the check for a repository, past where sizes stopped.
    sh(&root.join("second"), &["init", "--quiet", "nested"]);
    fs::write(root.join("second/nested/a.txt"), "a").unwrap();
    sh(&root.join("second/nested"), &["add", "."]);
    identity(&root.join("second/nested"));
    sh(&root.join("second/nested"), &["commit", "-qm", "Nested"]);

    let view = ready(root);
    let found: Vec<&str> = view.large_files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(found, ["zz_alone.dta"]);
    assert!(view.large_files_cut_short);
    let second = view.files.iter().find(|f| f.path == "second").unwrap();
    assert!(second.nested_repository);
}
