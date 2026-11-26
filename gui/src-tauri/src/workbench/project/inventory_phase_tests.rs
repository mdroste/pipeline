use super::super::test_support::fixture;
use super::*;

fn request(ws: &str) -> super::super::ProjectMutation {
    super::super::ProjectMutation {
        workspace_id: ws.into(),
        operation_id: "scan_test".into(),
        action: super::super::ProjectAction::CheckForChanges,
    }
}

#[test]
fn scan_holds_the_project_lock_and_refuses_changed_preferences_before_commit() {
    let f = fixture();
    fs::write(f.root.join("paper.txt"), "body").unwrap();
    let scan = InventoryScan::for_request(&f.store, request(&f.ws)).unwrap();
    assert!(super::super::lock(&f.store, &f.ws).is_err());
    let scanned = scan.scan().unwrap();
    super::super::home::save_home(
        &f.store,
        &f.ws,
        0,
        ProjectHomeSettings {
            ignored_paths: vec!["ignored".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        scanned.commit(&f.store).unwrap_err().code,
        "storage_conflict"
    );
    assert!(records(&f.store, &f.ws, "inventory").unwrap().is_empty());
    let saved = InventoryScan::for_request(&f.store, request(&f.ws))
        .unwrap()
        .scan()
        .unwrap()
        .commit(&f.store)
        .unwrap();
    let inventory: Inventory = decode(&saved).unwrap();
    assert_eq!(inventory.files.len(), 1);
    assert_eq!(inventory.files[0].path, "paper.txt");
    assert!(super::super::lock(&f.store, &f.ws).is_ok());
}

#[test]
fn root_replacement_is_refused_before_scanning_and_before_publishing() {
    for after_scan in [false, true] {
        let f = fixture();
        fs::write(f.root.join("paper.txt"), "original").unwrap();
        let scan = InventoryScan::for_request(&f.store, request(&f.ws)).unwrap();
        let replace = || {
            fs::rename(&f.root, f.root.with_file_name("old-research")).unwrap();
            fs::create_dir(&f.root).unwrap();
            fs::write(f.root.join("other.txt"), "replacement").unwrap();
        };
        if after_scan {
            let scanned = scan.scan().unwrap();
            replace();
            assert!(scanned.commit(&f.store).is_err());
        } else {
            replace();
            assert!(scan.scan().is_err());
        }
        assert!(records(&f.store, &f.ws, "inventory").unwrap().is_empty());
    }
}

fn write(root: &Path, path: &str, body: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

fn listed(inventory: &Inventory) -> Vec<&str> {
    inventory.files.iter().map(|f| f.path.as_str()).collect()
}

#[test]
fn worktrees_of_agents_and_git_are_left_out_but_submodules_stay() {
    assert!(ignored(".claude/worktrees", &[]));
    assert!(ignored(".claude/worktrees/agent-1/paper.tex", &[]));
    assert!(ignored("code/.Claude/Worktrees/agent-1/main.do", &[]));
    assert!(!ignored(".claude/settings.json", &[]));
    assert!(!ignored("worktrees/notes.md", &[]));
    assert!(!ignored("claude/worktrees/notes.md", &[]));

    let f = fixture();
    let gitdir = |name: &str| {
        format!(
            "gitdir: {}",
            f.root.join(".git/worktrees").join(name).display()
        )
    };
    write(&f.root, "paper.tex", "body");
    write(&f.root, ".claude/settings.json", "{}");
    write(
        &f.root,
        ".claude/worktrees/agent-1/.git",
        &gitdir("agent-1"),
    );
    write(&f.root, ".claude/worktrees/agent-1/paper.tex", "copy");
    // A worktree folder is left out by name even before Git writes its marker.
    write(&f.root, ".claude/worktrees/agent-2/AGENTS.md", "copy");
    write(&f.root, "feature/main.do", "draft");
    write(
        &f.root,
        "vendor/lib/.git",
        "gitdir: ../../.git/modules/vendor/lib\n",
    );
    write(&f.root, "vendor/lib/tools.ado", "shared");
    write(&f.root, "data/raw.csv", "1,2");
    super::super::home::save_home(
        &f.store,
        &f.ws,
        0,
        ProjectHomeSettings {
            ignored_paths: vec!["data".into()],
            ..Default::default()
        },
    )
    .unwrap();
    let first: Inventory = decode(&refresh(&f.store, &f.ws).unwrap()).unwrap();
    assert_eq!(
        listed(&first),
        [
            ".claude/settings.json",
            "feature/main.do",
            "paper.tex",
            "vendor/lib/tools.ado"
        ]
    );

    // `feature` becomes a linked worktree. Its file was listed before, and is
    // left out now rather than reported missing.
    write(&f.root, "feature/.git", &format!("{}\n", gitdir("feature")));
    let second: Inventory = decode(&refresh(&f.store, &f.ws).unwrap()).unwrap();
    assert_eq!(
        listed(&second),
        [".claude/settings.json", "paper.tex", "vendor/lib/tools.ado"]
    );
    assert!(second.complete);
    assert!(second.files.iter().all(|f| f.status == "current"));
    // A task copy lists every folder, so none of its files reads as deleted.
    assert!(listed(&inventory_at(&f.root, &[]).unwrap()).contains(&"feature/main.do"));
}

#[test]
fn a_file_past_the_capture_budget_is_not_called_large() {
    let f = fixture();
    write(&f.root, "a.txt", "12345678");
    write(&f.root, "b.txt", "12345678");
    fs::File::create(f.root.join("big.bin"))
        .unwrap()
        .set_len(MAX_FILE + 1)
        .unwrap();
    write(&f.root, "c.txt", "1");
    CAPTURE_BUDGET.set(Some(10));
    let scanned = refresh(&f.store, &f.ws);
    CAPTURE_BUDGET.set(None);
    let inventory: Inventory = decode(&scanned.unwrap()).unwrap();
    let status = |path: &str| {
        let file = inventory.files.iter().find(|f| f.path == path).unwrap();
        (file.status.as_str(), file.hash.is_some())
    };
    assert_eq!(status("a.txt"), ("current", true));
    assert_eq!(status("b.txt"), ("unhashed_capture_limit", false));
    assert_eq!(status("big.bin"), ("unhashed_large_file", false));
    // What is left of the budget still covers a smaller file.
    assert_eq!(status("c.txt"), ("current", true));
    assert!(!inventory.complete);
    assert_eq!(
        inventory.warnings,
        ["Pipeline reads at most 256 MB of files in one pass, so 1 file(s) were listed but not read. Leave out large data folders in Project settings and refresh."]
    );
}
