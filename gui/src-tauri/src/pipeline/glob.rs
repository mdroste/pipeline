//! Minimal glob matching for fan-out (map) steps.
//!
//! Supports `*` (any chars except `/`), `?` (one char except `/`), and `**/`
//! (zero or more path segments). Patterns match a file's path *relative* to the
//! fan-out root, with forward slashes. Compiled to a regex (the crate already
//! depends on `regex`) rather than pulling in a glob crate.

use regex::Regex;
use std::path::Path;

/// Translate a glob into an anchored regex string.
pub fn glob_to_regex(pattern: &str) -> String {
    let mut re = String::from("^");
    let mut rest = pattern;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix("**/") {
            re.push_str("(?:.*/)?");
            rest = tail;
            continue;
        }
        if let Some(tail) = rest.strip_prefix("**") {
            re.push_str(".*");
            rest = tail;
            continue;
        }
        let c = rest.chars().next().unwrap();
        match c {
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            // Escape regex metacharacters that are literal in globs.
            '.' | '+' | '(' | ')' | '|' | '^' | '$' | '{' | '}' | '[' | ']' | '\\' => {
                re.push('\\');
                re.push(c);
            }
            _ => re.push(c),
        }
        rest = &rest[c.len_utf8()..];
    }
    re.push('$');
    re
}

/// Whether `rel_path` (forward slashes, relative to the fan-out root) matches
/// `pattern`. A malformed pattern matches nothing.
pub fn glob_match(pattern: &str, rel_path: &str) -> bool {
    match Regex::new(&glob_to_regex(pattern)) {
        Ok(re) => re.is_match(rel_path),
        Err(_) => false,
    }
}

/// Expand `pattern` against `root`, returning matching files' absolute paths
/// (forward slashes), sorted, capped at `max`. Skips hidden files and symlinks;
/// the recursive walk is bounded defensively. Returns `(matches, truncated)`.
pub fn expand(root: &Path, pattern: &str, max: usize) -> (Vec<String>, bool) {
    const MAX_WALK: usize = 100_000;
    let mut matches: Vec<String> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut visited = 0usize;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > MAX_WALK {
                matches.sort();
                return (matches, true);
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if glob_match(pattern, &rel_str) {
                matches.push(path.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    matches.sort();
    let truncated = matches.len() > max;
    matches.truncate(max);
    (matches, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_matches_within_segment() {
        assert!(glob_match("*.tex", "main.tex"));
        assert!(!glob_match("*.tex", "chapters/main.tex")); // * doesn't cross /
        assert!(glob_match("chapters/*.tex", "chapters/intro.tex"));
        assert!(!glob_match("chapters/*.tex", "chapters/sub/intro.tex"));
    }

    #[test]
    fn doublestar_crosses_segments() {
        assert!(glob_match("**/*.tex", "main.tex")); // **/ matches zero segments
        assert!(glob_match("**/*.tex", "chapters/intro.tex"));
        assert!(glob_match("**/*.tex", "a/b/c/deep.tex"));
        assert!(glob_match("src/**/*.rs", "src/a/b.rs"));
        assert!(glob_match("src/**/*.rs", "src/b.rs"));
        assert!(!glob_match("src/**/*.rs", "other/b.rs"));
    }

    #[test]
    fn question_matches_one_char() {
        assert!(glob_match("fig?.png", "fig1.png"));
        assert!(!glob_match("fig?.png", "fig10.png"));
    }

    #[test]
    fn literals_are_escaped() {
        assert!(glob_match("a.b", "a.b"));
        assert!(!glob_match("a.b", "axb")); // '.' is literal, not "any char"
        assert!(glob_match("data (1).csv", "data (1).csv"));
    }

    #[test]
    fn glob_to_regex_shape() {
        assert_eq!(glob_to_regex("*.tex"), "^[^/]*\\.tex$");
        assert_eq!(glob_to_regex("**/*.md"), "^(?:.*/)?[^/]*\\.md$");
    }
}
