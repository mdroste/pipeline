//! Real Git in folders whose own config names a program that leaves a marker.
//! Each test first shows Git starting that program on its own, then runs the
//! reads a project page makes with no click.
use super::actions::{run, Action};
use super::guard::ReadGuard;
use super::tests::{identity, sh};
use super::*;
use std::os::unix::fs::PermissionsExt as _;
use std::process::Command;
use std::sync::atomic::AtomicU64;
use std::time::SystemTime;
use tempfile::TempDir;

struct Folder {
    temp: TempDir,
    root: PathBuf,
    marker: PathBuf,
}

/// A time no index entry has, so Git has to look inside the file.
fn stale(path: &Path) {
    static CLOCK: AtomicU64 = AtomicU64::new(1_000_000);
    let seconds = CLOCK.fetch_add(1, Ordering::Relaxed);
    let file = fs::File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
        .unwrap();
}

fn repository(root: &Path) {
    fs::create_dir_all(root).unwrap();
    sh(root, &["init"]);
    identity(root);
}

fn folder(attributes: &[u8]) -> Folder {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("paper");
    repository(&root);
    fs::write(root.join("paper.tex"), "draft\n").unwrap();
    fs::write(root.join(".gitattributes"), attributes).unwrap();
    sh(&root, &["add", "."]);
    sh(&root, &["commit", "-m", "First draft"]);
    Folder {
        marker: temp.path().join("program-ran"),
        temp,
        root,
    }
}

impl Folder {
    fn config(&self, key: &str, value: &str) {
        sh(&self.root, &["config", key, value]);
    }

    /// A shell command that leaves the marker and passes its input through.
    fn command(&self) -> String {
        format!("touch '{}'; cat", self.marker.display())
    }

