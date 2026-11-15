use super::*;
use zip::write::SimpleFileOptions;

fn deck(dir: &Path, target: &str) -> PathBuf {
    let path = dir.join("deck.pptx");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let rels = format!(
        r#"<Relationships><Relationship Id="b" Type="package/slide" Target="{target}"/><Relationship Id="a" Type="package/slide" Target="slides/slide1.xml"/></Relationships>"#
    );
    for (name, content) in [
        (
            "ppt/presentation.xml",
            r#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="42" r:id="b"/><p:sldId id="7" r:id="a"/></p:sldIdLst></p:presentation>"#,
        ),
        ("ppt/_rels/presentation.xml.rels", &rels),
        (
            "ppt/slides/slide1.xml",
            r#"<p:sld xmlns:p="p" xmlns:a="a"><a:p><a:r><a:t>Second &amp; last</a:t></a:r></a:p></p:sld>"#,
        ),
        (
            "ppt/slides/slide9.xml",
            r#"<p:sld xmlns:p="p" xmlns:a="a"><a:p><a:r><a:t>First slide</a:t></a:r></a:p><a:tbl><a:tc><a:p><a:t>Cell</a:t></a:p></a:tc></a:tbl></p:sld>"#,
        ),
        (
            "ppt/slides/_rels/slide9.xml.rels",
            r#"<Relationships><Relationship Id="notes" Type="package/notesSlide" Target="../notesSlides/notesSlide1.xml"/><Relationship Id="image" Type="package/image" Target="../media/image1.png"/><Relationship Id="web" Type="package/image" Target="https://example.invalid/a.png" TargetMode="External"/></Relationships>"#,
        ),
        (
            "ppt/notesSlides/notesSlide1.xml",
            r#"<notes xmlns:a="a"><a:p><a:t>Explain the estimate</a:t></a:p></notes>"#,
        ),
    ] {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.start_file("ppt/media/image1.png", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x10\0\0\0\x10")
        .unwrap();
    zip.finish().unwrap();
    path
}

#[test]
fn pptx_preserves_presentation_order_notes_and_slide_images() {
    let dir = tempfile::tempdir().unwrap();
    let path = deck(dir.path(), "slides/slide9.xml");
    let text = extract_pptx_text(&path).unwrap();
    assert!(text.find("First slide").unwrap() < text.find("Second & last").unwrap());
    assert!(text.contains("Cell"));
    assert!(text.contains("Speaker notes (not visible on slide):\nExplain the estimate"));
    let extraction = ExtractionResult {
        text,
        method: "pptx".into(),
        source_path: path.to_string_lossy().into_owned(),
        paper_hash: "fixture".into(),
        quality_notes: vec![],
    };
    let run = tempfile::tempdir().unwrap();
    let built = build(&extraction, run.path()).unwrap();
    assert_eq!(built.bundle.source_kind, "pptx");
    assert_eq!(built.bundle.pages.len(), 2);
    assert_eq!(built.added_artifacts.len(), 1);
    assert_eq!(built.added_artifacts[0].2, "figures");
    assert!(run.path().join(&built.added_artifacts[0].0).is_file());
    let image = &built.bundle.assets[0];
    assert_eq!(image.page, Some(1));
    assert_eq!(image.width, Some(16));
    assert!(built
        .bundle
        .nodes
        .iter()
        .any(|n| n.page == Some(1) && n.asset_ids.contains(&image.id)));
    built.bundle.validate().unwrap();
}

#[test]
fn pptx_rejects_escaping_targets_and_malformed_or_excessive_xml() {
    let dir = tempfile::tempdir().unwrap();
    let path = deck(dir.path(), "../../secret.xml");
    assert!(extract_pptx_text(&path).unwrap_err().contains("Unsafe"));
    assert!(text("<!DOCTYPE slide><slide/>").is_err());
    assert!(text("<slide><t>unclosed").is_err());
    assert!(text(&format!("{}{}", "<a>".repeat(129), "</a>".repeat(129))).is_err());
    let path = deck(dir.path(), "slides/missing.xml");
    assert!(extract_pptx_text(&path)
        .unwrap_err()
        .contains("Missing PowerPoint part"));
    let mut archive = open(&path).unwrap();
    assert!(xml(&mut archive, "ppt/presentation.xml", &mut 10).is_err());
}

#[tokio::test]
async fn pptx_passes_staging_dispatch_and_batch_discovery() {
    let dir = tempfile::tempdir().unwrap();
    let path = deck(dir.path(), "slides/slide9.xml");
    let found = crate::commands::scan_input_files(dir.path().to_str().unwrap()).unwrap();
    assert_eq!(found, vec![path.to_string_lossy().into_owned()]);
    let private = tempfile::tempdir().unwrap();
    let scoped =
        crate::pipeline::extract::stage_selected_source(&path, "document", private.path()).unwrap();
    let source = scoped.source_path.unwrap();
    let app: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    let result = crate::pipeline::extract::extract(
        &app,
        source.to_str().unwrap(),
        &crate::pipeline_config::ExtractionConfig::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.method, "pptx");
    assert!(result.text.contains("First slide"));
    assert!(result.quality_notes[0].contains("not rendered"));
}
