//! Format readers and catalog capture. Stata and Excel fixtures are built in
//! the submodules as bytes, so the tests need neither Stata nor Excel.
mod excel;
mod stata;

use super::*;
use crate::workbench::store::{CreateWorkspaceRequest, Store};
use excel::{workbook, SHEET};
use stata::{repeat_rows, survey};

fn store() -> (tempfile::TempDir, Store, String) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let ws = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Data".into(),
            root: None,
            operation_id: "workspace".into(),
        })
        .unwrap()
        .record
        .id;
    (temp, store, ws)
}
fn local(ws: &str, path: &std::path::Path, operation: &str) -> ImportDataset {
    ImportDataset {
        workspace_id: ws.into(),
        title: "Survey".into(),
        path: path.to_string_lossy().into_owned(),
        acquisition: DataAcquisition {
            provider: "local import".into(),
            source: "fixture".into(),
            retrieved_at: crate::workbench::desk::now(),
            requested_vintage: None,
            returned_vintage: None,
            series_ids: vec![],
            units: None,
            frequency: None,
            transformation: None,
        },
        supersedes: None,
        operation_id: operation.into(),
        sheet: None,
    }
}

#[test]
fn stata_and_excel_captures_feed_dictionary_policy_and_previews() {
    let (temp, store, ws) = store();
    let path = temp.path().join("survey.dta");
    std::fs::write(&path, survey(118, false).modern()).unwrap();
    let record = import(&store, local(&ws, &path, "dta")).unwrap();
    let version: DatasetVersion = serde_json::from_value(record.body.clone()).unwrap();
    assert!(version.artifact_id.is_some());
    assert_eq!((version.delimiter.as_str(), version.rows), ("dta", 4));
    assert_eq!(version.value_labels[0].name, "sexlbl");
    let body = serde_json::to_string(&record.body).unwrap();
    let hidden = dictionary_text(&body, &DataPolicy::default()).unwrap();
    assert!(hidden.contains("Refused") && hidden.contains("Household income"));
    let columns =
        |text: &str| serde_json::from_str::<serde_json::Value>(text).unwrap()["columns"].clone();
    for c in columns(&hidden).as_array().unwrap() {
        assert!(
            c.get("missing").is_none() && c.get("missingCodes").is_none(),
            "summaries are off"
        );
    }
    let shown = dictionary_text(
        &body,
        &DataPolicy {
            summaries: true,
            ..DataPolicy::default()
        },
    )
    .unwrap();
    assert_eq!(columns(&shown)[1]["missingCodes"][".b"], 1);
    let preview = preview_rows(&store, &ws, &record.id, 0, false).unwrap();
    assert_eq!(preview.total_rows, 4);
    assert!(preview.missing_codes.iter().any(|m| m.code == ".a"));
    assert!(
        preview_rows(&store, &ws, &record.id, 0, true).is_err(),
        "assistant rows stay off"
    );

    let book = temp.path().join("book.xlsx");
    std::fs::write(&book, workbook(SHEET, &[])).unwrap();
    let mut request = local(&ws, &book, "xlsx");
    request.sheet = Some("Data".into());
    let record = import(&store, request).unwrap();
    assert_eq!(record.body["delimiter"], "xlsx");
    assert_eq!(record.body["sourceFormat"]["sheet"], "Data");
    let preview = preview_rows(&store, &ws, &record.id, 0, false).unwrap();
    assert_eq!(preview.rows[0][0].as_deref(), Some("France"));

    // CSV dictionaries keep their original shape: no new optional fields appear.
    let csv_path = temp.path().join("plain.csv");
    std::fs::write(&csv_path, "x,y\n1,\n2,3\n").unwrap();
    let csv = import(&store, local(&ws, &csv_path, "csv")).unwrap();
    let mut keys: Vec<_> = csv.body["columns"][0]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "description",
            "inferredType",
            "missing",
            "name",
            "origin",
            "units"
        ]
    );
    assert!(csv.body.get("connector").is_none() && csv.body.get("valueLabels").is_none());
    let other = temp.path().join("data.sav");
    std::fs::write(&other, b"x").unwrap();
    assert!(import(&store, local(&ws, &other, "sav"))
        .unwrap_err()
        .message
        .contains("exporter dictionary"));
}

