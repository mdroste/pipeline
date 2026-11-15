import { invoke } from "@tauri-apps/api/core";
import type {
  Project,
  ProjectIssueLedger,
  ProjectsResponse,
  TrashedProject,
} from "./reviewCollectionTypes";

/** Review collections group runs and retain decisions about findings. */
export const reviewCollectionClient = {
  list: () => invoke<ProjectsResponse>("list_projects"),
  trashed: () => invoke<TrashedProject[]>("list_trashed_projects"),
  create: (args: { name: string; description: string | null }) =>
    invoke<Project>("create_project", args),
  update: (args: { id: string; name: string; description: string }) =>
    invoke<Project>("update_project", args),
  setRun: (args: { projectId: string; runId: string; included: boolean }) =>
    invoke<Project>("set_project_run", args),
  trash: (args: { id: string }) => invoke<void>("delete_project", args),
  restore: (args: { id: string }) =>
    invoke<void>("restore_trashed_project", args),
  remove: (args: { id: string }) =>
    invoke<void>("permanently_delete_trashed_project", args),
  syncLedger: (args: { projectId: string }) =>
    invoke<ProjectIssueLedger>("sync_project_issue_ledger", args),
  updateIssue: (args: {
    projectId: string;
    issueId: string;
    status: string;
    note: string;
  }) => invoke<ProjectIssueLedger>("update_project_issue", args),
  mergeIssues: (args: {
    projectId: string;
    sourceIssueId: string;
    targetIssueId: string;
  }) => invoke<ProjectIssueLedger>("merge_project_issues", args),
};
