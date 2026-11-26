//! A derived index; bibliography files remain authoritative and unchanged.
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CitationRequest {
    pub bibliographies: Vec<String>,
    pub source_path: Option<String>,
    pub source_text: Option<String>,
    pub checkpoint_id: Option<String>,
}

pub fn citation_keys(text: &str) -> Vec<String> {
    scan_citations(text).0
}

fn whitespace(bytes: &[u8], at: &mut usize) {
    while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
        *at += 1;
    }
}

/// Read one balanced argument, honoring escaped delimiters and braces inside
/// optional arguments. Byte positions stay on UTF-8 boundaries at delimiters.
fn argument<'a>(text: &'a str, at: &mut usize, open: u8, close: u8) -> Option<&'a str> {
    let bytes = text.as_bytes();
    if bytes.get(*at) != Some(&open) {
        return None;
    }
    *at += 1;
    let start = *at;
    let mut depth = 1usize;
    let mut braces = 0usize;
    while let Some(&byte) = bytes.get(*at) {
        *at += 1;
        if byte == b'\\' {
            *at = (*at + 1).min(bytes.len());
            continue;
        }
        if open != b'{' {
            if byte == b'{' {
                braces += 1;
            }
            if byte == b'}' {
                braces = braces.saturating_sub(1);
            }
        }
        if braces == 0 {
            if byte == open {
                depth += 1;
            }
            if byte == close {
                depth -= 1;
            }
            if depth == 0 {
                return Some(&text[start..*at - 1]);
            }
        }
    }
    None
}

fn scan_citations(text: &str) -> (Vec<String>, Vec<String>) {
    let visible = studio::citation_source(text);
    let bytes = visible.as_bytes();
    let mut at = 0usize;
    let mut keys = BTreeSet::new();
    let mut warnings = BTreeSet::new();
    while at < bytes.len() {
        if bytes[at] != b'\\' {
            at += 1;
            continue;
        }
        at += 1;
        let start = at;
        while bytes.get(at).is_some_and(u8::is_ascii_alphabetic) {
            at += 1;
        }
        let command = visible[start..at].to_ascii_lowercase();
        if !command.contains("cite") {
            continue;
        }
        let multi = matches!(
            command.as_str(),
            "cites"
                | "parencites"
                | "textcites"
                | "autocites"
                | "footcites"
                | "footcitetexts"
                | "smartcites"
                | "supercites"
        );
        let supported = multi
            || matches!(
                command.as_str(),
                "cite"
                    | "citep"
                    | "citet"
                    | "citealp"
                    | "citealt"
                    | "citeauthor"
                    | "citeyear"
                    | "citeyearpar"
                    | "citetitle"
                    | "citeurl"
                    | "citedate"
                    | "citefield"
                    | "fullcite"
                    | "footfullcite"
                    | "footcite"
                    | "footcitetext"
                    | "textcite"
                    | "autocite"
                    | "parencite"
                    | "smartcite"
                    | "supercite"
                    | "nocite"
                    | "notecite"
                    | "pnotecite"
                    | "fnotecite"
            );
        if !supported {
            warnings.insert(format!(
                "Citation command \\{command} has incomplete parser coverage; review it manually."
            ));
        }
        if bytes.get(at) == Some(&b'*') {
            at += 1;
        }
        whitespace(bytes, &mut at);
        if multi {
            while bytes.get(at) == Some(&b'(') {
                if argument(&visible, &mut at, b'(', b')').is_none() {
                    break;
                }
                whitespace(bytes, &mut at);
            }
        }
        let mut groups = 0usize;
        loop {
            while bytes.get(at) == Some(&b'[') {
                if argument(&visible, &mut at, b'[', b']').is_none() {
                    break;
                }
                whitespace(bytes, &mut at);
            }
            let Some(group) = argument(&visible, &mut at, b'{', b'}') else {
                warnings.insert(format!("Citation command \\{command} could not be read completely; review its arguments."));
                break;
            };
            groups += 1;
            for key in group.split(',').map(str::trim).filter(|key| *key != "*") {
                if key.is_empty() || key.contains(['\\', '{', '}']) {
                    warnings.insert(format!("Citation command \\{command} contains an empty or generated key; review it manually."));
                } else {
                    keys.insert(key.to_owned());
                }
            }
            whitespace(bytes, &mut at);
            if !multi || !matches!(bytes.get(at), Some(b'[' | b'{')) {
                break;
            }
        }
        if groups == 0 {
            warnings.insert(format!("No literal keys were read from \\{command}."));
        }
    }
    (keys.into_iter().collect(), warnings.into_iter().collect())
}

