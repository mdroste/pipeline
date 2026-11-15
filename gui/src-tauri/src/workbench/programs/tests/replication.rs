use super::*;

#[cfg(unix)]
pub(super) fn capsule_request(f: &Fixture, plan: &DeskRecord, include: bool) -> capsule::Export {
    capsule::Export {
        workspace_id: f.ws.clone(),
        title: "Fixture replication".into(),
        path: f
            ._temp
            .path()
            .join("fixture.pwrc")
            .to_string_lossy()
            .into_owned(),
        selections: vec![capsule::Selection {
            plan_id: plan.id.clone(),
            included_paths: if include {
                vec!["analysis.py".into()]
            } else {
                vec![]
            },
            external_requirements: if include {
                BTreeMap::new()
            } else {
                BTreeMap::from([(
                    "analysis.py".into(),
                    "Obtain the licensed script from its author".into(),
                )])
            },
            expected: vec![capsule::Expected {
                output: "results.json".into(),
                pointer: "/estimate".into(),
                value: 1.25,
                absolute_tolerance: 1e-9,
                relative_tolerance: 1e-8,
            }],
        }],
    }
}
#[test]
fn capsule_archive_rejects_traversal_before_import() {
    use std::io::Write;
    let f = fixture(false);
    let path = f._temp.path().join("bad.pwrc");
    let mut archive = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    archive
        .start_file("../outside", zip::write::SimpleFileOptions::default())
        .unwrap();
    archive.write_all(b"bad").unwrap();
    archive.finish().unwrap();
    assert!(capsule::inspect(path.to_str().unwrap()).is_err());
    assert!(!f._temp.path().join("outside").exists());
}
