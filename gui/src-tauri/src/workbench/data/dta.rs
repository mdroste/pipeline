//! Bounded Stata `.dta` reader for releases 113–115 (Stata 8–12) and 117–119
//! (Stata 13 and later). One sequential pass hashes every byte it reads,
//! profiles every row and keeps at most a preview window, so dictionary-only
//! captures never hold the data in memory. Missing values are classified from
//! their stored bit patterns and are never decoded as numbers.
use super::profile::{cell_text, Profile, Window, MAX_COLUMNS, PREVIEW_COLUMNS};
use super::{MissingCell, SourceFormat, ValueLabel, ValueLabelSet, VariableDefinition};
use crate::workbench::store::{WorkbenchError, WorkbenchResult};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;

/// Largest value-label table parsed; larger sets are reported as omitted.
const MAX_LABEL_TABLE: u64 = 16 * 1024 * 1024;
const MAX_KEPT_LABELS: usize = 1000;
const MAX_KEPT_LABEL_TEXT: usize = 64 * 1024;
/// Bytes of one strL read for a preview cell.
const MAX_STRL_PREVIEW: u64 = 64 * 1024;
/// Code index for an empty string; 0 is `.` and 1–26 are `.a`–`.z`.
const EMPTY: usize = 27;

fn bad(message: &str) -> WorkbenchError {
    WorkbenchError::invalid(format!("Stata file refused: {message}"))
}
fn unsupported(release: u16) -> WorkbenchError {
    WorkbenchError::invalid(format!(
        "Stata release {release} is not supported. Pipeline reads releases 113–115 and 117–119 (Stata 8 and later); re-save the file with Stata 13 or later."
    ))
}
pub(super) fn code_name(index: usize) -> String {
    match index {
        0 => ".".into(),
        1..=26 => format!(".{}", (b'a' + index as u8 - 1) as char),
        _ => "empty".into(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Str(usize),
    StrL,
    Byte,
    Int,
    Long,
    Float,
    Double,
}
impl Kind {
    fn modern(code: u64) -> Option<Self> {
        Some(match code {
            1..=2045 => Self::Str(code as usize),
            32768 => Self::StrL,
            65526 => Self::Double,
            65527 => Self::Float,
            65528 => Self::Long,
            65529 => Self::Int,
            65530 => Self::Byte,
            _ => return None,
        })
    }
    fn legacy(code: u8) -> Option<Self> {
        Some(match code {
            1..=244 => Self::Str(code as usize),
            251 => Self::Byte,
            252 => Self::Int,
            253 => Self::Long,
            254 => Self::Float,
            255 => Self::Double,
            _ => return None,
        })
    }
    fn width(self) -> usize {
        match self {
            Self::Str(n) => n,
            Self::StrL | Self::Double => 8,
            Self::Long | Self::Float => 4,
            Self::Int => 2,
            Self::Byte => 1,
        }
    }
    fn name(self) -> String {
        match self {
            Self::Str(n) => format!("str{n}"),
            Self::StrL => "strL".into(),
            Self::Byte => "byte".into(),
            Self::Int => "int".into(),
            Self::Long => "long".into(),
            Self::Float => "float".into(),
            Self::Double => "double".into(),
        }
    }
    fn numeric(self) -> bool {
        !matches!(self, Self::Str(_) | Self::StrL)
    }
}

fn uint(bytes: &[u8], big: bool) -> u64 {
    let fold = |v: u64, b: &u8| (v << 8) | u64::from(*b);
    if big {
        bytes.iter().fold(0, fold)
    } else {
        bytes.iter().rev().fold(0, fold)
    }
}
/// Canonical extended codes sit at fixed steps above the system-missing base.
fn canonical(delta: u64, shift: u32) -> usize {
    let index = delta >> shift;
    if delta & ((1 << shift) - 1) == 0 && index <= 26 {
        index as usize
    } else {
        0
    }
}
/// Missing-code index for one stored cell, or `None` for a value.
fn missing(kind: Kind, cell: &[u8], big: bool) -> Option<usize> {
    match kind {
        Kind::Byte => {
            let v = cell[0] as i8;
            (v > 100).then(|| (i16::from(v) - 101) as usize)
        }
        Kind::Int => {
            let v = uint(cell, big) as u16 as i16;
            (v > 32740).then(|| (v - 32741) as usize)
        }
        Kind::Long => {
            let v = uint(cell, big) as u32 as i32;
            (v > 2_147_483_620).then(|| (v - 2_147_483_621) as usize)
        }
        Kind::Float => {
            let bits = uint(cell, big) as u32;
            if bits & 0x8000_0000 == 0 && bits >= 0x7f00_0000 {
                Some(canonical(u64::from(bits - 0x7f00_0000), 11))
            } else {
                f32::from_bits(bits).is_nan().then_some(0)
            }
        }
        Kind::Double => {
            let bits = uint(cell, big);
            if bits >> 63 == 0 && bits >= 0x7fe0_0000_0000_0000 {
                Some(canonical(bits - 0x7fe0_0000_0000_0000, 40))
            } else {
                f64::from_bits(bits).is_nan().then_some(0)
            }
        }
        Kind::Str(_) => (cell[0] == 0).then_some(EMPTY),
        Kind::StrL => cell.iter().all(|b| *b == 0).then_some(EMPTY),
    }
}
fn float_text(v: f64, single: Option<f32>) -> String {
    if v != 0.0 && !(1e-6..1e15).contains(&v.abs()) {
        single.map_or_else(|| format!("{v:e}"), |s| format!("{s:e}"))
    } else {
        single.map_or_else(|| v.to_string(), |s| s.to_string())
    }
}
fn value_text(kind: Kind, cell: &[u8], big: bool, utf8: bool) -> String {
    match kind {
        Kind::Byte => (cell[0] as i8).to_string(),
        Kind::Int => (uint(cell, big) as u16 as i16).to_string(),
        Kind::Long => (uint(cell, big) as u32 as i32).to_string(),
        Kind::Float => {
            let f = f32::from_bits(uint(cell, big) as u32);
            float_text(f64::from(f), Some(f))
        }
        Kind::Double => float_text(f64::from_bits(uint(cell, big)), None),
        Kind::Str(_) | Kind::StrL => text(cell, utf8),
    }
}
/// NUL-terminated text. Releases before 118 are UTF-8 when valid, else Latin-1.
fn text(bytes: &[u8], utf8: bool) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    let bytes = &bytes[..end];
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) if !utf8 => bytes.iter().map(|b| char::from(*b)).collect(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Sequential reader that hashes every consumed byte and refuses to read past
/// the length observed when the file was opened.
struct Input<R> {
    inner: R,
    pos: u64,
    len: u64,
    hasher: Sha256,
    big: bool,
}
impl<R: Read> Input<R> {
    fn fill(&mut self, buf: &mut [u8]) -> WorkbenchResult<()> {
        if self.len - self.pos < buf.len() as u64 {
            return Err(bad("the file is truncated"));
        }
        self.inner
            .read_exact(buf)
            .map_err(|_| bad("the file is truncated or unreadable"))?;
        self.hasher.update(&*buf);
        self.pos += buf.len() as u64;
        Ok(())
    }
    fn vec(&mut self, n: u64) -> WorkbenchResult<Vec<u8>> {
        if self.len - self.pos < n {
            return Err(bad("the file is truncated"));
        }
        let mut v = vec![0; n as usize];
        self.fill(&mut v)?;
        Ok(v)
    }
    fn skip(&mut self, mut n: u64) -> WorkbenchResult<()> {
        let mut buf = [0u8; 64 * 1024];
        while n > 0 {
            let k = n.min(buf.len() as u64) as usize;
            self.fill(&mut buf[..k])?;
            n -= k as u64;
        }
        Ok(())
    }
    fn skip_to(&mut self, offset: u64) -> WorkbenchResult<()> {
        if offset < self.pos {
            return Err(bad("its section map is out of order"));
        }
        self.skip(offset - self.pos)
    }
    fn expect(&mut self, tag: &str) -> WorkbenchResult<()> {
        if self.vec(tag.len() as u64)? != tag.as_bytes() {
            return Err(bad(&format!("expected {tag} at byte {}", self.pos)));
        }
        Ok(())
    }
    fn uint(&mut self, width: usize) -> WorkbenchResult<u64> {
        let mut b = [0u8; 8];
        self.fill(&mut b[..width])?;
        Ok(uint(&b[..width], self.big))
    }
    fn finish(mut self) -> WorkbenchResult<String> {
        let mut probe = [0u8; 1];
        if self.pos != self.len || !matches!(self.inner.read(&mut probe), Ok(0)) {
            return Err(bad("it has unexpected bytes after its last section"));
        }
        Ok(hex::encode(self.hasher.finalize()))
    }
}

struct Var {
    name: String,
    kind: Kind,
    format: String,
    labels: String,
    label: String,
}
struct Meta {
    release: u16,
    utf8: bool,
    /// Bytes of a value-label set name.
    name_width: usize,
    rows: u64,
    vars: Vec<Var>,
    label: String,
    timestamp: String,
    /// Section offsets for releases 117–119.
    map: Option<[u64; 14]>,
}
/// Descriptor arrays shared by both layouts. Tagged releases locate each
/// section through the map; legacy releases store them back to back.
fn descriptors<R: Read>(
    input: &mut Input<R>,
    kinds: Vec<Kind>,
    widths: (usize, usize, usize),
    utf8: bool,
    map: Option<&[u64; 14]>,
) -> WorkbenchResult<Vec<Var>> {
    let k = kinds.len() as u64;
    let (name, format, label) = widths;
    let section = |input: &mut Input<R>, index: usize, tag: &str, width: usize| {
        if let Some(map) = map {
            input.skip_to(map[index])?;
            input.expect(&format!("<{tag}>"))?;
        }
        let raw = input.vec(k * width as u64)?;
        if map.is_some() {
            input.expect(&format!("</{tag}>"))?;
        }
        Ok::<_, WorkbenchError>(raw.chunks(width).map(|c| text(c, utf8)).collect::<Vec<_>>())
    };
    let names = section(input, 3, "varnames", name)?;
    if map.is_none() {
        // Legacy sort list: K+1 two-byte entries.
        input.skip((k + 1) * 2)?;
    }
    let formats = section(input, 5, "formats", format)?;
    let labels = section(input, 6, "value_label_names", name)?;
    let var_labels = section(input, 7, "variable_labels", label)?;
    let mut seen = HashSet::new();
    let mut vars = Vec::with_capacity(kinds.len());
    for (i, kind) in kinds.into_iter().enumerate() {
        let name = names[i].clone();
        crate::workbench::desk::check_text(&name, 500)
            .map_err(|_| bad("a variable has no name"))?;
        if !seen.insert(name.clone()) {
            return Err(bad("variable names are not unique"));
        }
        vars.push(Var {
            name,
            kind,
            format: formats[i].clone(),
            labels: labels[i].clone(),
            label: var_labels[i].clone(),
        });
    }
    Ok(vars)
}
fn check_variables(k: u64) -> WorkbenchResult<()> {
    if k == 0 {
        return Err(bad("it has no variables"));
    }
    if k > MAX_COLUMNS as u64 {
        return Err(WorkbenchError::invalid(format!(
            "This Stata file has {k} variables; Pipeline catalogs at most {MAX_COLUMNS}. Save a subset with `keep` first."
        )));
    }
    Ok(())
}
/// Releases 117–119 (the leading `<` has been read).
fn modern<R: Read>(input: &mut Input<R>) -> WorkbenchResult<Meta> {
    input.expect("stata_dta><header><release>")?;
    let release = String::from_utf8(input.vec(3)?)
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| bad("its release number is unreadable"))?;
    if !(117..=119).contains(&release) {
        return Err(unsupported(release));
    }
    input.expect("</release><byteorder>")?;
    input.big = match &input.vec(3)?[..] {
        b"MSF" => true,
        b"LSF" => false,
        _ => return Err(bad("its byte order is unknown")),
    };
    input.expect("</byteorder><K>")?;
    let k = input.uint(if release == 119 { 4 } else { 2 })?;
    input.expect("</K><N>")?;
    let rows = input.uint(if release == 117 { 4 } else { 8 })?;
    input.expect("</N><label>")?;
    let label_len = input.uint(if release == 117 { 1 } else { 2 })?;
    let label = input.vec(label_len)?;
    input.expect("</label><timestamp>")?;
    let stamp_len = input.uint(1)?;
    let timestamp = input.vec(stamp_len)?;
    input.expect("</timestamp></header>")?;
    let map_at = input.pos;
    input.expect("<map>")?;
    let mut map = [0u64; 14];
    for m in &mut map {
        *m = input.uint(8)?;
    }
    input.expect("</map>")?;
    if map[0] != 0
        || map[1] != map_at
        || map.windows(2).any(|w| w[0] > w[1])
        || map[13] != input.len
    {
        return Err(bad("its section map does not match the file"));
    }
    check_variables(k)?;
    input.skip_to(map[2])?;
    input.expect("<variable_types>")?;
    let mut kinds = Vec::with_capacity(k as usize);
    for _ in 0..k {
        kinds.push(Kind::modern(input.uint(2)?).ok_or_else(|| bad("a variable type is unknown"))?);
    }
    input.expect("</variable_types>")?;
    let utf8 = release >= 118;
    let widths = if utf8 { (129, 57, 321) } else { (33, 49, 81) };
    let vars = descriptors(input, kinds, widths, utf8, Some(&map))?;
    input.skip_to(map[9])?;
    input.expect("<data>")?;
    Ok(Meta {
        release,
        utf8,
        name_width: widths.0,
        rows,
        vars,
        label: text(&label, utf8),
        timestamp: text(&timestamp, utf8),
        map: Some(map),
    })
}
/// Releases 113–115 (the release byte has been read).
fn legacy<R: Read>(input: &mut Input<R>, release: u8) -> WorkbenchResult<Meta> {
    let mut head = [0u8; 3];
    input.fill(&mut head)?;
    input.big = match head[0] {
        1 => true,
        2 => false,
        _ => return Err(bad("its byte order is unknown")),
    };
    if head[1] != 1 {
        return Err(bad("it is not a Stata dataset"));
    }
    let k = input.uint(2)?;
    let rows = input.uint(4)?;
    let label = input.vec(81)?;
    let timestamp = input.vec(18)?;
    check_variables(k)?;
    let mut kinds = Vec::with_capacity(k as usize);
    for code in input.vec(k)? {
        kinds.push(Kind::legacy(code).ok_or_else(|| bad("a variable type is unknown"))?);
    }
    let format = if release == 113 { 12 } else { 49 };
    let vars = descriptors(input, kinds, (33, format, 81), false, None)?;
    loop {
        let kind = input.uint(1)?;
        let len = input.uint(4)?;
        if kind == 0 && len == 0 {
            break;
        }
        input.skip(len)?;
    }
    Ok(Meta {
        release: u16::from(release),
        utf8: false,
        name_width: 33,
        rows,
        vars,
        label: text(&label, false),
        timestamp: text(&timestamp, false),
        map: None,
    })
}

/// strL `(v,o)` reference stored in a data cell.
fn strl_key(cell: &[u8], release: u16, big: bool) -> (u64, u64) {
    let v = match release {
        117 => 4,
        118 => 2,
        _ => 3,
    };
    (uint(&cell[..v], big), uint(&cell[v..], big))
}
struct Pending {
    row: usize,
    column: usize,
    key: (u64, u64),
}
/// Profile all rows; fill the preview window with values and missing codes.
fn data<R: Read>(
    input: &mut Input<R>,
    meta: &Meta,
    window: &mut Option<Window>,
) -> WorkbenchResult<(Vec<[usize; 28]>, Vec<Pending>)> {
    let width: usize = meta.vars.iter().map(|v| v.kind.width()).sum();
    let mut offsets = Vec::with_capacity(meta.vars.len());
    let mut at = 0;
    for v in &meta.vars {
        offsets.push(at);
        at += v.kind.width();
    }
    let end = meta.map.map_or(input.len, |m| m[10]);
    if meta
        .rows
        .checked_mul(width as u64)
        .is_none_or(|bytes| bytes > end.saturating_sub(input.pos))
    {
        return Err(bad("its observation count does not fit the file"));
    }
    let mut counts = vec![[0usize; 28]; meta.vars.len()];
    let mut pending = Vec::new();
    let batch = ((1 << 20) / width).max(1) as u64;
    let mut buf = vec![0u8; width * batch.min(meta.rows) as usize];
    let mut row = 0u64;
    while row < meta.rows {
        let take = batch.min(meta.rows - row) as usize;
        input.fill(&mut buf[..take * width])?;
        for (r, record) in buf[..take * width].chunks(width).enumerate() {
            let index = (row as usize) + r;
            let keep = window.as_ref().is_some_and(|w| w.wants(index));
            let mut cells = Vec::new();
            for (j, var) in meta.vars.iter().enumerate() {
                let cell = &record[offsets[j]..offsets[j] + var.kind.width()];
                let code = missing(var.kind, cell, input.big);
                if let Some(code) = code {
                    counts[j][code] += 1;
                }
                if !keep || j >= PREVIEW_COLUMNS {
                    continue;
                }
                let w = window.as_mut().expect("window kept");
                let line = w.rows.len();
                match code {
                    Some(code) => {
                        if code != EMPTY {
                            w.missing_codes.push(MissingCell {
                                row: line,
                                column: j,
                                code: code_name(code),
                            });
                        }
                        cells.push(None);
                    }
                    None if var.kind == Kind::StrL => {
                        pending.push(Pending {
                            row: line,
                            column: j,
                            key: strl_key(cell, meta.release, input.big),
                        });
                        cells.push(None);
                    }
                    None => cells.push(Some(cell_text(&value_text(
                        var.kind, cell, input.big, meta.utf8,
                    )))),
                }
            }
            if keep {
                window.as_mut().expect("window kept").rows.push(cells);
            }
        }
        row += take as u64;
    }
    Ok((counts, pending))
}
/// Walk the strL table, keeping preview text only for referenced entries.
fn strls<R: Read>(
    input: &mut Input<R>,
    meta: &Meta,
    wanted: &HashSet<(u64, u64)>,
) -> WorkbenchResult<HashMap<(u64, u64), String>> {
    let map = meta.map.expect("modern layout");
    input.skip_to(map[10])?;
    input.expect("<strls>")?;
    let mut found = HashMap::new();
    loop {
        match &input.vec(3)?[..] {
            b"GSO" => {
                let v = input.uint(4)?;
                let o = input.uint(if meta.release == 117 { 4 } else { 8 })?;
                let kind = input.uint(1)?;
                let len = input.uint(4)?;
                if kind != 129 && kind != 130 {
                    return Err(bad("its long-string table is malformed"));
                }
                if wanted.contains(&(v, o)) {
                    let keep = len.min(MAX_STRL_PREVIEW);
                    let raw = input.vec(keep)?;
                    input.skip(len - keep)?;
                    let shown = if kind == 129 {
                        format!("[binary strL, {len} bytes]")
                    } else {
                        cell_text(&text(&raw, meta.utf8))
                    };
                    found.insert((v, o), shown);
                } else {
                    input.skip(len)?;
                }
            }
            b"</s" => {
                input.expect("trls>")?;
                return Ok(found);
            }
            _ => return Err(bad("its long-string table is malformed")),
        }
    }
}
/// One value-label table: `n`, text length, offsets, values, then label text.
fn label_table(raw: &[u8], big: bool, utf8: bool) -> WorkbenchResult<Vec<ValueLabel>> {
    let malformed = || bad("a value-label table is malformed");
    if raw.len() < 8 {
        return Err(malformed());
    }
    let word = |i: usize| uint(&raw[i..i + 4], big) as usize;
    let (n, text_len) = (word(0), word(4));
    if n > (raw.len() - 8) / 8 || 8 + 8 * n + text_len != raw.len() {
        return Err(malformed());
    }
    let labels = &raw[8 + 8 * n..];
    (0..n)
        .map(|i| {
            let offset = word(8 + 4 * i);
            let value = word(8 + 4 * n + 4 * i) as u32 as i32;
            if offset >= text_len {
                return Err(malformed());
            }
            Ok(ValueLabel {
                value: if value > 2_147_483_620 {
                    code_name((value - 2_147_483_621) as usize)
                } else {
                    value.to_string()
                },
                label: text(&labels[offset..], utf8),
            })
        })
        .collect()
}
#[derive(Default)]
struct Labels {
    sets: Vec<ValueLabelSet>,
    kept_text: usize,
    unattached: usize,
    oversized: Vec<String>,
}
impl Labels {
    fn read<R: Read>(
        &mut self,
        input: &mut Input<R>,
        meta: &Meta,
        wanted: &HashSet<&str>,
    ) -> WorkbenchResult<()> {
        let len = input.uint(4)?;
        let name = text(&input.vec(meta.name_width as u64)?, meta.utf8);
        input.skip(3)?;
        if !wanted.contains(name.as_str()) {
            self.unattached += 1;
            return input.skip(len);
        }
        if len > MAX_LABEL_TABLE {
            self.oversized.push(name);
            return input.skip(len);
        }
        let raw = input.vec(len)?;
        let all = label_table(&raw, input.big, meta.utf8)?;
        let mut set = ValueLabelSet {
            name,
            labels: Vec::new(),
            omitted: 0,
        };
        for entry in all {
            let size = entry.label.len() + entry.value.len();
            if set.labels.len() < MAX_KEPT_LABELS && self.kept_text + size <= MAX_KEPT_LABEL_TEXT {
                self.kept_text += size;
                set.labels.push(entry);
            } else {
                set.omitted += 1;
            }
        }
        self.sets.push(set);
        Ok(())
    }
}

/// Result of one bounded pass over a `.dta` file.
#[derive(Debug)]
pub(super) struct Scan {
    pub profile: Profile,
    pub window: Option<Window>,
    /// SHA-256 of exactly the bytes that were read and profiled.
    pub hash: String,
}

pub(super) fn scan<R: Read>(reader: R, len: u64, start: Option<usize>) -> WorkbenchResult<Scan> {
    let mut input = Input {
        inner: reader,
        pos: 0,
        len,
        hasher: Sha256::new(),
        big: false,
    };
    let first = input.uint(1)? as u8;
    let meta = match first {
        b'<' => modern(&mut input)?,
        113..=115 => legacy(&mut input, first)?,
        102..=112 | 116 => return Err(unsupported(u16::from(first))),
        _ => return Err(bad("it is not a Stata dataset")),
    };
    let mut window = start.map(Window::new);
    if let Some(w) = window.as_mut() {
        w.columns = meta
            .vars
            .iter()
            .take(PREVIEW_COLUMNS)
            .map(|v| v.name.clone())
            .collect();
    }
    let (counts, pending) = data(&mut input, &meta, &mut window)?;
    let wanted: HashSet<&str> = meta
        .vars
        .iter()
        .filter(|v| !v.labels.is_empty())
        .map(|v| v.labels.as_str())
        .collect();
    let mut labels = Labels::default();
    if let Some(map) = meta.map {
        input.expect("</data>")?;
        let keys = pending.iter().map(|p| p.key).collect::<HashSet<_>>();
        let found = strls(&mut input, &meta, &keys)?;
        if let Some(w) = window.as_mut() {
            for p in &pending {
                w.rows[p.row][p.column] = Some(
                    found
                        .get(&p.key)
                        .cloned()
                        .ok_or_else(|| bad("a long string is missing from its table"))?,
                );
            }
        }
        input.skip_to(map[11])?;
        input.expect("<value_labels>")?;
        loop {
            match &input.vec(5)?[..] {
                b"<lbl>" => {
                    labels.read(&mut input, &meta, &wanted)?;
                    input.expect("</lbl>")?;
                }
                b"</val" => {
                    input.expect("ue_labels>")?;
                    break;
                }
                _ => return Err(bad("its value-label section is malformed")),
            }
        }
        input.skip_to(map[12])?;
        input.expect("</stata_dta>")?;
    } else {
        while input.pos < input.len {
            labels.read(&mut input, &meta, &wanted)?;
        }
    }
    let hash = input.finish()?;
    Ok(Scan {
        profile: profile(&meta, &counts, labels, &wanted),
        window,
        hash,
    })
}

fn profile(meta: &Meta, counts: &[[usize; 28]], labels: Labels, wanted: &HashSet<&str>) -> Profile {
    let columns = meta
        .vars
        .iter()
        .zip(counts)
        .map(|(v, c)| {
            let missing_codes: BTreeMap<String, usize> = c
                .iter()
                .enumerate()
                .filter(|(_, n)| **n > 0)
                .map(|(i, n)| (code_name(i), *n))
                .collect();
            let some = |s: &str| (!s.trim().is_empty()).then(|| s.to_string());
            VariableDefinition {
                name: v.name.clone(),
                inferred_type: if v.kind.numeric() { "number" } else { "text" }.into(),
                missing: c.iter().sum(),
                units: None,
                description: None,
                origin: "declared in the Stata file; missing counts cover all rows".into(),
                label: some(&v.label),
                storage_type: Some(v.kind.name()),
                display_format: some(&v.format),
                value_labels: some(&v.labels),
                missing_codes,
            }
        })
        .collect();
    let mut limitations = vec![
        "Stata missing values stay missing: system missing (.) and extended missing values (.a–.z) are counted separately under missingCodes and are never read as numbers. Empty strings are missing in string variables.".to_string(),
        "Dates and times remain Stata numbers (for example, days since 1 January 1960 for %td); displayFormat records each variable's format.".into(),
    ];
    if !meta.utf8 {
        limitations.push(
            "Text in Stata releases before 118 was read as UTF-8 when valid and otherwise as Latin-1."
                .into(),
        );
    }
    if meta.vars.iter().any(|v| v.kind == Kind::StrL) {
        limitations.push(
            "strL variables are missing only when empty; row previews show at most 500 bytes of each long string."
                .into(),
        );
    }
    let defined: HashSet<&str> = labels.sets.iter().map(|s| s.name.as_str()).collect();
    let mut undefined: Vec<&str> = wanted
        .iter()
        .filter(|w| !defined.contains(**w) && !labels.oversized.iter().any(|o| o == **w))
        .copied()
        .collect();
    undefined.sort_unstable();
    if !undefined.is_empty() {
        limitations.push(format!(
            "Value-label sets attached but not defined in the file: {}.",
            undefined
                .into_iter()
                .take(20)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !labels.oversized.is_empty() {
        limitations.push(format!(
            "Value-label sets too large to catalog: {}.",
            labels.oversized.join(", ")
        ));
    }
    if labels.sets.iter().any(|s| s.omitted > 0) {
        limitations.push(format!(
            "Value labels are limited to {MAX_KEPT_LABELS} entries per set and 64 KiB in total; omitted counts are recorded per set."
        ));
    }
    if labels.unattached > 0 {
        limitations.push(format!(
            "{} value-label set(s) not attached to any variable were not catalogued.",
            labels.unattached
        ));
    }
    Profile {
        rows: meta.rows as usize,
        columns,
        value_labels: labels.sets,
        source_format: Some(SourceFormat {
            kind: "stata".into(),
            release: Some(meta.release.to_string()),
            label: (!meta.label.is_empty()).then(|| meta.label.clone()),
            timestamp: (!meta.timestamp.trim().is_empty()).then(|| meta.timestamp.clone()),
            ..SourceFormat::default()
        }),
        limitations,
    }
}
