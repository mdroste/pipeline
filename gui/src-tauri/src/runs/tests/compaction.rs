use super::*;
use crate::runs::{RunFinishMeta, RunWriter};

/// Numbers in the shape of real data: they compress to about half, so the
/// compressed copies are still large enough to be linked.
fn noisy(seed: u64, lines: usize, line: impl Fn(u64) -> String) -> Vec<u8> {
    let mut state = seed;
    (0..lines)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            line(state)
        })
        .collect::<String>()
        .into_bytes()
}

fn bundle() -> Vec<u8> {
    noisy(7, 12_000, |n| {
        format!("{{\"id\":{},\"value\":{}}},", n >> 44, n % 1_000_000)
    })
}

fn data() -> Vec<u8> {
    noisy(11, 16_000, |n| {
        format!(
            "{},{},{}\n",
            1_000 + (n >> 40) % 3_000,
            2_000 + (n >> 20) % 25,
            n % 1_000_000
        )
    })
}

/// A finished run as a review leaves it, with `extra` files besides.
fn finished_run(root: &Path, run_id: &str, extra: &[(&str, &[u8])]) -> PathBuf {
    let mut writer = RunWriter::create_in(root, run_id).unwrap();
    writer
        .add_bytes(
            "context/document_bundle.json",
            "Document",
            "context",
            &bundle(),
        )
        .unwrap();
    writer
        .add_bytes(
            "source-snapshot/tree/data/counties.csv",
            "counties.csv",
            "source",
            &data(),
        )
        .unwrap();
    for (rel, bytes) in extra {
        writer.add_bytes(rel, rel, "source", bytes).unwrap();
    }
    writer
        .add_bytes("report.json", "Report data", "context", b"{}")
        .unwrap();
    let dir = writer.dir().to_path_buf();
    writer
        .finish(RunFinishMeta {
            status: "done".into(),
            ..Default::default()
        })
        .unwrap();
    // A capture keeps the selected file's modified time.
    fs::File::options()
        .write(true)
        .open(dir.join("source-snapshot/tree/data/counties.csv"))
        .unwrap()
        .set_modified(captured_at())
        .unwrap();
    dir
}

fn captured_at() -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000)
}

fn original(dir: &Path, rel: &str) -> Vec<u8> {
    stored::read_run_file_bytes(dir, rel, 64 * 1024 * 1024)
        .unwrap()
        .unwrap()
}

#[test]
fn a_finished_run_keeps_large_files_compressed_and_reads_them_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let own = b"a researcher's own archive".repeat(20_000);
    let run = finished_run(
        temp.path(),
        "a_first",
        &[("source-snapshot/tree/raw.csv.gz", &own)],
    );

    assert_eq!(compact_runs_in(temp.path(), |_| false).unwrap(), 1);

    let record = stored::read_record(&run).unwrap().unwrap();
    assert_eq!(record.version, VERSION);
    assert!(record.compressed.contains("context/document_bundle.json"));
    assert!(record
        .compressed
        .contains("source-snapshot/tree/data/counties.csv"));
    assert!(!run.join("context/document_bundle.json").exists());
    assert!(run.join("context/document_bundle.json.gz").is_file());
    assert!(
        fs::metadata(run.join("context/document_bundle.json.gz"))
            .unwrap()
            .len()
            <= bundle().len() as u64 * 4 / 5
    );
    assert_eq!(original(&run, "context/document_bundle.json"), bundle());
    assert_eq!(
        original(&run, "source-snapshot/tree/data/counties.csv"),
        data()
    );
    // The folder's own `.gz` file is a captured file, not a compressed copy.
    assert_eq!(
        fs::read(run.join("source-snapshot/tree/raw.csv.gz")).unwrap(),
        own
    );
    assert!(!run.join(SCRATCH_DIR).exists());

    // A second pass finds nothing left to do.
    assert_eq!(compact_runs_in(temp.path(), |_| false).unwrap(), 0);
}

