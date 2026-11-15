//! Another Pipeline process must distinguish a live provider worker from a crashed owner.
use super::session_lease;
use crate::workbench::{
    api, claude,
    store::{CreateSessionRequest, Store},
};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const NAME: &str =
    "workbench::submission::lease_tests::another_instance_recovers_only_after_the_turn_owner_exits";
const ROOT: &str = "PIPELINE_LEASE_REGRESSION_STORE";
const MODE: &str = "PIPELINE_LEASE_REGRESSION_MODE";

fn child(root: &Path, mode: &str) -> Command {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", NAME, "--nocapture"])
        .env(ROOT, root)
        .env(MODE, mode);
    cmd
}

#[test]
fn another_instance_recovers_only_after_the_turn_owner_exits() {
    if let Some(root) = std::env::var_os(ROOT) {
        let root = Path::new(&root);
        let store = Store::open_at(&root.join("store")).unwrap();
        let ids: Vec<String> =
            serde_json::from_slice(&std::fs::read(root.join("sessions.json")).unwrap()).unwrap();
        let mode = std::env::var(MODE).unwrap();
        if mode == "owner" {
            let _leases = ids
                .iter()
                .map(|id| session_lease(&store, id).unwrap().unwrap())
                .collect::<Vec<_>>();
            for (id, ns) in ids.iter().zip([api::NAMESPACE, claude::NAMESPACE]) {
                let binding = store.bind_session(id, ns, "thread").unwrap();
                store
                    .record_turn_submission(&binding.id, &format!("submission-{id}"), "turn")
                    .unwrap();
            }
            std::fs::write(root.join("ready"), []).unwrap();
            // If the parent test unwinds or exits, its pipe closes and so does the owner.
            let mut byte = [0u8];
            let _ = std::io::stdin().read(&mut byte);
        } else {
            assert!(api::reconcile(store.clone(), ids[0].clone()).unwrap());
            assert!(claude::reconcile(store.clone(), ids[1].clone()).unwrap());
            for id in &ids {
                let snapshot = store.conversation_snapshot(id).unwrap();
                assert_eq!(snapshot.turns.len(), 1);
                if mode == "live" {
                    assert!(snapshot.turns[0].terminal_at.is_none());
                } else {
                    assert_eq!(snapshot.turns[0].state, "interrupted");
                }
            }
        }
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let store = Store::open_at(&root.join("store")).unwrap();
    let ids = ["api", "claude"].map(|provider| {
        store
            .create_session(CreateSessionRequest {
                workspace_id: None,
                title: provider.into(),
                operation_id: format!("create-{provider}"),
            })
            .unwrap()
            .record
            .id
    });
    std::fs::write(
        root.join("sessions.json"),
        serde_json::to_vec(&ids).unwrap(),
    )
    .unwrap();
    let mut owner = child(root, "owner").stdin(Stdio::piped()).spawn().unwrap();
    let limit = Instant::now() + Duration::from_secs(20);
    while !root.join("ready").exists() {
        assert!(
            owner.try_wait().unwrap().is_none(),
            "Turn owner exited before acquiring leases"
        );
        assert!(Instant::now() < limit, "Turn owner did not acquire leases");
        std::thread::sleep(Duration::from_millis(10));
    }
    // Temporary-command cleanup can move files inside jobs/. These leases
    // have to keep their inode outside any disposable storage category.
    for id in &ids {
        use sha2::{Digest, Sha256};
        let key = format!("{:x}", Sha256::digest(id.as_bytes()));
        assert!(root
            .join("store")
            .join(format!(".turn-{key}.lock"))
            .is_file());
    }
    assert!(child(root, "live").status().unwrap().success());
    owner.kill().unwrap();
    owner.wait().unwrap();
    assert!(child(root, "crashed").status().unwrap().success());
}