    /// A program Git starts directly, without a shell.
    fn program(&self) -> String {
        let path = self.temp.path().join("program");
        let text = format!("#!/bin/sh\ntouch '{}'\n", self.marker.display());
        fs::write(&path, text).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn append_config(&self, text: &[u8]) {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(self.root.join(".git/config"))
            .unwrap();
        file.write_all(text).unwrap();
    }

    fn clear_marker(&self) {
        if self.marker.exists() {
            fs::remove_file(&self.marker).unwrap();
        }
    }

    /// Git without the guard starts the program, so a clean read means something.
    fn git_alone_starts_it(&self, args: &[&str]) {
        stale(&self.root.join("paper.tex"));
        let _ = Command::new("git")
            .arg("--no-optional-locks")
            .args(args)
            .current_dir(&self.root)
            .output()
            .unwrap();
        assert!(self.marker.exists(), "git {args:?} started nothing");
        self.clear_marker();
    }

    /// Every read a project page makes with no click. The file's text has changed
    /// at the same length, so a right answer needs Git to compare contents.
    fn read_everything(&self) -> Box<RepositoryView> {
        let paper = self.root.join("paper.tex");
        fs::write(&paper, "DRAFT\n").unwrap();
        stale(&paper);
        let view = ready(&self.root);
        let paths: Vec<&str> = view.files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(paths, ["paper.tex"]);
        let status = repository::inspect(&self.root).expect("a repository");
        assert_eq!(status.head, view.status.head);
        assert_eq!(status.changed, 1);
        let sha = view.status.head.clone().unwrap();
        assert_eq!(status.commits[0].sha, sha);
        assert_eq!(history(&self.root, 0).unwrap().commits[0].sha, sha);
        assert_eq!(commit_detail(&self.root, &sha).unwrap().sha, sha);
        let working = DiffRequest::Working {
            path: "paper.tex".into(),
            old_path: None,
        };
        assert!(diff(&self.root, working).unwrap().text.contains("+DRAFT"));
        let commit = DiffRequest::Commit {
            sha,
            path: "paper.tex".into(),
            old_path: None,
        };
        diff(&self.root, commit).unwrap();
        // The login-shell read, which isolated task copies and the fetch share.
        let output = tasks::git(&self.root, &["status", "--porcelain=v1", "-z"]).unwrap();
        assert!(String::from_utf8_lossy(&output).contains("paper.tex"));
        assert!(!self.marker.exists(), "a read started the folder's program");
        view
    }
}

fn ready(root: &Path) -> Box<RepositoryView> {
    match state(Some(root)).unwrap() {
        VersionControl::Ready(view) => view,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_clean_filter_stays_silent_and_the_status_stays_right() {
    let f = folder(b"paper.tex filter=evil\n");
    f.config("filter.evil.clean", &f.command());
    f.git_alone_starts_it(&["status"]);
    f.read_everything();
}

#[test]
fn a_process_filter_stays_silent() {
    let f = folder(b"paper.tex filter=evil\n");
    f.config("filter.evil.process", &f.command());
    f.git_alone_starts_it(&["status"]);
    f.read_everything();
}

#[test]
fn a_required_filter_neither_runs_nor_fails_the_read() {
    let f = folder(b"paper.tex filter=evil\n");
    f.config("filter.evil.clean", &f.command());
    f.config("filter.evil.required", "true");
    f.git_alone_starts_it(&["status"]);
    f.read_everything();
}

#[test]
fn a_required_filter_with_no_command_does_not_fail_the_read() {
    let f = folder(b"paper.tex filter=evil\n");
    f.config("filter.evil.required", "true");
    f.read_everything();
}

#[test]
fn a_signature_check_stays_silent() {
    let f = folder(b"");
    let head = sh(&f.root, &["rev-parse", "HEAD"]);
    fs::write(f.root.join("paper.tex"), "signed\n").unwrap();
    sh(&f.root, &["add", "."]);
    let tree = sh(&f.root, &["write-tree"]);
    let body = format!(
        "tree {tree}\nparent {head}\n\
         author Ada Author <ada@example.org> 1700000000 +0000\n\
         committer Ada Author <ada@example.org> 1700000000 +0000\n\
         gpgsig -----BEGIN PGP SIGNATURE-----\n \n abc\n -----END PGP SIGNATURE-----\n\n\
         Signed\n"
    );
    let object = f.temp.path().join("commit");
    fs::write(&object, body).unwrap();
    let path = object.to_str().unwrap();
    let signed = sh(&f.root, &["hash-object", "-t", "commit", "-w", path]);
    sh(&f.root, &["update-ref", "HEAD", &signed]);
    f.config("gpg.program", &f.program());
    f.config("log.showSignature", "true");
    f.git_alone_starts_it(&["log", "-1"]);
    f.config("gpg.openpgp.program", &f.program());
    let view = f.read_everything();
    assert_eq!(view.status.commits[0].subject, "Signed");
}

#[test]
fn a_filter_from_an_included_file_stays_silent() {
    let f = folder(b"paper.tex filter=evil\n");
    let included = f.temp.path().join("shared.cfg");
    let text = format!("[filter \"evil\"]\n\tclean = {}\n", f.command());
    fs::write(&included, text).unwrap();
    f.config("include.path", included.to_str().unwrap());
    f.git_alone_starts_it(&["status"]);
    f.read_everything();
}

#[test]
fn a_filter_conditionally_included_stays_silent() {
    let f = folder(b"paper.tex filter=evil\n");
    let included = f.temp.path().join("shared.cfg");
    let text = format!("[filter \"evil\"]\n\tclean = {}\n", f.command());
    fs::write(&included, text).unwrap();
    f.config("includeIf.gitdir:/.path", included.to_str().unwrap());
    f.git_alone_starts_it(&["status"]);
    f.read_everything();
}

#[test]
fn a_filter_in_the_worktree_scope_stays_silent() {
    let f = folder(b"paper.tex filter=evil\n");
    f.config("extensions.worktreeConfig", "true");
    sh(
        &f.root,
        &["config", "--worktree", "filter.evil.clean", &f.command()],
    );
    f.git_alone_starts_it(&["status"]);
    f.read_everything();
}

#[test]
fn a_filter_named_with_an_equals_sign_stays_silent() {
    let f = folder(b"paper.tex filter=a=b\n");
    f.config("filter.a=b.clean", &f.command());
    f.git_alone_starts_it(&["status"]);
    f.read_everything();
}

#[test]
fn a_filter_with_an_empty_name_stays_silent() {
    let f = folder(b"paper.tex filter=\n");
    f.append_config(format!("[filter \"\"]\n\tclean = {}\n", f.command()).as_bytes());
    f.git_alone_starts_it(&["status"]);
    f.read_everything();
}

#[test]
fn a_filter_whose_name_cannot_be_spelled_refuses_every_read() {
    let f = folder(b"paper.tex filter=caf\xe9\n");
    let mut section = b"[filter \"caf\xe9\"]\n\tclean = ".to_vec();
    section.extend_from_slice(f.command().as_bytes());
    section.push(b'\n');
    f.append_config(&section);
    f.git_alone_starts_it(&["status"]);
    match state(Some(&f.root)).unwrap() {
        VersionControl::NotRepository { problem } => {
            assert!(problem.unwrap().contains("Git settings"));
        }
        other => panic!("{other:?}"),
    }
    assert!(repository::inspect(&f.root).is_none());
    assert!(!f.marker.exists());
}

#[test]
fn diff_drivers_stay_silent_and_the_diff_stays_right() {
    let f = folder(b"paper.tex diff=evil\n");
    f.config("diff.evil.textconv", &f.command());
    fs::write(f.root.join("paper.tex"), "DRAFT\n").unwrap();
    f.git_alone_starts_it(&["diff", "HEAD"]);
    f.config("diff.external", &f.program());
    f.git_alone_starts_it(&["diff", "HEAD"]);
    f.config("diff.evil.command", &f.program());
    f.read_everything();
}

#[test]
fn a_config_change_after_an_action_is_seen_by_the_next_read() {
    let f = folder(b"paper.tex filter=evil\n");
    let git = Git::open(&f.root).unwrap();
    git.read(&["status"]).unwrap();
    let value = f.command();
    git.act(
        &["config", "filter.evil.clean", &value],
        LOCAL_TIMEOUT,
        None,
    )
    .unwrap();
    stale(&f.root.join("paper.tex"));
    git.read(&["status"]).unwrap();
    assert!(!f.marker.exists());
}

#[test]
fn a_commit_the_researcher_chooses_keeps_the_repositorys_filters() {
    let f = folder(b"paper.tex filter=evil\n");
    f.config("filter.evil.clean", &f.command());
    fs::write(f.root.join("paper.tex"), "Second draft\n").unwrap();
    let git = Git::open(&f.root).unwrap();
    let cancel = AtomicBool::new(false);
    let action = Action::Commit {
        paths: vec!["paper.tex".into()],
        summary: "Second draft".into(),
        description: None,
    };
    run(&git, action, &cancel, &|_| Ok(())).map(|_| ()).unwrap();
    assert!(f.marker.exists());
}

/// A submodule at `sub`, whose own config names a filter for `notes.txt`.
fn with_submodule() -> Folder {
    let f = folder(b"");
    let source = f.temp.path().join("source");
    repository(&source);
    fs::write(source.join("notes.txt"), "notes\n").unwrap();
    fs::write(source.join(".gitattributes"), "notes.txt filter=evil\n").unwrap();
    sh(&source, &["add", "."]);
    sh(&source, &["commit", "-m", "Notes"]);
    sh(
        &f.root,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "--quiet",
            "add",
            source.to_str().unwrap(),
            "sub",
        ],
    );
    sh(&f.root, &["commit", "-m", "Add notes"]);
    let sub = f.root.join("sub");
    identity(&sub);
    sh(&sub, &["config", "filter.evil.clean", &f.command()]);
    stale(&sub.join("notes.txt"));
    f
}

#[test]
fn a_submodules_own_config_stays_silent() {
    let f = with_submodule();
    f.git_alone_starts_it(&["status"]);
    // A tracked file can ask for submodules to be looked into.
    sh(
        &f.root,
        &[
            "config",
            "-f",
            ".gitmodules",
            "submodule.sub.ignore",
            "none",
        ],
    );
    sh(&f.root, &["commit", "-am", "Watch the submodule"]);
    f.clear_marker();
    stale(&f.root.join("sub/notes.txt"));
    f.git_alone_starts_it(&["status"]);
    f.read_everything();
}

#[test]
fn a_submodule_on_another_commit_still_shows_and_its_diff_stays_silent() {
    let f = with_submodule();
    let sub = f.root.join("sub");
    fs::write(sub.join("notes.txt"), "more notes\n").unwrap();
    sh(
        &sub,
        &["-c", "filter.evil.clean=", "commit", "-am", "More notes"],
    );
    sh(&sub, &["config", "diff.evil.textconv", &f.command()]);
    let attributes = sh(&sub, &["rev-parse", "--git-path", "info/attributes"]);
    let attributes = sub.join(attributes);
    fs::create_dir_all(attributes.parent().unwrap()).unwrap();
    fs::write(&attributes, "notes.txt diff=evil\n").unwrap();
    f.config("diff.submodule", "diff");
    f.git_alone_starts_it(&["diff", "HEAD", "--", "sub"]);
    let row = ready(&f.root)
        .files
        .into_iter()
        .find(|file| file.path == "sub")
        .expect("the submodule is listed");
    assert!(row.is_submodule);
    let request = DiffRequest::Working {
        path: "sub".into(),
        old_path: None,
    };
    assert_eq!(diff(&f.root, request).unwrap().kind, "diff");
    assert!(!f.marker.exists());
}

#[test]
fn a_missing_object_in_a_partial_clone_starts_no_fetch() {
    let f = folder(b"");
    let remote = f.temp.path().join("remote.git");
    fs::create_dir(&remote).unwrap();
    sh(&remote, &["init", "--bare"]);
    fs::write(f.root.join("paper.tex"), "second\n").unwrap();
    sh(&f.root, &["commit", "-am", "Second draft"]);
    let old = sh(&f.root, &["rev-parse", "HEAD~1:paper.tex"]);
    let head = sh(&f.root, &["rev-parse", "HEAD"]);
    fs::remove_file(f.root.join(".git/objects").join(&old[..2]).join(&old[2..])).unwrap();
    sh(
        &f.root,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    f.config("core.repositoryformatversion", "1");
    f.config("extensions.partialClone", "origin");
    f.config("remote.origin.promisor", "true");
    let value = format!("touch '{}'; git-upload-pack", f.marker.display());
    f.config("remote.origin.uploadpack", &value);
    f.git_alone_starts_it(&["show", "--format=", &head, "--", "paper.tex"]);
    ready(&f.root);
    let commit = DiffRequest::Commit {
        sha: head,
        path: "paper.tex".into(),
        old_path: None,
    };
    assert!(diff(&f.root, commit).is_err());
    assert!(!f.marker.exists(), "a read reached the remote");
}

/// The Version control page's Check for updates.
fn check_for_updates(f: &Folder) -> WorkbenchResult<()> {
    let git = Git::open(&f.root).unwrap();
    let cancel = AtomicBool::new(false);
    run(&git, Action::Fetch, &cancel, &|_| Ok(())).map(|_| ())
}

/// Both fetch routes, which must agree: Check GitHub on the overview, and Check
/// for updates on the Version control page.
fn check_starts_nothing(f: &Folder) -> WorkbenchResult<repository::RepositoryStatus> {
    f.git_alone_starts_it(&["fetch", "origin"]);
    let result = repository::fetch(&f.root);
    assert!(!f.marker.exists(), "checking the remote started a program");
    assert_eq!(check_for_updates(f).is_ok(), result.is_ok());
    assert!(!f.marker.exists(), "Check for updates started a program");
    result
}

/// A bare remote holding this branch, connected as `origin`.
fn with_remote(f: &Folder) -> PathBuf {
    let remote = f.temp.path().join("remote.git");
    fs::create_dir(&remote).unwrap();
    sh(&remote, &["init", "--bare"]);
    let url = remote.to_str().unwrap();
    sh(&f.root, &["push", url, "HEAD:refs/heads/main"]);
    f.config("remote.origin.url", url);
    f.config("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*");
    remote
}

#[test]
fn checking_the_remote_ignores_the_folders_ssh_command() {
    let f = folder(b"");
    f.config("remote.origin.url", "ssh://git@127.0.0.1:1/paper.git");
    f.config("core.sshCommand", &f.command());
    assert!(check_starts_nothing(&f).is_err());
}

#[test]
fn checking_the_remote_ignores_the_folders_proxy() {
    let f = folder(b"");
    f.config("remote.origin.url", "git://127.0.0.1:1/paper.git");
    f.config("core.gitProxy", &f.program());
    assert!(check_starts_nothing(&f).is_err());
}

#[test]
fn checking_the_remote_refuses_a_remote_that_is_a_command() {
    let f = folder(b"");
    let url = format!("ext::sh -c touch% {}", f.marker.display());
    f.config("remote.origin.url", &url);
    f.config("protocol.ext.allow", "always");
    assert!(check_starts_nothing(&f).is_err());
}

#[test]
fn checking_the_remote_ignores_the_folders_upload_program_and_still_works() {
    let f = folder(b"");
    with_remote(&f);
    let value = format!("touch '{}'; git-upload-pack", f.marker.display());
    f.config("remote.origin.uploadpack", &value);
    check_starts_nothing(&f).unwrap();
}

#[test]
fn checking_the_remote_runs_no_hook_the_folder_names() {
    let f = folder(b"");
    with_remote(&f);
    f.config("hook.evil.event", "reference-transaction");
    f.config("hook.evil.command", &f.command());
    // Git starts it even when there is nothing new to fetch.
    check_starts_nothing(&f).unwrap();
}

#[test]
fn a_hook_named_with_an_equals_sign_stays_silent() {
    let f = folder(b"");
    with_remote(&f);
    f.config("hook.a=b.event", "reference-transaction");
    f.config("hook.a=b.command", &f.command());
    check_starts_nothing(&f).unwrap();
}

#[test]
fn a_hook_from_an_included_file_stays_silent() {
    let f = folder(b"");
    with_remote(&f);
    let included = f.temp.path().join("shared.cfg");
    let text = format!(
        "[hook \"evil\"]\n\tevent = reference-transaction\n\tcommand = {}\n",
        f.command()
    );
    fs::write(&included, text).unwrap();
    f.config("include.path", included.to_str().unwrap());
    check_starts_nothing(&f).unwrap();
}

#[test]
fn a_hook_the_researcher_also_named_runs_their_command_only() {
    let f = folder(b"");
    with_remote(&f);
    f.config("hook.mine.event", "reference-transaction");
    f.config("hook.mine.command", &f.command());
    let own = f.temp.path().join("own-hook-ran");
    let listing = format!(
        "global\0hook.mine.event\nreference-transaction\0\
         global\0hook.mine.command\ntouch '{}'\0\
         local\0hook.mine.event\nreference-transaction\0\
         local\0hook.mine.command\n{}\0",
        own.display(),
        f.command()
    );
    let guard = ReadGuard::listed_by(&|_| Ok(listing.clone().into_bytes())).unwrap();
    let mut command = Command::new("git");
    guard.fetch(&mut command, &["--quiet"], "origin").unwrap();
    let output = command.current_dir(&f.root).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(own.exists(), "the researcher's hook did not run");
    assert!(!f.marker.exists(), "the repository's hook ran");
}

#[test]
fn check_for_updates_reaches_the_remote() {
    let f = folder(b"");
    let remote = with_remote(&f);
    let theirs = f.temp.path().join("theirs");
    sh(
        f.temp.path(),
        &["clone", remote.to_str().unwrap(), "theirs"],
    );
    identity(&theirs);
    sh(
        &theirs,
        &["commit", "--allow-empty", "-m", "From a coauthor"],
    );
    sh(&theirs, &["push", "origin", "HEAD:main"]);
    check_for_updates(&f).unwrap();
    let log = sh(&f.root, &["log", "-1", "--format=%s", "origin/main"]);
    assert_eq!(log, "From a coauthor");
}

#[test]
fn pull_fetches_under_the_guard_and_merges_with_the_repositorys_filters() {
    let f = folder(b"paper.tex filter=evil\n");
    let remote = with_remote(&f);
    sh(&f.root, &["fetch", "origin"]);
    sh(&f.root, &["branch", "--set-upstream-to=origin/main"]);
    let theirs = f.temp.path().join("theirs");
    sh(
        f.temp.path(),
        &["clone", remote.to_str().unwrap(), "theirs"],
    );
    identity(&theirs);
    fs::write(theirs.join("paper.tex"), "their draft\n").unwrap();
    sh(&theirs, &["commit", "-am", "Their draft"]);
    sh(&theirs, &["push", "origin", "HEAD:main"]);
    let value = format!("touch '{}'; git-upload-pack", f.marker.display());
    f.config("remote.origin.uploadpack", &value);
    let merged = f.temp.path().join("merge-used-the-filter");
    f.config(
        "filter.evil.smudge",
        &format!("touch '{}'; cat", merged.display()),
    );
    let git = Git::open(&f.root).unwrap();
    let cancel = AtomicBool::new(false);
    run(&git, Action::Pull, &cancel, &|_| Ok(()))
        .map(|_| ())
        .unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join("paper.tex")).unwrap(),
        "their draft\n"
    );
    assert!(!f.marker.exists(), "the fetch used the folder's program");
    assert!(merged.exists(), "the merge did not use the folder's filter");
}

#[test]
fn a_failed_fetch_says_a_setting_of_the_folders_was_ignored() {
    let f = folder(b"");
    f.config("remote.origin.url", "ssh://git@127.0.0.1:1/paper.git");
    f.config("core.sshCommand", "ssh -i /no/such/key");
    let overview = repository::fetch(&f.root).unwrap_err().message;
    let page = check_for_updates(&f).unwrap_err().message;
    for message in [overview, page] {
        assert!(message.contains("core.sshCommand"), "{message}");
        assert!(message.contains("from a terminal"), "{message}");
        assert!(!message.contains("/no/such/key"), "{message}");
    }
}

#[test]
fn a_failed_fetch_in_an_ordinary_folder_gives_no_such_advice() {
    let f = folder(b"");
    f.config("remote.origin.url", "ssh://git@127.0.0.1:1/paper.git");
    for message in [
        repository::fetch(&f.root).unwrap_err().message,
        check_for_updates(&f).unwrap_err().message,
    ] {
        assert!(!message.contains("own Git settings"), "{message}");
    }
}

/// The Version control page's Pull.
fn pull(f: &Folder) -> WorkbenchResult<()> {
    let git = Git::open(&f.root).unwrap();
    let cancel = AtomicBool::new(false);
    run(&git, Action::Pull, &cancel, &|_| Ok(())).map(|_| ())
}

/// `origin` as a remote group, whose members each name the marker's program.
fn grouped(values: &[&str]) -> Folder {
    let f = folder(b"");
    let remote = with_remote(&f);
    sh(&f.root, &["fetch", "origin"]);
    sh(&f.root, &["branch", "--set-upstream-to=origin/main"]);
    let upload = format!("touch '{}'; git-upload-pack", f.marker.display());
    for member in ["e1", "e2"] {
        f.config(&format!("remote.{member}.url"), remote.to_str().unwrap());
        f.config(&format!("remote.{member}.uploadpack"), &upload);
    }
    for value in values {
        sh(&f.root, &["config", "--add", "remotes.origin", value]);
    }
    f
}

fn refused_on_every_route(f: &Folder) {
    for result in [
        repository::fetch(&f.root).map(|_| ()),
        check_for_updates(f),
        pull(f),
    ] {
        let message = result.unwrap_err().message;
        assert!(message.contains("remotes.<group>"), "{message}");
        assert!(message.contains("from a terminal"), "{message}");
        assert!(!f.marker.exists(), "a group started the folder's program");
    }
}

#[test]
fn a_remote_group_is_refused_on_every_fetch_route() {
    for values in [&["e1 e2"][..], &["e1 e1"], &["e1", "e2"]] {
        let f = grouped(values);
        // Git's children lose this argument, which is how a group gets through.
        f.git_alone_starts_it(&["fetch", "--upload-pack=git-upload-pack", "--", "origin"]);
        refused_on_every_route(&f);
    }
}

#[test]
fn a_group_of_one_is_refused_too() {
    refused_on_every_route(&grouped(&["e1"]));
}

#[test]
fn a_group_named_with_a_dot_is_refused() {
    let f = folder(b"");
    let remote = with_remote(&f);
    let url = remote.to_str().unwrap();
    let upload = format!("touch '{}'; git-upload-pack", f.marker.display());
    sh(&f.root, &["config", "--unset-all", "remote.origin.url"]);
    sh(&f.root, &["config", "--unset-all", "remote.origin.fetch"]);
    f.config("remote.a.b.url", url);
    for member in ["e1", "e2"] {
        f.config(&format!("remote.{member}.url"), url);
        f.config(&format!("remote.{member}.uploadpack"), &upload);
    }
    f.config("remotes.a.b", "e1 e2");
    f.git_alone_starts_it(&["fetch", "--upload-pack=git-upload-pack", "--", "a.b"]);
    assert!(repository::fetch(&f.root).is_err());
    assert!(check_for_updates(&f).is_err());
    assert!(!f.marker.exists());
}

#[test]
fn a_fetch_starts_no_helper_or_alias_the_folder_names() {
    let cases: [(&str, &str); 7] = [
        ("evil://paper.invalid/x", "alias.remote-evil"),
        ("evil::x", "alias.remote-evil"),
        // These pass the list of transports and have no helper of their own.
        ("ssh::x", "alias.remote-ssh"),
        ("git::x", "alias.remote-git"),
        ("file::x", "alias.remote-file"),
        ("ssh::x", "alias.remote-ssh.command"),
        ("evil::x", "alias.remote-evil.command"),
    ];
    for (url, alias) in cases {
        let f = folder(b"");
        f.config("remote.origin.url", url);
        f.config(alias, &format!("!touch '{}'", f.marker.display()));
        assert!(check_starts_nothing(&f).is_err(), "{url} {alias}");
    }
}

#[test]
fn an_address_rewritten_to_a_helper_starts_nothing() {
    let f = folder(b"");
    f.config("remote.origin.url", "https://paper.invalid/x");
    f.config("url.evil::x.insteadOf", "https://paper.invalid/x");
    f.config(
        "alias.remote-evil",
        &format!("!touch '{}'", f.marker.display()),
    );
    assert!(check_starts_nothing(&f).is_err());
}

#[test]
fn an_emptied_helper_setting_starts_no_alias_either() {
    let f = folder(b"");
    f.config("remote.origin.url", "https://paper.invalid/x");
    f.config("remote.origin.vcs", "evil");
    let alias = format!("!touch '{}'", f.marker.display());
    f.config("alias.remote-evil", &alias);
    // Emptying the setting would otherwise ask Git for a helper with no name.
    f.config("alias.remote-", &alias);
    assert!(check_starts_nothing(&f).is_err());
}

#[test]
fn a_remote_from_an_old_style_file_starts_nothing() {
    let f = folder(b"");
    let folder = f.root.join(".git/remotes");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("origin"), "URL: evil::x\n").unwrap();
    // Git lists a remote only from config, then takes its address from the file.
    f.config("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*");
    f.config(
        "alias.remote-evil",
        &format!("!touch '{}'", f.marker.display()),
    );
    let message = check_starts_nothing(&f).unwrap_err().message;
    assert!(!message.contains("no remote to check"), "{message}");
}

