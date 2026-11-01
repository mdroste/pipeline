//! Text search over a project folder, for the file browser. It walks with the
//! same no-follow listing as the tree, skips what task capture ignores, and
//! stops at its limits with a report of what it did not read.
use super::files::{EntryKind, SafeRoot};
use super::*;

const MAX_MATCHES: usize = 500;
const MAX_VISITED: usize = 20_000;
const MAX_LISTED: usize = 5_000;
const MAX_LINE_CHARS: usize = 240;
const TIME_LIMIT: Duration = Duration::from_secs(8);
const BINARY_CHECK: u64 = 8192;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileSearchRequest {
    pub workspace_id: String,
    pub checkpoint_id: Option<String>,
    pub query: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchMatch {
    pub path: String,
    pub line: usize,
    pub text: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchResult {
    pub matches: Vec<FileSearchMatch>,
    pub searched_files: usize,
    pub skipped_files: usize,
    pub truncated: bool,
}
/// The file's bytes, or `None` for a binary file. Checking the head first keeps
/// a folder of PDFs from using up the time limit.
fn read_text_candidate(safe: &SafeRoot, path: &str) -> WorkbenchResult<Option<Vec<u8>>> {
    let (mut file, _) = safe
        .open_read(path)?
        .ok_or_else(|| WorkbenchError::invalid(format!("Missing file: {path}")))?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(BINARY_CHECK)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.contains(&0) {
        return Ok(None);
    }
    Read::by_ref(&mut file)
        .take(MAX_FILE - bytes.len() as u64)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    Ok(Some(bytes))
}
/// The line around the match, so a long minified line still shows it.
fn excerpt(line: &str, lowered: &str, query: &str) -> String {
    let line = line.trim();
    if line.chars().count() <= MAX_LINE_CHARS {
        return line.to_string();
    }
    let at = lowered.find(query).unwrap_or(0);
    let before = lowered[..at].chars().count().saturating_sub(40);
    line.chars().skip(before).take(MAX_LINE_CHARS).collect()
}
pub fn search_files(folder: &Path, query: &str) -> WorkbenchResult<FileSearchResult> {
    search_within(folder, query, TIME_LIMIT)
}
fn search_within(
    folder: &Path,
    query: &str,
    time_limit: Duration,
) -> WorkbenchResult<FileSearchResult> {
    let query = query.trim().to_lowercase();
    if query.chars().count() < 2 || query.len() > 500 {
        return Err(WorkbenchError::invalid(
            "Search for at least two characters",
        ));
    }
    let safe = SafeRoot::open(folder)?;
    let started = Instant::now();
    let mut result = FileSearchResult {
        matches: Vec::new(),
        searched_files: 0,
        skipped_files: 0,
        truncated: false,
    };
    let mut folders = vec![String::new()];
    let mut visited = 0;
    'walk: while let Some(directory) = folders.pop() {
        // The limit also reaches inside a listing, which can outlast it alone.
        let listing = safe.list_until(&directory, MAX_LISTED, |_| started.elapsed() >= time_limit);
        let (entries, partial) = match listing {
            Ok(listing) => listing,
            Err(_) => {
                result.skipped_files += 1;
                continue;
            }
        };
        result.truncated |= partial;
        for (name, stat) in entries {
            visited += 1;
            if visited > MAX_VISITED
                || result.matches.len() >= MAX_MATCHES
                || started.elapsed() > time_limit
            {
                result.truncated = true;
                break 'walk;
            }
            let path = if directory.is_empty() {
                name
            } else {
                format!("{directory}/{name}")
            };
            if ignored(&path, &[]) {
                continue;
            }
            match stat.kind {
                EntryKind::Folder => folders.push(path),
                EntryKind::File if stat.size <= MAX_FILE => {
                    let bytes = match read_text_candidate(&safe, &path) {
                        Ok(Some(bytes)) => bytes,
                        Ok(None) => continue,
                        Err(_) => {
                            result.skipped_files += 1;
                            continue;
                        }
                    };
                    let Ok(text) = std::str::from_utf8(&bytes) else {
                        result.skipped_files += 1;
                        continue;
                    };
                    result.searched_files += 1;
                    if !text.to_lowercase().contains(&query) {
                        continue;
                    }
                    for (index, line) in text.lines().enumerate() {
                        let lowered = line.to_lowercase();
                        if lowered.contains(&query) {
                            result.matches.push(FileSearchMatch {
                                path: path.clone(),
                                line: index + 1,
                                text: excerpt(line, &lowered, &query),
                            });
                            if result.matches.len() >= MAX_MATCHES {
                                break;
                            }
                        }
                    }
                }
                EntryKind::File => result.skipped_files += 1,
                _ => {}
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_out_of_time_inside_a_listing_is_reported() {
        let temp = tempfile::tempdir().unwrap();
        let result = search_within(temp.path(), "anything", Duration::ZERO).unwrap();
        assert!(result.truncated);
        assert_eq!(result.searched_files, 0);
        let result = search_within(temp.path(), "anything", TIME_LIMIT).unwrap();
        assert!(!result.truncated);
    }

    #[test]
    fn finds_text_case_insensitively_and_skips_what_it_should() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("code")).unwrap();
        fs::create_dir_all(root.join("node_modules/x")).unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join("code/run.do"), "first\nRegress Wage\n").unwrap();
        fs::write(root.join("notes.md"), "regress again").unwrap();
        fs::write(root.join("image.bin"), b"regress\0binary").unwrap();
        fs::write(root.join("node_modules/x/a.js"), "regress").unwrap();
        fs::write(root.join(".git/config"), "regress").unwrap();
        fs::write(root.join("latin.txt"), b"regress caf\xe9").unwrap();
        let result = search_files(root, "REGRESS").unwrap();
        let mut hits: Vec<_> = result
            .matches
            .iter()
            .map(|m| (m.path.as_str(), m.line))
            .collect();
        hits.sort();
        assert_eq!(hits, [("code/run.do", 2), ("notes.md", 1)]);
        assert_eq!(result.searched_files, 2);
        assert_eq!(result.skipped_files, 1);
        assert!(!result.truncated);
        assert!(search_files(root, "x").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn never_follows_a_link_out_of_the_project() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "needle").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
        assert!(search_files(&root, "needle").unwrap().matches.is_empty());
    }

    #[test]
    fn long_lines_keep_the_match_visible() {
        let line = format!("{}needle{}", "a".repeat(1000), "b".repeat(1000));
        let shown = excerpt(&line, &line, "needle");
        assert!(shown.contains("needle"));
        assert_eq!(shown.chars().count(), MAX_LINE_CHARS);
    }
}
