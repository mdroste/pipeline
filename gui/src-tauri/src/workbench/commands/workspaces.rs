//! Workspace records and root registration.

use super::run_store;
use crate::workbench::store::{
    ClearWorkspaceRootRequest, CreateWorkspaceRequest, Mutation, RegisterWorkspaceRootRequest,
    UpdateWorkspaceRequest, WorkbenchResult, Workspace, WorkspaceList,
};

#[tauri::command]
pub async fn workbench_create_workspace(
    request: CreateWorkspaceRequest,
) -> WorkbenchResult<Mutation<Workspace>> {
    run_store(move |store| store.create_workspace(request)).await
}

#[tauri::command]
pub async fn workbench_register_workspace_root(
    request: RegisterWorkspaceRootRequest,
) -> WorkbenchResult<Mutation<Workspace>> {
    run_store(move |store| store.register_workspace_root(request)).await
}

#[tauri::command]
pub async fn workbench_clear_workspace_root(
    request: ClearWorkspaceRootRequest,
) -> WorkbenchResult<Mutation<Workspace>> {
    run_store(move |store| store.clear_workspace_root(request)).await
}

#[tauri::command]
pub async fn workbench_get_workspace(workspace_id: String) -> WorkbenchResult<Workspace> {
    run_store(move |store| store.workspace(&workspace_id)).await
}

#[tauri::command]
pub async fn workbench_list_workspaces(
    include_archived: bool,
    after_id: Option<String>,
) -> WorkbenchResult<WorkspaceList> {
    run_store(move |store| store.list_workspaces_page(include_archived, after_id.as_deref())).await
}

#[tauri::command]
pub async fn workbench_update_workspace(
    request: UpdateWorkspaceRequest,
) -> WorkbenchResult<Mutation<Workspace>> {
    run_store(move |store| store.update_workspace(request)).await
}