#[test]
fn a_remote_that_uses_a_helper_is_refused_and_named() {
    let f = folder(b"");
    f.config("remote.origin.url", "hg::https://paper.invalid/x");
    for message in [
        repository::fetch(&f.root).unwrap_err().message,
        check_for_updates(&f).unwrap_err().message,
    ] {
        assert!(message.contains("remote.<name>.url"), "{message}");
        assert!(message.contains("from a terminal"), "{message}");
    }
}

#[test]
fn a_fetch_does_not_follow_submodules_whatever_the_folder_asks() {
    let f = with_submodule();
    with_remote(&f);
    let sub = f.root.join("sub");
    sh(
        &sub,
        &[
            "config",
            "remote.origin.uploadpack",
            &format!("touch '{}'; git-upload-pack", f.marker.display()),
        ],
    );
    f.config("submodule.recurse", "true");
    f.config("fetch.recurseSubmodules", "true");
    f.config("submodule.sub.fetchRecurseSubmodules", "true");
    let source = f.temp.path().join("source");
    sh(&source, &["commit", "--allow-empty", "-m", "More notes"]);
    let plain = &[
        "-c",
        "protocol.file.allow=always",
        "fetch",
        "--recurse-submodules=yes",
    ];
    f.git_alone_starts_it(&[plain.as_slice(), &["--", "origin"]].concat());
    sh(&f.root, &["update-ref", "-d", "refs/remotes/origin/main"]);
    repository::fetch(&f.root).unwrap();
    assert!(!f.marker.exists());
    sh(&f.root, &["update-ref", "-d", "refs/remotes/origin/main"]);
    check_for_updates(&f).unwrap();
    assert!(!f.marker.exists());
}

