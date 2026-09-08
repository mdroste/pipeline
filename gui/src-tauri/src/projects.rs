//! Local project records group immutable Pipeline runs without changing their
//! manifests. Projects are intentionally small metadata files: deleting one
//! never deletes a run or any source artifact.

use fs2::FileExt as _;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

pub(crate) mod ledger;
pub use ledger::{ProjectIssue, ProjectIssueEvidence, ProjectIssueLedger, ProjectIssueOccurrence};

const PROJECT_SCHEMA_VERSION: u32 = 1;
const MAX_PROJECT_BYTES: u64 = 1_000_000;
const MAX_PROJECT_NAME_BYTES: usize = 200;
const MAX_PROJECT_DESCRIPTION_BYTES: usize = 20_000;
const MAX_PROJECT_RUNS: usize = 2_000;

fn default_schema_version() -> u32 {
    PROJECT_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub created: String,
    pub updated: String,
    #[serde(default)]
    pub run_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectsResponse {
    pub projects: Vec<Project>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrashedProject {
    pub project: Project,
    pub deleted_at: String,
}

fn projects_dir() -> Result<PathBuf, String> {
    let dir = crate::storage::data_root()?.join("projects");
    ensure_projects_dir(&dir)?;
    Ok(dir)
}

fn ensure_projects_dir(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|error| format!("Failed to create projects directory: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("Failed to secure projects directory: {error}"))?;
    }
    Ok(())
}

fn project_trash_dir(dir: &Path) -> Result<PathBuf, String> {
    let trash = dir.join(".trash");
    ensure_projects_dir(&trash)?;
    Ok(trash)
}

fn validate_project_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 96
        || !id
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || value == '-' || value == '_')
    {
        return Err("Invalid project id".to_string());
    }
    Ok(())
}

fn validate_text(name: &str, description: &str) -> Result<(String, String), String> {
    let name = name.trim();
    let description = description.trim();
    if name.is_empty() {
        return Err("Project name cannot be empty".to_string());
    }
    if name.len() > MAX_PROJECT_NAME_BYTES {
        return Err(format!(
            "Project name cannot exceed {MAX_PROJECT_NAME_BYTES} bytes"
        ));
    }
    if description.len() > MAX_PROJECT_DESCRIPTION_BYTES {
        return Err(format!(
            "Project description cannot exceed {MAX_PROJECT_DESCRIPTION_BYTES} bytes"
        ));
    }
    Ok((name.to_string(), description.to_string()))
}

fn project_path(dir: &Path, id: &str) -> Result<PathBuf, String> {
    validate_project_id(id)?;
    Ok(dir.join(format!("{id}.json")))
}

fn lock_projects(dir: &Path) -> Result<fs::File, String> {
    ensure_projects_dir(dir)?;
    let path = dir.join(".projects.lock");
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("Failed to open projects lock: {error}"))?;
    file.lock_exclusive()
        .map_err(|error| format!("Failed to lock projects: {error}"))?;
    Ok(file)
}

fn load_project_from(dir: &Path, id: &str) -> Result<Project, String> {
    load_project_path(&project_path(dir, id)?, id)
}

