//! Stata `.dta` fixtures written byte by byte, and reader tests.
use super::*;

const DOUBLE_MISSING: u64 = 0x7fe0_0000_0000_0000;
const FLOAT_MISSING: u32 = 0x7f00_0000;
/// Stata long-equivalent code for `.a`–`.z` in value-label tables.
const LONG_MISSING: i64 = 2_147_483_621;

fn put(out: &mut Vec<u8>, value: u64, width: usize, big: bool) {
    let bytes = value.to_le_bytes();
    if big {
        out.extend(bytes[..width].iter().rev());
    } else {
        out.extend(&bytes[..width]);
    }
}
fn fixed(text: &[u8], width: usize) -> Vec<u8> {
    let mut v = text.to_vec();
    v.resize(width, 0);
    v
}
#[derive(Clone)]
struct Var {
    name: &'static str,
    /// Release 117+ type code (legacy writers translate it).
    code: u16,
    format: &'static str,
    labels: &'static str,
    label: &'static str,
}
fn var(name: &'static str, code: u16, format: &'static str) -> Var {
    Var {
        name,
        code,
        format,
        labels: "",
        label: "",
    }
}
#[derive(Clone)]
enum Cell {
    Byte(i64),
    Int(i64),
    Long(i64),
    FloatBits(u32),
    DoubleBits(u64),
    Text(Vec<u8>),
    /// strL `(v, o)` reference.
    Strl(u64, u64),
}
fn double(v: f64) -> Cell {
    Cell::DoubleBits(v.to_bits())
}
fn text(s: &str) -> Cell {
    Cell::Text(s.as_bytes().to_vec())
}
fn width(code: u16) -> usize {
    match code {
        1..=2045 => code as usize,
        32768 | 65526 => 8,
        65527 | 65528 => 4,
        65529 => 2,
        _ => 1,
    }
}
fn encode(cell: &Cell, code: u16, release: u16, big: bool, out: &mut Vec<u8>) {
    match cell {
        Cell::Byte(v) => put(out, *v as u64, 1, big),
        Cell::Int(v) => put(out, *v as u64, 2, big),
        Cell::Long(v) => put(out, *v as u64, 4, big),
        Cell::FloatBits(b) => put(out, u64::from(*b), 4, big),
        Cell::DoubleBits(b) => put(out, *b, 8, big),
        Cell::Text(t) => out.extend(fixed(t, width(code))),
        Cell::Strl(v, o) => {
            let vw = match release {
                117 => 4,
                118 => 2,
                _ => 3,
            };
            put(out, *v, vw, big);
            put(out, *o, 8 - vw, big);
        }
    }
}
fn label_table(entries: &[(i64, &str)], big: bool) -> Vec<u8> {
    let mut text = Vec::new();
    let mut offsets = Vec::new();
    for (_, label) in entries {
        offsets.push(text.len() as u64);
        text.extend(label.as_bytes());
        text.push(0);
    }
    let mut out = Vec::new();
    put(&mut out, entries.len() as u64, 4, big);
    put(&mut out, text.len() as u64, 4, big);
    for o in offsets {
        put(&mut out, o, 4, big);
    }
    for (v, _) in entries {
        put(&mut out, *v as u64, 4, big);
    }
    out.extend(text);
    out
}
pub(super) struct Dta {
    release: u16,
    big: bool,
    vars: Vec<Var>,
    rows: Vec<Vec<Cell>>,
    /// `(v, o, type, content)` long strings.
    strls: Vec<(u64, u64, u8, Vec<u8>)>,
    labels: Vec<(&'static str, Vec<(i64, &'static str)>)>,
}
impl Dta {
    pub(super) fn modern(&self) -> Vec<u8> {
        let (r, big) = (self.release, self.big);
        let (name, fmt, lab) = if r == 117 {
            (33, 49, 81)
        } else {
            (129, 57, 321)
        };
        let k = self.vars.len() as u64;
        let mut out = b"<stata_dta><header><release>".to_vec();
        out.extend(r.to_string().as_bytes());
        out.extend(b"</release><byteorder>");
        out.extend(if big { b"MSF" } else { b"LSF" });
        out.extend(b"</byteorder><K>");
        put(&mut out, k, if r == 119 { 4 } else { 2 }, big);
        out.extend(b"</K><N>");
        put(
            &mut out,
            self.rows.len() as u64,
            if r == 117 { 4 } else { 8 },
            big,
        );
        out.extend(b"</N><label>");
        let label = b"Household survey";
        put(
            &mut out,
            label.len() as u64,
            if r == 117 { 1 } else { 2 },
            big,
        );
        out.extend(label);
        out.extend(b"</label><timestamp>");
        out.push(17);
        out.extend(b"28 Oct 2026 09:30");
        out.extend(b"</timestamp></header>");
        let mut map = [0u64; 14];
        map[1] = out.len() as u64;
        out.extend(b"<map>");
        out.extend([0u8; 14 * 8]);
        out.extend(b"</map>");
        map[2] = out.len() as u64;
        out.extend(b"<variable_types>");
        for v in &self.vars {
            put(&mut out, u64::from(v.code), 2, big);
        }
        out.extend(b"</variable_types>");
        let section = |out: &mut Vec<u8>, tag: &str, items: Vec<Vec<u8>>| {
            out.extend(format!("<{tag}>").as_bytes());
            for item in items {
                out.extend(item);
            }
            out.extend(format!("</{tag}>").as_bytes());
        };
        map[3] = out.len() as u64;
        section(
            &mut out,
            "varnames",
            self.vars
                .iter()
                .map(|v| fixed(v.name.as_bytes(), name))
                .collect(),
        );
        map[4] = out.len() as u64;
        let sort = if r == 119 { 4 } else { 2 };
        section(
            &mut out,
            "sortlist",
            vec![vec![0u8; sort * (k as usize + 1)]],
        );
        map[5] = out.len() as u64;
        section(
            &mut out,
            "formats",
            self.vars
                .iter()
                .map(|v| fixed(v.format.as_bytes(), fmt))
                .collect(),
        );
        map[6] = out.len() as u64;
        section(
            &mut out,
            "value_label_names",
            self.vars
                .iter()
                .map(|v| fixed(v.labels.as_bytes(), name))
                .collect(),
        );
        map[7] = out.len() as u64;
        section(
            &mut out,
            "variable_labels",
            self.vars
                .iter()
                .map(|v| fixed(v.label.as_bytes(), lab))
                .collect(),
        );
        map[8] = out.len() as u64;
        section(&mut out, "characteristics", Vec::new());
        map[9] = out.len() as u64;
        out.extend(b"<data>");
        for row in &self.rows {
            for (cell, v) in row.iter().zip(&self.vars) {
                encode(cell, v.code, r, big, &mut out);
            }
        }
        out.extend(b"</data>");
        map[10] = out.len() as u64;
        out.extend(b"<strls>");
        for (v, o, t, content) in &self.strls {
            out.extend(b"GSO");
            put(&mut out, *v, 4, big);
            put(&mut out, *o, if r == 117 { 4 } else { 8 }, big);
            out.push(*t);
            put(&mut out, content.len() as u64, 4, big);
            out.extend(content);
        }
        out.extend(b"</strls>");
        map[11] = out.len() as u64;
        out.extend(b"<value_labels>");
        for (set, entries) in &self.labels {
            let table = label_table(entries, big);
            out.extend(b"<lbl>");
            put(&mut out, table.len() as u64, 4, big);
            out.extend(fixed(set.as_bytes(), name));
            out.extend([0u8; 3]);
            out.extend(table);
            out.extend(b"</lbl>");
        }
        out.extend(b"</value_labels>");
        map[12] = out.len() as u64;
        out.extend(b"</stata_dta>");
        map[13] = out.len() as u64;
        let at = map[1] as usize + 5;
        for (i, m) in map.iter().enumerate() {
            let mut b = Vec::new();
            put(&mut b, *m, 8, big);
            out[at + 8 * i..at + 8 * i + 8].copy_from_slice(&b);
        }
        out
    }
    /// Releases 114/115 (little-endian).
    fn legacy(&self) -> Vec<u8> {
        let big = false;
        let k = self.vars.len() as u64;
        let mut out = vec![self.release as u8, 2, 1, 0];
        put(&mut out, k, 2, big);
        put(&mut out, self.rows.len() as u64, 4, big);
        out.extend(fixed(b"Legacy survey", 81));
        out.extend(fixed(b"28 Oct 2026 09:30", 18));
        for v in &self.vars {
            out.push(match v.code {
                1..=244 => v.code as u8,
                65530 => 251,
                65529 => 252,
                65528 => 253,
                65527 => 254,
                _ => 255,
            });
        }
        for v in &self.vars {
            out.extend(fixed(v.name.as_bytes(), 33));
        }
        out.extend(vec![0u8; 2 * (k as usize + 1)]);
        for v in &self.vars {
            out.extend(fixed(v.format.as_bytes(), 49));
        }
        for v in &self.vars {
            out.extend(fixed(v.labels.as_bytes(), 33));
        }
        for v in &self.vars {
            out.extend(fixed(v.label.as_bytes(), 81));
        }
        out.extend([0u8; 5]);
        for row in &self.rows {
            for (cell, v) in row.iter().zip(&self.vars) {
                encode(cell, v.code, self.release, big, &mut out);
            }
        }
        for (set, entries) in &self.labels {
            let table = label_table(entries, big);
            put(&mut out, table.len() as u64, 4, big);
            out.extend(fixed(set.as_bytes(), 33));
            out.extend([0u8; 3]);
            out.extend(table);
        }
        out
    }
}
/// A Stata 13+ survey: labels, value labels with a labelled missing code,
/// system and extended missing values in every numeric type, and strL.
pub(super) fn survey(release: u16, big: bool) -> Dta {
    let mut sex = var("sex", 65530, "%8.0g");
    sex.labels = "sexlbl";
    sex.label = "Sex of respondent";
    let mut income = var("income", 65526, "%10.2f");
    income.label = "Household income (USD)";
    Dta {
        release,
        big,
        vars: vec![
            var("id", 65528, "%12.0g"),
            income,
            sex,
            var("name", 8, "%8s"),
            var("interview", 65529, "%td"),
            var("score", 65527, "%9.0g"),
            var("note", 32768, "%9s"),
        ],
        rows: vec![
            vec![
                Cell::Long(1),
                double(1234.5),
                Cell::Byte(1),
                text("Ann"),
                Cell::Int(21915),
                Cell::FloatBits(1.5f32.to_bits()),
                Cell::Strl(7, 1),
            ],
            vec![
                Cell::Long(2),
                Cell::DoubleBits(DOUBLE_MISSING),
                Cell::Byte(2),
                text(""),
                Cell::Int(32742),
                Cell::FloatBits(FLOAT_MISSING + 0x800),
                Cell::Strl(0, 0),
            ],
            vec![
                Cell::Long(2_147_483_621),
                Cell::DoubleBits(DOUBLE_MISSING + (2 << 40)),
                Cell::Byte(127),
                text("Zoë"),
                Cell::Int(-5),
                Cell::FloatBits(FLOAT_MISSING),
                Cell::Strl(7, 1),
            ],
            vec![
                Cell::Long(4),
                double(-3.25),
                Cell::Byte(101),
                text("Bob"),
                Cell::Int(32741),
                Cell::FloatBits((-0.25f32).to_bits()),
                Cell::Strl(7, 4),
            ],
        ],
        strls: vec![
            (7, 1, 130, b"A long answer\0".to_vec()),
            (7, 4, 130, b"Second\0".to_vec()),
        ],
        labels: vec![
            (
                "sexlbl",
                vec![(1, "Male"), (2, "Female"), (LONG_MISSING + 1, "Refused")],
            ),
            ("unused", vec![(1, "Never attached")]),
        ],
    }
}
pub(super) fn scan(bytes: &[u8], start: Option<usize>) -> WorkbenchResult<dta::Scan> {
    dta::scan(bytes, bytes.len() as u64, start)
}
pub(super) fn column<'a>(p: &'a profile::Profile, name: &str) -> &'a VariableDefinition {
    p.columns.iter().find(|c| c.name == name).unwrap()
}
pub(super) fn codes(c: &VariableDefinition) -> Vec<(&str, usize)> {
    c.missing_codes
        .iter()
        .map(|(k, v)| (k.as_str(), *v))
        .collect()
}

