import { invoke } from "@tauri-apps/api/core";
import type {
  ClearWorkspaceRootRequest,
  CreateWorkspaceRequest,
  Mutation,
  ReconcileResult,
  RegisterWorkspaceRootRequest,
  UpdateWorkspaceRequest,
  Workspace,
  WorkspaceList,
} from "../workbenchTypes";

export const workspaceClient = {
  createWorkspace(request: CreateWorkspaceRequest) {
    return invoke<Mutation<Workspace>>("workbench_create_workspace", {
      request,
    });
  },
  registerWorkspaceRoot(request: RegisterWorkspaceRootRequest) {
    return invoke<Mutation<Workspace>>("workbench_register_workspace_root", {
      request,
    });
  },
  clearWorkspaceRoot(request: ClearWorkspaceRootRequest) {
    return invoke<Mutation<Workspace>>("workbench_clear_workspace_root", {
      request,
    });
  },
  getWorkspace(workspaceId: string) {
    return invoke<Workspace>("workbench_get_workspace", { workspaceId });
  },
  listWorkspaces(includeArchived = false) {
    return invoke<WorkspaceList>("workbench_list_workspaces", {
      includeArchived,
    });
  },
  updateWorkspace(request: UpdateWorkspaceRequest) {
    return invoke<Mutation<Workspace>>("workbench_update_workspace", {
      request,
    });
  },
  reconcileWorkspaceRoots(operationId: string) {
    return invoke<ReconcileResult>("workbench_reconcile_workspace_roots", {
      operationId,
    });
  },
};
