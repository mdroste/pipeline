//! SQLite connection policy and the ordered, atomic schema migration chain.

use super::{WorkbenchError, WorkbenchResult, CURRENT_SCHEMA_VERSION};
use rusqlite::{Connection, OptionalExtension as _, Transaction, TransactionBehavior};
use std::time::Duration;

pub(super) fn configure_timeout(connection: &Connection) -> WorkbenchResult<()> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| WorkbenchError::storage("Failed to configure database timeout", error))
}

pub(super) fn configure_database(connection: &Connection) -> WorkbenchResult<()> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;",
        )
        .map_err(|error| WorkbenchError::storage("Failed to configure Workbench database", error))
}

pub(super) fn configure_connection(connection: &Connection) -> WorkbenchResult<()> {
    connection
        .execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;")
        .map_err(|error| WorkbenchError::storage("Failed to configure Workbench connection", error))
}

pub(super) fn schema_version(connection: &Connection) -> WorkbenchResult<u32> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| WorkbenchError::storage("Failed to read Workbench schema version", error))
}

pub(super) fn database_has_objects(connection: &Connection) -> WorkbenchResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name NOT LIKE 'sqlite_%')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| WorkbenchError::storage("Failed to inspect Workbench database", error))
}

/// The chain in order. A database at version N has applied the first N entries.
const MIGRATIONS: [(&str, &str); CURRENT_SCHEMA_VERSION as usize] = [
    (
        include_str!("../migrations/001_initial.sql"),
        "Failed to apply Workbench migration 1",
    ),
    (
        include_str!("../migrations/002_independent_workspaces.sql"),
        "Failed to apply Workbench migration 2",
    ),
    (
        include_str!("../migrations/003_research_harness.sql"),
        "Failed to apply Workbench migration 3",
    ),
    (
        include_str!("../migrations/004_native_instruction_sources.sql"),
        "Failed to apply Workbench migration 4",
    ),
    (
        include_str!("../migrations/005_release_readiness.sql"),
        "Failed to apply Workbench migration 5",
    ),
    (
        include_str!("../migrations/006_project_surface.sql"),
        "Failed to apply Workbench migration 6",
    ),
    (
        include_str!("../migrations/007_research_studio.sql"),
        "Failed to apply Workbench migration 7",
    ),
    (
        include_str!("../migrations/008_theory_notes.sql"),
        "Failed to apply Workbench migration 8",
    ),
    (
        include_str!("../migrations/009_project_exchange.sql"),
        "Failed to apply Workbench migration 9",
    ),
    (
        include_str!("../migrations/010_preferences.sql"),
        "Failed to apply Workbench migration 10",
    ),
    (
        include_str!("../migrations/011_research_desk.sql"),
        "Failed to apply Workbench migration 11",
    ),
    (
        include_str!("../migrations/012_task_exchanges.sql"),
        "Failed to migrate task exchanges",
    ),
    (
        include_str!("../migrations/013_research_programs.sql"),
        "Failed to migrate research programs",
    ),
    (
        include_str!("../migrations/014_agent_base_prompts.sql"),
        "Failed to migrate agent base prompts",
    ),
    (
        include_str!("../migrations/015_self_discovery.sql"),
        "Failed to migrate self-discovery roles",
    ),
    (
        include_str!("../migrations/016_execution_adapters.sql"),
        "Failed to migrate execution adapters",
    ),
    (
        include_str!("../migrations/017_conversation_tools.sql"),
        "Failed to apply Workbench migration 17",
    ),
    (
        include_str!("../migrations/018_transcript_page_index.sql"),
        "Failed to apply Workbench migration 18",
    ),
    (
        include_str!("../migrations/019_store_location.sql"),
        "Failed to apply Workbench migration 19",
    ),
];

fn apply_migrations(transaction: &Transaction<'_>, from: u32, through: u32) -> WorkbenchResult<()> {
    for (version, (sql, context)) in (1u32..).zip(MIGRATIONS) {
        if from < version && version <= through {
            transaction
                .execute_batch(sql)
                .map_err(|error| WorkbenchError::storage(context, error))?;
        }
    }
    Ok(())
}

/// A private in-memory database with exactly the schema of version `through`,
/// built by the same chain every honest store ran. Restore compares archived
/// databases against it, so it runs under the same constraints as `migrate`.
pub(in crate::workbench) fn reference_schema(through: u32) -> WorkbenchResult<Connection> {
    let mut connection = Connection::open_in_memory()
        .map_err(|error| WorkbenchError::storage("Failed to create reference schema", error))?;
    connection
        .pragma_update(None, "foreign_keys", false)
        .map_err(|error| WorkbenchError::storage("Failed to prepare reference schema", error))?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start reference schema", error))?;
    apply_migrations(&transaction, 0, through)?;
    transaction
        .commit()
        .map_err(|error| WorkbenchError::storage("Failed to finish reference schema", error))?;
    Ok(connection)
}

pub(in crate::workbench) fn migrate(connection: &mut Connection, from: u32) -> WorkbenchResult<()> {
    connection
        .pragma_update(None, "foreign_keys", false)
        .map_err(|error| {
            WorkbenchError::storage("Failed to suspend migration constraints", error)
        })?;
    let migration = (|| {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                WorkbenchError::storage("Failed to start Workbench migration", error)
            })?;
        apply_migrations(&transaction, from, CURRENT_SCHEMA_VERSION)?;
        transaction
            .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
            .map_err(|error| {
                WorkbenchError::storage("Failed to record Workbench schema version", error)
            })?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit Workbench migration", error))
    })();
    let constraints = connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(|error| WorkbenchError::storage("Failed to restore migration constraints", error));
    migration?;
    constraints?;
    let violation: Option<String> = connection
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()
        .map_err(|error| {
            WorkbenchError::storage("Failed to validate migrated references", error)
        })?;
    if let Some(table) = violation {
        return Err(WorkbenchError::storage(
            "Migrated store contains an invalid reference",
            table,
        ));
    }
    Ok(())
}
