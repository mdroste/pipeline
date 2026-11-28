use super::*;

fn folder() -> (tempfile::TempDir, PathBuf) {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path().join("researcher");
    (temporary, folder)
}

fn paper(directory: &Path, name: &str, text: &str) -> PathBuf {
    let path = directory.join(name);
    fs::write(&path, text).unwrap();
    path
}

#[test]
fn an_empty_folder_is_an_empty_profile() {
    let (_temporary, folder) = folder();
    let profile = load(&folder).unwrap();
    assert_eq!(profile.about, "");
    assert_eq!(profile.style, "");
    assert!(profile.samples.is_empty());
    assert_eq!(background(&folder), None);
    // Reading never creates the folder.
    assert!(!folder.exists());
}

#[test]
fn a_note_is_saved_as_plain_text_and_cleared_by_saving_nothing() {
    let (_temporary, folder) = folder();
    let saved = save_note(
        &folder,
        Note::About,
        "  Labor economist.\r\nBritish spelling.  ",
    )
    .unwrap();
    assert_eq!(saved, "Labor economist.\nBritish spelling.");
    assert_eq!(
        fs::read_to_string(folder.join("about.md")).unwrap(),
        "Labor economist.\nBritish spelling.\n"
    );
    assert_eq!(background(&folder).as_deref(), Some(saved.as_str()));
    assert_eq!(load(&folder).unwrap().style, "");

    save_note(&folder, Note::Style, "First person plural.").unwrap();
    assert_eq!(load(&folder).unwrap().style, "First person plural.");
    // Style notes are not the background conversations receive.
    assert_eq!(background(&folder).as_deref(), Some(saved.as_str()));

    assert_eq!(save_note(&folder, Note::About, " \n ").unwrap(), "");
    assert!(!folder.join("about.md").exists());
    assert_eq!(background(&folder), None);
    // Clearing a note that is already clear is not an error.
    assert_eq!(save_note(&folder, Note::About, "").unwrap(), "");
}

#[test]
fn a_note_over_the_limit_is_refused_and_the_saved_one_kept() {
    let (_temporary, folder) = folder();
    save_note(&folder, Note::About, "Economist.").unwrap();
    let error = save_note(&folder, Note::About, &"a".repeat(MAX_NOTE_BYTES + 1)).unwrap_err();
    assert!(error.contains("Shorten Background"), "{error}");
    assert!(save_note(&folder, Note::About, "bad\0text").is_err());
    assert_eq!(background(&folder).as_deref(), Some("Economist."));
    save_note(&folder, Note::About, &"a".repeat(MAX_NOTE_BYTES)).unwrap();
}

#[test]
fn a_note_lengthened_outside_pipeline_is_shown_whole_and_sent_in_part() {
    let (_temporary, folder) = folder();
    fs::create_dir_all(&folder).unwrap();
    // Two-byte characters, so the limit falls inside one.
    let long = "é".repeat(MAX_NOTE_BYTES);
    fs::write(folder.join("about.md"), &long).unwrap();
    assert_eq!(load(&folder).unwrap().about, long);
    let sent = background(&folder).unwrap();
    assert!(sent.len() <= MAX_NOTE_BYTES);
    assert!(long.starts_with(&sent));
}

#[test]
fn adding_a_paper_keeps_the_file_and_its_text() {
    let (temporary, folder) = folder();
    let source = paper(
        temporary.path(),
        "minimum_wages-final.tex",
        "We study minimum wages.\r\nFirms enter less often.",
    );
    let sample = add_sample(&folder, &source).unwrap();
    assert_eq!(sample.title, "minimum wages final");
    assert_eq!(sample.kind, "paper");
    assert_eq!(sample.file_name, "minimum_wages-final.tex");
    assert_eq!(sample.words, Some(8));
    assert!(sample.use_as_example);
    assert_eq!(sample.id.len(), 16);
    assert!(sample.content_hash.starts_with(&sample.id));
    let writing = folder.join("writing");
    assert_eq!(
        fs::read(writing.join(format!("{}.original.tex", sample.id))).unwrap(),
        fs::read(&source).unwrap()
    );
    assert_eq!(samples(&folder).unwrap(), vec![sample.clone()]);

    let read = read_sample(&folder, &sample.id, 0, 23).unwrap();
    assert_eq!(read.text, "We study minimum wages.");
    assert_eq!((read.start, read.end), (0, 23));
    let rest = read_sample(&folder, &sample.id, read.end, MAX_READ_BYTES * 2).unwrap();
    assert_eq!(rest.text, "\nFirms enter less often.");
    assert_eq!(rest.end, rest.total);

    // The same file again, under any name, is the same sample.
    let renamed = paper(
        temporary.path(),
        "copy.tex",
        "We study minimum wages.\r\nFirms enter less often.",
    );
    assert_eq!(add_sample(&folder, &renamed).unwrap(), sample);
    assert_eq!(samples(&folder).unwrap().len(), 1);
    // No temporary files are left beside the sample.
    assert_eq!(fs::read_dir(&writing).unwrap().count(), 3);
}

