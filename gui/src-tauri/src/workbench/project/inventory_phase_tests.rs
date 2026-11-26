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
