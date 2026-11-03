//! Which model-written files may leave a producer unit.
//!
//! A unit writes under `by-step/<step>/<unit>/files/` while its call runs,
//! before anyone knows whether its response will be accepted. Rejected
//! responses are evidence and never downstream context, and the same holds for
//! the files written alongside them, so:
//!
//! - an attempt's files move to `rejected-files/` before another attempt
//!   starts, and when the call ends without an accepted response;
//! - a host-owned marker stands for a call in progress. A process that dies
//!   mid-call moves nothing, so the marker keeps that unit's files out of later
//!   steps and out of Resume. The marker sits with the run's checkpoints, which
//!   interrupted runs keep and finished runs retire.
//!
//! Both locations are beside, never inside, the model-writable unit root, so a
//! provider can neither forge an acceptance nor reach its discarded files.

use std::path::{Path, PathBuf};

const FILES: &str = "files";
const REJECTED_ROOT: &str = "rejected-files";
const OPEN_CALLS: &str = "checkpoints/open-unit-calls";

/// The registered location of files set aside from rejected attempts.
pub(crate) const REJECTED_FILES_DIR: &str = "artifacts/rejected-files";

/// `<artifacts>/by-step/<step>/<unit>`, split at the parts this module needs.
struct UnitPath<'a> {
    artifacts: &'a Path,
    relative: PathBuf,
}

impl<'a> UnitPath<'a> {
    fn parse(unit_root: &'a Path) -> Option<Self> {
        let unit = unit_root.file_name()?;
        let step_root = unit_root.parent()?;
        let step = step_root.file_name()?;
        Some(Self {
            artifacts: step_root.parent()?.parent()?,
            relative: Path::new(step).join(unit),
        })
    }

    fn open_call(&self) -> PathBuf {
        self.artifacts.join(OPEN_CALLS).join(&self.relative)
    }
}

fn parsed(unit_root: &str) -> Result<UnitPath<'_>, String> {
    UnitPath::parse(Path::new(unit_root))
        .ok_or_else(|| format!("Unexpected unit artifact path: {unit_root}"))
}

/// Marks a unit's call as started.
pub(crate) fn begin_call(unit_root: &str) -> Result<(), String> {
    let marker = parsed(unit_root)?.open_call();
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not record a started step: {error}"))?;
    }
    std::fs::write(&marker, b"")
        .map_err(|error| format!("Could not record a started step: {error}"))
}

/// Records the unit's accepted response, which releases its files.
pub(crate) fn accept_call(unit_root: &str) -> Result<(), String> {
    match std::fs::remove_file(parsed(unit_root)?.open_call()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not record an accepted step: {error}")),
    }
}

/// Moves the unit's files out of the producer tree, keeping them as evidence.
/// The rename moves a model-made link itself, never what it points to. Windows
/// holds a just-written file for a moment, so the move waits for its release,
/// up to the limit or a Stop; the open-call marker covers a move that gives up.
pub(crate) async fn set_aside_files(unit_root: &str) -> Result<(), String> {
    let unit = parsed(unit_root)?;
    let files = Path::new(unit_root).join(FILES);
    if let Err(error) = std::fs::symlink_metadata(&files) {
        return match error.kind() {
            std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(format!(
                "Could not inspect a rejected attempt's files: {error}"
            )),
        };
    }
    let rejected = unit.artifacts.join(REJECTED_ROOT).join(&unit.relative);
    std::fs::create_dir_all(&rejected)
        .map_err(|error| format!("Could not set aside a rejected attempt's files: {error}"))?;
    // Looks at the entry itself: a moved dangling link still fills its name.
    let mut number = 1u32;
    let destination = loop {
        let candidate = rejected.join(format!("attempt-{number:02}"));
        match std::fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break candidate,
            Ok(_) => number += 1,
            Err(error) => {
                return Err(format!(
                    "Could not set aside a rejected attempt's files: {error}"
                ))
            }
        }
    };
    crate::commands::await_or_cancel(crate::engines::rename_when_free(&files, &destination), None)
        .await?
        .map_err(|error| format!("Could not set aside a rejected attempt's files: {error}"))
}