#[test]
fn stata_118_reads_labels_formats_and_distinct_missing_codes() {
    let bytes = survey(118, false).modern();
    let s = scan(&bytes, Some(0)).unwrap();
    assert_eq!(s.hash, crate::workbench::desk::hash(&bytes));
    let p = &s.profile;
    assert_eq!(p.rows, 4);
    let f = p.source_format.as_ref().unwrap();
    assert_eq!(
        (f.kind.as_str(), f.release.as_deref()),
        ("stata", Some("118"))
    );
    assert_eq!(f.label.as_deref(), Some("Household survey"));
    let income = column(p, "income");
    assert_eq!(income.label.as_deref(), Some("Household income (USD)"));
    assert_eq!(income.storage_type.as_deref(), Some("double"));
    assert_eq!(income.display_format.as_deref(), Some("%10.2f"));
    assert_eq!(
        (income.missing, codes(income)),
        (2, vec![(".", 1), (".b", 1)])
    );
    let sex = column(p, "sex");
    assert_eq!(sex.value_labels.as_deref(), Some("sexlbl"));
    assert_eq!(codes(sex), vec![(".", 1), (".z", 1)]);
    assert_eq!(codes(column(p, "id")), vec![(".", 1)]);
    assert_eq!(codes(column(p, "interview")), vec![(".", 1), (".a", 1)]);
    assert_eq!(
        column(p, "interview").display_format.as_deref(),
        Some("%td")
    );
    assert_eq!(codes(column(p, "score")), vec![(".", 1), (".a", 1)]);
    assert_eq!(codes(column(p, "name")), vec![("empty", 1)]);
    assert_eq!(column(p, "name").storage_type.as_deref(), Some("str8"));
    assert_eq!(codes(column(p, "note")), vec![("empty", 1)]);
    assert_eq!(column(p, "note").inferred_type, "text");
    // Only the attached set is catalogued; its labelled missing code stays a code.
    assert_eq!(p.value_labels.len(), 1);
    assert_eq!(p.value_labels[0].name, "sexlbl");
    let labels: Vec<_> = p.value_labels[0]
        .labels
        .iter()
        .map(|l| (l.value.as_str(), l.label.as_str()))
        .collect();
    assert_eq!(
        labels,
        vec![("1", "Male"), ("2", "Female"), (".a", "Refused")]
    );
    assert!(p.limitations.iter().any(|l| l.contains("not attached")));
    let w = s.window.unwrap();
    assert_eq!(w.columns.len(), 7);
    let row = |i: usize| w.rows[i].iter().map(|c| c.as_deref()).collect::<Vec<_>>();
    assert_eq!(
        row(0),
        vec![
            Some("1"),
            Some("1234.5"),
            Some("1"),
            Some("Ann"),
            Some("21915"),
            Some("1.5"),
            Some("A long answer")
        ]
    );
    assert_eq!(
        row(1),
        vec![Some("2"), None, Some("2"), None, None, None, None]
    );
    assert_eq!(row(2)[3], Some("Zoë"));
    assert_eq!(row(2)[6], Some("A long answer"));
    assert_eq!(
        row(3),
        vec![
            Some("4"),
            Some("-3.25"),
            None,
            Some("Bob"),
            None,
            Some("-0.25"),
            Some("Second")
        ]
    );
    let cell = |r, c| {
        w.missing_codes
            .iter()
            .find(|m| m.row == r && m.column == c)
            .map(|m| m.code.as_str())
    };
    assert_eq!(cell(1, 1), Some("."));
    assert_eq!(cell(1, 4), Some(".a"));
    assert_eq!(cell(2, 1), Some(".b"));
    assert_eq!(cell(2, 2), Some(".z"));
    assert_eq!(cell(1, 3), None, "empty strings carry no extended code");
}