#[test]
fn only_prose_formats_are_accepted() {
    let (temporary, folder) = folder();
    let data = paper(temporary.path(), "data.csv", "a,b\n1,2\n");
    let error = add_sample(&folder, &data).unwrap_err();
    assert!(error.contains("PDF, Word, TeX, Markdown"), "{error}");
    assert!(add_sample(&folder, &temporary.path().join("missing.pdf")).is_err());
    assert!(samples(&folder).unwrap().is_empty());
}

#[test]
fn a_file_with_no_readable_text_is_kept_but_is_not_an_example() {
    let (temporary, folder) = folder();
    let path = temporary.path().join("scan.txt");
    fs::write(&path, [b'a', 0, b'b']).unwrap();
    let sample = add_sample(&folder, &path).unwrap();
    assert_eq!(sample.words, None);
    assert!(!sample.use_as_example);
    assert!(read_sample(&folder, &sample.id, 0, 10).is_err());
    let error = update_sample(
        &folder,
        SampleUpdate {
            id: sample.id.clone(),
            title: "Scan".into(),
            year: None,
            kind: "paper".into(),
            use_as_example: true,
        },
    )
    .unwrap_err();
    assert!(error.contains("no readable text"), "{error}");
}

#[test]
fn latin1_text_is_read_one_byte_to_a_character() {
    let (temporary, folder) = folder();
    let path = temporary.path().join("old.tex");
    fs::write(&path, [b'c', b'a', b'f', 0xe9]).unwrap();
    let sample = add_sample(&folder, &path).unwrap();
    assert_eq!(
        read_sample(&folder, &sample.id, 0, 100).unwrap().text,
        "café"
    );
}

#[test]
fn details_can_be_edited_and_are_checked() {
    let (temporary, folder) = folder();
    let source = paper(temporary.path(), "proposal.md", "Aims and approach.");
    let sample = add_sample(&folder, &source).unwrap();
    let update = |title: &str, year: Option<u16>, kind: &str| SampleUpdate {
        id: sample.id.clone(),
        title: title.into(),
        year,
        kind: kind.into(),
        use_as_example: false,
    };
    let saved = update_sample(&folder, update("  NSF proposal ", Some(2023), "grant")).unwrap();
    assert_eq!(saved.title, "NSF proposal");
    assert_eq!(saved.year, Some(2023));
    assert_eq!(saved.kind, "grant");
    assert!(!saved.use_as_example);
    assert_eq!(samples(&folder).unwrap(), vec![saved.clone()]);
    // What the file is does not change with its details.
    assert_eq!(saved.content_hash, sample.content_hash);
    assert_eq!(saved.added_at, sample.added_at);

    assert!(update_sample(&folder, update(" ", None, "paper")).is_err());
    assert!(update_sample(&folder, update("Title", Some(23), "paper")).is_err());
    assert!(update_sample(&folder, update("Title", None, "novel")).is_err());
    assert_eq!(samples(&folder).unwrap(), vec![saved]);
}

#[test]
fn removing_a_sample_removes_only_pipelines_copy() {
    let (temporary, folder) = folder();
    let source = paper(temporary.path(), "paper.txt", "Text.");
    let sample = add_sample(&folder, &source).unwrap();
    remove_sample(&folder, &sample.id).unwrap();
    assert!(samples(&folder).unwrap().is_empty());
    assert_eq!(fs::read_dir(folder.join("writing")).unwrap().count(), 0);
    assert!(source.exists());
    assert!(remove_sample(&folder, &sample.id).is_err());
}