fn load_project_path(path: &Path, id: &str) -> Result<Project, String> {
    validate_project_id(id)?;
    let file = crate::safety::open_regular_file(path)
        .map_err(|error| format!("Failed to open project '{id}': {error}"))?;
    let size = file
        .metadata()
        .map_err(|error| format!("Failed to inspect project '{id}': {error}"))?
        .len();
    if size > MAX_PROJECT_BYTES {
        return Err(format!("Project '{id}' exceeds the storage limit"));
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(MAX_PROJECT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Failed to read project '{id}': {error}"))?;
    if bytes.len() as u64 > MAX_PROJECT_BYTES {
        return Err(format!("Project '{id}' exceeds the storage limit"));
    }
    let project: Project = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Project '{id}' is invalid: {error}"))?;
    if project.id != id {
        return Err(format!("Project '{id}' has mismatched metadata"));
    }
    validate_project(&project)?;
    Ok(project)
}

fn sync_dir(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("Failed to sync project storage: {error}"))?;
    Ok(())
}

fn validate_project(project: &Project) -> Result<(), String> {
    validate_project_id(&project.id)?;
    validate_text(&project.name, &project.description)?;
    if project.schema_version > PROJECT_SCHEMA_VERSION {
        return Err(format!(
            "Project '{}' was created by a newer Pipeline version",
            project.name
        ));
    }
    if project.run_ids.len() > MAX_PROJECT_RUNS {
        return Err(format!("Project '{}' contains too many runs", project.name));
    }
    let mut seen = std::collections::HashSet::new();
    for run_id in &project.run_ids {
        crate::runs::validate_run_id(run_id)?;
        if !seen.insert(run_id) {
            return Err(format!(
                "Project '{}' contains a duplicate run",
                project.name
            ));
        }
    }
    Ok(())
}

fn write_project_to(dir: &Path, project: &Project) -> Result<(), String> {
    validate_project(project)?;
    ensure_projects_dir(dir)?;
    let destination = project_path(dir, &project.id)?;
    let bytes = serde_json::to_vec_pretty(project)
        .map_err(|error| format!("Failed to serialize project: {error}"))?;
    if bytes.len() as u64 > MAX_PROJECT_BYTES {
        return Err("Project exceeds the storage limit".to_string());
    }
    let mut temp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|error| format!("Failed to create project staging file: {error}"))?;
    temp.write_all(&bytes)
        .map_err(|error| format!("Failed to stage project: {error}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| format!("Failed to flush project: {error}"))?;
    temp.persist(destination)
        .map_err(|error| format!("Failed to save project: {}", error.error))?;
    Ok(())
}

fn list_projects_from(dir: &Path) -> Result<ProjectsResponse, String> {
    ensure_projects_dir(dir)?;
    let mut projects = Vec::new();
    let mut warnings = Vec::new();
    for entry in fs::read_dir(dir)
        .map_err(|error| format!("Failed to list projects: {error}"))?
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        match load_project_from(dir, id) {
            Ok(project) => projects.push(project),
            Err(error) => warnings.push(error),
        }
    }
    projects.sort_by(|left, right| {
        right
            .updated
            .cmp(&left.updated)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(ProjectsResponse { projects, warnings })
}

fn create_project_in(dir: &Path, name: &str, description: &str) -> Result<Project, String> {
    let (name, description) = validate_text(name, description)?;
    let _lock = lock_projects(dir)?;
    let base = {
        let slug = crate::pipeline_config::slugify(&name);
        if slug.is_empty() {
            "project".to_string()
        } else {
            slug.chars().take(64).collect()
        }
    };
    let mut id = base.clone();
    let trash = project_trash_dir(dir)?;
    for suffix in 1..=10_000u32 {
        if !project_path(dir, &id)?.exists() && !trash.join(&id).exists() {
            break;
        }
        id = format!("{base}-{suffix}");
    }
    if project_path(dir, &id)?.exists() || trash.join(&id).exists() {
        return Err("Could not allocate a unique project id".to_string());
    }
    let now = chrono::Local::now().to_rfc3339();
    let project = Project {
        schema_version: PROJECT_SCHEMA_VERSION,
        id,
        name,
        description,
        created: now.clone(),
        updated: now,
        run_ids: Vec::new(),
    };
    // A prior interrupted deletion may have left an orphaned ledger for this
    // deterministic id. A newly created project must never inherit it.
    ledger::delete_ledger_from(dir, &project.id)?;
    write_project_to(dir, &project)?;
    Ok(project)
}

fn update_project_in(
    dir: &Path,
    id: &str,
    name: &str,
    description: &str,
) -> Result<Project, String> {
    let (name, description) = validate_text(name, description)?;
    let _lock = lock_projects(dir)?;
    let mut project = load_project_from(dir, id)?;
    project.name = name;
    project.description = description;
    project.updated = chrono::Local::now().to_rfc3339();
    write_project_to(dir, &project)?;
    Ok(project)
}

fn set_project_run_in(
    dir: &Path,
    project_id: &str,
    run_id: &str,
    included: bool,
    require_run: bool,
) -> Result<Project, String> {
    crate::runs::validate_run_id(run_id)?;
    if require_run {
        crate::runs::load_manifest(run_id)?;
    }
    let _lock = lock_projects(dir)?;
    let mut project = load_project_from(dir, project_id)?;
    if included {
        if !project.run_ids.iter().any(|value| value == run_id) {
            if project.run_ids.len() >= MAX_PROJECT_RUNS {
                return Err("Project has reached its run limit".to_string());
            }
            project.run_ids.push(run_id.to_string());
        }
    } else {
        project.run_ids.retain(|value| value != run_id);
    }
    project.updated = chrono::Local::now().to_rfc3339();
    write_project_to(dir, &project)?;
    Ok(project)
}

#[tauri::command]
pub fn list_projects() -> Result<ProjectsResponse, String> {
    list_projects_from(&projects_dir()?)
}

#[tauri::command]
pub fn create_project(name: String, description: Option<String>) -> Result<Project, String> {
    create_project_in(
        &projects_dir()?,
        &name,
        description.as_deref().unwrap_or(""),
    )
}

#[tauri::command]
pub fn update_project(id: String, name: String, description: String) -> Result<Project, String> {
    update_project_in(&projects_dir()?, &id, &name, &description)
}

#[tauri::command]
pub fn set_project_run(
    project_id: String,
    run_id: String,
    included: bool,
) -> Result<Project, String> {
    set_project_run_in(&projects_dir()?, &project_id, &run_id, included, included)
}

#[tauri::command]
pub fn delete_project(id: String) -> Result<(), String> {
    let dir = projects_dir()?;
    delete_project_in(&dir, &id)
}

fn delete_project_in(dir: &Path, id: &str) -> Result<(), String> {
    let _lock = lock_projects(dir)?;
    let source = project_path(dir, id)?;
    let _ = load_project_from(dir, id)?;
    let trash = project_trash_dir(dir)?
        .canonicalize()
        .map_err(|error| format!("Failed to resolve project Trash: {error}"))?;
    let destination = trash.join(id);
    if destination.exists() {
        return Err(
            "A trashed copy of this project already exists; restore or permanently delete it first"
                .to_string(),
        );
    }
    fs::create_dir(&destination)
        .map_err(|error| format!("Failed to create project Trash entry: {error}"))?;
    if let Err(error) = fs::rename(&source, destination.join("project.json")) {
        let _ = fs::remove_dir(&destination);
        return Err(format!("Failed to move project to Trash: {error}"));
    }
    let ledger_path = ledger::ledger_path(dir, id)?;
    if ledger_path.exists() {
        if let Err(error) = fs::rename(&ledger_path, destination.join("ledger.json")) {
            let _ = fs::rename(destination.join("project.json"), &source);
            let _ = fs::remove_dir(&destination);
            return Err(format!(
                "Failed to move the project issue ledger to Trash: {error}"
            ));
        }
    }
    sync_dir(dir)?;
    sync_dir(&trash)?;
    Ok(())
}

fn list_trashed_projects_from(dir: &Path) -> Result<Vec<TrashedProject>, String> {
    let trash = project_trash_dir(dir)?;
    let mut projects = Vec::new();
    for entry in fs::read_dir(&trash)
        .map_err(|error| format!("Failed to list project Trash: {error}"))?
        .flatten()
    {
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let Some(id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if validate_project_id(&id).is_err() {
            continue;
        }
        let Ok(project) = load_project_path(&entry.path().join("project.json"), &id) else {
            continue;
        };
        let deleted_at = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map(chrono::DateTime::<chrono::Local>::from)
            .map(|time| time.to_rfc3339())
            .unwrap_or_default();
        projects.push(TrashedProject {
            project,
            deleted_at,
        });
    }
    projects.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));
    Ok(projects)
}

#[tauri::command]
pub fn list_trashed_projects() -> Result<Vec<TrashedProject>, String> {
    list_trashed_projects_from(&projects_dir()?)
}

fn restore_trashed_project_in(dir: &Path, id: &str) -> Result<(), String> {
    validate_project_id(id)?;
    let _lock = lock_projects(dir)?;
    let trash = project_trash_dir(dir)?
        .canonicalize()
        .map_err(|error| format!("Failed to resolve project Trash: {error}"))?;
    let source = trash.join(id);
    let canonical = source
        .canonicalize()
        .map_err(|_| "Trashed project not found".to_string())?;
    if canonical.parent() != Some(trash.as_path()) || !canonical.is_dir() {
        return Err("Invalid trashed project id".to_string());
    }
    let project = load_project_path(&canonical.join("project.json"), id)?;
    let destination = project_path(dir, id)?;
    if destination.exists() {
        return Err("A project with this id already exists".to_string());
    }
    fs::rename(canonical.join("project.json"), &destination)
        .map_err(|error| format!("Failed to restore project: {error}"))?;
    let trashed_ledger = canonical.join("ledger.json");
    if trashed_ledger.exists() {
        let ledger_destination = ledger::ledger_path(dir, id)?;
        if let Some(parent) = ledger_destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to prepare project ledger restore: {error}"))?;
        }
        if let Err(error) = fs::rename(&trashed_ledger, &ledger_destination) {
            let _ = fs::rename(&destination, canonical.join("project.json"));
            return Err(format!("Failed to restore project issue ledger: {error}"));
        }
    }
    fs::remove_dir(&canonical)
        .map_err(|error| format!("Failed to finish project restore: {error}"))?;
    validate_project(&project)?;
    sync_dir(dir)?;
    sync_dir(&trash)
}

#[tauri::command]
pub fn restore_trashed_project(id: String) -> Result<(), String> {
    restore_trashed_project_in(&projects_dir()?, &id)
}

fn permanently_delete_trashed_project_in(dir: &Path, id: &str) -> Result<(), String> {
    validate_project_id(id)?;
    let _lock = lock_projects(dir)?;
    let trash = project_trash_dir(dir)?
        .canonicalize()
        .map_err(|error| format!("Failed to resolve project Trash: {error}"))?;
    let source = trash.join(id);
    let canonical = source
        .canonicalize()
        .map_err(|_| "Trashed project not found".to_string())?;
    if canonical.parent() != Some(trash.as_path()) || !canonical.is_dir() {
        return Err("Invalid trashed project id".to_string());
    }
    fs::remove_dir_all(&canonical)
        .map_err(|error| format!("Failed to permanently delete trashed project: {error}"))?;
    sync_dir(&trash)
}

#[tauri::command]
pub fn permanently_delete_trashed_project(id: String) -> Result<(), String> {
    permanently_delete_trashed_project_in(&projects_dir()?, &id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_lifecycle_preserves_order_and_membership() {
        let root = tempfile::tempdir().unwrap();
        let mut project =
            create_project_in(root.path(), "Monetary Networks", "Revision work").unwrap();
        assert_eq!(project.id, "monetary-networks");
        assert!(project.run_ids.is_empty());

        project = set_project_run_in(root.path(), &project.id, "run_001", true, false).unwrap();
        project = set_project_run_in(root.path(), &project.id, "run_001", true, false).unwrap();
        assert_eq!(project.run_ids, vec!["run_001"]);

        project = update_project_in(root.path(), &project.id, "Networks", "New note").unwrap();
        assert_eq!(project.name, "Networks");
        assert_eq!(
            list_projects_from(root.path()).unwrap().projects,
            vec![project]
        );
    }

    #[test]
    fn malformed_project_is_reported_without_hiding_valid_projects() {
        let root = tempfile::tempdir().unwrap();
        create_project_in(root.path(), "Valid", "").unwrap();
        fs::write(root.path().join("broken.json"), b"not-json").unwrap();
        let response = list_projects_from(root.path()).unwrap();
        assert_eq!(response.projects.len(), 1);
        assert_eq!(response.warnings.len(), 1);
    }

    #[test]
    fn project_text_and_ids_are_bounded() {
        let root = tempfile::tempdir().unwrap();
        assert!(create_project_in(root.path(), "", "").is_err());
        assert!(create_project_in(root.path(), &"x".repeat(201), "").is_err());
        assert!(set_project_run_in(root.path(), "bad/id", "run", true, false).is_err());
    }

    #[test]
    fn deleted_projects_and_ledgers_can_be_restored_from_trash() {
        let root = tempfile::tempdir().unwrap();
        let project = create_project_in(root.path(), "Restorable", "Kept").unwrap();
        fs::create_dir_all(root.path().join("ledgers")).unwrap();
        fs::write(
            ledger::ledger_path(root.path(), &project.id).unwrap(),
            b"ledger-placeholder",
        )
        .unwrap();

        delete_project_in(root.path(), &project.id).unwrap();
        assert!(list_projects_from(root.path()).unwrap().projects.is_empty());
        let trashed = list_trashed_projects_from(root.path()).unwrap();
        assert_eq!(trashed.len(), 1);
        assert_eq!(trashed[0].project.name, "Restorable");

        restore_trashed_project_in(root.path(), &project.id).unwrap();
        assert_eq!(list_projects_from(root.path()).unwrap().projects.len(), 1);
        assert!(ledger::ledger_path(root.path(), &project.id)
            .unwrap()
            .exists());
        assert!(list_trashed_projects_from(root.path()).unwrap().is_empty());
    }

    #[test]
    fn new_projects_do_not_reuse_ids_reserved_by_trash() {
        let root = tempfile::tempdir().unwrap();
        let original = create_project_in(root.path(), "Shared name", "First").unwrap();
        delete_project_in(root.path(), &original.id).unwrap();

        let replacement = create_project_in(root.path(), "Shared name", "Second").unwrap();
        assert_eq!(original.id, "shared-name");
        assert_eq!(replacement.id, "shared-name-1");
        restore_trashed_project_in(root.path(), &original.id).unwrap();
        assert_eq!(list_projects_from(root.path()).unwrap().projects.len(), 2);
    }
}
