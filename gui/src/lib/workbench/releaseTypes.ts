/** App-owned Workspace records. */

export interface ResearchRecipe {
  id: string;
  workspaceId: string | null;
  sourceRecipeId: string | null;
  name: string;
  description: string;
  instructions: string;
  requiredInputs: string[];
  requiredTools: string[];
  suggestedPermissionMode: "inspect" | "edit";
  expectedChecks: string[];
  version: number;
  revision: number;
  builtIn: boolean;
}

export interface RecipeInputCheck {
  input: string;
  available: boolean;
  detail: string;
}
export interface RecipeRun {
  id: string;
  workspaceId: string;
  sessionId: string;
  recipe: ResearchRecipe;
  status: "active" | "completed" | "incomplete";
  artifacts: Array<Record<string, unknown>>;
  checks: Array<Record<string, unknown>>;
  unresolvedIssues: string[];
  missingEvidence: string[];
  startedAt: string;
  completedAt: string | null;
}

export interface PerformanceBudget {
  metric: string;
  unit: "ms" | "percent" | "mib";
  budgetValue: number;
  rationale: string;
}
export interface ResearchEvaluation {
  id: string;
  fixtureId: string;
  fixtureVersion: number;
  variant: "recipe" | "plain_workspace" | "ordinary_codex";
  model: string | null;
  settings: Record<string, unknown>;
  outcome: Record<string, unknown>;
  latencyMs: number | null;
  inputTokens: number | null;
  outputTokens: number | null;
  createdAt: string;
}

export interface ReviewHandoff {
  version: number;
  id: string;
  workspaceId: string;
  sessionId: string | null;
  paperId: string;
  revisionId: string;
  contentHash: string;
  mediaKind: string;
  stagedPath: string;
  inputInterpretation: "document" | "source_tree";
  metadata: Record<string, unknown>;
  externalReference: string | null;
  createdAt: string;
  linkedAt: string | null;
}

export interface ResearchArchiveReport {
  path: string;
  workspaceCount: number;
  sessionCount: number;
  blobCount: number;
  bytes: number;
  nativeBindingsRetired: number;
  portabilityNote: string;
  /** Records whose files the backup left out, by the names researchers use. */
  omittedFiles: string[];
}

export interface MissingBackupFile {
  description: string;
  reference: string;
}

export interface ExchangeSelection {
  includeNotes: boolean;
  recordKinds: string[];
  paperIds: string[];
  executionIds: string[];
  includeSources: boolean;
  includeLedger: boolean;
  includeCompiledPdfs: boolean;
}
export interface ExchangeObjectEntry {
  kind: string;
  id: string;
  fingerprint: string;
  title: string;
}
export interface ExchangeExternalReference {
  kind: string;
  id: string;
  reason: string;
}
export interface ExchangePreview {
  objects: ExchangeObjectEntry[];
  blobCount: number;
  blobBytes: number;
  exclusions: string[];
  externalReferences: ExchangeExternalReference[];
  limitations: string[];
}
export interface ExchangeReport {
  path: string;
  bytes: number;
  packageHash: string;
  objectCount: number;
  blobCount: number;
  exclusions: string[];
  externalReferences: ExchangeExternalReference[];
  limitations: string[];
}
export interface ExchangeInspection {
  path: string;
  packageHash: string;
  workspaceName: string;
  sourceNamespace: string;
  createdAt: string;
  workspaceRoot: string | null;
  counts: Record<string, number>;
  blobCount: number;
  blobBytes: number;
  exclusions: string[];
  externalReferences: number;
  limitations: string[];
  ownExport: boolean;
}
export type ExchangeTarget =
  | { kind: "newWorkspace"; name: string; root: string | null }
  | { kind: "existingWorkspace"; workspaceId: string };
export interface ExchangeDecision {
  kind: string;
  id: string;
  outcome: string;
  newId: string | null;
  title: string;
}
export interface ExchangeImportPreview {
  targetWorkspaceId: string | null;
  new: number;
  identical: number;
  remapped: number;
  conflicts: ExchangeDecision[];
  decisions: ExchangeDecision[];
  blobCount: number;
  limitations: string[];
}
export interface ExchangeImportReport {
  importId: string;
  workspaceId: string;
  new: number;
  identical: number;
  remapped: number;
  conflicts: number;
  blobs: number;
  limitations: string[];
}
export interface ExchangeConflict {
  id: string;
  importId: string;
  objectKind: string;
  objectId: string;
  local: unknown;
  base?: unknown;
  imported: unknown;
  state: string;
  recordedAt: string;
}
export interface StorageCategory {
  key: string;
  label: string;
  bytes: number;
  entries: number;
  disposable: boolean;
  note: string;
  partial?: boolean;
}
export interface TrashEntry {
  id: string;
  category: string;
  originalPath: string;
  sizeBytes: number;
  reason: string;
  movedAt: string;
  state: string;
}
export interface StorageReport {
  root: string;
  categories: StorageCategory[];
  trash: TrashEntry[];
  activeJobs: number;
  note: string;
}
export interface PrunePlan {
  entries: Array<{ category: string; path: string; bytes: number }>;
  bytes: number;
  applied: boolean;
  trashIds: string[];
  previewToken: string;
}
export interface DraftWorkflow {
  name: string;
  canonicalJson: string;
  fingerprint: string;
  steps: Array<{ id: string; label: string; source: string }>;
  unsupported: string[];
  notes: string[];
}

export interface ResearchArchiveInspection {
  path: string;
  formatVersion: number;
  storeSchemaVersion: number;
  workspaceRoots: string[];
  portabilityNote: string;
  omittedFileCount: number;
}
