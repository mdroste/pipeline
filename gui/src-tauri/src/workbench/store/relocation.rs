//! Keeps stored file references valid when the research data folder moves.
//!
//! References are absolute paths, so a folder copied to a new drive, a new mount
//! point, or a renamed home folder leaves every one naming the old location.
//! Each real open compares them with the folder being opened and rewrites, in
//! one transaction, those whose file is actually here. A reference whose file is
//! not here keeps its text and is tried again at the next open, so an unplugged
//! drive never costs a link. The current folder is recorded only once nothing
//! remains elsewhere.
//!
//! Nothing here reads, moves, or changes a stored file, and a failed rebase
//! leaves the database as it was. Retention treats any reference still outside
//! the folder as unknown and refuses to clean research files, which is what
//! protects them when this repair is skipped, fails, or can't finish.
//!
//! A record whose file was left out of a research backup holds a placeholder
//! instead of a path. It names no location, so it never counts as outside.

use super::{now, Store, WorkbenchError, WorkbenchResult, CURRENT_SCHEMA_VERSION};
use rusqlite::{params, Connection, OptionalExtension as _, TransactionBehavior};
use sha2::{Digest as _, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The columns that keep a file under `blobs/` or `context/` in use. Retention
/// counts only these, and only these hold the folder's record back when one
/// can't be resolved. `review_handoffs.staged_path` is rebased separately, and
/// provenance such as `artifacts.original_path` stays as it was recorded.
pub(crate) const REFERENCE_COLUMNS: [(&str, &str); 5] = [
    ("artifacts", "storage_reference"),
    ("paper_revisions", "text_reference"),
    ("source_versions", "text_reference"),
    ("retained_blobs", "storage_reference"),
    ("context_snapshots", "body_reference"),
];

/// Folders under the data root that hold referenced files.
const REFERENCED_AREAS: [&str; 2] = ["blobs", "context"];
/// Folders whose entries can be moved to Trash, matching retention's journal rule.
const TRASHABLE_AREAS: [&str; 4] = ["blobs", "jobs", "context", "backups"];
/// How a stored file is named inside a research backup.
const PORTABLE_PREFIX: &str = "workspace://blobs/";
/// Stands in for a required reference whose file was left out of a research
/// backup. The name stays so the same file, if it ever returns, reconnects at
/// the next open.
const LEFT_OUT_PREFIX: &str = "unavailable://blobs/";

/// Whether `reference` names a stored file inside the data folder at `root`.
/// Compared by path component, so `<root>/blobs/../x` does not count as inside.
pub(crate) fn is_local_reference(reference: &str, root: &Path) -> bool {
    let Ok(relative) = Path::new(reference).strip_prefix(root) else {
        return false;
    };
    let parts = relative.components().collect::<Vec<_>>();
    parts.len() >= 2
        && parts
            .iter()
            .all(|part| matches!(part, Component::Normal(_)))
        && REFERENCED_AREAS
            .iter()
            .any(|area| parts[0].as_os_str() == *area)
}

/// The placeholder for a file a backup left out, from its portable name.
pub(crate) fn left_out_reference(portable: &str) -> String {
    let name = portable.strip_prefix(PORTABLE_PREFIX).unwrap_or(portable);
    format!("{LEFT_OUT_PREFIX}{name}")
}

pub(crate) fn is_left_out_reference(reference: &str) -> bool {
    reference.strip_prefix(LEFT_OUT_PREFIX).is_some_and(|name| {
        let parts = segments(name);
        !parts.is_empty() && parts.iter().all(|part| is_plain(part))
    })
}

/// Where a left-out file would sit, should it return to the folder at `root`.
pub(crate) fn left_out_file(reference: &str, root: &Path) -> Option<PathBuf> {
    is_left_out_reference(reference).then(|| {
        let name = segments(&reference[LEFT_OUT_PREFIX.len()..]);
        under(&root.join("blobs"), &name)
    })
}

/// How a backup names a reference, wherever this folder has been. A reference
/// that doesn't name anything under `blobs/` gets a stable name of its own, so
/// it can be listed as missing without its path travelling in the archive.
pub(crate) fn portable_reference(reference: &str) -> String {
    let parts = segments(reference);
    match parts.iter().rposition(|part| *part == "blobs") {
        Some(at) if at + 1 < parts.len() && parts[at + 1..].iter().all(|part| is_plain(part)) => {
            format!("{PORTABLE_PREFIX}{}", parts[at + 1..].join("/"))
        }
        _ => format!(
            "{PORTABLE_PREFIX}unknown-{:x}",
            Sha256::digest(reference.as_bytes())
        ),
    }
}

fn left_out_error(what: &str) -> WorkbenchError {
    WorkbenchError::invalid(format!(
        "{what} isn't available. The file was already missing when the research backup was made, so restoring it couldn't bring the file back."
    ))
}

/// Stops a reader at a record whose file a backup left out, before the
/// placeholder reaches the filesystem as if it were a path.
pub(crate) fn require_available(what: &str, reference: &str) -> WorkbenchResult<()> {
    if is_left_out_reference(reference) {
        return Err(left_out_error(what));
    }
    Ok(())
}

/// The error for a reference a reader refused. `..` means the record was damaged
/// or tampered with. Anything else outside the folder is far more likely to be a
/// moved folder, and a researcher should read that rather than a security alarm.
pub(crate) fn outside_folder_error(what: &str, reference: &Path, tampered: &str) -> WorkbenchError {
    if is_left_out_reference(&reference.to_string_lossy()) {
        return left_out_error(what);
    }
    if reference
        .components()
        .any(|part| part == Component::ParentDir)
    {
        return WorkbenchError::invalid(tampered);
    }
    WorkbenchError::invalid(format!(
        "{what} is saved outside the current research data folder. The folder may have moved or been copied without all of its files. If you still have the file, open the folder that holds it, or copy it here and restart Pipeline."
    ))
}

/// Replaces the recorded data folder. Restore also uses this, because an
/// archive's own record names a folder on another computer.
pub(crate) fn record_location(connection: &Connection, root: &Path) -> WorkbenchResult<()> {
    connection
        .execute(
            "INSERT INTO store_location(id,root,recorded_at) VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET root=excluded.root,recorded_at=excluded.recorded_at",
            params![root.to_string_lossy(), now()],
        )
        .map(|_| ())
        .map_err(|error| WorkbenchError::storage("Failed to record the data folder", error))
}

/// A stored path in parts, whichever operating system wrote it.
fn segments(path: &str) -> Vec<&str> {
    path.split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .collect()
}

fn is_plain(part: &str) -> bool {
    !matches!(part, "." | "..")
}

fn under(root: &Path, parts: &[&str]) -> PathBuf {
    let mut path = root.to_path_buf();
    for part in parts {
        path.push(part);
    }
    path
}

/// True for a real file or folder. A link here would not be the stored file.
fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file() || meta.is_dir())
}

