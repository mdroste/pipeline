use super::super::{
    export_archive, import_archive, inspect_archive, ArchiveReport, ExportArchiveRequest,
    ImportArchiveRequest,
};
use super::*;
use crate::researcher::{self, Note};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use zip::write::SimpleFileOptions;

/// A research data folder: the store in `workbench`, About you beside it.
struct Home {
    directory: tempfile::TempDir,
    store: Store,
    folder: PathBuf,
}

fn home() -> Home {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open_at(&directory.path().join("workbench")).unwrap();
    let folder = directory.path().join("researcher");
    assert_eq!(
        researcher::folder_beside(store.root_path()).as_deref(),
        Some(folder.as_path())
    );
    Home {
        directory,
        store,
        folder,
    }
}

impl Home {
    fn add(&self, name: &str, content: &[u8]) -> researcher::Sample {
        let path = self.directory.path().join(name);
        fs::write(&path, content).unwrap();
        researcher::add_sample(&self.folder, &path).unwrap()
    }

    fn export(&self) -> (PathBuf, ArchiveReport) {
        let path = self.directory.path().join("backup.pwrx");
        let report = export_archive(
            &self.store,
            ExportArchiveRequest {
                path: path.to_string_lossy().into(),
            },
        )
        .unwrap();
        (path, report)
    }

    fn import(&self, path: &Path) -> WorkbenchResult<ArchiveReport> {
        import_archive(
            &self.store,
            ImportArchiveRequest {
                path: path.to_string_lossy().into(),
                root_mappings: BTreeMap::new(),
            },
        )
    }

    fn stage(&self, budget: &mut Budget) -> (tempfile::TempDir, Staged) {
        let staging = tempfile::tempdir().unwrap();
        let mut staged = stage(&self.store, staging.path(), budget).unwrap();
        stage_originals(&mut staged, staging.path(), budget).unwrap();
        (staging, staged)
    }
}

/// A background, style notes, a paper with text, and one with none.
fn written() -> (Home, researcher::Sample, researcher::Sample) {
    let home = home();
    researcher::save_note(&home.folder, Note::About, "Labor economist.").unwrap();
    researcher::save_note(&home.folder, Note::Style, "First person plural.").unwrap();
    let paper = home.add("paper.md", b"We study minimum wages.");
    let scan = home.add("scan.txt", &[b'a', 0, b'b']);
    (home, paper, scan)
}

fn wide() -> Budget {
    Budget {
        entries: 1_000,
        bytes: 1_000_000,
        manifest_bytes: 1_000_000,
    }
}

fn manifest(path: &Path) -> Value {
    let mut archive = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    let mut bytes = Vec::new();
    archive
        .by_name("manifest.json")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn rewrite(path: &Path, mut rewrite: impl FnMut(&str, Vec<u8>) -> Vec<u8>) {
    let mut archive = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        entries.push((entry.name().unwrap().into_owned(), bytes));
    }
    drop(archive);
    let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(&name, SimpleFileOptions::default()).unwrap();
        zip.write_all(&rewrite(&name, bytes)).unwrap();
    }
    zip.finish().unwrap();
}

fn edit_manifest(path: &Path, edit: impl Fn(&mut Value)) {
    rewrite(path, |name, bytes| {
        if name != "manifest.json" {
            return bytes;
        }
        let mut manifest: Value = serde_json::from_slice(&bytes).unwrap();
        edit(&mut manifest);
        serde_json::to_vec(&manifest).unwrap()
    });
}

fn assert_nothing_restored(target: &Home) {
    assert!(!target.folder.exists());
    let sessions: i64 = target
        .store
        .connection()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
        .unwrap();
    assert_eq!(sessions, 0);
}

