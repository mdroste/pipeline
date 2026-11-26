//! Workspaces persistence operations.

use super::*;

/// A workspace from `Store::prepare_workspace`, not yet saved.
pub struct PendingWorkspace {
    pub id: String,
    name: String,
    root: Option<String>,
    root_identity: Option<String>,
    operation_id: String,
}

impl PendingWorkspace {
    /// Saves the workspace in `connection`'s open transaction.
    pub fn insert(self, connection: &Connection) -> WorkbenchResult<Mutation<Workspace>> {
        let timestamp = now();
        ensure_operation_unused(connection, &self.operation_id)?;
        connection
            .execute(
                "INSERT INTO workspaces (id, name, root, root_identity, settings_revision, revision, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 0, 1, ?5, ?5)",
                params![self.id, self.name, self.root, self.root_identity, timestamp],
            )
            .map_err(|error| map_constraint("Failed to create workspace", error))?;
        let workspace = load_workspace(connection, &self.id)?;
        let sequence = insert_change(
            connection,
            &self.operation_id,
            "workspace",
            &self.id,
            "workspace_created",
            Some(&self.id),
            None,
            &json!({ "revision": 1, "hasRoot": workspace.root.is_some() }),
            &timestamp,
        )?;
        Ok(Mutation {
            record: workspace,
            sequence,
        })
    }
}

impl Store {
    pub fn create_workspace(
        &self,
        request: CreateWorkspaceRequest,
    ) -> WorkbenchResult<Mutation<Workspace>> {
        let pending = self.prepare_workspace(request)?;
        self.insert_workspace(pending)
    }

    fn insert_workspace(&self, pending: PendingWorkspace) -> WorkbenchResult<Mutation<Workspace>> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                WorkbenchError::storage("Failed to start workspace creation", error)
            })?;
        let created = pending.insert(&transaction)?;
        transaction.commit().map_err(|error| {
            WorkbenchError::storage("Failed to commit workspace creation", error)
        })?;
        Ok(created)
    }

    /// Checks a new workspace and gives it an id without saving it, so a caller
    /// can save it in the same transaction as work that may fail.
    pub fn prepare_workspace(
        &self,
        request: CreateWorkspaceRequest,
    ) -> WorkbenchResult<PendingWorkspace> {
        if request
            .root
            .as_deref()
            .is_none_or(|root| root.trim().is_empty())
        {
            return Err(WorkbenchError::invalid("Choose a folder for this project"));
        }
        self.prepare_workspace_record(request)
    }

    fn prepare_workspace_record(
        &self,
        request: CreateWorkspaceRequest,
    ) -> WorkbenchResult<PendingWorkspace> {
        validate_id("operation id", &request.operation_id)?;
        let name = validate_workspace_name(&request.name)?;
        let (root, root_identity) = resolve_optional_root(request.root.as_deref(), &self.root)?;
        Ok(PendingWorkspace {
            id: new_id("workspace")?,
            name,
            root,
            root_identity,
            operation_id: request.operation_id,
        })
    }

    /// Fixtures for data saved before Projects required folders.
    #[cfg(test)]
    pub(crate) fn create_legacy_workspace(
        &self,
        request: CreateWorkspaceRequest,
    ) -> WorkbenchResult<Mutation<Workspace>> {
        self.insert_workspace(self.prepare_workspace_record(request)?)
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
        _request: ClearWorkspaceRootRequest,
    ) -> WorkbenchResult<Mutation<Workspace>> {
        Err(WorkbenchError::invalid(
            "Projects require a folder. Choose another folder to change its location.",
        ))
    }

    pub fn workspace(&self, workspace_id: &str) -> WorkbenchResult<Workspace> {
        validate_id("workspace id", workspace_id)?;
        Self::workspace_on(&self.connection()?, workspace_id)
    }

    pub(crate) fn workspace_on(
        connection: &rusqlite::Connection,
        workspace_id: &str,
    ) -> WorkbenchResult<Workspace> {
        validate_id("workspace id", workspace_id)?;
        load_workspace(connection, workspace_id)
    }

    pub fn list_workspaces(&self, include_archived: bool) -> WorkbenchResult<WorkspaceList> {
        self.list_workspaces_page(include_archived, None)
    }

    pub fn list_workspaces_page(
        &self,
        include_archived: bool,
        after_id: Option<&str>,
    ) -> WorkbenchResult<WorkspaceList> {
        let connection = self.connection()?;
        let filter = if include_archived {
            ""
        } else {
            " AND archived_at IS NULL"
        };
        let sql = format!(
            "SELECT id, name, root, root_identity, settings_revision, revision, archived_at, missing_root_at, created_at, updated_at FROM workspaces WHERE id > ?2{filter} ORDER BY id LIMIT ?1"
        );
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| WorkbenchError::storage("Failed to prepare workspace list", error))?;
        let mut workspaces = statement
            .query_map(
                params![MAX_WORKSPACES_PER_LIST as i64, after_id.unwrap_or("")],
                read_workspace_row,
            )
            .map_err(|error| WorkbenchError::storage("Failed to list workspaces", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| WorkbenchError::storage("Failed to decode workspaces", error))?;
        workspaces.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
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

#[cfg(test)]
mod folder_tests {
    use super::*;

    #[test]
    fn new_projects_validate_the_folder_before_inserting_anything() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::open_at(&temporary.path().join("store")).unwrap();
        let file = temporary.path().join("paper.txt");
        std::fs::write(&file, "paper").unwrap();
        for root in [
            None,
            Some(String::new()),
            Some("   ".into()),
            Some(file.to_string_lossy().into_owned()),
            Some(
                temporary
                    .path()
                    .join("missing")
                    .to_string_lossy()
                    .into_owned(),
            ),
        ] {
            let failure = store
                .create_workspace(CreateWorkspaceRequest {
                    name: "Study".into(),
                    root,
                    operation_id: "create".into(),
                })
                .unwrap_err();
            assert_eq!(failure.code, "invalid_input");
            assert!(store.list_workspaces(true).unwrap().workspaces.is_empty());
        }
        let root = temporary.path().join("project");
        std::fs::create_dir(&root).unwrap();
        let project = store
            .create_workspace(CreateWorkspaceRequest {
                name: "Study".into(),
                root: Some(root.to_string_lossy().into_owned()),
                operation_id: "create".into(),
            })
            .unwrap()
            .record;
        assert_eq!(project.revision, 1);
        assert!(project.root_identity.is_some());
        assert_eq!(
            store.registered_root(&project).unwrap(),
            root.canonicalize().unwrap()
        );
    }
}