/// Where `reference` lives under `root`, when that same relative file is there.
/// The old prefix is the recorded folder when the reference sits under it.
/// Otherwise it is whatever precedes the last stored-area folder, which is how a
/// store moved before folders were recorded gives up its old location.
pub(crate) fn resolve_reference(
    reference: &str,
    recorded: Option<&str>,
    root: &Path,
) -> Option<PathBuf> {
    let parts = segments(reference);
    if !parts.iter().all(|part| is_plain(part)) {
        return None;
    }
    let recorded = recorded.map(segments).unwrap_or_default();
    let start = if !recorded.is_empty() && parts.starts_with(&recorded) {
        recorded.len()
    } else {
        parts
            .iter()
            .rposition(|part| REFERENCED_AREAS.contains(part))?
    };
    let relative = parts.get(start..)?;
    if relative.len() < 2 || !REFERENCED_AREAS.contains(&relative[0]) {
        return None;
    }
    let path = under(root, relative);
    exists(&path).then_some(path)
}

/// New locations for one trashed entry. The journal must already have the shape
/// retention requires, so a row that disagrees with itself is never laundered
/// into a valid one here.
fn resolve_trash(id: &str, original: &str, trash: &str, root: &Path) -> Option<(PathBuf, PathBuf)> {
    let original = segments(original);
    let trash = segments(trash);
    let [.., area, name] = original[..] else {
        return None;
    };
    let [.., folder, owner, trash_area, trash_name] = trash[..] else {
        return None;
    };
    if folder != "trash"
        || owner != id
        || trash_area != area
        || trash_name != name
        || !TRASHABLE_AREAS.contains(&area)
        || ![id, name].iter().all(|part| is_plain(part))
    {
        return None;
    }
    let restored = under(root, &[area, name]);
    let trashed = under(root, &["trash", id, area, name]);
    (exists(&restored) || exists(&trashed)).then_some((restored, trashed))
}

