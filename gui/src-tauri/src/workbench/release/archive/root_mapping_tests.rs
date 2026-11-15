//! Root permutations preserve original ownership of command paths.
use super::{export_archive, import_archive, ExportArchiveRequest, ImportArchiveRequest};
use crate::workbench::store::{CreateWorkspaceRequest, Store};
use rusqlite::params;
use std::{collections::BTreeMap, fs, path::PathBuf};

struct Fixture {
    temp: tempfile::TempDir,
    roots: Vec<String>,
    ids: Vec<String>,
    archive: String,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let original = Store::open_at(&temp.path().join("original")).unwrap();
    let mut roots = Vec::new();
    let mut ids = Vec::new();
    for index in 0..4 {
        let root = temp.path().join(format!("project-{index}"));
        fs::create_dir_all(root.join("analysis")).unwrap();
        roots.push(root.canonicalize().unwrap().to_string_lossy().into_owned());
        if index == 3 {
            continue;
        }
        let workspace = original
            .create_workspace(CreateWorkspaceRequest {
                name: format!("Project {index}"),
                root: Some(roots[index].clone()),
                operation_id: format!("create-{index}"),
            })
            .unwrap()
            .record;
        let cwd = PathBuf::from(&roots[index])
            .join("analysis")
            .to_string_lossy()
            .into_owned();
        let c = original.connection().unwrap();
        c.execute("INSERT INTO execution_profiles(id,workspace_id,name,adapter,argv_json,cwd,environment_json,inputs_json,timeout_seconds,outputs_json,created_at,updated_at) VALUES(?1,?1,'Analysis','command','[]',?2,'{}','[]',30,'[]','t','t')", params![workspace.id, cwd]).unwrap();
        c.execute("INSERT INTO research_executions(id,workspace_id,adapter,command_json,cwd,input_manifest_json,dependency_hash,outcome,output_manifest_json,validation_json,snapshot_consistency,created_at) VALUES(?1,?1,'command','[]',?2,'{}','hash','completed','{}','{}','complete','t')", params![workspace.id, cwd]).unwrap();
        c.execute("INSERT INTO desk_records(id,workspace_id,kind,title,body_json,content_hash,created_at) VALUES(?1,?1,'execution_plan','Plan',?2,'captured-hash','t')", params![workspace.id, serde_json::json!({"profile":{"cwd":cwd}}).to_string()]).unwrap();
        ids.push(workspace.id);
    }
    let archive = temp
        .path()
        .join("backup.pwrx")
        .to_string_lossy()
        .into_owned();
    export_archive(
        &original,
        ExportArchiveRequest {
            path: archive.clone(),
        },
    )
    .unwrap();
    Fixture {
        temp,
        roots,
        ids,
        archive,
    }
}

#[test]
fn swaps_cycles_chains_and_detachments_keep_paths_with_the_original_workspace() {
    let f = fixture();
    for (case, destinations) in [
        [Some(1), Some(0), Some(2)],
        [Some(1), Some(2), Some(0)],
        [Some(1), Some(2), Some(3)],
        [Some(0), None, Some(3)],
    ]
    .into_iter()
    .enumerate()
    {
        let target = Store::open_at(&f.temp.path().join(format!("target-{case}"))).unwrap();
        let mappings = (0..3)
            .map(|index| {
                (
                    f.roots[index].clone(),
                    destinations[index].map(|next| f.roots[next].clone()),
                )
            })
            .collect();
        import_archive(
            &target,
            ImportArchiveRequest {
                path: f.archive.clone(),
                root_mappings: mappings,
            },
        )
        .unwrap();
        let c = target.connection().unwrap();
        for (index, next) in destinations.into_iter().enumerate() {
            let root: Option<String> = c
                .query_row(
                    "SELECT root FROM workspaces WHERE id=?1",
                    [&f.ids[index]],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(root, next.map(|next| f.roots[next].clone()));
            let cwd = PathBuf::from(&f.roots[next.unwrap_or(index)])
                .join("analysis")
                .to_string_lossy()
                .into_owned();
            for query in [
                "SELECT cwd FROM execution_profiles WHERE workspace_id=?1",
                "SELECT cwd FROM research_executions WHERE workspace_id=?1",
                "SELECT json_extract(body_json,'$.profile.cwd') FROM desk_records WHERE workspace_id=?1",
            ] {
                let actual: String = c.query_row(query, [&f.ids[index]], |row| row.get(0)).unwrap();
                assert_eq!(actual, cwd, "case {case}, workspace {index}");
            }
        }
        let violations: i64 = c
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(violations, 0);
    }
}

#[test]
fn duplicate_or_invalid_final_roots_leave_the_destination_empty() {
    let f = fixture();
    for (case, invalid) in [
        f.roots[1].clone(),
        f.temp.path().join("missing").to_string_lossy().into_owned(),
    ]
    .into_iter()
    .enumerate()
    {
        let target = Store::open_at(&f.temp.path().join(format!("invalid-{case}"))).unwrap();
        let mut mappings: BTreeMap<_, _> = f.roots[..3]
            .iter()
            .map(|root| (root.clone(), Some(root.clone())))
            .collect();
        mappings.insert(f.roots[0].clone(), Some(invalid));
        assert!(import_archive(
            &target,
            ImportArchiveRequest {
                path: f.archive.clone(),
                root_mappings: mappings
            }
        )
        .is_err());
        assert!(target.list_workspaces(false).unwrap().workspaces.is_empty());
    }
}
