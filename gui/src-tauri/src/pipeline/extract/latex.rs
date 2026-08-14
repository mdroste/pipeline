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

/// How many directory levels above the selected project root an explicitly
/// referenced file may live and still be read (e.g. `\input{../output/x.tex}`
/// from a `draft/dev` selection). Everything further away stays blocked.
pub(super) const MAX_EXTERNAL_PARENT_LEVELS: usize = 3;

/// A referenced file outside the selected directory is readable only when it
/// stays near the project: within `MAX_EXTERNAL_PARENT_LEVELS` ancestors of
/// the canonical project root, and never bounded by the filesystem root
/// itself (a shallow project must not open the whole disk).
pub(super) fn within_external_read_bound(canonical: &Path, project_root: &Path) -> bool {
    let mut ancestor = project_root.to_path_buf();
    for _ in 0..MAX_EXTERNAL_PARENT_LEVELS {
        if !ancestor.pop() || ancestor.parent().is_none() {
            return false;
        }
        if canonical.starts_with(&ancestor) {
            return true;
        }
    }
    false
}

fn external_reference_extension_allowed(reference: &LatexReference, resolved: &Path) -> bool {
    !reference.default_extensions.is_empty()
        && reference
            .default_extensions
            .iter()
            .any(|extension| ext_eq(resolved, extension))
}

#[derive(Debug)]
pub(super) enum ResolvedLatexReference {
    /// Canonical file inside the selected project directory.
    Internal(PathBuf),
    /// Canonical file outside the selected directory but explicitly
    /// referenced, of the expected file type, and within the read bound.
    External(PathBuf),
}

pub(super) fn resolve_latex_reference(
    reference: &LatexReference,
    current_dir: &Path,
    project_root: &Path,
) -> Option<ResolvedLatexReference> {
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
    // A match inside the selected directory always wins over an equally
    // valid resolution outside it.
    for base in [current_dir, project_root] {
        for relative in &relative_candidates {
            let candidate = base.join(relative);
            let Ok(canonical) = candidate.canonicalize() else {
                continue;
            };
            if canonical.starts_with(project_root) && canonical.is_file() {
                return Some(ResolvedLatexReference::Internal(canonical));
            }
        }
    }
    for base in [current_dir, project_root] {
        for relative in &relative_candidates {
            let candidate = base.join(relative);
            let Ok(canonical) = candidate.canonicalize() else {
                continue;
            };
            if canonical.is_file()
                && external_reference_extension_allowed(reference, &canonical)
                && within_external_read_bound(&canonical, project_root)
            {
                return Some(ResolvedLatexReference::External(canonical));
            }
        }
    }
    None
}