#[test]
fn no_other_setting_takes_a_fetch_outside_the_guard() {
    type Setup = fn(&Folder, &str, &str);
    fn member(f: &Folder, url: &str, upload: &str) {
        f.config("remote.e1.url", url);
        f.config("remote.e1.fetch", "+refs/heads/*:refs/remotes/e1/*");
        f.config("remote.e1.uploadpack", upload);
    }
    let cases: [(&str, Setup); 9] = [
        ("fetch.all", |f, url, upload| {
            member(f, url, upload);
            f.config("fetch.all", "true");
        }),
        ("fetch.parallel", |f, url, upload| {
            member(f, url, upload);
            f.config("fetch.parallel", "4");
        }),
        ("insteadOf to ext::", |f, _, _| {
            let target = format!("url.ext::sh -c touch% {}.insteadOf", f.marker.display());
            f.config("remote.origin.url", "https://paper.invalid/x");
            f.config("protocol.ext.allow", "always");
            f.config(&target, "https://paper.invalid/x");
        }),
        ("insteadOf to a local path", |f, url, upload| {
            f.config("remote.origin.url", "https://paper.invalid/x");
            f.config(&format!("url.{url}.insteadOf"), "https://paper.invalid/x");
            f.config("remote.origin.uploadpack", upload);
        }),
        ("pushInsteadOf to ext::", |f, url, _| {
            let target = format!("url.ext::sh -c touch% {}.pushInsteadOf", f.marker.display());
            f.config(&target, url);
        }),
        ("promisor member with a missing object", |f, url, upload| {
            member(f, url, upload);
            f.config("core.repositoryformatversion", "1");
            f.config("extensions.partialClone", "e1");
            f.config("remote.e1.promisor", "true");
            let blob = sh(&f.root, &["rev-parse", "HEAD:paper.tex"]);
            let object = f
                .root
                .join(".git/objects")
                .join(&blob[..2])
                .join(&blob[2..]);
            fs::remove_file(object).unwrap();
        }),
        ("promisor origin with a filter", |f, _, upload| {
            f.config("core.repositoryformatversion", "1");
            f.config("extensions.partialClone", "origin");
            f.config("remote.origin.promisor", "true");
            f.config("remote.origin.partialclonefilter", "blob:none");
            f.config("remote.origin.uploadpack", upload);
        }),
        ("promisor remotes from the server", |f, url, upload| {
            member(f, url, upload);
            f.config("promisor.acceptFromServer", "all");
        }),
        ("bundle address to ext::", |f, _, _| {
            let target = format!("ext::sh -c touch% {}", f.marker.display());
            f.config("transfer.bundleURI", "true");
            f.config("fetch.bundleURI", &target);
        }),
    ];
    for (name, setup) in cases {
        let f = folder(b"");
        let remote = with_remote(&f);
        let upload = format!("touch '{}'; git-upload-pack", f.marker.display());
        setup(&f, remote.to_str().unwrap(), &upload);
        let _ = repository::fetch(&f.root);
        assert!(!f.marker.exists(), "{name}: checking the remote");
        let _ = check_for_updates(&f);
        assert!(!f.marker.exists(), "{name}: Check for updates");
    }
}

