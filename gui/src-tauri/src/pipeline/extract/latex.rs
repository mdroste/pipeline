use super::*;

#[derive(Debug)]
pub(super) struct LatexReference {
    target: String,
    default_extensions: &'static [&'static str],
    recursive: bool,
}

pub(super) fn latex_without_comments(content: &str) -> String {
    let mut visible = String::with_capacity(content.len());
    for line in content.split_inclusive('\n') {
        let mut slash_count = 0usize;
        let mut comment_at = None;
        for (index, character) in line.char_indices() {
            if character == '%' && slash_count & 1 == 0 {
                comment_at = Some(index);
                break;
            }
            if character == '\\' {
                slash_count += 1;
            } else {
                slash_count = 0;
            }
        }
        if let Some(index) = comment_at {
            visible.push_str(&line[..index]);
            if line.ends_with('\n') {
                visible.push('\n');
            }
        } else {
            visible.push_str(line);
        }
    }
    visible
}

pub(super) fn latex_references(content: &str) -> Vec<LatexReference> {
    fn collect(
        output: &mut Vec<LatexReference>,
        content: &str,
        pattern: &str,
        capture: usize,
        extensions: &'static [&'static str],
        recursive: bool,
        comma_separated: bool,
    ) {
        let regex = Regex::new(pattern).expect("LaTeX dependency regex is invalid");
        for captures in regex.captures_iter(content) {
            let Some(value) = captures.get(capture) else {
                continue;
            };
            let values: Vec<&str> = if comma_separated {
                value.as_str().split(',').collect()
            } else {
                vec![value.as_str()]
            };
            output.extend(values.into_iter().filter_map(|value| {
                let target = value.trim().trim_matches('"');
                if target.is_empty()
                    || target.contains('#')
                    || target.contains('\\')
                    || target.contains("://")
                {
                    return None;
                }
                Some(LatexReference {
                    target: target.to_string(),
                    default_extensions: extensions,
                    recursive,
                })
            }));
        }
    }

    let visible = latex_without_comments(content);
    let mut references = Vec::new();
    collect(
        &mut references,
        &visible,
        r"\\(?:input|include|subfile)\s*\{([^}]+)\}",
        1,
        &["tex"],
        true,
        false,
    );
    collect(
        &mut references,
        &visible,
        r"\\includegraphics\*?(?:\s*\[[^\]]*\])?\s*\{([^}]+)\}",
        1,
        &["pdf", "png", "jpg", "jpeg", "webp", "eps", "svg"],
        false,
        false,
    );
    let graphics_paths = Regex::new(r"\\graphicspath\s*\{((?:\s*\{[^{}]*\})+)\s*\}")
        .expect("LaTeX graphicspath regex is invalid")
        .captures_iter(&visible)
        .flat_map(|capture| {
            Regex::new(r"\{([^{}]+)\}")
                .expect("LaTeX graphicspath entry regex is invalid")
                .captures_iter(capture.get(1).map(|value| value.as_str()).unwrap_or(""))
                .filter_map(|entry| entry.get(1).map(|value| value.as_str().trim().to_string()))
                .collect::<Vec<_>>()
        })
        .filter(|path| !path.is_empty() && !path.contains('#') && !path.contains('\\'))
        .collect::<Vec<_>>();
    let graphics = Regex::new(r"\\includegraphics\*?(?:\s*\[[^\]]*\])?\s*\{([^}]+)\}")
        .expect("LaTeX graphics regex is invalid");
    for capture in graphics.captures_iter(&visible) {
        let Some(target) = capture.get(1).map(|value| value.as_str().trim()) else {
            continue;
        };
        for directory in &graphics_paths {
            references.push(LatexReference {
                target: format!(
                    "{}/{}",
                    directory.trim_end_matches('/'),
                    target.trim_start_matches('/')
                ),
                default_extensions: &["pdf", "png", "jpg", "jpeg", "webp", "eps", "svg"],
                recursive: false,
            });
        }
    }
    let import =
        Regex::new(r"\\(?:import|subimport|inputfrom|includefrom)\s*\{([^}]+)\}\s*\{([^}]+)\}")
            .expect("LaTeX import regex is invalid");
    for capture in import.captures_iter(&visible) {
        let Some(directory) = capture.get(1).map(|value| value.as_str().trim()) else {
            continue;
        };
        let Some(file) = capture.get(2).map(|value| value.as_str().trim()) else {
            continue;
        };
        if directory.contains('#')
            || directory.contains('\\')
            || file.contains('#')
            || file.contains('\\')
        {
            continue;
        }
        references.push(LatexReference {
            target: format!(
                "{}/{}",
                directory.trim_end_matches('/'),
                file.trim_start_matches('/')
            ),
            default_extensions: &["tex"],
            recursive: true,
        });
    }
    collect(
        &mut references,
        &visible,
        r"\\bibliography\s*\{([^}]+)\}",
        1,
        &["bib"],
        false,
        true,
    );
    collect(
        &mut references,
        &visible,
        r"\\addbibresource(?:\s*\[[^\]]*\])?\s*\{([^}]+)\}",
        1,
        &["bib"],
        false,
        false,
    );
    collect(
        &mut references,
        &visible,
        r"\\usepackage(?:\s*\[[^\]]*\])?\s*\{([^}]+)\}",
        1,
        &["sty"],
        true,
        true,
    );
    collect(
        &mut references,
        &visible,
        r"\\documentclass(?:\s*\[[^\]]*\])?\s*\{([^}]+)\}",
        1,
        &["cls"],
        true,
        false,
    );
    collect(
        &mut references,
        &visible,
        r"\\(?:lstinputlisting|verbatiminput)(?:\s*\[[^\]]*\])?\s*\{([^}]+)\}",
        1,
        &[],
        false,
        false,
    );
    collect(
        &mut references,
        &visible,
        r"\\inputminted(?:\s*\[[^\]]*\])?\s*\{[^}]+\}\s*\{([^}]+)\}",
        1,
        &[],
        false,
        false,
    );
    references
}

