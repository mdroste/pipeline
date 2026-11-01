import { invoke } from "@tauri-apps/api/core";
import type { RepositoryStatus } from "./repositoryClient";

/** Mirrors workbench::project::version_control. Paths are relative to the
 * project folder; remote addresses never come back to the page. */
export type FileStatus =
  | "modified"
  | "added"
  | "deleted"
  | "renamed"
  | "copied"
  | "untracked"
  | "conflicted";

export interface ChangedFile {
  path: string;
  oldPath: string | null;
  status: FileStatus;
  staged: boolean;
  unstaged: boolean;
  isDirectory: boolean;
  isSubmodule: boolean;
  conflict: string | null;
}
export interface LargeFile {
  path: string;
  size: number;
}
export interface BranchInfo {
  name: string;
  current: boolean;
  upstream: string | null;
  remoteRef: string | null;
}
export interface RepositoryView {
  status: RepositoryStatus;
  files: ChangedFile[];
  filesTruncated: boolean;
  largeFiles: LargeFile[];
  branches: BranchInfo[];
  operation: "merge" | "rebase" | "cherry-pick" | "revert" | null;
  operationMessage: string | null;
  /** Conflicted files elsewhere in the repository, outside the project folder. */
  outsideConflicts: number;
  identity: { name: string; email: string } | null;
  folderPrefix: string;
  canUndoCommit: boolean;
}
export type VersionControlState =
  | { state: "noFolder" }
  | { state: "gitMissing" }
  | { state: "notRepository"; problem: string | null }
  | ({ state: "ready" } & RepositoryView);

export type ConflictChoice = "ours" | "theirs" | "edited";
export type VersionControlAction =
  | { kind: "initialize" }
  | { kind: "setRemote"; url: string }
  | { kind: "setIdentity"; name: string; email: string }
  | {
      kind: "commit";
      paths: string[];
      summary: string;
      description: string | null;
    }
  | { kind: "discard"; paths: string[] }
  | { kind: "ignore"; path: string }
  | { kind: "undoCommit" }
  | { kind: "switchBranch"; name: string }
  | { kind: "createBranch"; name: string }
  | { kind: "fetch" }
  | { kind: "pull" }
  | { kind: "push" }
  | { kind: "resolve"; path: string; choice: ConflictChoice }
  | { kind: "abortOperation" };

export interface ActionResult {
  state: VersionControlState;
  notice: string | null;
  undoneMessage: string | null;
}

export type DiffRequest =
  | { kind: "working"; path: string; oldPath: string | null }
  | { kind: "commit"; sha: string; path: string; oldPath: string | null };
export interface FileDiff {
  kind:
    "diff" | "new" | "conflict" | "binary" | "tooLarge" | "folder" | "empty";
  text: string;
}

export interface HistoryCommit {
  sha: string;
  author: string;
  date: string;
  subject: string;
  isMerge: boolean;
  refs: string[];
  unpushed: boolean;
}
export interface HistoryPage {
  commits: HistoryCommit[];
  hasMore: boolean;
}
export interface CommitFile {
  path: string;
  oldPath: string | null;
  status: FileStatus;
}
export interface CommitDetail {
  sha: string;
  author: string;
  date: string;
  message: string;
  files: CommitFile[];
  filesTruncated: boolean;
}

export const versionControlClient = {
  state: (workspaceId: string) =>
    invoke<VersionControlState>("workbench_version_control_state", {
      workspaceId,
    }),
  diff: (workspaceId: string, request: DiffRequest) =>
    invoke<FileDiff>("workbench_version_control_diff", {
      workspaceId,
      request,
    }),
  history: (workspaceId: string, skip: number) =>
    invoke<HistoryPage>("workbench_version_control_history", {
      workspaceId,
      skip,
    }),
  commit: (workspaceId: string, sha: string) =>
    invoke<CommitDetail>("workbench_version_control_commit", {
      workspaceId,
      sha,
    }),
  act: (workspaceId: string, action: VersionControlAction) =>
    invoke<ActionResult>("workbench_version_control_action", {
      workspaceId,
      action,
    }),
  /** Stops a running push, pull, or fetch. */
  cancel: (workspaceId: string) =>
    invoke<void>("workbench_version_control_cancel", { workspaceId }),
};
