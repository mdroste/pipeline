import { invoke } from "@tauri-apps/api/core";
import type {
  ResearchNote,
  PaperWithRevision,
  PaperReadResult,
  PaperSearchHit,
  SourceRecord,
  SourceImportResult,
  ResearchLedger,
  ClaimRecord,
  EvidenceRecord,
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
  importSource(request: {
    workspaceId: string;
    title: string;
    citationKey: string | null;
    identifiers: Record<string, unknown>;
    versionLabel: string | null;
    path: string | null;
    locator: string | null;
    accessState: SourceRecord["accessState"];
    acquiredVia: string;
    operationId: string;
  }) {
    return invoke<SourceImportResult>("workbench_import_source", { request });
  },
  listSources(workspaceId: string) {
    return invoke<SourceRecord[]>("workbench_list_sources", { workspaceId });
  },
  proposeClaim(request: {
    workspaceId: string;
    paperId: string | null;
    claim: string;
    kind: string;
    origin: string;
    operationId: string;
  }) {
    return invoke<ClaimRecord>("workbench_propose_claim", { request });
  },
  setClaimState(request: {
    claimId: string;
    state: ClaimRecord["workflowState"];
    operationId: string;
  }) {
    return invoke<ClaimRecord>("workbench_set_claim_state", { request });
  },
  proposeEvidence(request: {
    workspaceId: string;
    claimVersionId: string;
    targetType: string;
    targetId: string;
    locator: Record<string, unknown> | null;
    relation: EvidenceRecord["relation"];
    assessor: string;
    operationId: string;
  }) {
    return invoke<EvidenceRecord>("workbench_propose_evidence", { request });
  },
  confirmEvidence(request: { evidenceId: string; operationId: string }) {
    return invoke<EvidenceRecord>("workbench_confirm_evidence", { request });
  },
  researchLedger(workspaceId: string) {
    return invoke<ResearchLedger>("workbench_research_ledger", { workspaceId });
  },
};
