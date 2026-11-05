//! Match tool-reported paths to the exact build's declared source files.
use serde_json::Value;

fn absolute(path: &str) -> bool {
    path.starts_with('/')
        || (path.as_bytes().get(1) == Some(&b':')
            && path.as_bytes().get(2) == Some(&b'/')
            && path.as_bytes()[0].is_ascii_alphabetic())
}

fn parts(path: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts)
}

pub(super) fn source_path(input: &str, cwd: &str, manifest: &Value) -> Option<String> {
    let root = crate::file_viewer::normalize_external_path(cwd);
    if !absolute(&root) {
        return None;
    }
    let windows = root.starts_with("//") || root.as_bytes().get(1) == Some(&b':');
    let same = |left: &str, right: &str| {
        if windows {
            left.eq_ignore_ascii_case(right)
        } else {
            left == right
        }
    };
    let input = crate::file_viewer::normalize_external_path(input.trim());
    let full = if absolute(&input) {
        input
    } else {
        format!("{}/{input}", root.trim_end_matches('/'))
    };
    let base = parts(root.trim_end_matches('/'))?;
    let source = parts(&full)?;
    if source.len() <= base.len() || !source.iter().zip(&base).all(|(a, b)| same(a, b)) {
        return None;
    }
    let relative = source[base.len()..].join("/");
    manifest["files"].as_array()?.iter().find_map(|file| {
        let declared = file["path"].as_str()?;
        super::files::relative(declared).ok()?;
        same(declared, &relative).then(|| declared.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_verbatim_and_relative_inputs_match_the_declared_windows_source() {
        let manifest = json!({"files":[{"path":"chapters/Introduction.tex"}]});
        for input in [
            r"C:\My Paper\chapters\Introduction.tex",
            "c:/My Paper/chapters/introduction.tex",
            r"\\?\C:\My Paper\chapters\Introduction.tex",
            "./chapters/Introduction.tex",
        ] {
            assert_eq!(
                source_path(input, r"\\?\C:\My Paper", &manifest).as_deref(),
                Some("chapters/Introduction.tex")
            );
        }
        assert!(source_path(
            r"C:\My Paper Elsewhere\chapters\Introduction.tex",
            r"\\?\C:\My Paper",
            &manifest
        )
        .is_none());
        assert!(source_path(
            "../chapters/Introduction.tex",
            r"\\?\C:\My Paper",
            &manifest
        )
        .is_none());
    }

    #[test]
    fn unc_and_unix_builds_keep_their_root_and_declared_file_boundaries() {
        let manifest = json!({"files":[{"path":"main.tex"}]});
        assert_eq!(
            source_path(
                r"\\server\share\paper\main.tex",
                r"\\?\UNC\server\share\paper",
                &manifest
            )
            .as_deref(),
            Some("main.tex")
        );
        assert_eq!(
            source_path("./main.tex", "/Users/me/paper", &manifest).as_deref(),
            Some("main.tex")
        );
        assert!(source_path("/Users/me/other/main.tex", "/Users/me/paper", &manifest).is_none());
        assert!(source_path("Main.tex", "/Users/me/paper", &manifest).is_none());
        assert!(source_path("undeclared.tex", "/Users/me/paper", &manifest).is_none());
    }
}