#[test]
fn ids_cannot_name_files_outside_the_folder() {
    let (temporary, folder) = folder();
    fs::write(temporary.path().join("secret.json"), "{}").unwrap();
    for id in [
        "../secret",
        "..\\secret",
        "",
        "ABCDEF0123456789",
        "0123456789abcde",
    ] {
        assert!(read_sample(&folder, id, 0, 10).is_err(), "{id}");
        assert!(remove_sample(&folder, id).is_err(), "{id}");
    }
    assert!(temporary.path().join("secret.json").exists());
}

#[test]
fn files_pipeline_did_not_name_are_ignored() {
    let (temporary, folder) = folder();
    let source = paper(temporary.path(), "paper.txt", "Text.");
    let sample = add_sample(&folder, &source).unwrap();
    let writing = folder.join("writing");
    let record = fs::read(writing.join(format!("{}.json", sample.id))).unwrap();
    // A sync client's conflicted copy, a damaged record, and a record whose
    // name is not its id.
    fs::write(
        writing.join(format!("{} (conflicted copy).json", sample.id)),
        &record,
    )
    .unwrap();
    fs::write(writing.join("0123456789abcdef.json"), "not json").unwrap();
    fs::write(writing.join("fedcba9876543210.json"), &record).unwrap();
    fs::write(folder.join("about (conflicted copy).md"), "Other.").unwrap();
    assert_eq!(samples(&folder).unwrap(), vec![sample]);
    assert_eq!(background(&folder), None);
}

#[test]
fn the_shelf_has_a_limit() {
    let (temporary, folder) = folder();
    for index in 0..MAX_SAMPLES {
        let source = paper(
            temporary.path(),
            &format!("p{index}.txt"),
            &format!("Paper {index}."),
        );
        add_sample(&folder, &source).unwrap();
    }
    let extra = paper(temporary.path(), "extra.txt", "One more.");
    let error = add_sample(&folder, &extra).unwrap_err();
    assert!(error.contains("up to 20"), "{error}");
    assert_eq!(samples(&folder).unwrap().len(), MAX_SAMPLES);
    // The refused file leaves nothing behind.
    assert_eq!(
        fs::read_dir(folder.join("writing")).unwrap().count(),
        MAX_SAMPLES * 3
    );
    // A file already on the shelf is still found when the shelf is full.
    let again = paper(temporary.path(), "again.txt", "Paper 0.");
    add_sample(&folder, &again).unwrap();
}

#[test]
fn the_folder_sits_beside_the_conversation_store() {
    let data = Path::new("/data/pipeline");
    assert_eq!(
        folder_beside(&data.join("workbench")),
        Some(data.join("researcher"))
    );
}

#[test]
fn a_store_anywhere_else_has_no_about_you() {
    // Tests and tools open stores at a temporary folder's root; its parent is
    // shared by everything else there.
    for root in ["/tmp/store-1", "/tmp/workbench-copy", "/"] {
        assert_eq!(folder_beside(Path::new(root)), None, "{root}");
    }
}

fn example(
    folder: &Path,
    temporary: &Path,
    name: &str,
    text: &str,
    kind: &str,
    year: u16,
) -> Sample {
    let added = add_sample(folder, &paper(temporary, name, text)).unwrap();
    update_sample(
        folder,
        SampleUpdate {
            id: added.id,
            title: name.into(),
            year: Some(year),
            kind: kind.into(),
            use_as_example: true,
        },
    )
    .unwrap()
}