#[test]
fn large_stata_files_are_catalogued_without_keeping_a_copy() {
    let (temp, store, ws) = store();
    let path = temp.path().join("large.dta");
    // Repeat the four 35-byte records until the file passes the 32 MiB retention limit.
    let times = 33 * 1024 * 1024 / 140 + 1;
    let bytes = repeat_rows(&survey(118, false).modern(), times);
    assert!(bytes.len() > 32 * 1024 * 1024);
    std::fs::write(&path, &bytes).unwrap();
    let prepared = prepare_local(local(&ws, &path, "large")).unwrap();
    let record = commit(&store, prepared).unwrap();
    let version: DatasetVersion = serde_json::from_value(record.body).unwrap();
    assert!(version.artifact_id.is_none());
    assert_eq!(version.content_hash, crate::workbench::desk::hash(&bytes));
    assert_eq!(version.rows, 4 * times);
    assert_eq!(
        version.external_reference.as_deref(),
        Some("large.dta (local file; bytes not retained)")
    );
    assert!(version.diagnostics_coverage.contains("not retained"));
    assert_eq!(version.columns[1].missing_codes[".b"], times);
    assert!(preview_rows(&store, &ws, &record.id, 0, false).is_err());
    let blobs = std::fs::read_dir(store.root_path().join("blobs")).map_or(0, |d| d.count());
    assert_eq!(blobs, 0, "dictionary-only captures write no blob");
}

#[test]
fn csv_preserves_missing_unicode_and_newlines() {
    let rows = csv::parse("name,x\r\n\"α,β\",\r\n\"two\nlines\",0\r\n", b',').unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1], vec!["α,β", ""]);
    assert_eq!(rows[2], vec!["two\nlines", "0"]);
    assert!(csv::missing(&rows[1][1]));
    assert!(!csv::missing(&rows[2][1]));
    assert!(csv::parse("x,y\n1", b',').is_err());
    assert!(csv::parse("x\n\"bad", b',').is_err());
}

#[test]
fn csv_windows_page_rows_and_keep_missing_cells_missing() {
    let mut text = String::from("name,value\n");
    for row in 0..450 {
        text.push_str(&format!("r{row},{row}\n"));
    }
    let first = super::csv::window(text.as_bytes(), "csv", 0).unwrap();
    assert_eq!(first.columns, ["name", "value"]);
    let last = super::csv::window(text.as_bytes(), "csv", 440).unwrap();
    assert_eq!(last.rows.len(), 10);
    assert_eq!(last.rows[0][0].as_deref(), Some("r440"));
    let duplicate = super::csv::window(b"a,a\n1,NA\n", "csv", 0).unwrap();
    assert_eq!(duplicate.rows[0], [Some("1".to_string()), None]);
}

#[test]
fn csv_within_the_stated_limits_imports_and_previews_past_two_million_cells() {
    let (temp, store, ws) = store();
    let path = temp.path().join("panel.csv");
    let names = (0..10).map(|c| format!("c{c}")).collect::<Vec<_>>();
    let mut text = names.join(",") + "\n";
    for row in 0..300_000 {
        text.push_str(&format!("r{row},{row},,3,4,5,6,7,8,{}.5\n", row % 7));
    }
    assert!(text.len() < 32 * 1024 * 1024);
    std::fs::write(&path, &text).unwrap();
    let record = import(&store, local(&ws, &path, "cells")).unwrap();
    let version: DatasetVersion = serde_json::from_value(record.body.clone()).unwrap();
    assert_eq!((version.rows, version.columns.len()), (300_000, 10));
    assert_eq!(version.columns[0].inferred_type, "text");
    assert_eq!(version.columns[1].inferred_type, "number");
    assert_eq!(
        (
            version.columns[2].inferred_type.as_str(),
            version.columns[2].missing
        ),
        ("unknown", 300_000)
    );
    assert_eq!(version.columns[9].inferred_type, "number");
    let page = preview_rows(&store, &ws, &record.id, 250_000, false).unwrap();
    assert_eq!((page.start, page.total_rows), (250_000, 300_000));
    assert_eq!(page.rows.len(), 25);
    let first: Vec<_> = page.rows[0].iter().map(|c| c.as_deref()).collect();
    assert_eq!(
        first,
        [
            Some("r250000"),
            Some("250000"),
            None,
            Some("3"),
            Some("4"),
            Some("5"),
            Some("6"),
            Some("7"),
            Some("8"),
            Some("2.5"),
        ]
    );
    assert_eq!(page.rows[24][0].as_deref(), Some("r250024"));
    let past = preview_rows(&store, &ws, &record.id, 300_000, false).unwrap();
    assert!(past.rows.is_empty());
}