/// Answers every request with `200` and a cookie, for as long as the tests run.
fn cookie_server() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\
                  Set-Cookie: session=abc; Path=/\r\n\
                  Content-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    port
}

/// A fetch must not let a folder pick a file for Git to replace with its cookies.
/// `prefix` is `http` or a URL form, which outranks a plain override.
fn cookies_stay_out_of(prefix: impl Fn(u16) -> String) {
    let f = folder(b"");
    let port = cookie_server();
    let chosen = f.temp.path().join("chosen.txt");
    let original = "researcher file, must survive\n";
    fs::write(&chosen, original).unwrap();
    f.config(
        "remote.origin.url",
        &format!("http://127.0.0.1:{port}/paper.git"),
    );
    // Without this, Git stops before its HTTP helper writes the cookies out.
    f.config("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*");
    let prefix = prefix(port);
    f.config(&format!("{prefix}.cookieFile"), chosen.to_str().unwrap());
    f.config(&format!("{prefix}.saveCookies"), "true");
    let _ = Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(&f.root)
        .output()
        .unwrap();
    assert_ne!(
        fs::read_to_string(&chosen).unwrap(),
        original,
        "Git alone left the file alone, so this test proves nothing"
    );
    fs::write(&chosen, original).unwrap();
    let _ = repository::fetch(&f.root);
    assert_eq!(fs::read_to_string(&chosen).unwrap(), original);
    let _ = check_for_updates(&f);
    assert_eq!(fs::read_to_string(&chosen).unwrap(), original);
}