#[test]
fn writing_holds_style_notes_and_readable_examples_only() {
    let (temporary, folder) = folder();
    assert!(writing(&folder).is_empty());
    save_note(&folder, Note::Style, "  First person plural.\n").unwrap();
    let kept = example(&folder, temporary.path(), "kept.md", "Kept.", "paper", 2020);
    let unmarked = add_sample(&folder, &paper(temporary.path(), "off.md", "Off.")).unwrap();
    update_sample(
        &folder,
        SampleUpdate {
            id: unmarked.id,
            title: "Off".into(),
            year: None,
            kind: "paper".into(),
            use_as_example: false,
        },
    )
    .unwrap();
    let scan = temporary.path().join("scan.txt");
    fs::write(&scan, [b'a', 0]).unwrap();
    add_sample(&folder, &scan).unwrap();
    let found = writing(&folder);
    assert_eq!(found.style.as_deref(), Some("First person plural."));
    assert_eq!(found.examples, vec![kept.clone()]);
    // Text that has gone missing is not offered.
    fs::remove_file(folder.join(format!("writing/{}.txt", kept.id))).unwrap();
    assert!(writing(&folder).examples.is_empty());
}

#[test]
fn passages_prefer_the_kind_asked_for_then_the_newest() {
    let (temporary, folder) = folder();
    let old = example(
        &folder,
        temporary.path(),
        "old.md",
        "Old paper.",
        "paper",
        2015,
    );
    let grant = example(
        &folder,
        temporary.path(),
        "grant.md",
        "A grant.",
        "grant",
        2023,
    );
    let new = example(
        &folder,
        temporary.path(),
        "new.md",
        "New paper.",
        "paper",
        2024,
    );
    let reply = example(
        &folder,
        temporary.path(),
        "reply.md",
        "A reply.",
        "response",
        2025,
    );
    let examples = vec![old, grant, new, reply];
    let titles = |kind: &str| {
        passages(&folder, &examples, kind)
            .into_iter()
            .map(|passage| passage.title)
            .collect::<Vec<_>>()
    };
    assert_eq!(titles("paper"), ["new.md", "old.md", "reply.md"]);
    assert_eq!(titles("grant"), ["grant.md", "reply.md", "new.md"]);
    let first = &passages(&folder, &examples, "grant")[0];
    assert_eq!(
        (first.kind.as_str(), first.year, first.text.as_str()),
        ("grant", Some(2023), "A grant.")
    );
}

#[test]
fn a_passage_starts_at_the_introduction() {
    for (heading, text) in [
        ("1 Introduction", "Title\nAbstract text.\n\n1 Introduction\nWe study wages.\n"),
        ("I. INTRODUCTION", "Title\nI. INTRODUCTION\nWe study wages.\n"),
        ("# Introduction", "# Title\n\n## Introduction\n\nWe study wages.\n"),
        (
            "section",
            "\\documentclass{article}\n\\begin{document}\n\\section{Introduction}\nWe study wages.\n",
        ),
        ("section*", "\\section*{Introduction}\r\nWe study wages.\r\n"),
    ] {
        assert_eq!(passage(text).replace('\r', ""), "We study wages.", "{heading}");
    }
    // A sentence that mentions the introduction is not a heading.
    assert_eq!(
        passage("In the introduction we argue.\nMore."),
        "In the introduction we argue.\nMore."
    );
    // With no heading, a LaTeX file starts after its preamble.
    assert_eq!(
        passage(
            "\\documentclass{article}\n\\usepackage{x}\n\\begin{document}\nBody text.\n\\end{document}"
        ),
        "Body text.\n\\end{document}"
    );
}

#[test]
fn a_long_passage_ends_at_a_paragraph_or_sentence_within_the_limit() {
    let paragraph = format!("{}.", "word ".repeat(100).trim_end());
    let text = format!("1 Introduction\n{}", [paragraph.as_str(); 20].join("\n\n"));
    let cut = passage(&text);
    assert!(cut.len() <= MAX_PASSAGE_BYTES);
    assert!(cut.ends_with("word."), "{}", &cut[cut.len() - 20..]);
    assert!(cut.len() > MAX_PASSAGE_BYTES / 2);
    // One long paragraph ends at a sentence.
    let sentences = "This is a sentence. ".repeat(400);
    let cut = passage(&sentences);
    assert!(cut.ends_with("sentence.") && cut.len() <= MAX_PASSAGE_BYTES);
    // Text with no break at all is cut on a character boundary.
    let cut = passage(&"é".repeat(MAX_PASSAGE_BYTES));
    assert!(cut.len() <= MAX_PASSAGE_BYTES && cut.chars().all(|c| c == 'é'));
}