#[test]
fn stata_119_and_117_headers_and_byte_orders() {
    for (release, big) in [(119, true), (119, false), (117, true), (118, true)] {
        let bytes = survey(release, big).modern();
        let s = scan(&bytes, Some(0)).unwrap_or_else(|e| panic!("{release} {big}: {}", e.message));
        assert_eq!(s.profile.rows, 4, "{release}");
        assert_eq!(
            s.profile.source_format.as_ref().unwrap().release.as_deref(),
            Some(release.to_string().as_str())
        );
        assert_eq!(
            codes(column(&s.profile, "income")),
            vec![(".", 1), (".b", 1)]
        );
        let w = s.window.unwrap();
        assert_eq!(
            w.rows[3][6].as_deref(),
            Some("Second"),
            "strL key layout for {release}"
        );
        assert_eq!(w.rows[0][4].as_deref(), Some("21915"));
    }
    // Release 117 text is UTF-8 when valid, otherwise Latin-1.
    let mut d = survey(117, false);
    d.rows[0][3] = Cell::Text(vec![b'c', b'a', b'f', 0xe9]);
    let s = scan(&d.modern(), Some(0)).unwrap();
    assert_eq!(s.window.unwrap().rows[0][3].as_deref(), Some("café"));
    assert!(s.profile.limitations.iter().any(|l| l.contains("Latin-1")));
}