#[test]
fn a_fetch_cannot_overwrite_a_file_through_the_cookie_settings() {
    cookies_stay_out_of(|_| "http".to_string());
}

#[test]
fn a_fetch_cannot_overwrite_a_file_through_cookie_settings_for_one_address() {
    cookies_stay_out_of(|port| format!("http.http://127.0.0.1:{port}"));
}

/// Counts the connections a proxy receives, for as long as the tests run.
fn counting_proxy() -> (u16, std::sync::Arc<AtomicU64>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let hits = std::sync::Arc::new(AtomicU64::new(0));
    let seen = hits.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            seen.fetch_add(1, Ordering::SeqCst);
            drop(stream);
        }
    });
    (port, hits)
}

#[test]
fn a_fetch_ignores_every_proxy_the_folder_names() {
    for key in [
        "http.proxy",
        "http.http://paper.invalid.proxy",
        "remote.origin.proxy",
    ] {
        let f = folder(b"");
        let (port, hits) = counting_proxy();
        f.config("remote.origin.url", "http://paper.invalid/paper.git");
        f.config(key, &format!("http://127.0.0.1:{port}"));
        let _ = Command::new("git")
            .args(["fetch", "origin"])
            .current_dir(&f.root)
            .output()
            .unwrap();
        let alone = hits.load(Ordering::SeqCst);
        assert!(alone > 0, "Git alone did not use {key}");
        assert!(repository::fetch(&f.root).is_err());
        assert!(check_for_updates(&f).is_err());
        assert_eq!(hits.load(Ordering::SeqCst), alone, "{key} was used");
    }
}

