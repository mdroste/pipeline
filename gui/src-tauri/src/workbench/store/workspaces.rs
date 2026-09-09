//! Workspaces persistence operations.

use super::*;

impl Store {
    pub fn create_workspace(
        &self,
        request: CreateWorkspaceRequest,
    ) -> WorkbenchResult<Mutation<Workspace>> {
        validate_id("operation id", &request.operation_id)?;
        let name = validate_workspace_name(&request.name)?;
        let (root, root_identity) = resolve_optional_root(request.root.as_deref(), &self.root)?;
        let id = new_id("workspace")?;
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                WorkbenchError::storage("Failed to start workspace creation", error)
            })?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        transaction
            .execute(
                "INSERT INTO workspaces (id, name, root, root_identity, settings_revision, revision, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 0, 1, ?5, ?5)",
                params![id, name, root, root_identity, timestamp],
            )
            .map_err(|error| map_constraint("Failed to create workspace", error))?;
        let workspace = load_workspace(&transaction, &id)?;
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "workspace",
            &id,
            "workspace_created",
            Some(&id),
            None,
            &json!({ "revision": 1, "hasRoot": workspace.root.is_some() }),
            &timestamp,
        )?;
        transaction.commit().map_err(|error| {
            WorkbenchError::storage("Failed to commit workspace creation", error)
        })?;
        Ok(Mutation {
            record: workspace,
            sequence,
        })
    }

    pub fn register_workspace_root(
        &self,
        request: RegisterWorkspaceRootRequest,
    ) -> WorkbenchResult<Mutation<Workspace>> {
        validate_id("workspace id", &request.workspace_id)?;
        validate_id("operation id", &request.operation_id)?;
        let canonical = canonical_workspace_root(Path::new(&request.root), &self.root)?;
        let root = canonical.to_string_lossy().into_owned();
        let identity = root_identity(&canonical)?;
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start root registration", error))?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        let current = load_workspace(&transaction, &request.workspace_id)?;
        require_revision(current.revision, request.expected_revision, "Workspace")?;
        let next_revision = current.revision + 1;
        transaction
            .execute(
                "UPDATE workspaces SET root = ?1, root_identity = ?2, missing_root_at = NULL, revision = ?3, updated_at = ?4 WHERE id = ?5",
                params![root, identity, next_revision, timestamp, request.workspace_id],
            )
            .map_err(|error| map_constraint("Failed to register workspace root", error))?;
        let workspace = load_workspace(&transaction, &request.workspace_id)?;
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "workspace",
            &request.workspace_id,
            "workspace_root_registered",
            Some(&request.workspace_id),
            None,
            &json!({ "revision": next_revision, "rootIdentity": workspace.root_identity }),
            &timestamp,
        )?;
        transaction.commit().map_err(|error| {
            WorkbenchError::storage("Failed to commit root registration", error)
        })?;
        Ok(Mutation {
            record: workspace,
            sequence,
        })
    }

    pub fn clear_workspace_root(
        &self,
        request: ClearWorkspaceRootRequest,
    ) -> WorkbenchResult<Mutation<Workspace>> {
        validate_id("workspace id", &request.workspace_id)?;
        validate_id("operation id", &request.operation_id)?;
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start root removal", error))?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        let current = load_workspace(&transaction, &request.workspace_id)?;
        require_revision(current.revision, request.expected_revision, "Workspace")?;
        let next_revision = current.revision + 1;
        transaction
            .execute(
                "UPDATE workspaces SET root = NULL, root_identity = NULL, missing_root_at = NULL, revision = ?1, updated_at = ?2 WHERE id = ?3",
                params![next_revision, timestamp, request.workspace_id],
            )
            .map_err(|error| WorkbenchError::storage("Failed to remove workspace root", error))?;
        let workspace = load_workspace(&transaction, &request.workspace_id)?;
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "workspace",
            &request.workspace_id,
            "workspace_root_cleared",
            Some(&request.workspace_id),
            None,
            &json!({ "revision": next_revision }),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit root removal", error))?;
        Ok(Mutation {
            record: workspace,
            sequence,
        })
    }

    pub fn workspace(&self, workspace_id: &str) -> WorkbenchResult<Workspace> {
        validate_id("workspace id", workspace_id)?;
        load_workspace(&self.connection()?, workspace_id)
    }

    pub fn list_workspaces(&self, include_archived: bool) -> WorkbenchResult<WorkspaceList> {
        let connection = self.connection()?;
        let filter = if include_archived {
            ""
        } else {
            " WHERE archived_at IS NULL"
        };
        let sql = format!(
            "SELECT id, name, root, root_identity, settings_revision, revision, archived_at, missing_root_at, created_at, updated_at FROM workspaces{filter} ORDER BY updated_at DESC, id LIMIT ?1"
        );
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| WorkbenchError::storage("Failed to prepare workspace list", error))?;
        let workspaces = statement
            .query_map([(MAX_WORKSPACES_PER_LIST + 1) as i64], read_workspace_row)
            .map_err(|error| WorkbenchError::storage("Failed to list workspaces", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| WorkbenchError::storage("Failed to decode workspaces", error))?;
        if workspaces.len() > MAX_WORKSPACES_PER_LIST {
            return Err(WorkbenchError::storage(
                "Workspace list exceeds its safety limit",
                MAX_WORKSPACES_PER_LIST,
            ));
        }
        Ok(WorkspaceList {
            workspaces,
            sequence: current_sequence(&connection)?,
        })
    }

    pub fn update_workspace(
        &self,
        request: UpdateWorkspaceRequest,
    ) -> WorkbenchResult<Mutation<Workspace>> {
        validate_id("workspace id", &request.workspace_id)?;
        validate_id("operation id", &request.operation_id)?;
        let name = request
            .name
            .as_deref()
            .map(validate_workspace_name)
            .transpose()?;
        if name.is_none() && request.archived.is_none() {
            return Err(WorkbenchError::invalid(
                "Workspace update must change its name or archived state",
            ));
        }
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start workspace update", error))?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        let current = load_workspace(&transaction, &request.workspace_id)?;
        require_revision(current.revision, request.expected_revision, "Workspace")?;
        let archived_at = match request.archived {
            Some(true) => current.archived_at.or_else(|| Some(timestamp.clone())),
            Some(false) => None,
            None => current.archived_at,
        };
        let next_revision = current.revision + 1;
        transaction
            .execute(
                "UPDATE workspaces SET name = ?1, archived_at = ?2, revision = ?3, updated_at = ?4 WHERE id = ?5",
                params![name.unwrap_or(current.name), archived_at, next_revision, timestamp, request.workspace_id],
            )
            .map_err(|error| WorkbenchError::storage("Failed to update workspace", error))?;
        let workspace = load_workspace(&transaction, &request.workspace_id)?;
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "workspace",
            &request.workspace_id,
            "workspace_updated",
            Some(&request.workspace_id),
            None,
            &json!({ "revision": next_revision, "archived": workspace.archived_at.is_some() }),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit workspace update", error))?;
        Ok(Mutation {
            record: workspace,
            sequence,
        })
    }
}