#[test]
fn stata_legacy_115_and_114_value_labels_and_missing() {
    let mut age = var("age", 65529, "%8.0g");
    age.labels = "agecat";
    for release in [115, 114, 113] {
        let d = Dta {
            release,
            big: false,
            vars: vec![
                age.clone(),
                var("wage", 65526, "%9.2f"),
                var("city", 6, "%6s"),
            ],
            rows: vec![
                vec![Cell::Int(30), double(12.5), text("Lima")],
                vec![
                    Cell::Int(32741 + 3),
                    Cell::DoubleBits(DOUBLE_MISSING + (26 << 40)),
                    text(""),
                ],
            ],
            strls: Vec::new(),
            labels: vec![(
                "agecat",
                vec![(30, "Thirty"), (LONG_MISSING + 3, "Not asked")],
            )],
        };
        let bytes = if release == 113 {
            legacy_113(&d)
        } else {
            d.legacy()
        };
        let s = scan(&bytes, Some(0)).unwrap_or_else(|e| panic!("{release}: {}", e.message));
        let p = &s.profile;
        assert_eq!(p.rows, 2);
        assert_eq!(codes(column(p, "age")), vec![(".c", 1)]);
        assert_eq!(codes(column(p, "wage")), vec![(".z", 1)]);
        assert_eq!(codes(column(p, "city")), vec![("empty", 1)]);
        assert_eq!(p.value_labels[0].labels[1].value, ".c");
        assert_eq!(column(p, "wage").display_format.as_deref(), Some("%9.2f"));
        assert_eq!(
            s.window.unwrap().rows[0],
            vec![Some("30".into()), Some("12.5".into()), Some("Lima".into())]
        );
    }
}
/// Release 113 differs from 114 only in its 12-byte format entries.
fn legacy_113(d: &Dta) -> Vec<u8> {
    let full = d.legacy();
    let k = d.vars.len();
    let formats_at = 109 + k + 33 * k + 2 * (k + 1);
    let mut out = full[..formats_at].to_vec();
    for v in &d.vars {
        out.extend(fixed(v.format.as_bytes(), 12));
    }
    out.extend(&full[formats_at + 49 * k..]);
    out
}

