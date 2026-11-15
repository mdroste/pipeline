//! Re-discover local manuscript inputs before preview and launch. A conservative
//! source/figure scan covers the first build and newly added includes; the last
//! recorder adds local inputs with unusual extensions. Outputs and distribution
//! files are not source inputs. Discovery never follows links or silently truncates.
use super::*;
use std::collections::BTreeSet;

pub(crate) fn refresh_manuscript_inputs(
    store: &Store,
    profile: &mut research::ExecutionProfile,
) -> WorkbenchResult<()> {
    if profile.adapter != "latex" {
        return Ok(());
    }
    let Some(id) = profile.id.strip_prefix("profile_") else {
        return Ok(());
    };
    let body: Option<String> = store.connection()?.query_row(
        "SELECT body_json FROM project_records WHERE id=?1 AND workspace_id=?2 AND kind='build'",
        params![id, profile.workspace_id], |row| row.get(0),
    ).optional().map_err(err)?;
    let Some(body) = body else {
        return Ok(());
    };
    let body: Value = serde_json::from_str(&body).map_err(err)?;
    if body["config"]["automaticDependencies"] != true {
        return Ok(());
    }
    let config: BuildConfig = serde_json::from_value(body["config"].clone()).map_err(err)?;
    // A profile edited separately still owns its declared inputs. Only expand
    // when it is the exact revision associated with these manuscript settings.
    if body["profile"]["revision"].as_i64() != Some(profile.revision) {
        return Ok(());
    }
    let project = root(store, &profile.workspace_id)?;
    let cwd = fs::canonicalize(&profile.cwd).map_err(err)?;
    if !cwd.starts_with(project) {
        return Err(WorkbenchError::invalid("Build folder left the project"));
    }
    profile.inputs = discover_project(
        store,
        &profile.workspace_id,
        &cwd,
        &config,
        &profile.outputs,
    )?;
    Ok(())
}

fn source_file(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| {
            [
                "tex", "bib", "bst", "sty", "cls", "clo", "def", "cfg", "png", "jpg", "jpeg",
                "pdf", "eps", "svg", "csv", "tsv", "dat", "tikz", "pgf",
            ]
            .contains(&s.to_ascii_lowercase().as_str())
        })
}

pub(super) fn discover_project(
    store: &Store,
    ws: &str,
    cwd: &Path,
    config: &BuildConfig,
    outputs: &[String],
) -> WorkbenchResult<Vec<String>> {
    let settings: ProjectHomeSettings = decode(&home_record(store, ws)?)?;
    let directory = config.directory.trim_end_matches('/');
    if directory != "." && !directory.is_empty() && ignored(directory, &settings.ignored_paths) {
        return Err(WorkbenchError::invalid(
            "The build folder is excluded in Project settings",
        ));
    }
    let exclusions: Vec<_> = settings
        .ignored_paths
        .into_iter()
        .filter_map(|p| {
            if directory == "." || directory.is_empty() {
                Some(p)
            } else {
                p.strip_prefix(&format!("{directory}/")).map(str::to_owned)
            }
        })
        .collect();
    discover_inputs(cwd, config, outputs, &exclusions)
}

#[cfg(test)]
fn discover(cwd: &Path, config: &BuildConfig, outputs: &[String]) -> WorkbenchResult<Vec<String>> {
    discover_inputs(cwd, config, outputs, &[])
}

fn discover_inputs(
    cwd: &Path,
    config: &BuildConfig,
    outputs: &[String],
    exclusions: &[String],
) -> WorkbenchResult<Vec<String>> {
    let safe = files::SafeRoot::open(cwd)?;
    let mut inputs: BTreeSet<String> = config.inputs.iter().cloned().collect();
    inputs.insert(config.root_document.clone());
    let mut folders = vec![String::new()];
    let mut visited = 0;
    let started = Instant::now();
    while let Some(folder) = folders.pop() {
        let (entries, truncated) = safe.list_until(&folder, 10_000, |_| {
            started.elapsed() > Duration::from_secs(5)
        })?;
        visited += entries.len();
        if truncated || visited > 10_000 || inputs.len() > 1_000 {
            return Err(WorkbenchError::invalid("Automatic TeX input discovery reached its limit. Use a smaller build folder or turn off automatic dependency tracking and list the inputs."));
        }
        for (name, stat) in entries {
            let path = if folder.is_empty() {
                name
            } else {
                format!("{folder}/{name}")
            };
            if ignored(&path, exclusions)
                || files::relative(&path).is_err()
                || outputs.contains(&path)
            {
                continue;
            }
            match stat.kind {
                files::EntryKind::Folder => folders.push(path),
                files::EntryKind::File if source_file(&path) => {
                    inputs.insert(path);
                }
                _ => {}
            }
        }
    }
    let recorder = Path::new(&config.expected_pdf)
        .with_extension("fls")
        .to_string_lossy()
        .into_owned();
    if safe
        .stat(&recorder)?
        .is_some_and(|s| s.kind == files::EntryKind::File)
    {
        let mut bytes = Vec::new();
        if let Some((file, _)) = safe.open_read(&recorder)? {
            file.take(2 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(err)?;
        }
        if bytes.len() > 2 * 1024 * 1024 {
            return Err(WorkbenchError::invalid("The TeX recorder exceeds 2 MB. Turn off automatic dependency tracking and list inputs manually."));
        }
        let text = String::from_utf8_lossy(&bytes);
        let recorded_outputs: BTreeSet<_> = text
            .lines()
            .filter_map(|line| line.strip_prefix("OUTPUT "))
            .filter_map(|p| local_path(cwd, p))
            .collect();
        for path in text
            .lines()
            .filter_map(|line| line.strip_prefix("INPUT "))
            .filter_map(|p| local_path(cwd, p))
        {
            if !build_output(&path)
                && !ignored(&path, exclusions)
                && !outputs.contains(&path)
                && !recorded_outputs.contains(&path)
                && safe
                    .stat(&path)?
                    .is_some_and(|s| s.kind == files::EntryKind::File)
            {
                inputs.insert(path);
            }
        }
    }
    if inputs.len() > 1_000 {
        return Err(WorkbenchError::invalid("TeX builds support up to 1,000 inputs. Choose a smaller build folder or list inputs manually."));
    }
    for path in &inputs {
        files::relative(path)?;
    }
    Ok(inputs.into_iter().collect())
}

fn build_output(path: &str) -> bool {
    [
        ".bbl", ".blg", ".log", ".bcf", ".run.xml", ".lof", ".lot", ".nav", ".snm", ".vrb", ".xdv",
        ".dvi",
    ]
    .iter()
    .any(|suffix| path.ends_with(suffix))
}

fn local_path(cwd: &Path, input: &str) -> Option<String> {
    let path = Path::new(input);
    let path = if path.is_absolute() {
        path.strip_prefix(cwd).ok()?
    } else {
        path
    };
    let path = path
        .components()
        .filter(|c| *c != Component::CurDir)
        .collect::<PathBuf>();
    let path = path.to_str()?.replace('\\', "/");
    files::relative(&path).ok()?;
    Some(path)
}

#[cfg(test)]
mod tests;
