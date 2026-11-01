//! The "Files read by code" check: literal paths that code reads, against the
//! package's files and the files its own code writes.

use super::{code_of_line, exports, file_name, regex, snippet};
use super::{Check, Inventory, Location, StataBlockComments, MAX_LOCATIONS};
use regex::Regex;
use std::sync::OnceLock;

/// `path` with `.` and `..` resolved, or `None` if it leaves the package.
fn normalized_path(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// Where a literal path may point: from the package root, the usual working
/// directory, and from the script's own folder. Scripts change directory, so a
/// leading `..` says little about where it starts.
fn package_paths(dir: &str, target: &str) -> Vec<String> {
    let target = target.replace('\\', "/").to_ascii_lowercase();
    let mut from_root = target.as_str();
    while let Some(rest) = from_root
        .strip_prefix("../")
        .or_else(|| from_root.strip_prefix("./"))
    {
        from_root = rest;
    }
    let mut paths: Vec<String> = normalized_path(from_root).into_iter().collect();
    if !dir.is_empty() {
        let beside = format!("{}/{target}", dir.to_ascii_lowercase());
        paths.extend(normalized_path(&beside));
    }
    paths
}

/// Statements that write the files they name. `using` alone says nothing:
/// `merge` and `append` read theirs, and `log` and `tabout` write theirs.
fn writes() -> &'static Regex {
    static WRITES: OnceLock<Regex> = OnceLock::new();
    regex(
        &WRITES,
        r"(?i)^\s*(?:(?:quietly|qui|noisily|noi|capture|cap)\s+)*(?:save|saveold|log\s+using|cmdlog\s+using|postfile|outfile|tabout|estimates\s+save|est\s+save)\b|^\s*(?:(?:quietly|qui|noisily|noi|capture|cap)\s+)*file\s+open\b.*\bwrite\b|\b(?:saveRDS|write_rds|write_dta|fwrite|writeLines|save\.image)\s*\(",
    )
}

/// Whether the text after an `open(...)` path gives a mode that writes.
fn opens_for_writing(after_path: &str) -> bool {
    static MODE: OnceLock<Regex> = OnceLock::new();
    regex(&MODE, r#"^\s*,\s*(?:mode\s*=\s*)?["'][rbt+]*[wax]"#).is_match(after_path)
}

/// Literal paths the package's own code creates, so that reading one later is
/// how derived data normally works.
fn written_paths(files: &[(String, String)]) -> std::collections::HashSet<String> {
    static QUOTED: OnceLock<Regex> = OnceLock::new();
    let quoted = regex(&QUOTED, r#"["']([^"'$`{}]+)["']"#);
    let mut written = std::collections::HashSet::new();
    for (path, text) in files {
        let dir = path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
        let mut comments = StataBlockComments::default();
        for line in text.lines() {
            let code = code_of_line(path, line, &mut comments);
            let mut record = |target: &str| {
                written.extend(package_paths(dir, target));
                // Stata adds this extension when a command leaves it out.
                if !file_name(target).contains('.') {
                    written.extend(package_paths(dir, &format!("{target}.dta")));
                }
            };
            if exports().is_match(&code) || writes().is_match(&code) {
                for captures in quoted.captures_iter(&code) {
                    record(&captures[1]);
                }
            } else {
                for captures in file_reads().captures_iter(&code) {
                    let end = captures.get(0).map_or(0, |whole| whole.end());
                    if opens_for_writing(&code[end..]) {
                        record(&captures[1]);
                    }
                }
            }
        }
    }
    written
}

/// Commands and calls that name a file to read, with its literal path.
fn file_reads() -> &'static Regex {
    static READS: OnceLock<Regex> = OnceLock::new();
    regex(
        &READS,
        r#"(?i)(?:\buse\s+|\busing\s+|\bdo\s+|\binclude\s+|\binsheet\s+using\s+|import\s+delimited\s+(?:using\s+)?|import\s+excel\s+(?:using\s+)?|read[._](?:csv|dta|table|delim|excel|rds|parquet)\w*\s*\(\s*|read_(?:csv|stata|excel|parquet)\s*\(\s*|source\s*\(\s*|load\s*\(\s*|open\s*\(\s*)["']([^"'$`{}]+\.[A-Za-z0-9]{1,6})["']"#,
    )
}

pub(super) fn referenced_file_checks(
    inventory: &Inventory,
    files: &[(String, String)],
    checks: &mut Vec<Check>,
) {
    let known: std::collections::HashSet<String> = inventory
        .files
        .iter()
        .map(|(path, _)| path.to_ascii_lowercase())
        .collect();
    let written = written_paths(files);
    let mut missing = Vec::new();
    for (path, text) in files {
        let dir = path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
        let mut comments = StataBlockComments::default();
        for (index, line) in text.lines().enumerate() {
            let code = code_of_line(path, line, &mut comments);
            // Lines that write outputs name files that need not exist yet.
            if exports().is_match(&code) || writes().is_match(&code) {
                continue;
            }
            for captures in file_reads().captures_iter(&code) {
                let target = captures[1].replace('\\', "/");
                if target.starts_with('/') || target.contains(':') || target.starts_with("~") {
                    continue;
                }
                let end = captures.get(0).map_or(0, |whole| whole.end());
                if opens_for_writing(&code[end..]) {
                    continue;
                }
                let left_out = |path: &str| {
                    inventory
                        .left_out_folders
                        .iter()
                        .any(|folder| path.starts_with(folder))
                };
                let exists = package_paths(dir, &target).iter().any(|candidate| {
                    known.contains(candidate)
                        || written.contains(candidate)
                        || left_out(candidate)
                        || known
                            .iter()
                            .any(|known| known.ends_with(&format!("/{candidate}")))
                });
                if !exists {
                    missing.push(Location {
                        path: path.clone(),
                        line: Some(index + 1),
                        snippet: snippet(line),
                    });
                }
            }
            if missing.len() >= MAX_LOCATIONS {
                break;
            }
        }
    }
    // A partial file list can't show that anything is missing.
    if inventory.truncated {
        checks.push(Check {
            id: "referenced_files",
            label: "Files read by code are in the package",
            status: "info",
            detail: "The package has more files than this check lists, so files read by code could not all be looked up.".into(),
            locations: Vec::new(),
        });
        return;
    }
    checks.push(Check {
        id: "referenced_files",
        label: "Files read by code are in the package",
        status: if missing.is_empty() { "pass" } else { "warn" },
        detail: if missing.is_empty() {
            "Every literal file path read by code was found. Paths built from macros or variables are not checked.".into()
        } else {
            "Code reads files that are not in the package. If they are restricted or too large to deposit, the README must say how to obtain them.".into()
        },
        locations: missing,
    });
}
