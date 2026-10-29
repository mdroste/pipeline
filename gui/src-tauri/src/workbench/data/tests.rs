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
