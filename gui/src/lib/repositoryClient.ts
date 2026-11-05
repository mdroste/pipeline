import { invoke } from "@tauri-apps/api/core";

/** Mirrors workbench::project::repository. Remote URLs never reach the page. */
export interface RepositoryRemote {
  name: string;
  /** Empty, like `owner` and `repo`, for an address with no host and name to
   * show, such as a shared-drive path. Such a remote still works. */
  host: string;
  owner: string;
  repo: string;
  /** Null for hosts other than GitHub and Bitbucket. */
  provider?: "github" | "bitbucket" | null;
  /** Present only for GitHub and Bitbucket. */
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

const HOST_NAMES: Record<string, string> = {
  "github.com": "GitHub",
  "bitbucket.org": "Bitbucket",
  "git.overleaf.com": "Overleaf",
};
export const remoteLabel = (remote: RepositoryRemote | null) =>
  remote?.host ? (HOST_NAMES[remote.host] ?? remote.host) : "the remote";

/** One line naming a remote, by Git's name when its address shows no host. */
export const remoteSummary = (remote: RepositoryRemote) =>
  remote.host
    ? `${remote.owner ? `${remote.owner}/` : ""}${remote.repo} on ${remoteLabel(remote)}`
    : `Remote ${remote.name}`;

const pathSegments = (value: string) =>
  value.split("/").map(encodeURIComponent).join("/");

/** Where a host shows a commit, or null for hosts with no known layout. */
export function commitUrl(remote: RepositoryRemote | null, sha: string) {
  if (!remote?.webUrl) return null;
  return remote.provider === "bitbucket"
    ? `${remote.webUrl}/commits/${sha}`
    : `${remote.webUrl}/commit/${sha}`;
}

/** The host's page for opening a pull request from a pushed branch. */
export function pullRequestUrl(
  remote: RepositoryRemote | null,
  branch: string,
) {
  if (!remote?.webUrl) return null;
  return remote.provider === "bitbucket"
    ? `${remote.webUrl}/pull-requests/new?source=${encodeURIComponent(branch)}&t=1`
    : `${remote.webUrl}/compare/${pathSegments(branch)}?expand=1`;
}
