import { invoke } from "@tauri-apps/api/core";
import type { CheckPolicy } from "./studioTypes";
export type { CheckPolicy } from "./studioTypes";
import type { DeskRecord } from "./deskClient";
import type { ProjectRecord, ResearchTask } from "./projectClient";

export interface GitHubSnapshot {
  repository: string;
  branch: string;
  head: string;
  checkedAt: string;
  scope: string;
  checksError?: string | null;
  issuesError?: string | null;
  issues?: {
    id: string;
    number: number;
    title: string;
    body: string;
    url: string;
  }[];
  pullRequests: {
    number: number;
    title: string;
    url: string;
    state: string;
    isDraft: boolean;
    headRefOid: string;
    baseRefName: string;
    headRefName: string;
    statusCheckRollup: unknown[];
    comments: {
      id: string;
      body: string;
      url: string;
      createdAt: string;
      author?: { login: string };
    }[];
  }[];
  runs: {
    id: number;
    name: string;
    head_sha: string;
    status: string;
    conclusion: string | null;
    html_url: string;
    artifacts_url: string;
  }[];
}
export const projectIntegrations = {
  saveCheckPolicy: (
    workspaceId: string,
    expectedRevision: number,
    policy: CheckPolicy,
  ) =>
    invoke<ProjectRecord<CheckPolicy>>("workbench_check_policy", {
      workspaceId,
      expectedRevision,
      policy,
    }),
  runCheck: (workspaceId: string) =>
    invoke("workbench_check_run", { workspaceId }),
  captureSubmission: (
    workspaceId: string,
    request: {
      title: string;
      handoff: string;
      files: Record<string, string>;
      revisions: string[];
      records: { id: string; kind: string; revision: number }[];
      executions: string[];
      operationId: string;
    },
  ) =>
    invoke<DeskRecord<SubmissionSnapshot>>("workbench_submission_capture", {
      workspaceId,
      request,
    }),
  exportSubmission: (
    workspaceId: string,
    id: string,
    expectedHash: string,
    path: string,
  ) =>
    invoke("workbench_submission_export", {
      workspaceId,
      id,
      expectedHash,
      path,
    }),
  citations: (
    workspaceId: string,
    request: {
      bibliographies: string[];
      sourcePath: string | null;
      sourceText?: string | null;
      checkpointId: string | null;
    },
  ) => invoke<CitationIndex>("workbench_citations", { workspaceId, request }),
  zotero: (
    collection: string | null,
    start = 0,
    expectedServer: string | null = null,
  ) =>
    invoke<ZoteroPreview>("workbench_zotero_preview", {
      collection,
      start,
      expectedServer,
    }),
  importZotero: (
    workspaceId: string,
    preview: ZoteroPreview,
    selected: string[],
  ) => invoke("workbench_zotero_import", { workspaceId, preview, selected }),
  github: (workspaceId: string) =>
    invoke<DeskRecord<GitHubSnapshot>>("workbench_github_inspect", {
      workspaceId,
    }),
  draftPullRequest: (
    workspaceId: string,
    request: {
      expectedHead: string;
      expectedRepository: string;
      title: string;
      body: string;
      base: string;
    },
  ) => invoke<string>("workbench_github_draft", { workspaceId, request }),
  importGitHubComment: (
    workspaceId: string,
    snapshotId: string,
    commentId: string,
  ) =>
    invoke<ProjectRecord<ResearchTask>>("workbench_github_comment", {
      workspaceId,
      snapshotId,
      commentId,
    }),
};
export interface CitationEntry {
  key: string;
  title: string | null;
  author: string | null;
  year: string | null;
  doi: string | null;
  url: string | null;
  path: string;
  line: number;
  warnings: string[];
}
export interface CitationIndex {
  entries: CitationEntry[];
  versions: Record<string, string>;
  sourceHash: string;
  citations: string[];
  missing: string[];
  warnings: string[];
  coverage: string;
}
export interface ZoteroPreview {
  serverId: string | null;
  collection: string;
  start: number;
  hasMore: boolean;
  items: {
    key: string;
    version?: number;
    data: { name?: string; title?: string; itemType?: string };
  }[];
}
export interface SubmissionSnapshot {
  handoff: string;
  capturedAt: string;
  revisions: string[];
  files: {
    path: string;
    hash: string;
    size: number;
    origin: Record<string, unknown>;
  }[];
}