#[test]
fn identical_files_in_a_later_run_become_links_that_count_once() {
    let temp = tempfile::tempdir().unwrap();
    let first = finished_run(temp.path(), "a_first", &[]);
    let second = finished_run(temp.path(), "b_second", &[]);
    let usage = || {
        crate::runs::retention::run_folders_usage(temp.path(), "Test scan", &mut Default::default())
            .unwrap()
            .1
    };
    let before = usage();

    assert_eq!(compact_runs_in(temp.path(), |_| false).unwrap(), 2);

    for rel in [
        "context/document_bundle.json.gz",
        "source-snapshot/tree/data/counties.csv.gz",
    ] {
        let (left, links) = stored::file_identity(&first.join(rel)).unwrap();
        let (right, _) = stored::file_identity(&second.join(rel)).unwrap();
        assert_eq!(left, right, "{rel} should be one file");
        assert_eq!(links, 2);
    }
    assert_eq!(
        original(&second, "source-snapshot/tree/data/counties.csv"),
        data()
    );
    let after = usage();
    let one_copy = fs::metadata(first.join("context/document_bundle.json.gz"))
        .unwrap()
        .len()
        + fs::metadata(first.join("source-snapshot/tree/data/counties.csv.gz"))
            .unwrap()
            .len();
    assert!(after < before / 2, "{after} of {before}");
    // Manifests, records and reports are small; the shared files count once.
    assert!(
        after <= one_copy + 32 * 1024,
        "{after} for one copy of {one_copy}"
    );
}

#[test]
fn a_held_or_unfinished_run_is_left_alone() {
    let temp = tempfile::tempdir().unwrap();
    let held = finished_run(temp.path(), "a_held", &[]);
    let unfinished = finished_run(temp.path(), "b_unfinished", &[]);
    fs::remove_file(unfinished.join("report.json")).unwrap();

    assert_eq!(
        compact_runs_in(temp.path(), |id| id == "a_held").unwrap(),
        0
    );

    for dir in [&held, &unfinished] {
        assert!(dir.join("context/document_bundle.json").is_file());
        assert!(stored::read_record(dir).unwrap().is_none());
    }
}

#[test]
fn a_stopped_run_with_a_report_is_compacted_but_its_checkpoints_stay_unlinked() {
    let temp = tempfile::tempdir().unwrap();
    let checkpoint = data();
    let first = finished_run(
        temp.path(),
        "a_stopped",
        &[("artifacts/checkpoints/001_review.json", &checkpoint)],
    );
    let second = finished_run(
        temp.path(),
        "b_stopped",
        &[("artifacts/checkpoints/001_review.json", &checkpoint)],
    );

    assert_eq!(compact_runs_in(temp.path(), |_| false).unwrap(), 2);

    assert!(second.join("context/document_bundle.json.gz").is_file());
    for dir in [&first, &second] {
        let (_, links) =
            stored::file_identity(&dir.join("artifacts/checkpoints/001_review.json")).unwrap();
        assert_eq!(links, 1);
    }
}

#[test]
fn a_rerun_restores_compressed_source_files_that_match_their_fingerprint() {
    let temp = tempfile::tempdir().unwrap();
    let run = finished_run(temp.path(), "a_first", &[]);
    let tree = run.join("source-snapshot/tree");
    let fingerprint = crate::runs::receipts::source_fingerprint(&tree).unwrap();

    compact_runs_in(temp.path(), |_| false).unwrap();
    assert_ne!(
        crate::runs::receipts::source_fingerprint(&tree).unwrap(),
        fingerprint
    );

    // A re-run copies the parent's tree as it is on disk, then expands it.
    let copy = temp.path().join("restored");
    copy_subtree(&run, "source-snapshot/tree", &copy);
    stored::expand_copy(&run, &copy, "source-snapshot/tree").unwrap();
    assert_eq!(
        crate::runs::receipts::source_fingerprint(&copy.join("source-snapshot/tree")).unwrap(),
        fingerprint
    );
    // So does its modified time, which a folder's inventory fingerprint reads.
    assert_eq!(
        fs::metadata(copy.join("source-snapshot/tree/data/counties.csv"))
            .unwrap()
            .modified()
            .unwrap(),
        captured_at()
    );
}

#[test]
fn captured_files_with_different_times_are_not_linked() {
    let temp = tempfile::tempdir().unwrap();
    let first = finished_run(temp.path(), "a_first", &[]);
    let second = finished_run(temp.path(), "b_second", &[]);
    fs::File::options()
        .write(true)
        .open(second.join("source-snapshot/tree/data/counties.csv"))
        .unwrap()
        .set_modified(captured_at() + std::time::Duration::from_secs(60))
        .unwrap();

    compact_runs_in(temp.path(), |_| false).unwrap();

    let identity = |dir: &Path, rel: &str| stored::file_identity(&dir.join(rel)).unwrap().0;
    let data = "source-snapshot/tree/data/counties.csv.gz";
    let bundle = "context/document_bundle.json.gz";
    assert_ne!(identity(&first, data), identity(&second, data));
    // Derived context files carry no captured time and are still shared.
    assert_eq!(identity(&first, bundle), identity(&second, bundle));
}