/// Ends a unit without an accepted response. The marker stays until the run
/// is finalized, so the unit stays out even when its files could not be moved.
pub(crate) async fn reject_call(unit_root: &str) -> Result<(), String> {
    begin_call(unit_root)?;
    set_aside_files(unit_root).await
}

/// Whether a unit folder holds a call that never ended in an accepted response.
pub(crate) fn call_is_open(unit_root: &Path) -> bool {
    UnitPath::parse(unit_root).is_some_and(|unit| unit.open_call().exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(artifacts: &Path) -> String {
        let root = artifacts.join("by-step/step--1/unit--2");
        std::fs::create_dir_all(root.join(FILES)).unwrap();
        std::fs::write(root.join("files/table.csv"), "a").unwrap();
        root.to_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn each_rejected_attempt_keeps_its_own_files_outside_the_unit() {
        let folder = tempfile::tempdir().unwrap();
        let root = unit(folder.path());
        set_aside_files(&root).await.unwrap();
        std::fs::create_dir_all(Path::new(&root).join(FILES)).unwrap();
        set_aside_files(&root).await.unwrap();
        set_aside_files(&root).await.unwrap();

        let rejected = folder.path().join("rejected-files/step--1/unit--2");
        assert!(rejected.join("attempt-01/table.csv").is_file());
        assert!(rejected.join("attempt-02").is_dir());
        assert!(!rejected.join("attempt-03").exists());
        assert!(!Path::new(&root).join(FILES).exists());
    }

    #[tokio::test]
    async fn a_call_stays_open_until_a_response_is_accepted() {
        let folder = tempfile::tempdir().unwrap();
        let root = unit(folder.path());
        assert!(!call_is_open(Path::new(&root)));
        begin_call(&root).unwrap();
        assert!(call_is_open(Path::new(&root)));
        accept_call(&root).unwrap();
        assert!(!call_is_open(Path::new(&root)));
        accept_call(&root).unwrap();

        reject_call(&root).await.unwrap();
        assert!(call_is_open(Path::new(&root)));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_model_made_link_moves_without_touching_its_target() {
        let folder = tempfile::tempdir().unwrap();
        let root = unit(folder.path());
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("kept.txt"), "kept").unwrap();
        std::fs::remove_dir_all(Path::new(&root).join(FILES)).unwrap();
        std::os::unix::fs::symlink(outside.path(), Path::new(&root).join(FILES)).unwrap();

        set_aside_files(&root).await.unwrap();

        assert!(outside.path().join("kept.txt").is_file());
        assert!(std::fs::symlink_metadata(Path::new(&root).join(FILES)).is_err());
    }

    /// `exists` follows a link, so a dangling one would read as a free name
    /// and the next attempt's files would land on it.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_dangling_link_keeps_its_slot_and_the_next_attempt_gets_its_own() {
        let folder = tempfile::tempdir().unwrap();
        let root = unit(folder.path());
        let files = Path::new(&root).join(FILES);
        std::fs::remove_dir_all(&files).unwrap();
        std::os::unix::fs::symlink("/nonexistent/target", &files).unwrap();

        set_aside_files(&root).await.unwrap();
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join("table.csv"), "second").unwrap();
        set_aside_files(&root).await.unwrap();

        let rejected = folder.path().join("rejected-files/step--1/unit--2");
        let first = std::fs::symlink_metadata(rejected.join("attempt-01")).unwrap();
        assert!(first.file_type().is_symlink());
        assert_eq!(
            std::fs::read_to_string(rejected.join("attempt-02/table.csv")).unwrap(),
            "second"
        );
    }

    /// A rename never opens what it moves, so a FIFO cannot block the step.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_model_made_fifo_moves_without_being_opened() {
        use std::os::unix::ffi::OsStrExt as _;
        let folder = tempfile::tempdir().unwrap();
        let root = unit(folder.path());
        let files = Path::new(&root).join(FILES);
        std::fs::remove_dir_all(&files).unwrap();
        let path = std::ffi::CString::new(files.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);

        set_aside_files(&root).await.unwrap();

        assert!(std::fs::symlink_metadata(&files).is_err());
        assert!(folder
            .path()
            .join("rejected-files/step--1/unit--2/attempt-01")
            .exists());
    }
}