#[test]
fn csv_profile_enforces_the_row_limit_and_rejects_ragged_rows() {
    let exact = "x\n".to_string() + &"1\n".repeat(1_000_000);
    assert_eq!(
        csv::profile(exact.as_bytes(), "csv").unwrap().rows,
        1_000_000
    );
    let over = exact + "1\n";
    assert!(csv::profile(over.as_bytes(), "csv")
        .unwrap_err()
        .message
        .contains("one million rows"));
    assert!(csv::profile(b"a,b\n1,2\n3\n", "csv")
        .unwrap_err()
        .message
        .contains("row 3"));
    assert!(csv::window(b"a,b\n1,2\n3\n", "csv", 0).is_err());
    assert!(csv::profile(b"", "csv").is_err());
    let tsv = csv::profile(b"a\tb\n1\t\n", "tsv").unwrap();
    assert_eq!(
        (tsv.rows, tsv.columns[1].inferred_type.as_str()),
        (1, "unknown")
    );
}

#[test]
fn a_truncated_dataset_blob_is_repaired_by_the_next_import() {
    let (temp, store, ws) = store();
    let path = temp.path().join("wages.csv");
    let text = "id,wage\n1,12.5\n2,14.25\n";
    std::fs::write(&path, text).unwrap();
    let digest = crate::workbench::desk::hash(text.as_bytes());
    let blob = store
        .root_path()
        .join("blobs")
        .join(format!("{digest}.csv"));
    std::fs::write(&blob, &text.as_bytes()[..9]).unwrap();
    let record = import(&store, local(&ws, &path, "repair")).unwrap();
    assert_eq!(std::fs::read_to_string(&blob).unwrap(), text);
    let page = preview_rows(&store, &ws, &record.id, 1, false).unwrap();
    assert_eq!(page.rows[0][1].as_deref(), Some("14.25"));
}

#[test]
fn csv_ending_in_blank_lines_keeps_its_rows() {
    for text in ["a,b\n1,2\n\n", "a,b\r\n1,2\r\n\r\n\r\n", "a,b\n1,2\n\n\n\n"] {
        assert_eq!(csv::parse(text, b',').unwrap().len(), 2, "{text:?}");
    }
    assert_eq!(csv::profile(b"a,b\n1,2\n\n", "csv").unwrap().rows, 1);
    // In one column an empty line is a value, unless nothing follows it.
    assert_eq!(csv::parse("a\n1\n\n2\n\n", b',').unwrap().len(), 4);
    assert_eq!(csv::parse("a\n1\n\n2", b',').unwrap().len(), 4);
    // Quotes make it a cell, not a blank line.
    assert_eq!(csv::parse("a\n\"\"\n", b',').unwrap().len(), 2);
    assert!(csv::parse("\n\n", b',').is_err());
}

#[test]
fn csv_errors_name_the_row_and_the_encoding() {
    let ragged = csv::profile(b"a,b\n1,2\n3\n4,5\n", "csv").unwrap_err();
    assert_eq!(
        ragged.message,
        "CSV row 3 has 1 column, but the header has 2"
    );
    let gap = csv::profile(b"a,b\n1,2\n\n4,5\n", "csv").unwrap_err();
    assert!(gap.message.contains("row 3"), "{}", gap.message);
    let excel = csv::profile(b"name\ncaf\xe9\n", "csv").unwrap_err();
    assert!(excel.message.contains("Windows-1252"), "{}", excel.message);
    assert!(!excel.message.contains("utf-8 sequence"));
    let wide = csv::window(b"\xff\xfea\0", "csv", 0).unwrap_err();
    assert!(wide.message.contains("UTF-16"), "{}", wide.message);
}