#[test]
fn checking_the_remote_ignores_the_folders_alternate_refs_command() {
    let f = folder(b"");
    // Checks both routes by hand: the first fetch leaves nothing for a second.
    let shared = f.temp.path().join("shared.git");
    fs::create_dir(&shared).unwrap();
    sh(&shared, &["init", "--bare"]);
    sh(
        &f.root,
        &["push", shared.to_str().unwrap(), "HEAD:refs/heads/shared"],
    );
    let alternates = format!("{}\n", shared.join("objects").display());
    fs::write(f.root.join(".git/objects/info/alternates"), alternates).unwrap();
    let remote = f.temp.path().join("remote.git");
    fs::create_dir(&remote).unwrap();
    sh(&remote, &["init", "--bare"]);
    let url = remote.to_str().unwrap();
    sh(&f.root, &["commit", "--allow-empty", "-m", "On the remote"]);
    sh(&f.root, &["push", url, "HEAD:refs/heads/main"]);
    sh(&f.root, &["reset", "--hard", "HEAD~1"]);
    f.config("remote.origin.url", url);
    f.config("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*");
    f.config("core.alternateRefsCommand", &f.command());
    f.git_alone_starts_it(&["fetch", "origin"]);
    sh(&f.root, &["update-ref", "-d", "refs/remotes/origin/main"]);
    repository::fetch(&f.root).unwrap();
    assert!(!f.marker.exists());
    sh(&f.root, &["update-ref", "-d", "refs/remotes/origin/main"]);
    check_for_updates(&f).unwrap();
    assert!(!f.marker.exists());
}

