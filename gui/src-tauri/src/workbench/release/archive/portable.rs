//! Preflight names before copying anything onto a case-insensitive filesystem.
use super::safe_archive_name;
use crate::workbench::store::{WorkbenchError, WorkbenchResult};
use std::collections::HashMap;
use std::fs::File;

fn portable_component(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        });
    !name.is_empty()
        && !matches!(name, "." | "..")
        && !name.ends_with(['.', ' '])
        && name.encode_utf16().count() <= 255
        && !name
            .chars()
            .any(|c| c < ' ' || matches!(c, '<' | '>' | ':' | '"' | '\\' | '|' | '?' | '*'))
        && !device
}

#[derive(Default)]
pub(in crate::workbench::release) struct PortablePaths {
    // Include parent folders: A/x and a/y also collide, as do a and a/x.
    paths: HashMap<String, (String, bool)>,
}

impl PortablePaths {
    pub(in crate::workbench::release) fn insert(&mut self, name: &str) -> WorkbenchResult<()> {
        if !safe_archive_name(name) {
            return Err(WorkbenchError::invalid(format!(
                "Archive contains an unsafe traversal path: {name:?}"
            )));
        }
        if !name.split('/').all(portable_component) {
            return Err(WorkbenchError::invalid(format!("The filename {name:?} cannot be restored on Windows and macOS. Rename the source file before creating a portable backup.")));
        }
        let mut prefix = String::new();
        let components: Vec<_> = name.split('/').collect();
        for (index, part) in components.iter().enumerate() {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            let file = index + 1 == components.len();
            let key = prefix.to_uppercase();
            if let Some((old, old_file)) = self.paths.get(&key) {
                if old != &prefix || *old_file || file {
                    return Err(WorkbenchError::invalid(format!("Archive contains duplicate or conflicting paths {old:?} and {prefix:?}. Rename the source files or folders so their names also differ without letter case.")));
                }
            } else {
                self.paths.insert(key, (prefix.clone(), file));
            }
        }
        Ok(())
    }
}

pub(in crate::workbench::release) fn validate_entry_paths(
    archive: &mut zip::ZipArchive<File>,
) -> WorkbenchResult<()> {
    let mut paths = PortablePaths::default();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| WorkbenchError::invalid(format!("Invalid archive entry: {e}")))?;
        let name = entry
            .name()
            .map_err(|e| WorkbenchError::invalid(format!("Invalid archive entry name: {e}")))?;
        paths.insert(&name)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_refused_by_windows_are_reported_on_every_host() {
        for name in [
            "notes?.txt",
            "a:b.txt",
            "CON.txt",
            "nul",
            "LPT9.csv",
            "COM¹.txt",
            "trailing.",
            "trailing ",
            "a/./b",
            "a//b",
            "control\u{1}.txt",
        ] {
            assert!(
                PortablePaths::default()
                    .insert(&format!("conversation-files/session/{name}"))
                    .is_err(),
                "{name}"
            );
        }
        PortablePaths::default()
            .insert("conversation-files/session/β result.csv")
            .unwrap();
    }

    #[test]
    fn case_aliases_and_file_directory_conflicts_are_refused_before_extraction() {
        for pair in [
            ["a/Results.csv", "a/results.csv"],
            ["A/x", "a/y"],
            ["a", "a/x"],
            ["a/x", "a"],
            ["a/x", "a/x"],
        ] {
            let mut paths = PortablePaths::default();
            paths.insert(pair[0]).unwrap();
            assert!(paths.insert(pair[1]).is_err(), "{pair:?}");
        }
        let mut paths = PortablePaths::default();
        paths.insert("a/x").unwrap();
        paths.insert("a/y").unwrap();
    }
}
