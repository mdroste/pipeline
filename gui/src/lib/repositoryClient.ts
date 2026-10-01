import { invoke } from "@tauri-apps/api/core";

/** Mirrors workbench::project::repository. Remote URLs never reach the page. */
export interface RepositoryRemote {
  name: string;
  host: string;
  owner: string;
  repo: string;
  /** Present only for github.com. */
  webUrl: string | null;
}
export interface RepositoryCommit {
  sha: string;
  author: string;
  date: string;
  subject: string;
}
export interface RepositoryStatus {
  /** Null while HEAD is detached. */
  branch: string | null;
  /** Null before the first commit. */
  head: string | null;
  upstream: string | null;
  /** Relative to `upstream` as of the last fetch. */
  ahead: number;
  behind: number;
  fetchedAt: string | null;
  changed: number;
  untracked: number;
  conflicts: number;
  changedPaths: string[];
  remote: RepositoryRemote | null;
  commits: RepositoryCommit[];
  incoming: RepositoryCommit[];
}

export const repositoryClient = {
  /** Local state only; null when the project folder is not a Git repository. */
  status: (workspaceId: string) =>
    invoke<RepositoryStatus | null>("workbench_repository_status", {
      workspaceId,
    }),
  /** Contacts the remote with the researcher's own Git credentials. Changes no files. */
  fetch: (workspaceId: string) =>
    invoke<RepositoryStatus>("workbench_repository_fetch", { workspaceId }),
};

export const remoteLabel = (remote: RepositoryRemote | null) =>
  remote?.host === "github.com" ? "GitHub" : (remote?.host ?? "the remote");