/// The guard over a listing `list` gives, on a call in `f`.
fn status_under(f: &Folder, guard: &ReadGuard) {
    let mut command = Command::new("git");
    guard.read(&mut command, &["status", "--porcelain=v2"]);
    let output = command.current_dir(&f.root).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn git_without_scopes_still_finds_the_repositorys_keys() {
    let f = folder(b"paper.tex filter=evil\ngoal.txt filter=other\n");
    let included = f.temp.path().join("shared.cfg");
    let text = format!("[filter \"evil\"]\n\tclean = {}\n", f.command());
    fs::write(&included, text).unwrap();
    f.config("include.path", included.to_str().unwrap());
    f.config("extensions.worktreeConfig", "true");
    sh(
        &f.root,
        &["config", "--worktree", "filter.other.clean", &f.command()],
    );
    fs::write(f.root.join("goal.txt"), "goal\n").unwrap();
    sh(
        &f.root,
        &[
            "-c",
            "filter.evil.clean=",
            "-c",
            "filter.other.clean=",
            "add",
            ".",
        ],
    );
    sh(
        &f.root,
        &[
            "-c",
            "filter.evil.clean=",
            "-c",
            "filter.other.clean=",
            "commit",
            "-m",
            "Goal",
        ],
    );
    let old_git = |args: &[&str]| {
        if args.contains(&"--show-scope") {
            return Err(WorkbenchError::invalid(
                "Git task setup failed: error: unknown option `show-scope'",
            ));
        }
        let mut command = Command::new("git");
        command.args(args);
        tasks::capture_git(command, &f.root, tasks::GitLimits::default())
    };
    let guard = ReadGuard::listed_by(&old_git).unwrap();
    stale(&f.root.join("paper.tex"));
    stale(&f.root.join("goal.txt"));
    status_under(&f, &guard);
    assert!(!f.marker.exists());
}

#[test]
fn git_without_scopes_outside_a_repository_has_nothing_to_override() {
    let temp = tempfile::tempdir().unwrap();
    let old_git = |args: &[&str]| {
        if args.contains(&"--show-scope") {
            return Err(WorkbenchError::invalid(
                "Git task setup failed: error: unknown option `show-scope'",
            ));
        }
        let mut command = Command::new("git");
        command.args(args);
        tasks::capture_git(command, temp.path(), tasks::GitLimits::default())
    };
    ReadGuard::listed_by(&old_git).unwrap();
}

#[test]
fn the_researchers_own_filter_still_runs_where_the_repository_set_the_same_key() {
    let f = folder(b"paper.tex filter=shared\n");
    let own = f.temp.path().join("own-filter-ran");
    let listing = format!(
        "global\0filter.shared.clean\ntouch '{}'; cat\0local\0filter.shared.clean\n{}\0",
        own.display(),
        f.command()
    );
    let guard = ReadGuard::listed_by(&|_| Ok(listing.clone().into_bytes())).unwrap();
    f.config("filter.shared.clean", &f.command());
    stale(&f.root.join("paper.tex"));
    status_under(&f, &guard);
    assert!(own.exists(), "the researcher's filter did not run");
    assert!(!f.marker.exists(), "the repository's filter ran");
}