/// Copy `run/rel` to `copy/rel` file by file, as a re-run's restore does.
fn copy_subtree(run: &Path, rel: &str, copy: &Path) {
    let mut pending = vec![run.join(rel)];
    while let Some(path) = pending.pop() {
        let target = copy.join(path.strip_prefix(run).unwrap());
        if path.is_dir() {
            fs::create_dir_all(&target).unwrap();
            pending.extend(
                fs::read_dir(&path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else {
            fs::copy(&path, &target).unwrap();
            // As `copy_regular` does.
            let modified = fs::metadata(&path).unwrap().modified().unwrap();
            fs::File::options()
                .write(true)
                .open(&target)
                .unwrap()
                .set_modified(modified)
                .unwrap();
        }
    }
}

/// Compacts a copy of a real run store, then reads back every listed file
/// and restores each captured folder. Point it at a copy, never at the store:
/// `PIPELINE_COMPACTION_PROBE=<copy of runs/> cargo test --lib -- --ignored compaction_probe`
#[test]
#[ignore]
fn compaction_probe() {
    let Ok(root) = std::env::var("PIPELINE_COMPACTION_PROBE") else {
        return;
    };
    // The installed app's folder, so pages render with the Poppler that made them.
    if let Ok(resources) = std::env::var("PIPELINE_PROBE_RESOURCES") {
        crate::env::initialize_resources(Some(PathBuf::from(resources)));
    }
    let root = PathBuf::from(root);
    let usage = || {
        crate::runs::retention::run_folders_usage(&root, "Probe scan", &mut Default::default())
            .unwrap()
    };
    let runs: Vec<(PathBuf, RunManifest)> = fs::read_dir(&root)
        .unwrap()
        .flatten()
        .filter_map(|entry| {
            let json = fs::read_to_string(entry.path().join("manifest.json")).ok()?;
            Some((entry.path(), serde_json::from_str(&json).ok()?))
        })
        .collect();
    // Only files that match their manifest entry now must match afterwards.
    let matching = |dir: &Path, entry: &ArtifactEntry| {
        stored::read_run_file_bytes(dir, &entry.rel_path, 512 << 20)
            .ok()
            .flatten()
            .is_some_and(|bytes| {
                bytes.len() as u64 == entry.bytes && short_sha256(&bytes) == entry.sha256
            })
    };
    let baseline: Vec<Vec<bool>> = runs
        .iter()
        .map(|(dir, manifest)| {
            manifest
                .artifacts
                .iter()
                .map(|e| matching(dir, e))
                .collect()
        })
        .collect();

    let before = usage();
    let started = std::time::Instant::now();
    let changed = compact_runs_in(&root, |_| false).unwrap();
    let after = usage();
    eprintln!(
        "compacted {changed} of {} runs in {:.1?}: {:.1} MB -> {:.1} MB",
        before.0,
        started.elapsed(),
        before.1 as f64 / 1e6,
        after.1 as f64 / 1e6
    );

    for ((dir, manifest), baseline) in runs.iter().zip(baseline) {
        let record = stored::read_record(dir).unwrap().unwrap_or_default();
        eprintln!(
            "{}: {} compressed, {} pages rebuilt from {}",
            dir.file_name().unwrap().to_string_lossy(),
            record.compressed.len(),
            record.pages.as_ref().map_or(0, |pages| pages.paths.len()),
            record
                .pages
                .as_ref()
                .map_or("-", |pages| pages.pdf.as_str()),
        );
        for (entry, matched) in manifest.artifacts.iter().zip(baseline) {
            if matched {
                assert!(matching(dir, entry), "{} changed", entry.rel_path);
            }
        }
        if let Some(pages) = &manifest.page_artifacts {
            for page in 1..=pages.count {
                let rel = pages.rel_path(page).unwrap();
                let bytes = stored::read_run_file_bytes(dir, &rel, 32 << 20)
                    .unwrap()
                    .unwrap();
                assert!(bytes.starts_with(&[0xff, 0xd8, 0xff]), "{rel}");
            }
        }
        let Ok(json) = fs::read_to_string(dir.join("context/source-snapshot.json")) else {
            continue;
        };
        let source: serde_json::Value = serde_json::from_str(&json).unwrap();
        let Some(rel) = source["path"]
            .as_str()
            .filter(|rel| rel.starts_with("source-snapshot/tree"))
        else {
            continue;
        };
        let copy = tempfile::tempdir().unwrap();
        copy_subtree(dir, rel, copy.path());
        stored::expand_copy(dir, copy.path(), rel).unwrap();
        assert_eq!(
            crate::runs::receipts::source_fingerprint(&copy.path().join(rel)).unwrap(),
            source["fingerprint"].as_str().unwrap(),
            "{} restores its captured folder",
            dir.display()
        );
    }
}

/// A PDF with one line of text on each page.
fn pdf(pages: &[&str]) -> Vec<u8> {
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        String::new(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut kids = Vec::new();
    for text in pages {
        let stream = format!("BT /F1 24 Tf 72 700 Td ({text}) Tj ET\n");
        let content = objects.len() + 2;
        kids.push(format!("{} 0 R", objects.len() + 1));
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {content} 0 R >>"
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}endstream",
            stream.len()
        ));
    }
    objects[1] = format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        pages.len()
    );
    let mut pdf = "%PDF-1.4\n".to_string();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
    }
    let xref = pdf.len();
    pdf.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));
    pdf.into_bytes()
}

