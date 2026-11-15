/** App-owned Workspace records. */

export interface ResearchNote {
  id: string;
  workspaceId: string;
  paperId: string | null;
  kind:
    | "question"
    | "assumption"
    | "decision"
    | "next_step"
    | "notation"
    | "handoff";
  body: string;
  state: "proposed" | "accepted" | "rejected" | "retired";
  origin: string;
  pinned: boolean;
  revision: number;
  createdAt: string;
  updatedAt: string;
  importedStatus?: string | null;
}

export interface PaperRevision {
  id: string;
  paperId: string;
  inputKind: string;
  entrypoint: string;
  dependencyManifest: Record<string, unknown>;
  contentHash: string;
  textReference: string | null;
  compiledArtifactId: string | null;
  extraction: Record<string, unknown>;
  captureComplete: boolean;
  capturedAt: string;
}

export interface ResearchPaper {
  id: string;
  workspaceId: string;
  title: string;
  role: "manuscript" | "appendix" | "other";
  currentRevisionId: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface PaperWithRevision {
  paper: ResearchPaper;
  revision: PaperRevision | null;
}
export interface PaperReadResult {
  revisionId: string;
  contentHash: string;
  start: number;
  end: number;
  text: string;
}
export interface PaperSearchHit {
  revisionId: string;
  contentHash: string;
  start: number;
  end: number;
  line: number;
  page: number | null;
  excerpt: string;
}

export interface ClaimRecord {
  id: string;
  workspaceId: string;
  paperId: string | null;
  workflowState: "proposed" | "under_review" | "accepted" | "retired";
  versionId: string;
  version: number;
  claim: string;
  kind: string;
  origin: string;
  paperLocator: Record<string, unknown> | null;
  dependencyHash: string | null;
  createdAt: string;
  updatedAt: string;
  importedStatus?: string | null;
}

export interface EvidenceRecord {
  id: string;
  workspaceId: string;
  claimVersionId: string;
  targetType: string;
  targetId: string;
  locator: Record<string, unknown> | null;
  relation: "supports" | "contradicts" | "qualifies";
  assessment:
    | "not_checked"
    | "model_assessed"
    | "human_confirmed"
    | "check_passed"
    | "check_failed";
  assessor: string;
  dependencyHash: string | null;
  freshness: "current" | "stale" | "unknown";
  staleReason: string | null;
  createdAt: string;
  updatedAt: string;
  importedStatus?: string | null;
}

export interface ResearchLedger {
  claims: ClaimRecord[];
  evidence: EvidenceRecord[];
  unsupportedAcceptedClaims: string[];
  staleClaims: string[];
}