pub(super) fn resolve_latex_reference(
    reference: &LatexReference,
    current_dir: &Path,
    project_root: &Path,
) -> Option<PathBuf> {
    let raw = Path::new(&reference.target);
    if raw.is_absolute() {
        return None;
    }
    let mut relative_candidates = vec![raw.to_path_buf()];
    if raw.extension().is_none() {
        relative_candidates.extend(reference.default_extensions.iter().map(|extension| {
            let mut candidate = raw.to_path_buf();
            candidate.set_extension(extension);
            candidate
        }));
    }
    for base in [current_dir, project_root] {
        for relative in &relative_candidates {
            let candidate = base.join(relative);
            let Ok(canonical) = candidate.canonicalize() else {
                continue;
            };
            if canonical.starts_with(project_root) && canonical.is_file() {
                return Some(canonical);
            }
        }
    }
    None
}

pub(super) fn copy_scoped_source_file(
    source: &Path,
    destination: &Path,
    total_bytes: &mut u64,
) -> Result<(), String> {
    let mut input = open_regular_file(source)?;
    let size = input
        .metadata()
        .map_err(|error| format!("Failed to inspect {}: {error}", source.display()))?
        .len();
    if size > MAX_SCOPED_SOURCE_FILE_BYTES {
        return Err(format!(
            "Referenced source file '{}' exceeds the {} MB per-file limit",
            source.display(),
            MAX_SCOPED_SOURCE_FILE_BYTES / (1024 * 1024)
        ));
    }
    if total_bytes.saturating_add(size) > MAX_SCOPED_SOURCE_TOTAL_BYTES {
        return Err(format!(
            "Referenced source files exceed the {} MB total limit",
            MAX_SCOPED_SOURCE_TOTAL_BYTES / (1024 * 1024)
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create private source directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(|error| {
            format!(
                "Failed to create private source file '{}': {error}",
                destination.display()
            )
        })?;
    let mut limited = std::io::Read::take(&mut input, MAX_SCOPED_SOURCE_FILE_BYTES + 1);
    let copied = std::io::copy(&mut limited, &mut output)
        .map_err(|error| format!("Failed to stage '{}': {error}", source.display()))?;
    if copied > MAX_SCOPED_SOURCE_FILE_BYTES {
        return Err(format!(
            "Referenced source file '{}' exceeds the {} MB per-file limit",
            source.display(),
            MAX_SCOPED_SOURCE_FILE_BYTES / (1024 * 1024)
        ));
    }
    if copied != size {
        return Err(format!(
            "Source file '{}' changed while it was being staged",
            source.display()
        ));
    }
    *total_bytes = total_bytes.saturating_add(copied);
    Ok(())
}

pub(super) fn stage_latex_project(
    main_file: &Path,
    project_root: &Path,
    destination_root: &Path,
) -> Result<(), String> {
    let main_file = main_file
        .canonicalize()
        .map_err(|error| format!("Failed to resolve {}: {error}", main_file.display()))?;
    let project_root = project_root
        .canonicalize()
        .map_err(|error| format!("Failed to resolve {}: {error}", project_root.display()))?;
    if !main_file.starts_with(&project_root) {
        return Err("The selected LaTeX file is outside its project root".to_string());
    }

    let mut pending = VecDeque::from([(main_file, true)]);
    let mut visited = HashSet::new();
    let mut total_bytes = 0u64;
    while let Some((source, recurse)) = pending.pop_front() {
        if !visited.insert(source.clone()) {
            continue;
        }
        if visited.len() > MAX_SCOPED_SOURCE_FILES {
            return Err(format!(
                "LaTeX source closure exceeds the {MAX_SCOPED_SOURCE_FILES}-file limit"
            ));
        }
        let relative = source
            .strip_prefix(&project_root)
            .map_err(|_| "A referenced LaTeX source escaped the project root".to_string())?;
        copy_scoped_source_file(&source, &destination_root.join(relative), &mut total_bytes)?;
        if !recurse {
            continue;
        }
        let content = match read_utf8_capped(&source, MAX_LATEX_SIZE) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let current_dir = source.parent().unwrap_or(&project_root);
        for reference in latex_references(&content) {
            if let Some(path) = resolve_latex_reference(&reference, current_dir, &project_root) {
                pending.push_back((path, reference.recursive));
            }
        }
    }
    Ok(())
}

pub(crate) fn stage_selected_source(
    selected: &Path,
    input_mode: &str,
    private_root: &Path,
) -> Result<ScopedSourceContext, String> {
    if input_mode == "none" || selected.as_os_str().is_empty() {
        return Ok(ScopedSourceContext::default());
    }
    if input_mode == "folder" {
        let selected = selected
            .canonicalize()
            .map_err(|error| format!("Failed to resolve selected folder: {error}"))?;
        if !selected.is_dir() {
            return Err(format!(
                "Selected folder '{}' is not a directory",
                selected.display()
            ));
        }
        return Ok(ScopedSourceContext {
            source_path: Some(selected.clone()),
            read_root: Some(selected),
        });
    }

    let latex_main = if selected.is_dir() {
        find_main_tex(selected)
    } else if ext_eq(selected, "tex") {
        Some(selected.to_path_buf())
    } else {
        None
    };
    let destination_root = private_root.join("source");
    fs::create_dir_all(&destination_root).map_err(|error| {
        format!(
            "Failed to create private source root '{}': {error}",
            destination_root.display()
        )
    })?;
    if let Some(main_file) = latex_main {
        let project_root = if selected.is_dir() {
            selected
        } else {
            selected.parent().unwrap_or_else(|| Path::new("."))
        };
        stage_latex_project(&main_file, project_root, &destination_root)?;
        return Ok(ScopedSourceContext {
            source_path: Some(destination_root.clone()),
            read_root: Some(destination_root),
        });
    }

    let name = selected
        .file_name()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| std::ffi::OsStr::new("source"));
    let destination = destination_root.join(name);
    let mut total_bytes = 0u64;
    copy_scoped_source_file(selected, &destination, &mut total_bytes)?;
    Ok(ScopedSourceContext {
        source_path: Some(destination),
        read_root: Some(destination_root),
    })
}

/// Extract text from a .tex file, resolving \input{} and \include{} recursively.
pub(super) fn extract_latex(
    path: &Path,
    depth: usize,
    root_dir: &Path,
    warnings: &mut Vec<String>,
) -> Result<String, String> {
    if depth > 10 {
        warnings.push("LaTeX \\input{} nesting exceeds 10 levels — possible circular includes. Output may be incomplete.".to_string());
        return Ok(String::new());
    }

    let content = read_utf8_capped(path, MAX_LATEX_SIZE)?;

    let parent = path.parent().unwrap_or(Path::new("."));
    let re = Regex::new(r"\\(?:input|include)\{([^}]+)\}").expect("LaTeX include regex is invalid");

    let mut result = String::new();
    let mut last_end = 0;

    for cap in re.captures_iter(&content) {
        let full_match = cap.get(0).unwrap();
        result.push_str(&content[last_end..full_match.start()]);

        let include_name = &cap[1];
        let mut include_path = parent.join(include_name);
        if include_path.extension().is_none() {
            include_path.set_extension("tex");
        }

        // Validate the resolved path stays within the root directory to prevent
        // path traversal via malicious \input{../../../etc/passwd}
        let canonical = include_path.canonicalize().ok();
        let safe = canonical
            .as_ref()
            .is_some_and(|resolved| resolved.starts_with(root_dir));

        if safe && include_path.is_file() {
            match extract_latex(&include_path, depth + 1, root_dir, warnings) {
                Ok(included) => result.push_str(&included),
                Err(_) => result.push_str(&content[full_match.start()..full_match.end()]),
            }
        } else {
            // Diagnose why the include failed
            if !include_path.exists() {
                warnings.push(format!(
                    "\\input{{{}}} — file not found. If this is a multi-file project, select the project folder instead of a single .tex file.",
                    include_name
                ));
            } else if canonical.is_some() && !safe {
                warnings.push(format!(
                    "\\input{{{}}} — resolves outside the project directory (blocked). Select the project folder instead of a single .tex file.",
                    include_name
                ));
            }
            result.push_str(&content[full_match.start()..full_match.end()]);
        }

        if result.len() > MAX_LATEX_SIZE {
            return Err(format!(
                "LaTeX extraction exceeded {} byte limit — paper has too many includes",
                MAX_LATEX_SIZE
            ));
        }

        last_end = full_match.end();
    }

    result.push_str(&content[last_end..]);
    Ok(result)
}
