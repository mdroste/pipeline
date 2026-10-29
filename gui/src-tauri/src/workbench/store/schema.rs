//! SQLite connection policy and the ordered, atomic schema migration chain.

use super::{WorkbenchError, WorkbenchResult, CURRENT_SCHEMA_VERSION};
use rusqlite::{Connection, OptionalExtension as _, TransactionBehavior};
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
        if from < 1 {
            transaction
                .execute_batch(include_str!("../migrations/001_initial.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 1", error)
                })?;
        }
        if from < 2 {
            transaction
                .execute_batch(include_str!("../migrations/002_independent_workspaces.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 2", error)
                })?;
        }
        if from < 3 {
            transaction
                .execute_batch(include_str!("../migrations/003_research_harness.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 3", error)
                })?;
        }
        if from < 4 {
            transaction
                .execute_batch(include_str!(
                    "../migrations/004_native_instruction_sources.sql"
                ))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 4", error)
                })?;
        }
        if from < 5 {
            transaction
                .execute_batch(include_str!("../migrations/005_release_readiness.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 5", error)
                })?;
        }
        if from < 6 {
            transaction
                .execute_batch(include_str!("../migrations/006_project_surface.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 6", error)
                })?;
        }
        if from < 7 {
            transaction
                .execute_batch(include_str!("../migrations/007_research_studio.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 7", error)
                })?;
        }
        if from < 8 {
            transaction
                .execute_batch(include_str!("../migrations/008_theory_notes.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 8", error)
                })?;
        }
        if from < 9 {
            transaction
                .execute_batch(include_str!("../migrations/009_project_exchange.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 9", error)
                })?;
        }
        if from < 10 {
            transaction
                .execute_batch(include_str!("../migrations/010_preferences.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 10", error)
                })?;
        }
        if from < 11 {
            transaction
                .execute_batch(include_str!("../migrations/011_research_desk.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 11", error)
                })?;
        }
        if from < 12 {
            transaction
                .execute_batch(include_str!("../migrations/012_task_exchanges.sql"))
                .map_err(|e| WorkbenchError::storage("Failed to migrate task exchanges", e))?;
        }
        if from < 13 {
            transaction
                .execute_batch(include_str!("../migrations/013_research_programs.sql"))
                .map_err(|e| WorkbenchError::storage("Failed to migrate research programs", e))?;
        }
        if from < 14 {
            transaction
                .execute_batch(include_str!("../migrations/014_agent_base_prompts.sql"))
                .map_err(|e| WorkbenchError::storage("Failed to migrate agent base prompts", e))?;
        }
        if from < 15 {
            transaction
                .execute_batch(include_str!("../migrations/015_self_discovery.sql"))
                .map_err(|e| {
                    WorkbenchError::storage("Failed to migrate self-discovery roles", e)
                })?;
        }
        if from < 16 {
            transaction
                .execute_batch(include_str!("../migrations/016_execution_adapters.sql"))
                .map_err(|e| WorkbenchError::storage("Failed to migrate execution adapters", e))?;
        }
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