struct ReferenceMove {
    table: &'static str,
    column: &'static str,
    rowid: i64,
    from: String,
    to: String,
}

struct TrashMove {
    rowid: i64,
    from: String,
    original: String,
    trash: String,
}

#[derive(Default)]
struct Plan {
    references: Vec<ReferenceMove>,
    trash: Vec<TrashMove>,
    unresolved: usize,
}

fn plan_references(
    connection: &Connection,
    recorded: Option<&str>,
    root: &Path,
    plan: &mut Plan,
) -> WorkbenchResult<()> {
    for (table, column) in REFERENCE_COLUMNS {
        let mut statement = connection
            .prepare(&format!(
                "SELECT rowid,{column} FROM {table} WHERE {column} IS NOT NULL"
            ))
            .map_err(|error| WorkbenchError::storage("Failed to read file locations", error))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| WorkbenchError::storage("Failed to read file locations", error))?;
        for row in rows {
            let (rowid, from) = row
                .map_err(|error| WorkbenchError::storage("Failed to read file location", error))?;
            if is_local_reference(&from, root) {
                continue;
            }
            match resolve_reference(&from, recorded, root) {
                Some(to) => plan.references.push(ReferenceMove {
                    table,
                    column,
                    rowid,
                    to: to.to_string_lossy().into_owned(),
                    from,
                }),
                None if is_left_out_reference(&from) => {}
                None => plan.unresolved += 1,
            }
        }
    }
    Ok(())
}

/// A prepared Review keeps its private copy under `handoffs/<id>/`, which moves
/// with the folder. Nothing in that area is cleaned up, and a copy that's gone
/// is prepared again from the paper, so it never holds up the folder's record.
fn plan_handoffs(connection: &Connection, root: &Path, plan: &mut Plan) -> WorkbenchResult<()> {
    let mut statement = connection
        .prepare("SELECT rowid,id,staged_path FROM review_handoffs WHERE staged_path<>''")
        .map_err(|error| WorkbenchError::storage("Failed to read handoff locations", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| WorkbenchError::storage("Failed to read handoff locations", error))?;
    for row in rows {
        let (rowid, id, from) =
            row.map_err(|error| WorkbenchError::storage("Failed to read handoff location", error))?;
        if Path::new(&from).starts_with(root) {
            continue;
        }
        let parts = segments(&from);
        let [.., folder, owner, name] = parts[..] else {
            continue;
        };
        if folder != "handoffs" || owner != id || ![owner, name].iter().all(|part| is_plain(part)) {
            continue;
        }
        let to = under(root, &["handoffs", owner, name]);
        if exists(&to) {
            plan.references.push(ReferenceMove {
                table: "review_handoffs",
                column: "staged_path",
                rowid,
                to: to.to_string_lossy().into_owned(),
                from: from.clone(),
            });
        }
    }
    Ok(())
}

fn plan_trash(connection: &Connection, root: &Path, plan: &mut Plan) -> WorkbenchResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT rowid,id,original_path,trash_path FROM storage_trash WHERE state='trashed'",
        )
        .map_err(|error| WorkbenchError::storage("Failed to read Trash locations", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|error| WorkbenchError::storage("Failed to read Trash locations", error))?;
    for row in rows {
        let (rowid, id, original, trash) =
            row.map_err(|error| WorkbenchError::storage("Failed to read Trash location", error))?;
        if Path::new(&original).starts_with(root) && Path::new(&trash).starts_with(root) {
            continue;
        }
        match resolve_trash(&id, &original, &trash, root) {
            Some((original, new_trash)) => plan.trash.push(TrashMove {
                rowid,
                from: trash,
                original: original.to_string_lossy().into_owned(),
                trash: new_trash.to_string_lossy().into_owned(),
            }),
            None => plan.unresolved += 1,
        }
    }
    Ok(())
}

