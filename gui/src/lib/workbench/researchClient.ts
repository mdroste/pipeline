import { invoke } from "@tauri-apps/api/core";
import type {
  ResearchNote,
  PaperWithRevision,
  PaperReadResult,
  DocumentVersion,
  PaperSearchHit,
} from "./researchTypes";

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
    documentType?: import("../documentTypes").DocumentType;
    path: string;
    operationId: string;
  }) {
    return invoke<PaperWithRevision>("workbench_import_paper", { request });
  },
  updateDocument(request: {
    workspaceId: string;
    paperId: string;
    expectedUpdatedAt: string;
    title: string;
    documentType: import("../documentTypes").DocumentType;
    pinned: boolean;
    archived: boolean;
    currentRevisionId?: string | null;
    operationId: string;
  }) {
    return invoke<PaperWithRevision>("workbench_update_document", { request });
  },
  documentVersions(workspaceId: string, paperId: string) {
    return invoke<DocumentVersion[]>("workbench_document_versions", {
      workspaceId,
      paperId,
    });
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