pub(super) fn copy_scoped_source_file(
    source: &Path,
    destination: &Path,
    total_bytes: &mut u64,
    per_file_cap: u64,
) -> Result<(), String> {
    let mut input = open_regular_file(source)?;
    let size = input
        .metadata()
        .map_err(|error| format!("Failed to inspect {}: {error}", source.display()))?
        .len();
    if size > per_file_cap {
        return Err(format!(
            "Source file '{}' exceeds the {} MB staging limit",
            source.display(),
            per_file_cap / (1024 * 1024)
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
    let mut limited = std::io::Read::take(&mut input, per_file_cap + 1);
    let copied = std::io::copy(&mut limited, &mut output)
        .map_err(|error| format!("Failed to stage '{}': {error}", source.display()))?;
    if copied > per_file_cap {
        return Err(format!(
            "Source file '{}' exceeds the {} MB staging limit",
            source.display(),
            per_file_cap / (1024 * 1024)
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

    let mut pending = VecDeque::from([(main_file.clone(), true)]);
    let mut visited = HashSet::new();
    let mut total_bytes = 0u64;
    let mut skipped = Vec::new();
    while let Some((source, recurse)) = pending.pop_front() {
        if !visited.insert(source.clone()) {
            continue;
        }
        if visited.len() > MAX_SCOPED_SOURCE_FILES {
            return Err(format!(
                "LaTeX source closure exceeds the {MAX_SCOPED_SOURCE_FILES}-file limit"
            ));
        }
        // An oversized referenced file (a scanned figure, a data set) costs
        // that one file, not the whole source view. Only the main file over
        // the cap remains fatal.
        if source != main_file {
            let size = fs::metadata(&source).map(|metadata| metadata.len()).ok();
            if size.is_some_and(|size| size > MAX_SCOPED_SOURCE_FILE_BYTES) {
                push_warning(
                    &mut skipped,
                    format!(
                        "Skipped referenced source file '{}' ({} MB): it exceeds the {} MB per-file staging limit",
                        source.display(),
                        size.unwrap_or_default() / (1024 * 1024),
                        MAX_SCOPED_SOURCE_FILE_BYTES / (1024 * 1024)
                    ),
                );
                continue;
            }
        }
        let destination = match source.strip_prefix(&project_root) {
            Ok(relative) => destination_root.join(relative),
            Err(_) => external_staging_destination(&source, &project_root, destination_root)?,
        };
        copy_scoped_source_file(
            &source,
            &destination,
            &mut total_bytes,
            MAX_SCOPED_SOURCE_FILE_BYTES,
        )?;
        if !recurse {
            continue;
        }
        let content = match read_utf8_capped(&source, MAX_LATEX_SIZE) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let current_dir = source.parent().unwrap_or(&project_root);
        for reference in latex_references(&content) {
            match resolve_latex_reference(&reference, current_dir, &project_root) {
                Some(
                    ResolvedLatexReference::Internal(path) | ResolvedLatexReference::External(path),
                ) => pending.push_back((path, reference.recursive)),
                None => {}
            }
        }
    }
    // This stage has no console/quality-note channel, so record skips inside
    // the staged view itself: the models reading this root (and any user
    // inspecting it) see why a referenced file is absent. Best-effort — a
    // project file that already claimed the name wins.
    if !skipped.is_empty() {
        let _ = fs::write(
            destination_root.join(STAGING_NOTES_FILE),
            skipped.join("\n") + "\n",
        );
    }
    Ok(())
}

/// Written into the staged source root when referenced files were skipped.
pub(super) const STAGING_NOTES_FILE: &str = "_pipeline_staging_notes.txt";

/// Map a bounded external source file into the private staged view. The
/// `_external/up{N}/` prefix encodes which project-root ancestor the path is
/// relative to, so two distinct external files can never collide.
fn external_staging_destination(
    source: &Path,
    project_root: &Path,
    destination_root: &Path,
) -> Result<PathBuf, String> {
    for (level, ancestor) in project_root.ancestors().skip(1).enumerate() {
        if level >= MAX_EXTERNAL_PARENT_LEVELS {
            break;
        }
        if let Ok(relative) = source.strip_prefix(ancestor) {
            return Ok(destination_root
                .join("_external")
                .join(format!("up{}", level + 1))
                .join(relative));
        }
    }
    Err("A referenced LaTeX source escaped the project root".to_string())
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
    // The primary selected document gets the PDF staging budget, not the
    // LaTeX-closure per-file cap: scanned PDFs routinely exceed 32 MB, and
    // losing this root silently removes visual Read evidence for every step.
    copy_scoped_source_file(
        selected,
        &destination,
        &mut total_bytes,
        MAX_STAGED_PDF_BYTES,
    )?;
    Ok(ScopedSourceContext {
        source_path: Some(destination),
        read_root: Some(destination_root),
    })
}

/// Record a quality note once. Repeated includes of the same blocked or
/// external file would otherwise flood the console with identical lines.
fn push_warning(warnings: &mut Vec<String>, message: impl Into<String>) {
    let message = message.into();
    if !warnings.iter().any(|existing| existing == &message) {
        warnings.push(message);
    }
}

/// Extract text from a .tex file, resolving \input{} and \include{} recursively.
pub(super) fn extract_latex(
    path: &Path,
    root_dir: &Path,
    warnings: &mut Vec<String>,
) -> Result<String, String> {
    let mut stack = Vec::new();
    extract_latex_inner(path, root_dir, warnings, &mut stack)
}

fn extract_latex_inner(
    path: &Path,
    root_dir: &Path,
    warnings: &mut Vec<String>,
    stack: &mut Vec<PathBuf>,
) -> Result<String, String> {
    if stack.len() > 10 {
        push_warning(
            warnings,
            "LaTeX \\input{} nesting exceeds 10 levels. Output may be incomplete.".to_string(),
        );
        return Ok(String::new());
    }

    let content = read_utf8_capped(path, MAX_LATEX_SIZE)?;
    // Track the chain of files currently being expanded so a circular
    // include is skipped precisely instead of recursing to the depth limit.
    let opened = path.canonicalize().ok();
    if let Some(opened) = &opened {
        stack.push(opened.clone());
    }
    let result = expand_latex_includes(&content, path, root_dir, warnings, stack);
    if opened.is_some() {
        stack.pop();
    }
    result
}

/// True when the byte offset falls inside a LaTeX comment (an unescaped `%`
/// earlier on the same line). Mirrors `latex_without_comments`' escape rule:
/// `%` starts a comment unless preceded by an odd run of backslashes.
fn inside_latex_comment(content: &str, offset: usize) -> bool {
    let line_start = content[..offset]
        .rfind('\n')
        .map_or(0, |position| position + 1);
    let mut slash_count = 0usize;
    for character in content[line_start..offset].chars() {
        if character == '%' && slash_count & 1 == 0 {
            return true;
        }
        if character == '\\' {
            slash_count += 1;
        } else {
            slash_count = 0;
        }
    }
    false
}

fn expand_latex_includes(
    content: &str,
    path: &Path,
    root_dir: &Path,
    warnings: &mut Vec<String>,
    stack: &mut Vec<PathBuf>,
) -> Result<String, String> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let re = Regex::new(r"\\(?:input|include)\{([^}]+)\}").expect("LaTeX include regex is invalid");

    let mut result = String::new();
    let mut last_end = 0;

    for cap in re.captures_iter(content) {
        let full_match = cap.get(0).unwrap();
        // A commented-out \input{} is inert LaTeX: expanding it would splice
        // stale content mid-comment into the text every reviewer reads (or
        // emit a spurious file-not-found note). Leaving last_end untouched
        // keeps the commented line verbatim in the output.
        if inside_latex_comment(content, full_match.start()) {
            continue;
        }
        result.push_str(&content[last_end..full_match.start()]);

        let include_name = &cap[1];
        let mut include_path = parent.join(include_name);
        if include_path.extension().is_none() {
            include_path.set_extension("tex");
        }

        // Includes inside the selected directory are always allowed. A target
        // outside it is read only when it is a nearby regular .tex file this
        // document explicitly references, which keeps traversal like
        // \input{../../../../etc/passwd} blocked while sibling-output layouts
        // (\input{../output/estimates/numbers.tex}) still resolve.
        let canonical = include_path.canonicalize().ok();
        let internal = canonical
            .as_ref()
            .is_some_and(|resolved| resolved.starts_with(root_dir) && resolved.is_file());
        let external = !internal
            && canonical.as_ref().is_some_and(|resolved| {
                ext_eq(resolved, "tex")
                    && resolved.is_file()
                    && within_external_read_bound(resolved, root_dir)
            });

        if let Some(resolved) = canonical.as_ref().filter(|_| internal || external) {
            if stack.iter().any(|open| open == resolved) {
                push_warning(
                    warnings,
                    format!("\\input{{{include_name}}} — circular include skipped."),
                );
                result.push_str(&content[full_match.start()..full_match.end()]);
            } else {
                if external {
                    push_warning(
                        warnings,
                        format!(
                            "\\input{{{include_name}}} — read from outside the selected directory ({}).",
                            resolved.display()
                        ),
                    );
                }
                match extract_latex_inner(resolved, root_dir, warnings, stack) {
                    Ok(included) => result.push_str(&included),
                    Err(error) => {
                        // Reviewers must know a referenced chapter is absent;
                        // the not-found and blocked paths already warn.
                        push_warning(
                            warnings,
                            format!(
                                "\\input{{{include_name}}} — could not be read ({error}); left unexpanded."
                            ),
                        );
                        result.push_str(&content[full_match.start()..full_match.end()]);
                    }
                }
            }
        } else {
            // Diagnose why the include failed
            if !include_path.exists() {
                push_warning(warnings, format!(
                    "\\input{{{include_name}}} — file not found. If this is a multi-file project, select the project folder instead of a single .tex file.",
                ));
            } else if canonical.is_some() {
                push_warning(warnings, format!(
                    "\\input{{{include_name}}} — resolves outside the selected directory (blocked). Only nearby .tex files explicitly referenced by the project are read from outside it; move this file into the project folder to include it.",
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
