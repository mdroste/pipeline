import { invoke } from "@tauri-apps/api/core";
import type {
  ResearchNote,
  PaperWithRevision,
  PaperReadResult,
  PaperSearchHit,
} from "../workbenchTypes";

export const researchClient = {
  createNote(request: {
    workspaceId: string;
    paperId: string | null;
    kind: ResearchNote["kind"];
    body: string;
    state?: ResearchNote["state"];
    origin: string;
    pinned: boolean;
    operationId: string;
  }) {
    return invoke<ResearchNote>("workbench_create_note", { request });
  },
  updateNote(request: {
    noteId: string;
    expectedRevision: number;
    body?: string;
    state?: ResearchNote["state"];
    pinned?: boolean;
    operationId: string;
  }) {
    return invoke<ResearchNote>("workbench_update_note", { request });
  },
  listNotes(workspaceId: string, includeRejected = false) {
    return invoke<ResearchNote[]>("workbench_list_notes", {
      workspaceId,
      includeRejected,
    });
  },
  importPaper(request: {
    workspaceId: string;
    paperId: string | null;
    title: string;
    role: "manuscript" | "appendix" | "other";
    path: string;
    operationId: string;
  }) {
    return invoke<PaperWithRevision>("workbench_import_paper", { request });
  },
  listPapers(workspaceId: string) {
    return invoke<PaperWithRevision[]>("workbench_list_papers", {
      workspaceId,
    });
  },
  paperRead(request: {
    workspaceId: string;
    revisionId: string;
    start?: number;
    length?: number;
  }) {
    return invoke<PaperReadResult>("workbench_paper_read", { request });
  },
  paperSearch(
    workspaceId: string,
    revisionId: string,
    query: string,
    limit = 20,
  ) {
    return invoke<PaperSearchHit[]>("workbench_paper_search", {
      workspaceId,
      revisionId,
      query,
      limit,
    });
  },
};
