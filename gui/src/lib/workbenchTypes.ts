/** App-owned Workbench DTOs. Raw Codex protocol payloads never cross this boundary. */

export interface WorkbenchError {
  code: string;
  message: string;
  retryable: boolean;
  recovery: string | null;
}

export interface Workspace {
  id: string;
  name: string;
  root: string | null;
  rootIdentity: string | null;
  settingsRevision: number;
  revision: number;
  archivedAt: string | null;
  missingRootAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface WorkbenchSession {
  id: string;
  workspaceId: string | null;
  paperId: string | null;
  title: string;
  presetId: string | null;
  overrides: Record<string, unknown>;
  draft: string;
  revision: number;
  archivedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface Mutation<T> {
  record: T;
  sequence: number;
}

export interface WorkspaceList {
  workspaces: Workspace[];
  sequence: number;
}

export interface SessionList {
  sessions: WorkbenchSession[];
  sequence: number;
}

export interface SessionSnapshot {
  workspace: Workspace | null;
  session: WorkbenchSession;
  sequence: number;
}

export interface SessionBinding {
  id: string;
  sessionId: string;
  runtimeNamespace: string;
  providerThreadId: string;
  incarnation: number;
  createdAt: string;
  retiredAt: string | null;
  retirementReason: string | null;
  harnessFingerprint: string | null;
  instructionSources: string[];
}

export interface ConversationTurn {
  id: string;
  bindingId: string;
  clientSubmissionId: string;
  providerTurnId: string | null;
  state: string;
  error: unknown | null;
  createdAt: string;
  updatedAt: string;
  terminalAt: string | null;
}

export interface TranscriptItem {
  id: string;
  turnId: string | null;
  providerItemId: string;
  itemKind: string;
  payload: Record<string, unknown> | null;
  isFinal: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface ConversationSnapshot extends SessionSnapshot {
  activeBinding: SessionBinding | null;
  turns: ConversationTurn[];
  items: TranscriptItem[];
}

export interface ReconcileResult {
  changed: number;
  missing: number;
  restored: number;
  sequence: number;
}

export type WorkspaceAccountStatus = "signedOut" | "chatgpt" | "unsupported";

export interface WorkspaceAccountState {
  status: WorkspaceAccountStatus;
  email: string | null;
  planType: string | null;
  unsupportedAccountType: string | null;
  requiresOpenaiAuth: boolean;
}

export interface WorkbenchCodexStatus {
  connected: boolean;
  epoch: number;
  pid: number | null;
  executable: string | null;
  userAgent: string | null;
  platformFamily: string | null;
  platformOs: string | null;
  codexHome: string | null;
}

export interface LoginStart {
  loginId: string;
  authUrl: string;
}

export interface ReasoningEffortOption {
  reasoningEffort: string;
  description: string;
}

export interface WorkspaceModel {
  id: string;
  model: string;
  displayName: string;
  description: string;
  isDefault: boolean;
  defaultReasoningEffort: string;
  supportedReasoningEfforts: ReasoningEffortOption[];
}

export interface WorkspaceModelCatalog {
  models: WorkspaceModel[];
}

export interface RateLimitWindow {
  usedPercent: number;
  remainingPercent: number;
  windowDurationMins: number | null;
  resetsAt: number | null;
}

export interface RateLimitBucket {
  limitId: string | null;
  limitName: string | null;
  planType: string | null;
  primary: RateLimitWindow | null;
  secondary: RateLimitWindow | null;
}

export interface WorkspaceRateLimits {
  source: "perBucket" | "legacy";
  buckets: RateLimitBucket[];
}

export interface SendTurnRequest {
  sessionId: string;
  text: string;
  clientSubmissionId: string;
  model?: string | null;
  effort?: string | null;
}

export interface SendTurnResult {
  threadId: string;
  turnId: string;
  draftCleared: boolean;
}

export interface WorkbenchEvent {
  kind: string;
  epoch: number;
  threadId?: string;
  turnId?: string;
  itemId?: string;
  itemKind?: string;
  delta?: string;
  requestId?: string | number;
  method?: string;
  params?: Record<string, unknown>;
  status?: string;
  success?: boolean;
  error?: string | null;
  [key: string]: unknown;
}

export interface ResolveServerRequest {
  requestId: string | number;
  method: string;
  result?: Record<string, unknown> | null;
  declineMessage?: string | null;
}

export interface CreateWorkspaceRequest {
  name: string;
  root?: string | null;
  operationId: string;
}

export interface RegisterWorkspaceRootRequest {
  workspaceId: string;
  root: string;
  operationId: string;
  expectedRevision: number;
}

export interface ClearWorkspaceRootRequest {
  workspaceId: string;
  operationId: string;
  expectedRevision: number;
}

export interface UpdateWorkspaceRequest {
  workspaceId: string;
  expectedRevision: number;
  operationId: string;
  name?: string | null;
  archived?: boolean | null;
}

export interface CreateSessionRequest {
  workspaceId?: string | null;
  title: string;
  operationId: string;
}

export interface UpdateSessionRequest {
  sessionId: string;
  expectedRevision: number;
  operationId: string;
  title?: string | null;
  draft?: string | null;
  overrides?: Record<string, unknown> | null;
  archived?: boolean | null;
  presetId?: string | null;
  paperId?: string | null;
  clearPaper?: boolean | null;
}

export interface MoveSessionRequest {
  sessionId: string;
  expectedRevision: number;
  operationId: string;
  /** `null` files the conversation as unfiled. */
  workspaceId: string | null;
}

export interface DeleteSessionRequest {
  sessionId: string;
  operationId: string;
}

/** Automatic conversation titles: an optional, separate, tool-free model call. */
export interface TitlePreferences {
  enabled: boolean;
  /** Exact model id, or `null` for the cheapest available model. */
  model: string | null;
  /** Reasoning effort, or `null` for the lowest the model advertises. */
  effort: string | null;
}

export interface HarnessModule {
  id: string;
  kind: "instruction_pack" | "context_provider" | "tool" | "inspector" | "recipe";
  version: number;
  name: string;
  description: string;
  requiresWorkspace: boolean;
  capability: string;
}

export interface HarnessPreset {
  id: string;
  workspaceId: string | null;
  name: string;
  description: string;
  instructions: string;
  modules: string[];
  builtIn: boolean;
  sourcePresetId: string | null;
  revision: number;
}

export interface HarnessCatalog {
  schemaVersion: number;
  toolCatalogVersion: number;
  modules: HarnessModule[];
  presets: HarnessPreset[];
}

export interface EffectiveHarness {
  schemaVersion: number;
  sessionId: string;
  workspaceId: string | null;
  preset: HarnessPreset;
  mode: "inspect" | "edit";
  webSearch: boolean;
  commandNetwork: boolean;
  permissionProfile: string;
  contextBudgetBytes: number;
  enabledModules: string[];
  unavailableModules: string[];
  diagnostics: string[];
  developerInstructions: string;
  contextPreview: string;
  contextTruncated: boolean;
  dynamicTools: Array<Record<string, unknown>>;
  fingerprint: string;
  valueSources: Record<string, unknown>;
  /** Derived per-catalog-module availability for this session; absent from older backends. */
  moduleAvailability?: ModuleAvailability[];
  /** Host-owned labelled bands of `developerInstructions`; absent from older backends. */
  instructionSections?: InstructionSection[];
}

export interface ModuleAvailability {
  id: string;
  available: boolean;
  reasons: string[];
}

export interface InstructionSection {
  id: string;
  label: string;
  text: string;
}

export interface WorkspaceConfig {
  scopeKey: string;
  workspaceId: string | null;
  schemaVersion: number;
  body: Record<string, unknown>;
  revision: number;
  updatedAt: string;
}

export interface ResearchNote {
  id: string;
  workspaceId: string;
  paperId: string | null;
  kind: "question" | "assumption" | "decision" | "next_step" | "notation" | "handoff";
  body: string;
  state: "proposed" | "accepted" | "rejected" | "retired";
  origin: string;
  pinned: boolean;
  revision: number;
  createdAt: string;
  updatedAt: string;
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

export interface PaperWithRevision { paper: ResearchPaper; revision: PaperRevision | null; }
export interface PaperReadResult { revisionId: string; contentHash: string; start: number; end: number; text: string; }
export interface PaperSearchHit { revisionId: string; contentHash: string; start: number; end: number; line: number; page: number | null; excerpt: string; }

export interface SourceRecord {
  id: string;
  workspaceId: string;
  title: string;
  citationKey: string | null;
  identifiers: Record<string, unknown>;
  versionId: string;
  versionLabel: string | null;
  locator: string | null;
  accessState: "metadata" | "abstract" | "partial" | "full" | "unavailable";
  acquiredVia: string;
  accessedAt: string | null;
  contentHash: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface SourceImportResult { source: SourceRecord; duplicateCandidates: SourceRecord[]; }

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
}

export interface EvidenceRecord {
  id: string;
  workspaceId: string;
  claimVersionId: string;
  targetType: string;
  targetId: string;
  locator: Record<string, unknown> | null;
  relation: "supports" | "contradicts" | "qualifies";
  assessment: "not_checked" | "model_assessed" | "human_confirmed" | "check_passed" | "check_failed";
  assessor: string;
  dependencyHash: string | null;
  freshness: "current" | "stale" | "unknown";
  staleReason: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ResearchLedger {
  claims: ClaimRecord[];
  evidence: EvidenceRecord[];
  unsupportedAcceptedClaims: string[];
  staleClaims: string[];
}

export interface ExecutionProfile {
  id: string;
  workspaceId: string;
  name: string;
  adapter: "latex" | "stata" | "command";
  argv: string[];
  cwd: string;
  environment: Record<string, string>;
  inputs: string[];
  timeoutSeconds: number;
  outputs: string[];
  testedAt: string | null;
  testStatus: "passed" | "failed" | null;
  revision: number;
}

export interface ResearchExecution {
  id: string;
  workspaceId: string;
  sessionId: string | null;
  profileId: string | null;
  adapter: string;
  command: string[];
  cwd: string;
  inputManifest: Record<string, unknown>;
  dependencyHash: string;
  outcome: "queued" | "running" | "completed" | "failed" | "interrupted" | "timed_out" | "outcome_unknown";
  startedAt: string | null;
  endedAt: string | null;
  exitStatus: number | null;
  stdout: string | null;
  stderr: string | null;
  outputManifest: Record<string, unknown>;
  validation: Record<string, unknown>;
  snapshotConsistency: "complete" | "partial" | "uncertain";
  createdAt: string;
}

export interface ResearchResultV1 {
  resultId: string; estimand: string; specificationId: string; sampleId: string; estimate: number;
  standardError: number | null; confidenceInterval: [number, number] | null; n: number | null; units: string;
  transformation: string | null; uncertaintyMethod: string | null; sourceExecutionId: string; artifactLocator: string;
}

export interface ResultComparison {
  comparable: boolean; passed: boolean; absoluteDifference: number | null; tolerance: number;
  incompatibilities: string[]; limitations: string;
}

export interface VerificationRecord {
  id: string; evidenceLinkId: string; method: string; checkerIdentity: string;
  inputHashes: Record<string, unknown>; observedResult: Record<string, unknown>;
  limitations: string; passed: boolean; createdAt: string;
}

export interface ResearchRecipe {
  id: string; workspaceId: string | null; sourceRecipeId: string | null; name: string;
  description: string; instructions: string; requiredInputs: string[]; requiredTools: string[];
  suggestedPermissionMode: "inspect" | "edit"; expectedChecks: string[]; version: number;
  revision: number; builtIn: boolean;
}

export interface RecipeInputCheck { input: string; available: boolean; detail: string; }
export interface RecipeRun {
  id: string; workspaceId: string; sessionId: string; recipe: ResearchRecipe;
  status: "active" | "completed" | "incomplete"; artifacts: Array<Record<string, unknown>>;
  checks: Array<Record<string, unknown>>; unresolvedIssues: string[]; missingEvidence: string[];
  startedAt: string; completedAt: string | null;
}

export interface PerformanceBudget { metric: string; unit: "ms" | "percent" | "mib"; budgetValue: number; rationale: string; }
export interface ResearchEvaluation {
  id: string; fixtureId: string; fixtureVersion: number;
  variant: "recipe" | "plain_workspace" | "ordinary_codex"; model: string | null;
  settings: Record<string, unknown>; outcome: Record<string, unknown>; latencyMs: number | null;
  inputTokens: number | null; outputTokens: number | null; createdAt: string;
}

export interface ReviewHandoff {
  version: number; id: string; workspaceId: string; sessionId: string | null; paperId: string;
  revisionId: string; contentHash: string; mediaKind: string; stagedPath: string;
  inputInterpretation: "document" | "source_tree"; metadata: Record<string, unknown>;
  externalReference: string | null; createdAt: string; linkedAt: string | null;
}

export interface ResearchArchiveReport {
  path: string; workspaceCount: number; sessionCount: number; blobCount: number; bytes: number;
  nativeBindingsRetired: number; portabilityNote: string;
}

export interface ExchangeSelection {
  includeNotes: boolean; recordKinds: string[]; paperIds: string[]; executionIds: string[];
  includeSources: boolean; includeLedger: boolean; includeCompiledPdfs: boolean;
}
export interface ExchangeObjectEntry { kind: string; id: string; fingerprint: string; title: string }
export interface ExchangeExternalReference { kind: string; id: string; reason: string }
export interface ExchangePreview {
  objects: ExchangeObjectEntry[]; blobCount: number; blobBytes: number; exclusions: string[];
  externalReferences: ExchangeExternalReference[]; limitations: string[];
}
export interface ExchangeReport {
  path: string; bytes: number; packageHash: string; objectCount: number; blobCount: number;
  exclusions: string[]; externalReferences: ExchangeExternalReference[]; limitations: string[];
}
export interface ExchangeInspection {
  path: string; packageHash: string; workspaceName: string; sourceNamespace: string; createdAt: string;
  workspaceRoot: string | null; counts: Record<string, number>; blobCount: number; blobBytes: number;
  exclusions: string[]; externalReferences: number; limitations: string[]; ownExport: boolean;
}
export type ExchangeTarget =
  | { kind: "newWorkspace"; name: string; root: string | null }
  | { kind: "existingWorkspace"; workspaceId: string };
export interface ExchangeDecision { kind: string; id: string; outcome: string; newId: string | null; title: string }
export interface ExchangeImportPreview {
  targetWorkspaceId: string | null; new: number; identical: number; remapped: number;
  conflicts: ExchangeDecision[]; decisions: ExchangeDecision[]; blobCount: number; limitations: string[];
}
export interface ExchangeImportReport {
  importId: string; workspaceId: string; new: number; identical: number; remapped: number;
  conflicts: number; blobs: number; limitations: string[];
}
export interface ExchangeConflict {
  id: string; importId: string; objectKind: string; objectId: string; local: unknown; imported: unknown;
  state: string; recordedAt: string;
}
export interface StorageCategory { key: string; label: string; bytes: number; entries: number; disposable: boolean; note: string }
export interface TrashEntry { id: string; category: string; originalPath: string; sizeBytes: number; reason: string; movedAt: string; state: string }
export interface StorageReport { root: string; categories: StorageCategory[]; trash: TrashEntry[]; activeJobs: number; note: string }
export interface PrunePlan { entries: Array<{ category: string; path: string; bytes: number }>; bytes: number; applied: boolean; trashIds: string[] }
export interface DraftWorkflow {
  name: string; canonicalJson: string; fingerprint: string;
  steps: Array<{ id: string; label: string; source: string }>; unsupported: string[]; notes: string[];
}

export interface ResearchArchiveInspection {
  path: string; formatVersion: number; storeSchemaVersion: number;
  workspaceRoots: string[]; portabilityNote: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function isSessionSnapshot(value: unknown): value is SessionSnapshot {
  if (!isRecord(value) || !isRecord(value.session)) {
    return false;
  }
  const { workspace, session } = value;
  const workspaceIsValid =
    workspace === null ||
    (isRecord(workspace) &&
      typeof workspace.id === "string" &&
      typeof workspace.name === "string" &&
      (typeof workspace.root === "string" || workspace.root === null) &&
      (typeof workspace.rootIdentity === "string" || workspace.rootIdentity === null) &&
      typeof workspace.revision === "number");
  return (
    typeof value.sequence === "number" &&
    workspaceIsValid &&
    typeof session.id === "string" &&
    (typeof session.workspaceId === "string" || session.workspaceId === null) &&
    (workspace === null
      ? session.workspaceId === null
      : isRecord(workspace) && session.workspaceId === workspace.id) &&
    typeof session.title === "string" &&
    isRecord(session.overrides) &&
    typeof session.draft === "string" &&
    typeof session.revision === "number"
  );
}