fn apply(connection: &mut Connection, plan: &Plan, root: &Path) -> WorkbenchResult<()> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start location update", error))?;
    for change in &plan.references {
        // `retained_blobs` keys on the reference, so it may already hold the
        // new row. One row for one file is the right end.
        let verb = if change.table == "retained_blobs" {
            "UPDATE OR REPLACE"
        } else {
            "UPDATE"
        };
        transaction
            .execute(
                &format!(
                    "{verb} {table} SET {column}=?1 WHERE rowid=?2 AND {column}=?3",
                    table = change.table,
                    column = change.column
                ),
                params![change.to, change.rowid, change.from],
            )
            .map_err(|error| WorkbenchError::storage("Failed to update a file location", error))?;
    }
    for change in &plan.trash {
        transaction
            .execute(
                "UPDATE storage_trash SET original_path=?1,trash_path=?2 WHERE rowid=?3 AND trash_path=?4",
                params![change.original, change.trash, change.rowid, change.from],
            )
            .map_err(|error| WorkbenchError::storage("Failed to update a Trash location", error))?;
    }
    if plan.unresolved == 0 {
        record_location(&transaction, root)?;
    }
    transaction
        .commit()
        .map_err(|error| WorkbenchError::storage("Failed to commit location update", error))
}

impl Store {
    /// Runs on every real open, under the initialization lock. It writes only
    /// when something changes, after the same backup a migration takes.
    pub(super) fn rebase_references(&self) -> WorkbenchResult<()> {
        let mut connection = self.connection()?;
        let recorded: Option<String> = connection
            .query_row("SELECT root FROM store_location WHERE id=1", [], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|error| WorkbenchError::storage("Failed to read the data folder", error))?;
        let mut plan = Plan::default();
        plan_references(&connection, recorded.as_deref(), &self.root, &mut plan)?;
        plan_trash(&connection, &self.root, &mut plan)?;
        plan_handoffs(&connection, &self.root, &mut plan)?;
        let rewrites = !plan.references.is_empty() || !plan.trash.is_empty();
        let current = self.root.to_string_lossy();
        if !rewrites && (plan.unresolved > 0 || recorded.as_deref() == Some(current.as_ref())) {
            return Ok(());
        }
        if rewrites {
            self.backup(&connection, CURRENT_SCHEMA_VERSION)?;
        }
        apply(&mut connection, &plan, &self.root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_references_are_compared_by_path_component() {
        let root = Path::new("/data/workbench");
        assert!(is_local_reference("/data/workbench/blobs/a.txt", root));
        assert!(is_local_reference("/data/workbench/blobs/tree/a.tex", root));
        assert!(is_local_reference("/data/workbench/context/a.txt", root));
        for outside in [
            "/old/workbench/blobs/a.txt",
            "/data/workbench-old/blobs/a.txt",
            "/data/workbench/blobs/../a.txt",
            "/data/workbench/blobs",
            "/data/workbench/jobs/a.txt",
            "workspace://blobs/a.txt",
        ] {
            assert!(!is_local_reference(outside, root), "{outside}");
        }
    }

    #[test]
    fn a_reference_resolves_only_to_the_same_file_under_the_new_folder() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir_all(root.join("blobs/source-tree-1")).unwrap();
        fs::create_dir(root.join("context")).unwrap();
        fs::write(root.join("blobs/a.txt"), "a").unwrap();
        fs::write(root.join("context/c.txt"), "c").unwrap();
        for (reference, recorded, expected) in [
            ("/old/blobs/a.txt", Some("/old"), Some("blobs/a.txt")),
            ("/old/blobs/a.txt", None, Some("blobs/a.txt")),
            ("/old/context/c.txt", None, Some("context/c.txt")),
            (r"D:\data\blobs\a.txt", None, Some("blobs/a.txt")),
            ("/blobs/old/store/blobs/a.txt", None, Some("blobs/a.txt")),
            (
                "/old/blobs/source-tree-1",
                None,
                Some("blobs/source-tree-1"),
            ),
            ("workspace://blobs/a.txt", None, Some("blobs/a.txt")),
            ("/old/blobs/gone.txt", None, None),
            ("/old/blobs/../blobs/a.txt", None, None),
            ("/old/blobs/x/../a.txt", Some("/old"), None),
            ("/old/a.txt", None, None),
            ("/old/blobs", None, None),
        ] {
            assert_eq!(
                resolve_reference(reference, recorded, root),
                expected.map(|path| root.join(path)),
                "{reference}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_not_the_stored_file() {
        let temporary = tempfile::tempdir().unwrap();
        fs::create_dir(temporary.path().join("blobs")).unwrap();
        fs::write(temporary.path().join("outside.txt"), "outside").unwrap();
        std::os::unix::fs::symlink(
            temporary.path().join("outside.txt"),
            temporary.path().join("blobs/a.txt"),
        )
        .unwrap();
        assert!(resolve_reference("/old/blobs/a.txt", None, temporary.path()).is_none());
    }

    #[test]
    fn a_trash_row_must_agree_with_itself_before_it_moves() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir_all(root.join("trash/t1/blobs")).unwrap();
        fs::write(root.join("trash/t1/blobs/a.txt"), "a").unwrap();
        let good = "/old/trash/t1/blobs/a.txt";
        assert_eq!(
            resolve_trash("t1", "/old/blobs/a.txt", good, root),
            Some((root.join("blobs/a.txt"), root.join("trash/t1/blobs/a.txt")))
        );
        for (id, original, trash) in [
            ("t2", "/old/blobs/a.txt", good),
            ("t1", "/old/blobs/b.txt", good),
            ("t1", "/old/keys/a.txt", "/old/trash/t1/keys/a.txt"),
            ("t1", "/old/blobs/a.txt", "/old/other/t1/blobs/a.txt"),
            ("..", "/old/blobs/a.txt", "/old/trash/../blobs/a.txt"),
            ("t1", "/old/blobs/gone.txt", "/old/trash/t1/blobs/gone.txt"),
        ] {
            assert!(resolve_trash(id, original, trash, root).is_none(), "{id}");
        }
    }

    #[test]
    fn a_backup_names_a_reference_the_same_way_from_any_folder() {
        for reference in [
            "/old/store/blobs/a.txt",
            r"D:\data\blobs\a.txt",
            "workspace://blobs/a.txt",
            "unavailable://blobs/a.txt",
        ] {
            assert_eq!(portable_reference(reference), "workspace://blobs/a.txt");
        }
        for odd in ["/old/blobs/../key", "/old/a.txt", "/old/blobs"] {
            let name = portable_reference(odd);
            assert!(name.starts_with("workspace://blobs/unknown-"), "{name}");
            assert_eq!(name, portable_reference(odd));
        }
    }

    #[test]
    fn a_placeholder_is_never_a_path() {
        let root = Path::new("/data/workbench");
        let reference = left_out_reference("workspace://blobs/a.txt");
        assert_eq!(reference, "unavailable://blobs/a.txt");
        assert!(is_left_out_reference(&reference));
        assert!(!is_local_reference(&reference, root));
        assert_eq!(
            left_out_file(&reference, root),
            Some(root.join("blobs/a.txt"))
        );
        for odd in [
            "unavailable://blobs/",
            "unavailable://blobs/../key",
            "unavailable://elsewhere/a.txt",
            "/data/workbench/blobs/a.txt",
        ] {
            assert!(!is_left_out_reference(odd), "{odd}");
            assert!(left_out_file(odd, root).is_none(), "{odd}");
        }
        let message = require_available("This file", &reference)
            .unwrap_err()
            .message;
        assert!(message.contains("already missing"), "{message}");
        assert!(require_available("This file", "/data/workbench/blobs/a.txt").is_ok());
    }

    #[test]
    fn refused_references_name_a_move_unless_they_traverse() {
        let moved = outside_folder_error("This text", Path::new("/old/blobs/a.txt"), "escaped");
        assert!(
            moved.message.contains("may have moved"),
            "{}",
            moved.message
        );
        let traversal =
            outside_folder_error("This text", Path::new("/new/blobs/../key"), "escaped");
        assert_eq!(traversal.message, "escaped");
    }
}