#[test]
fn page_images_the_pdf_renders_exactly_are_removed_and_rebuilt_on_demand() {
    let temp = tempfile::tempdir().unwrap();
    let source = pdf(&["First page", "Second page"]);
    let pdf_path = temp.path().join("paper.pdf");
    fs::write(&pdf_path, &source).unwrap();
    let rendered = temp.path().join("rendered");
    if crate::pipeline::extract::render_retained_pdf_pages(&pdf_path, &rendered, 2).is_err() {
        return; // Poppler is not installed here.
    }
    let page = |n: u32| fs::read(rendered.join(format!("page-{n}.jpg"))).unwrap();

    let mut writer = RunWriter::create_in(temp.path(), "a_paper").unwrap();
    writer
        .add_bytes("source-snapshot/paper.pdf", "paper.pdf", "source", &source)
        .unwrap();
    writer
        .add_bytes("artifacts/pages/page-1.jpg", "Page 1", "pages", &page(1))
        .unwrap();
    writer
        .add_bytes("artifacts/pages/page-2.jpg", "Page 2", "pages", &page(2))
        .unwrap();
    writer.compact_page_artifacts().unwrap();
    writer
        .add_bytes("report.json", "Report data", "context", b"{}")
        .unwrap();
    let run = writer.dir().to_path_buf();
    writer
        .finish(RunFinishMeta {
            status: "done".into(),
            ..Default::default()
        })
        .unwrap();
    // A second run whose pages its PDF cannot reproduce keeps them.
    let mut kept = RunWriter::create_in(temp.path(), "b_kept").unwrap();
    kept.add_bytes("source-snapshot/paper.pdf", "paper.pdf", "source", &source)
        .unwrap();
    kept.add_bytes(
        "artifacts/pages/page-1.jpg",
        "Page 1",
        "pages",
        b"not this render",
    )
    .unwrap();
    kept.add_bytes("report.json", "Report data", "context", b"{}")
        .unwrap();
    let kept_dir = kept.dir().to_path_buf();
    kept.finish(RunFinishMeta {
        status: "done".into(),
        ..Default::default()
    })
    .unwrap();

    compact_runs_in(temp.path(), |_| false).unwrap();

    assert!(!run.join("artifacts/pages/page-1.jpg").exists());
    let record = stored::read_record(&run).unwrap().unwrap();
    assert_eq!(record.pages.unwrap().pdf, "source-snapshot/paper.pdf");
    assert_eq!(original(&run, "artifacts/pages/page-2.jpg"), page(2));
    let rebuilt = stored::RebuiltPages::render(&run).unwrap().unwrap();
    assert_eq!(
        fs::read(rebuilt.path("artifacts/pages/page-1.jpg").unwrap()).unwrap(),
        page(1)
    );
    assert!(kept_dir.join("artifacts/pages/page-1.jpg").is_file());
    assert!(stored::read_record(&kept_dir)
        .unwrap()
        .unwrap()
        .pages
        .is_none());
}
