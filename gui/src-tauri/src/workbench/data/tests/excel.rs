//! `.xlsx` fixtures built with the zip writer, and reader tests.
use super::stata::{codes, column};
use super::*;
use std::io::Write;

fn xlsx(parts: &[(&str, String)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (name, body) in parts {
        zip.start_file(*name, options).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
const NS: &str = r#"xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub(super) fn workbook(sheet1: &str, extra: &[(&str, String)]) -> Vec<u8> {
    let mut parts = vec![
        ("[Content_Types].xml", "<Types/>".to_string()),
        (
            "_rels/.rels",
            format!(
                r#"<Relationships><Relationship Id="rId1" Type="{REL}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
            ),
        ),
        (
            "xl/workbook.xml",
            format!(
                r#"<workbook {NS}><workbookPr date1904="0"/><sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Notes &amp; sources" sheetId="2" r:id="rId2"/></sheets></workbook>"#
            ),
        ),
        (
            "xl/_rels/workbook.xml.rels",
            format!(
                r#"<Relationships><Relationship Id="rId1" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}/worksheet" Target="/xl/worksheets/sheet2.xml"/><Relationship Id="rId3" Type="{REL}/styles" Target="styles.xml"/><Relationship Id="rId4" Type="{REL}/sharedStrings" Target="sharedStrings.xml"/></Relationships>"#
            ),
        ),
        (
            "xl/styles.xml",
            format!(
                r#"<styleSheet {NS}><numFmts count="1"><numFmt numFmtId="164" formatCode="yyyy-mm-dd"/></numFmts><cellStyleXfs><xf numFmtId="49"/></cellStyleXfs><cellXfs count="3"><xf numFmtId="0"/><xf numFmtId="164"/><xf numFmtId="14"/></cellXfs></styleSheet>"#
            ),
        ),
        (
            "xl/sharedStrings.xml",
            format!(
                r#"<sst {NS}><si><t>country</t></si><si><t>gdp</t></si><si><r><t>Fr</t></r><r><t xml:space="preserve">ance</t></r><rPh><t>ふらんす</t></rPh></si><si><t>date</t></si><si><t>NA</t></si><si><t>A&amp;B &#233;</t></si></sst>"#
            ),
        ),
        (
            "xl/worksheets/sheet1.xml",
            format!(r#"<worksheet {NS}><sheetData>{sheet1}</sheetData></worksheet>"#),
        ),
        (
            "xl/worksheets/sheet2.xml",
            format!(
                r#"<worksheet {NS}><sheetData><row r="2"><c r="B2" t="inlineStr"><is><t>note</t></is></c></row><row r="3"><c r="B3" t="inlineStr"><is><t>From the ministry</t></is></c></row></sheetData></worksheet>"#
            ),
        ),
    ];
    for (name, body) in extra {
        parts.retain(|(n, _)| n != name);
        parts.push((name, body.clone()));
    }
    xlsx(&parts)
}
pub(super) const SHEET: &str = concat!(
    r#"<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c><c r="C1" t="s"><v>3</v></c><c r="D1" t="inlineStr"><is><t>flag</t></is></c></row>"#,
    r#"<row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2"><v>2.5</v></c><c r="C2" s="1"><v>45123</v></c><c r="D2" t="b"><v>1</v></c></row>"#,
    r#"<row r="3"><c r="A3" t="inlineStr"><is><t>Peru</t></is></c><c r="B3" t="e"><v>#N/A</v></c><c r="C3" s="1"/><c r="D3" t="b"><v>0</v></c></row>"#,
    r#"<row r="5"><c r="A5" t="s"><v>5</v></c><c r="B5" t="s"><v>4</v></c><c r="C5" s="2"><v>45200</v></c><c r="D5" t="b"><f>TRUE()</f><v>1</v></c></row>"#,
    r#"<row r="7" spans="1:4"/><row r="8"><c r="A8" s="1"/></row>"#,
);

#[test]
fn xlsx_reads_shared_inline_numbers_booleans_errors_and_formats() {
    let bytes = workbook(SHEET, &[]);
    let (p, w) = xlsx::read(&bytes, None, Some(0)).unwrap();
    assert_eq!(p.rows, 4, "gap row counts; trailing empty rows do not");
    let names: Vec<_> = p.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["country", "gdp", "date", "flag"]);
    let gdp = column(&p, "gdp");
    assert_eq!(gdp.inferred_type, "number");
    assert_eq!(
        (gdp.missing, codes(gdp)),
        (3, vec![("#N/A", 1), ("NA", 1), ("empty", 1)])
    );
    let date = column(&p, "date");
    assert_eq!(date.inferred_type, "number");
    assert_eq!(
        date.display_format.as_deref(),
        Some("mm-dd-yy | yyyy-mm-dd")
    );
    assert_eq!(codes(date), vec![("empty", 2)]);
    assert_eq!(column(&p, "flag").inferred_type, "boolean");
    assert_eq!(column(&p, "country").inferred_type, "text");
    let f = p.source_format.as_ref().unwrap();
    assert_eq!(f.sheet.as_deref(), Some("Data"));
    assert_eq!(f.sheets, vec!["Data".to_string(), "Notes & sources".into()]);
    assert_eq!(f.date_system.as_deref(), Some("1900"));
    assert!(p
        .limitations
        .iter()
        .any(|l| l.contains("date or time formats: date")));
    assert!(p
        .limitations
        .iter()
        .any(|l| l.contains("1 cells contain formulas")));
    let w = w.unwrap();
    let row = |i: usize| w.rows[i].iter().map(|c| c.as_deref()).collect::<Vec<_>>();
    assert_eq!(
        row(0),
        vec![Some("France"), Some("2.5"), Some("45123"), Some("TRUE")]
    );
    assert_eq!(row(1), vec![Some("Peru"), None, None, Some("FALSE")]);
    assert_eq!(row(2), vec![None, None, None, None]);
    assert_eq!(
        row(3),
        vec![Some("A&B é"), None, Some("45200"), Some("TRUE")]
    );
    assert_eq!(
        w.missing_codes,
        vec![
            MissingCell {
                row: 1,
                column: 1,
                code: "#N/A".into()
            },
            MissingCell {
                row: 3,
                column: 1,
                code: "NA".into()
            },
        ]
    );
    let (_, later) = xlsx::read(&bytes, None, Some(2)).unwrap();
    let later = later.unwrap();
    assert_eq!(later.rows.len(), 2);
    assert_eq!(later.rows[1][0].as_deref(), Some("A&B é"));
    assert_eq!(later.missing_codes[0].row, 1);
    let (notes, _) = xlsx::read(&bytes, Some("Notes & sources"), None).unwrap();
    assert_eq!((notes.rows, notes.columns[0].name.as_str()), (1, "note"));
    let wrong = xlsx::read(&bytes, Some("Missing"), None)
        .unwrap_err()
        .message;
    assert!(wrong.contains("Data, Notes & sources"));
    let (p1904, _) = xlsx::read(
        &workbook(SHEET, &[("xl/workbook.xml", format!(r#"<workbook {NS}><workbookPr date1904="1"/><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#))]),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        p1904.source_format.unwrap().date_system.as_deref(),
        Some("1904")
    );
}

#[test]
fn xlsx_refuses_malformed_ambiguous_and_oversized_workbooks() {
    let refuse = |sheet: &str| {
        xlsx::read(&workbook(sheet, &[]), None, None)
            .unwrap_err()
            .message
    };
    assert!(
        refuse(r#"<row r="1"><c r="A1" t="s"><v>0</v></c><c r="C1" t="s"><v>1</v></c></row>"#)
            .contains("header cell B1 is empty")
    );
    assert!(refuse(
        r#"<row r="1"><c r="A1" t="s"><v>0</v></c></row><row r="2"><c r="B2"><v>1</v></c></row>"#
    )
    .contains("outside the header"));
    assert!(
        refuse(r#"<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>0</v></c></row>"#)
            .contains("appears twice")
    );
    assert!(refuse(r#"<row r="1"><c r="A1" t="s"><v>99</v></c></row>"#)
        .contains("missing shared string"));
    assert!(refuse(r#"<row r="1"><c r="A1"><v>abc</v></c></row>"#).contains("invalid number"));
    assert!(refuse(r#"<row r="2"><c r="A2" t="s"><v>0</v></c></row><row r="1"><c r="A1" t="s"><v>0</v></c></row>"#).contains("out of order"));
    assert!(
        refuse(r#"<row r="1"><c r="B1" t="s"><v>0</v></c><c r="A1" t="s"><v>1</v></c></row>"#)
            .contains("reorders")
    );
    assert!(refuse("").contains("no header row"));
    assert!(refuse(r#"<row r="1"><c r="A1" t="s"><v>0</v>"#).contains("well-formed"));
    let dtd = workbook(
        SHEET,
        &[(
            "xl/sharedStrings.xml",
            r#"<!DOCTYPE sst [<!ENTITY x "y">]><sst/>"#.into(),
        )],
    );
    assert!(xlsx::read(&dtd, None, None)
        .unwrap_err()
        .message
        .contains("DTD"));
    assert!(xlsx::read(b"not a zip", None, None)
        .unwrap_err()
        .message
        .contains("not an .xlsx"));
    let no_book = xlsx(&[("_rels/.rels", "<Relationships/>".into())]);
    assert!(xlsx::read(&no_book, None, None)
        .unwrap_err()
        .message
        .contains("no workbook"));
    // A small archive whose workbook part expands past its 16 MiB limit.
    let bomb = workbook(SHEET, &[("xl/workbook.xml", " ".repeat(17 * 1024 * 1024))]);
    assert!(bomb.len() < 1024 * 1024);
    assert!(xlsx::read(&bomb, None, None)
        .unwrap_err()
        .message
        .contains("limit"));
    assert!(xlsx::is_date_format("[$-409]d mmm yyyy;@"));
    assert!(xlsx::is_date_format("[h]:mm:ss"));
    assert!(!xlsx::is_date_format("#,##0.00 \"days\""));
    assert!(!xlsx::is_date_format("[Red]0.00"));
}

/// Written by openpyxl with its built-in formats, so Comma and Currency cells
/// carry `numFmtId` 43 and 44 with no `numFmt` entry, as Excel saves them.
const FINANCE: &[u8] = include_bytes!("../../../../tests/fixtures/data/openpyxl-finance.xlsx");

#[test]
fn comma_and_currency_columns_are_numbers_and_only_real_dates_are_flagged() {
    let (p, w) = xlsx::read(FINANCE, Some("Finance"), Some(0)).unwrap();
    assert_eq!(p.rows, 2);
    let dates: Vec<_> = p
        .limitations
        .iter()
        .filter(|l| l.starts_with("Columns with date"))
        .collect();
    assert_eq!(dates, ["Columns with date or time formats: opened."]);
    assert_eq!(
        column(&p, "revenue").display_format.as_deref(),
        Some(r#"_(* #,##0.00_);_(* \(#,##0.00\);_(* "-"??_);_(@_)"#)
    );
    assert_eq!(
        column(&p, "cost").display_format.as_deref(),
        Some(r#"_("$"* #,##0.00_);_("$"* \(#,##0.00\);_("$"* "-"??_);_(@_)"#)
    );
    assert_eq!(
        column(&p, "units").display_format.as_deref(),
        Some(r#"_(* #,##0_);_(* \(#,##0\);_(* "-"_);_(@_)"#)
    );
    assert_eq!(
        column(&p, "price").display_format.as_deref(),
        Some(r##""$"#,##0.00_);[Red]\("$"#,##0.00\)"##)
    );
    assert_eq!(
        column(&p, "opened").display_format.as_deref(),
        Some("mm-dd-yy")
    );
    for name in ["revenue", "cost", "units", "price", "opened", "share"] {
        assert_eq!(column(&p, name).inferred_type, "number", "{name}");
    }
    let row = |i: usize| {
        w.as_ref().unwrap().rows[i]
            .iter()
            .map(|c| c.as_deref().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        row(0),
        ["2019", "1234.5", "987.25", "1500", "12.5", "43525", "0.125"]
    );
    assert_eq!(
        row(1),
        ["2020", "2345.75", "1100.5", "1625", "13.75", "43892", "0.25"]
    );
}

#[test]
fn unnamed_builtin_formats_are_dates_only_in_their_reserved_ranges() {
    let cell = |id: u32| {
        format!(
            r#"<styleSheet {NS}><cellXfs count="2"><xf numFmtId="0"/><xf numFmtId="{id}"/></cellXfs></styleSheet>"#
        )
    };
    let sheet = concat!(
        r#"<row r="1"><c r="A1" t="inlineStr"><is><t>value</t></is></c></row>"#,
        r#"<row r="2"><c r="A2" s="1"><v>45123</v></c></row>"#,
    );
    let flagged = |id: u32| {
        let book = workbook(sheet, &[("xl/styles.xml", cell(id))]);
        let (p, _) = xlsx::read(&book, None, None).unwrap();
        assert_eq!(
            column(&p, "value").display_format,
            Some(format!("built-in format {id}"))
        );
        p.limitations
            .iter()
            .any(|l| l == "Columns with date or time formats: value.")
    };
    for id in [27, 31, 36, 50, 58, 71, 81] {
        assert!(flagged(id), "{id} is a locale date format");
    }
    for id in [23, 26, 237, 59, 66, 70, 82, 163] {
        assert!(!flagged(id), "{id} is not a known date format");
    }
}

#[test]
fn viewer_pages_keep_blank_rows_past_the_preview_length() {
    let page = |start| file_table(FINANCE, FINANCE.len() as u64, "xlsx", Some("Paged"), start);
    let first = page(0).unwrap();
    assert_eq!((first.total_rows, first.rows.len()), (240, 200));
    assert!(serde_json::to_value(&first)
        .unwrap()
        .get("preview")
        .is_none());
    let cells = |row: usize| {
        first.rows[row]
            .iter()
            .map(|c| c.as_deref())
            .collect::<Vec<_>>()
    };
    assert_eq!(cells(99), [Some("r99"), Some("148.5")]);
    assert_eq!(cells(100), [None, None]);
    assert_eq!(cells(101), [Some("r101"), Some("151.5")]);
    assert_eq!(cells(150), [None, None]);
    assert_eq!(cells(151), [Some("r151"), Some("226.5")]);
    assert_eq!(cells(199), [Some("r199"), Some("298.5")]);
    // A page that opens inside the gap keeps the blank row at its own offset.
    let inside = page(100).unwrap();
    assert_eq!(inside.rows[0], [None, None]);
    assert_eq!(inside.rows[1][0].as_deref(), Some("r101"));
    assert_eq!(inside.rows[50], [None, None]);
    assert_eq!(inside.rows[139][0].as_deref(), Some("r239"));
    assert_eq!(inside.rows.len(), 140);
}