#[test]
fn about_you_round_trips_into_an_empty_data_folder() {
    let (source, paper, scan) = written();
    let (path, report) = source.export();
    assert!(
        report.portability_note.ends_with("About you is included."),
        "{}",
        report.portability_note
    );
    let inspection = inspect_archive(ExportArchiveRequest {
        path: path.to_string_lossy().into(),
    })
    .unwrap();
    assert_eq!(inspection.format_version, 4);
    let mut listed = manifest(&path)["aboutYou"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["originalReference"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    listed.sort();
    let mut expected = vec![
        "about.md".to_string(),
        "style.md".to_string(),
        format!("writing/{}.json", paper.id),
        format!("writing/{}.txt", paper.id),
        format!("writing/{}.original.md", paper.id),
        format!("writing/{}.json", scan.id),
        format!("writing/{}.original.txt", scan.id),
    ];
    expected.sort();
    assert_eq!(listed, expected);

    let target = home();
    let report = target.import(&path).unwrap();
    assert!(
        report.portability_note.ends_with("About you was restored."),
        "{}",
        report.portability_note
    );
    for name in ["about.md", "style.md"] {
        assert_eq!(
            fs::read(target.folder.join(name)).unwrap(),
            fs::read(source.folder.join(name)).unwrap()
        );
    }
    assert_eq!(
        researcher::samples(&target.folder).unwrap(),
        researcher::samples(&source.folder).unwrap()
    );
    assert_eq!(
        researcher::read_sample(&target.folder, &paper.id, 0, 100)
            .unwrap()
            .text,
        "We study minimum wages."
    );
    assert_eq!(
        fs::read(
            target
                .folder
                .join(format!("writing/{}.original.txt", scan.id))
        )
        .unwrap(),
        [b'a', 0, b'b']
    );
    // Nothing but the files Pipeline names is written.
    assert_eq!(
        fs::read_dir(target.folder.join("writing")).unwrap().count(),
        5
    );
}

#[test]
fn a_backup_with_no_about_you_says_nothing_and_restores_none() {
    let source = home();
    let (path, report) = source.export();
    assert!(!report.portability_note.contains("About you"));
    assert_eq!(manifest(&path)["aboutYou"], serde_json::json!([]));
    let target = home();
    let report = target.import(&path).unwrap();
    assert!(!report.portability_note.contains("About you"));
    assert!(!target.folder.exists());
}

#[test]
fn files_pipeline_did_not_name_stay_out_of_the_backup() {
    let (source, _, _) = written();
    fs::write(
        source.folder.join("about (conflicted copy).md"),
        "Another computer's version.",
    )
    .unwrap();
    fs::write(source.folder.join("writing/notes.txt"), "Stray.").unwrap();
    let (path, _) = source.export();
    let listed = manifest(&path)["aboutYou"].to_string();
    assert!(!listed.contains("conflicted") && !listed.contains("notes.txt"));
    let target = home();
    target.import(&path).unwrap();
    assert!(!target.folder.join("about (conflicted copy).md").exists());
}

#[test]
fn restore_keeps_what_is_already_on_this_computer() {
    let (source, paper, _scan) = written();
    let (path, _) = source.export();

    let target = home();
    researcher::save_note(&target.folder, Note::About, "Local background.").unwrap();
    let mine = target.add("same-paper.md", b"We study minimum wages.");
    assert_eq!(mine.id, paper.id);
    let mine = researcher::update_sample(
        &target.folder,
        researcher::SampleUpdate {
            id: mine.id,
            title: "My own title".into(),
            year: Some(2024),
            kind: "paper".into(),
            use_as_example: false,
        },
    )
    .unwrap();

    let report = target.import(&path).unwrap();
    let note = &report.portability_note;
    assert!(note.contains("About you was restored."), "{note}");
    assert!(note.contains("kept, not replaced"), "{note}");
    assert!(
        note.contains("1 paper already in Your writing stayed"),
        "{note}"
    );
    assert_eq!(
        fs::read_to_string(target.folder.join("about.md")).unwrap(),
        "Local background.\n"
    );
    // Style notes were not here, so they came from the backup.
    assert_eq!(
        fs::read_to_string(target.folder.join("style.md")).unwrap(),
        "First person plural.\n"
    );
    let restored = researcher::samples(&target.folder).unwrap();
    assert_eq!(restored.len(), 2);
    assert!(restored.contains(&mine));
    // The original added here keeps its own name.
    assert!(target
        .folder
        .join(format!("writing/{}.original.md", paper.id))
        .is_file());
}

#[test]
fn a_full_shelf_takes_no_more_papers_and_says_so() {
    let source = home();
    source.add("from-backup.md", b"Only in the backup.");
    let (path, _) = source.export();
    let target = home();
    for index in 0..researcher::MAX_SAMPLES {
        target.add(
            &format!("p{index}.txt"),
            format!("Paper {index}.").as_bytes(),
        );
    }
    let report = target.import(&path).unwrap();
    let note = &report.portability_note;
    assert!(
        note.contains("About you on this computer was left as it is."),
        "{note}"
    );
    assert!(
        note.contains("1 paper could not be restored because Your writing holds up to 20."),
        "{note}"
    );
    assert_eq!(
        researcher::samples(&target.folder).unwrap().len(),
        researcher::MAX_SAMPLES
    );
}

#[test]
fn a_changed_file_is_refused_before_anything_is_restored() {
    let (source, paper, _) = written();
    let (path, _) = source.export();
    for name in [
        "about-you/about.md".to_string(),
        format!("about-you/writing/{}.json", paper.id),
        format!("about-you/writing/{}.original.md", paper.id),
    ] {
        let copy = source.directory.path().join("changed.pwrx");
        fs::copy(&path, &copy).unwrap();
        rewrite(&copy, |entry, bytes| {
            if entry == name {
                b"changed".to_vec()
            } else {
                bytes
            }
        });
        let target = home();
        let error = target.import(&copy).unwrap_err();
        assert!(
            error.message.contains("size or hash check"),
            "{name}: {}",
            error.message
        );
        assert_nothing_restored(&target);
    }
}

#[test]
fn a_crafted_about_you_list_is_refused_before_anything_is_restored() {
    let (source, paper, _) = written();
    let (path, _) = source.export();
    let entry = |manifest: &mut Value, reference: &str| -> usize {
        manifest["aboutYou"]
            .as_array()
            .unwrap()
            .iter()
            .position(|file| file["originalReference"] == reference)
            .unwrap()
    };
    let record = format!("writing/{}.json", paper.id);
    let text = format!("writing/{}.txt", paper.id);
    type Edit<'a> = Box<dyn Fn(&mut Value) + 'a>;
    let cases: Vec<(&str, Edit)> = vec![
        (
            "a name Pipeline does not write",
            Box::new(|manifest| {
                let index = entry(manifest, "about.md");
                manifest["aboutYou"][index]["originalReference"] = "notes.md".into();
            }),
        ),
        (
            "a path out of the folder",
            Box::new(|manifest| {
                let index = entry(manifest, "about.md");
                manifest["aboutYou"][index]["originalReference"] = "../about.md".into();
            }),
        ),
        (
            "an entry stored somewhere else in the archive",
            Box::new(|manifest| {
                let index = entry(manifest, "about.md");
                manifest["aboutYou"][index]["archivePath"] = "about-you/style.md".into();
            }),
        ),
        (
            "an older format that could not hold About you",
            Box::new(|manifest| manifest["formatVersion"] = 3.into()),
        ),
        (
            "a file listed twice",
            Box::new(|manifest| {
                let index = entry(manifest, "about.md");
                let copy = manifest["aboutYou"][index].clone();
                manifest["aboutYou"].as_array_mut().unwrap().push(copy);
            }),
        ),
        (
            "a file left off the list",
            Box::new(|manifest| {
                let index = entry(manifest, &text);
                manifest["aboutYou"].as_array_mut().unwrap().remove(index);
            }),
        ),
    ];
    for (what, edit) in cases {
        let copy = source.directory.path().join("crafted.pwrx");
        fs::copy(&path, &copy).unwrap();
        edit_manifest(&copy, edit);
        let target = home();
        assert!(target.import(&copy).is_err(), "{what}");
        assert_nothing_restored(&target);
    }
    // The untouched backup still restores.
    let target = home();
    target.import(&path).unwrap();
    assert!(target.folder.join(&record).is_file());
}

#[test]
fn text_and_originals_must_come_with_their_record() {
    let (source, paper, scan) = written();
    let (staging, staged) = source.stage(&mut wide());
    let validate_without = |left_out: &str| {
        let files = staged
            .files
            .iter()
            .filter(|file| file.original_reference != left_out)
            .cloned()
            .collect::<Vec<_>>();
        validate(&files, staging.path(), &mut HashSet::new())
    };
    validate_without("nothing").unwrap();
    // A record that counts words needs its text, and neither a text nor an
    // original may arrive alone.
    assert!(validate_without(&format!("writing/{}.txt", paper.id)).is_err());
    assert!(validate_without(&format!("writing/{}.json", paper.id)).is_err());
    assert!(validate_without(&format!("writing/{}.json", scan.id)).is_err());
    // An original is optional: the backup may have had no room for it.
    validate_without(&format!("writing/{}.original.md", paper.id)).unwrap();

    // An original under another extension than its record gives.
    let mut files = staged.files.clone();
    let original = files
        .iter_mut()
        .find(|file| file.original_reference.ends_with(".original.md"))
        .unwrap();
    let renamed = format!("writing/{}.original.pdf", paper.id);
    fs::rename(
        staging.path().join(&original.archive_path),
        staging.path().join(format!("{PREFIX}/{renamed}")),
    )
    .unwrap();
    original.archive_path = format!("{PREFIX}/{renamed}");
    original.original_reference = renamed;
    assert!(validate(&files, staging.path(), &mut HashSet::new()).is_err());
}

#[test]
fn originals_are_left_out_first_when_the_backup_is_short_of_room() {
    let source = home();
    researcher::save_note(&source.folder, Note::About, "Labor economist.").unwrap();
    let small = source.add("small.txt", b"Short.");
    let large = source.add("large.txt", "A much longer paper. ".repeat(40).as_bytes());
    let (_staging, everything) = source.stage(&mut wide());
    assert_eq!(everything.files.len(), 7);
    assert_eq!(everything.note().as_deref(), Some("About you is included."));
    let size = |suffix: &str| {
        everything
            .files
            .iter()
            .filter(|file| file.original_reference.ends_with(suffix))
            .map(|file| file.size_bytes)
            .sum::<u64>()
    };
    let originals = size(".original.txt");
    let essentials = everything
        .files
        .iter()
        .map(|file| file.size_bytes)
        .sum::<u64>()
        - originals;

    // Room for the notes, records and text, and for the smaller original only.
    let mut budget = Budget {
        bytes: essentials + 100,
        ..wide()
    };
    let (staging, staged) = source.stage(&mut budget);
    let listed = staged
        .files
        .iter()
        .map(|file| file.original_reference.as_str())
        .collect::<Vec<_>>();
    assert_eq!(listed.len(), 6, "{listed:?}");
    assert!(listed.contains(&format!("writing/{}.original.txt", small.id).as_str()));
    assert!(!listed.contains(&format!("writing/{}.original.txt", large.id).as_str()));
    assert!(!staging
        .path()
        .join(format!("{PREFIX}/writing/{}.original.txt", large.id))
        .exists());
    assert_eq!(
        staged.note().as_deref(),
        Some("About you is included. The original files of 1 paper were left out to keep the backup within its size limit; their text is included.")
    );
    // What was used came out of the budget, and what was put back is there.
    assert_eq!(budget.bytes, 100 - "Short.".len() as u64);
    assert_eq!(budget.entries, wide().entries - 6);
    // A backup without that original still restores, with the paper's text.
    validate(&staged.files, staging.path(), &mut HashSet::new()).unwrap();
    let target = home();
    let mut installed = install(&target.store, &staged.files, staging.path()).unwrap();
    installed.commit();
    assert_eq!(
        researcher::samples(&target.folder).unwrap(),
        researcher::samples(&source.folder).unwrap()
    );
    assert!(researcher::read_sample(&target.folder, &large.id, 0, 10).is_ok());

    // With no room at all, nothing is staged and the backup still goes ahead.
    let mut none = Budget {
        entries: 0,
        ..wide()
    };
    let (_staging, staged) = source.stage(&mut none);
    assert!(staged.files.is_empty());
    assert_eq!(
        staged.note().as_deref(),
        Some("About you is included. Your background or style notes could not be added and were left out. 2 papers in Your writing could not be added and were left out.")
    );
}

#[test]
fn a_restore_that_fails_later_takes_back_only_what_it_wrote() {
    let (source, paper, _) = written();
    let (staging, staged) = source.stage(&mut wide());

    let target = home();
    researcher::save_note(&target.folder, Note::About, "Local background.").unwrap();
    let installed = install(&target.store, &staged.files, staging.path()).unwrap();
    assert!(target.folder.join("style.md").is_file());
    assert!(target
        .folder
        .join(format!("writing/{}.json", paper.id))
        .is_file());
    drop(installed);
    assert_eq!(
        fs::read_to_string(target.folder.join("about.md")).unwrap(),
        "Local background.\n"
    );
    assert_eq!(fs::read_dir(&target.folder).unwrap().count(), 1);

    let empty = home();
    drop(install(&empty.store, &staged.files, staging.path()).unwrap());
    assert!(!empty.folder.exists());
}
