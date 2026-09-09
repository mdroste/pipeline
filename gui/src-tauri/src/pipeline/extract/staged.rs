//! Resolve captured LaTeX references without reopening the original project.
use super::*;

const MANIFEST: &str = "_pipeline_source_map.json";

#[derive(Default, Serialize, Deserialize)]
pub(super) struct SourceMap {
    pub main: PathBuf,
    pub references: HashMap<String, PathBuf>,
}

impl SourceMap {
    pub fn insert(&mut self, directory: &Path, target: &str, destination: PathBuf) {
        self.references.insert(key(directory, target), destination);
    }

    pub fn save(&self, root: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_LATEX_SIZE {
            return Err("Captured LaTeX source map is too large".into());
        }
        fs::write(root.join(MANIFEST), bytes).map_err(|e| e.to_string())
    }
}

fn key(directory: &Path, target: &str) -> String {
    format!(
        "{}\n{target}",
        directory.to_string_lossy().replace('\\', "/")
    )
}

fn load(root: &Path) -> Option<Result<SourceMap, String>> {
    let path = root.join(MANIFEST);
    path.exists().then(|| {
        serde_json::from_str(&read_utf8_capped(&path, MAX_LATEX_SIZE)?).map_err(|e| e.to_string())
    })
}

fn resolve(root: &Path, relative: &Path) -> Option<PathBuf> {
    if !relative
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return None;
    }
    let path = root.join(relative).canonicalize().ok()?;
    (path.starts_with(root.canonicalize().ok()?) && path.is_file()).then_some(path)
}

pub(super) fn main_file(root: &Path) -> Option<Option<PathBuf>> {
    load(root).map(|map| map.ok().and_then(|map| resolve(root, &map.main)))
}

/// Some(None) means a captured project has no such reference: never fall back
/// to live paths, even when an absolute or parent-relative path still exists.
pub(crate) fn reference(root: &Path, directory: &Path, target: &str) -> Option<Option<PathBuf>> {
    load(root).map(|map| {
        let map = map.ok()?;
        let relative_dir = directory.strip_prefix(root).ok()?;
        let relative = map.references.get(&key(relative_dir, target))?;
        resolve(root, relative)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_latex_expands_nearby_includes_after_original_changes() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::write(
            project.join("chosen.tex"),
            "\\documentclass{article}\n\\input{../numbers}",
        )
        .unwrap();
        fs::write(temp.path().join("numbers.tex"), "original results").unwrap();
        let captured = temp.path().join("captured");
        fs::create_dir(&captured).unwrap();
        latex::stage_latex_project(&project.join("chosen.tex"), &project, &captured).unwrap();
        fs::write(temp.path().join("numbers.tex"), "changed results").unwrap();
        let main = main_file(&captured).unwrap().unwrap();
        let mut notes = Vec::new();
        let text =
            latex::extract_latex(&main, &captured.canonicalize().unwrap(), &mut notes).unwrap();
        assert!(text.contains("original results"));
        assert!(!text.contains("changed results"));
        assert_eq!(reference(&captured, &captured, "/etc/passwd"), Some(None));
    }
}