pub fn inspect(store: &Store, ws: &str, request: CitationRequest) -> WorkbenchResult<Value> {
    scope(store, ws)?;
    if request.bibliographies.len() > 32 {
        return Err(WorkbenchError::invalid(
            "Choose at most 32 bibliography files",
        ));
    }
    let directory = match request.checkpoint_id.as_deref() {
        Some(id) => session_task_root(store, ws, id)?,
        None => root(store, ws)?,
    };
    let safe = files::SafeRoot::open(&directory)?;
    let mut entries = Vec::new();
    let mut versions = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut total = 0;
    for path in request.bibliographies.iter().collect::<BTreeSet<_>>() {
        let bytes = safe.read(path)?;
        total += bytes.len();
        if total > 4 * 1024 * 1024 {
            return Err(WorkbenchError::invalid("Bibliography index exceeds 4 MiB"));
        }
        let text = std::str::from_utf8(&bytes).map_err(err)?;
        versions.insert(path.clone(), hash(&bytes));
        if path.to_ascii_lowercase().ends_with(".bib") {
            for entry in parse_bibtex(text)? {
                entries.push(json!({"key":entry.key,"title":entry.fields.get("title"),"author":entry.fields.get("author"),"year":entry.fields.get("year"),"doi":entry.fields.get("doi"),"url":entry.fields.get("url"),"path":path,"line":text[..entry.start].lines().count().max(1),"warnings":entry.warnings}));
            }
        } else if path.ends_with(".json") {
            let items: Vec<Value> = serde_json::from_str(text).map_err(err)?;
            for item in items {
                let key = item["id"]
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| item["id"].as_i64().map(|id| id.to_string()))
                    .ok_or_else(|| WorkbenchError::invalid("CSL items require an id"))?;
                let author = item["author"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|a| a["family"].as_str().or(a["literal"].as_str()).unwrap_or(""))
                    .collect::<Vec<_>>()
                    .join("; ");
                entries.push(json!({"key":key,"title":item["title"],"author":author,"year":item["issued"]["date-parts"][0][0].to_string(),"doi":item["DOI"],"url":item["URL"],"path":path,"line":1,"warnings":[]}));
            }
        } else {
            return Err(WorkbenchError::invalid(
                "Choose BibTeX (.bib) or CSL JSON (.json) files",
            ));
        }
        if entries.len() > 10_000 {
            return Err(WorkbenchError::invalid(
                "Index exceeds 10,000 bibliography entries",
            ));
        }
    }
    let mut seen = BTreeSet::new();
    for entry in &entries {
        if !seen.insert(entry["key"].as_str().unwrap_or("")) {
            warnings.push(format!("Duplicate key: {}", entry["key"]));
        }
    }
    let source = if let Some(text) = request.source_text {
        bounded(&text, 2 * 1024 * 1024)?;
        text
    } else if let Some(path) = &request.source_path {
        let bytes = safe.read(path)?;
        versions.insert(path.clone(), hash(&bytes));
        String::from_utf8(bytes).map_err(err)?
    } else {
        String::new()
    };
    bounded(&source, 2 * 1024 * 1024)?;
    let (keys, parser_warnings) = scan_citations(&source);
    warnings.extend(parser_warnings);
    let missing = keys
        .iter()
        .filter(|key| !seen.contains(key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    Ok(
        json!({"entries":entries,"versions":versions,"sourceHash":hash(source.as_bytes()),"citations":keys,"missing":missing,"warnings":warnings,"coverage":"Literal TeX citation commands in the selected source and selected bibliography files only. Custom macros, other source files, and scientific support require review."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spaced_multiline_and_multicite_arguments_are_all_checked() {
        let text = "\\cite {one} \\citep \n [see {p. ] 2}] [3] {two} \\cites(see)(overall)[pre]{three}[post]{four} \\cite% comment\n{five} \\parencites{six}{seven}";
        let (keys, warnings) = scan_citations(text);
        assert_eq!(
            keys,
            ["five", "four", "one", "seven", "six", "three", "two"]
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        for text in [
            "\\cite [broken",
            "\\cites{known}[note]",
            "\\unknowncite{known}",
            "\\cite{\\generated}",
        ] {
            assert!(!scan_citations(text).1.is_empty(), "{text}");
        }
    }

    #[test]
    fn check_receipts_do_not_pass_missing_keys_after_spaces_or_in_later_groups() {
        use super::super::test_support::fixture;
        let f = fixture();
        fs::write(f.root.join("refs.bib"), "@article{known,title={Known}}").unwrap();
        checks::save_policy(
            &f.store,
            &f.ws,
            0,
            checks::CheckPolicy {
                enabled: true,
                source_path: "main.tex".into(),
                bibliographies: vec!["refs.bib".into()],
            },
        )
        .unwrap();
        for text in ["\\cite {missing}", "\\cites{known}{missing}"] {
            fs::write(f.root.join("main.tex"), text).unwrap();
            let receipt = checks::run(&f.store, &f.ws, None).unwrap().unwrap();
            assert_eq!(receipt.body["outcome"], "needs_attention");
            assert_eq!(receipt.body["details"]["missing"], json!(["missing"]));
        }
        fs::write(f.root.join("main.tex"), "\\cite[unterminated").unwrap();
        assert_eq!(
            checks::run(&f.store, &f.ws, None).unwrap().unwrap().body["outcome"],
            "needs_attention"
        );
    }
    #[test]
    fn keys_skip_comments_verbatim_and_all_entries_command() {
        assert_eq!(
            citation_keys(
                "% \\cite{hidden}\n\\verb|\\cite{literal}| \\citep[a]{one,two} \\nocite{*}"
            ),
            vec!["one", "two"]
        );
    }
}