#[test]
fn malformed_truncated_and_unsupported_stata_files_are_refused() {
    let bytes = survey(118, false).modern();
    for cut in (0..bytes.len()).step_by(37).chain([bytes.len() - 1]) {
        assert!(scan(&bytes[..cut], None).is_err(), "truncated at {cut}");
        // A length that promises more bytes than exist is also refused.
        assert!(dta::scan(&bytes[..cut], bytes.len() as u64, None).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(scan(&trailing, None).is_err());
    let mut bad_type = bytes.clone();
    let at = find(&bad_type, b"<variable_types>") + 16;
    bad_type[at..at + 2].copy_from_slice(&3000u16.to_le_bytes());
    assert!(scan(&bad_type, None).unwrap_err().message.contains("type"));
    let mut bad_map = bytes.clone();
    let at = find(&bad_map, b"<map>") + 5 + 8 * 9;
    bad_map[at..at + 8].copy_from_slice(&5u64.to_le_bytes());
    assert!(scan(&bad_map, None).is_err());
    let mut too_many_rows = bytes.clone();
    let at = find(&too_many_rows, b"<N>") + 3;
    too_many_rows[at..at + 8].copy_from_slice(&1_000_000u64.to_le_bytes());
    assert!(scan(&too_many_rows, None)
        .unwrap_err()
        .message
        .contains("observation count"));
    let mut release = bytes.clone();
    release[28..31].copy_from_slice(b"116");
    assert!(scan(&release, None)
        .unwrap_err()
        .message
        .contains("not supported"));
    assert!(scan(&[105, 2, 1, 0], None)
        .unwrap_err()
        .message
        .contains("not supported"));
    assert!(scan(b"PK\x03\x04", None)
        .unwrap_err()
        .message
        .contains("not a Stata"));
    let mut bad_label = survey(118, false);
    bad_label.labels = vec![("sexlbl", vec![(1, "Male")])];
    let mut raw = bad_label.modern();
    let at = find(&raw, b"<lbl>") + 5 + 4 + 129 + 3;
    raw[at..at + 4].copy_from_slice(&9u32.to_le_bytes());
    assert!(scan(&raw, None)
        .unwrap_err()
        .message
        .contains("value-label"));
    let mut wide = survey(118, false);
    wide.vars = (0..2001).map(|_| var("x", 65530, "%8.0g")).collect();
    wide.rows.clear();
    assert!(scan(&wide.modern(), None)
        .unwrap_err()
        .message
        .contains("at most 2000"));
}
pub(super) fn find(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len()).position(|w| w == needle).unwrap()
}

/// Repeat the data section of a little-endian release-118 file `times` times.
pub(super) fn repeat_rows(bytes: &[u8], times: usize) -> Vec<u8> {
    let word = |b: &[u8], at: usize| u64::from_le_bytes(b[at..at + 8].try_into().unwrap());
    let map_at = find(bytes, b"<map>") + 5;
    let map: Vec<u64> = (0..14).map(|i| word(bytes, map_at + 8 * i)).collect();
    let (start, end) = (map[9] as usize + 6, map[10] as usize - 7);
    let mut out = bytes[..start].to_vec();
    for _ in 0..times {
        out.extend_from_slice(&bytes[start..end]);
    }
    out.extend_from_slice(&bytes[end..]);
    let added = ((end - start) * (times - 1)) as u64;
    for (i, m) in map.iter().enumerate().skip(10) {
        out[map_at + 8 * i..map_at + 8 * i + 8].copy_from_slice(&(m + added).to_le_bytes());
    }
    let n_at = find(bytes, b"<N>") + 3;
    let n = word(bytes, n_at) * times as u64;
    out[n_at..n_at + 8].copy_from_slice(&n.to_le_bytes());
    out
}
