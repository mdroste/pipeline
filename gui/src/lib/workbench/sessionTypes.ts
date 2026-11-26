/** App-owned Workspace records. */

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

/** The last conversation of a page, which is where the next page starts. */
export interface SessionCursor {
  updatedAt: string;
  id: string;
}

export interface SessionList {
  sessions: WorkbenchSession[];
  sequence: number;
  /** Set while older conversations remain. */
  nextCursor?: SessionCursor | null;
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

export interface TranscriptCursor {
  createdAt: string;
  id: string;
}
export interface TranscriptPage {
  items: TranscriptItem[];
  nextCursor: TranscriptCursor | null;
}

export interface ConversationSnapshot extends SessionSnapshot {
  olderCursor?: TranscriptCursor | null;
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

export interface CreateWorkspaceRequest {
  name: string;
  root: string;
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
