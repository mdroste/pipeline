//! Compile the captured dependency closure, never the live project. Parent-
//! relative references that staging moved under `_external` are rewritten only
//! in the disposable build copy using the capture map's validated targets.
use super::*;

pub(super) fn copy_project(root: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir(destination).map_err(|e| e.to_string())?;
    let mut pending = vec![root.to_path_buf()];
    let mut total = 0;
    let mut count = 0;
    let mut walk = crate::safety::WalkBudget::new_cancellable("Beamer build input");
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).map_err(|e| e.to_string())? {
            walk.entry()?;
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            let path = entry.path();
            if kind.is_dir() {
                walk.directory()?;
                pending.push(path);
            } else if kind.is_file() {
                count += 1;
                if count > MAX_SCOPED_SOURCE_FILES + 8 {
                    return Err("Beamer build has too many captured files".into());
                }
                let target = destination.join(path.strip_prefix(root).map_err(|e| e.to_string())?);
                latex::copy_scoped_source_file(
                    &path,
                    &target,
                    &mut total,
                    MAX_SCOPED_SOURCE_FILE_BYTES,
                )?;
            } else {
                return Err("Beamer build source contains a symbolic link or special file".into());
            }
        }
    }
    Ok(())
}

pub(super) fn rebase_references(root: &Path, destination: &Path) -> Result<(), String> {
    let mut pending = vec![destination.to_path_buf()];
    let reference = Regex::new(r"\\(?:input|include|subfile|includegraphics\*?|addbibresource|lstinputlisting|verbatiminput)(?:\s*\[[^\]]*\])?\s*\{([^}]+)\}|\\input\s+([^\s{}%\\]+)").unwrap();
    let imports =
        Regex::new(r"\\(?:import|subimport|inputfrom|includefrom)\s*\{([^}]+)\}\s*\{([^}]+)\}")
            .unwrap();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if !["tex", "sty", "cls"]
                .iter()
                .any(|extension| ext_eq(&path, extension))
            {
                continue;
            }
            let relative = path.strip_prefix(destination).map_err(|e| e.to_string())?;
            let original_dir = root.join(relative).parent().unwrap().to_path_buf();
            let raw = fs::read(&path).map_err(|e| e.to_string())?;
            // Editing ASCII filenames inside UTF-8 sources leaves their authored
            // encoding intact. Legacy sources remain unchanged and compile with
            // the original inputenc declaration.
            let Ok(source) = String::from_utf8(raw) else {
                continue;
            };
            let mapped = |name: &str| -> Option<String> {
                let captured = staged::reference(root, &original_dir, name)
                    .flatten()
                    .or_else(|| staged::reference(root, root, name).flatten())?;
                let canonical_root = root.canonicalize().ok()?;
                let relative = captured.strip_prefix(canonical_root).ok()?;
                let text = relative.to_str()?.replace('\\', "/");
                // Do not introduce TeX syntax through a source filename.
                (!text
                    .chars()
                    .any(|c| matches!(c, '{' | '}' | '%' | '#' | '\\')))
                .then_some(text)
            };
            let mut edits = Vec::new();
            for capture in reference.captures_iter(&source) {
                let value = capture.get(1).or_else(|| capture.get(2)).unwrap();
                if let Some(mut target) = mapped(value.as_str().trim()) {
                    if capture[0]
                        .strip_prefix(r"\include")
                        .is_some_and(|tail| tail.trim_start().starts_with('{'))
                    {
                        target = target.strip_suffix(".tex").unwrap_or(&target).to_string();
                    }
                    edits.push((value.start(), value.end(), target));
                }
            }
            for capture in imports.captures_iter(&source) {
                let name = format!(
                    "{}/{}",
                    capture[1].trim().trim_end_matches('/'),
                    capture[2].trim()
                );
                if let Some(target) = mapped(&name) {
                    let found = capture.get(0).unwrap();
                    edits.push((found.start(), found.end(), format!("\\input{{{target}}}")));
                }
            }
            edits.sort_by_key(|(start, _, _)| *start);
            let mut rewritten = source;
            for (start, end, value) in edits.into_iter().rev() {
                rewritten.replace_range(start..end, &value);
            }
            fs::write(path, rewritten).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
