use super::*;

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(6));
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn a_file_is_read_as_itself_then_from_its_compressed_copy() {
    let temp = tempfile::tempdir().unwrap();
    let run = temp.path().canonicalize().unwrap();
    fs::create_dir_all(run.join("context")).unwrap();
    fs::write(run.join("context/plain.json"), b"{\"plain\":true}").unwrap();
    let original = b"{\"compressed\":true}".repeat(100);
    fs::write(run.join("context/packed.json.gz"), gzip(&original)).unwrap();

    let plain = locate(&run, "context/plain.json").unwrap();
    assert!(matches!(plain, StoredFile::Plain(_)));
    assert_eq!(plain.read_at_most(1_000).unwrap().0, b"{\"plain\":true}");

    let packed = locate(&run, "context/packed.json").unwrap();
    assert!(matches!(packed, StoredFile::Gzip(_)));
    assert_eq!(packed.logical_len().unwrap(), Some(original.len() as u64));
    assert_eq!(
        packed.read_at_most(10_000).unwrap(),
        (original.clone(), false)
    );
    assert_eq!(
        packed.read_at_most(10).unwrap(),
        (original[..10].to_vec(), true)
    );
    assert_eq!(
        read_run_file_bytes(&run, "context/packed.json", 10_000).unwrap(),
        Some(original)
    );
    assert!(read_run_file_bytes(&run, "context/packed.json", 10).is_err());

    assert!(locate(&run, "context/missing.json").is_none());
    assert_eq!(
        read_run_file_bytes(&run, "context/missing.json", 10).unwrap(),
        None
    );
    assert!(read_run_file_bytes(&run, "../outside", 10).is_err());
    assert!(exists(&run, "context/packed.json"));
    assert!(!exists(&run, "context/missing.json"));
}

#[test]
fn a_removed_page_is_located_through_the_record() {
    let temp = tempfile::tempdir().unwrap();
    let run = temp.path().canonicalize().unwrap();
    fs::create_dir_all(run.join("source-snapshot")).unwrap();
    fs::write(run.join("source-snapshot/paper.pdf"), b"%PDF-1.4").unwrap();
    assert!(locate(&run, "artifacts/pages/page-2.jpg").is_none());
    save_record(
        &run,
        &CompactionRecord {
            version: 1,
            compressed: Default::default(),
            pages: Some(RebuildablePages {
                pdf: "source-snapshot/paper.pdf".into(),
                paths: vec![
                    "artifacts/pages/page-1.jpg".into(),
                    "artifacts/pages/page-2.jpg".into(),
                ],
            }),
        },
    )
    .unwrap();
    assert_eq!(
        locate(&run, "artifacts/pages/page-2.jpg"),
        Some(StoredFile::Page {
            pdf: run.join("source-snapshot/paper.pdf"),
            page: 2,
        })
    );
    assert!(locate(&run, "artifacts/pages/page-3.jpg").is_none());
}

#[test]
fn a_copy_expands_only_what_compaction_compressed() {
    let temp = tempfile::tempdir().unwrap();
    let run = temp.path().join("run");
    let copy = temp.path().join("copy");
    fs::create_dir_all(run.join("context")).unwrap();
    let data = b"year,value\n".repeat(50);
    let own = gzip(b"a researcher's own archive");
    save_record(
        &run,
        &CompactionRecord {
            version: 1,
            compressed: ["source-snapshot/tree/data.csv".to_string()].into(),
            pages: None,
        },
    )
    .unwrap();
    fs::create_dir_all(copy.join("source-snapshot/tree")).unwrap();
    fs::write(copy.join("source-snapshot/tree/data.csv.gz"), gzip(&data)).unwrap();
    fs::write(copy.join("source-snapshot/tree/own.csv.gz"), &own).unwrap();

    expand_copy(&run, &copy, "source-snapshot/tree").unwrap();

    assert_eq!(
        fs::read(copy.join("source-snapshot/tree/data.csv")).unwrap(),
        data
    );
    assert!(!copy.join("source-snapshot/tree/data.csv.gz").exists());
    // A `.gz` that was already in the folder stays as it was.
    assert_eq!(
        fs::read(copy.join("source-snapshot/tree/own.csv.gz")).unwrap(),
        own
    );
    assert!(!copy.join("source-snapshot/tree/own.csv").exists());
}

#[test]
fn hard_links_share_one_identity() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    let other = temp.path().join("other");
    fs::write(&first, b"shared").unwrap();
    fs::write(&other, b"shared").unwrap();
    fs::hard_link(&first, &second).unwrap();
    let (left, links) = file_identity(&first).unwrap();
    let (right, _) = file_identity(&second).unwrap();
    assert_eq!(left, right);
    assert_eq!(links, 2);
    assert_ne!(file_identity(&other).unwrap().0, left);
    assert!(file_identity(temp.path()).is_none());
}

#[test]
fn an_unreadable_record_is_reported_rather_than_ignored() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(read_record(temp.path()).unwrap(), None);
    fs::create_dir_all(temp.path().join("context")).unwrap();
    fs::write(temp.path().join(RECORD_PATH), "not json").unwrap();
    assert!(read_record(temp.path()).is_err());
}
