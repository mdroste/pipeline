/** App-owned Workspace records. */

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
  observedAt?: number;
  cached?: boolean;
  source: "perBucket" | "legacy";
  buckets: RateLimitBucket[];
}

/** Account-scoped observations reported by Claude Code; never a live balance. */
export interface ClaudeRateLimit {
  windows?: ClaudeRateLimit[];
  status: "allowed" | "allowed_warning" | "rejected";
  /** Unix seconds. */
  resetsAt: number | null;
  rateLimitType: string | null;
  usedPercent: number | null;
  isUsingOverage: boolean;
  /** Unix milliseconds when Pipeline received the report. */
  observedAt: number;
}

export interface SendTurnRequest {
  sessionId: string;
  text: string;
  clientSubmissionId: string;
  model?: string | null;
  effort?: string | null;
}

export interface SendTurnResult {
  epoch: number;
  threadId: string;
  turnId: string;
  draftCleared: boolean;
}

/** The app's one running turn, as the host reports it. */
export interface ActiveTurn {
  sessionId: string;
  title: string | null;
  threadId: string | null;
  turnId: string | null;
  epoch: number | null;
  /** Still being prepared. With no turnId, the runtime has not acknowledged it. */
  setup: boolean;
  runtime: string;
  /** Belongs to an Automation, which owns stopping it. */
  automated: boolean;
  /** Sent from the composer, so its text is the session draft until acknowledged. */
  composer: boolean;
}

/** What an approval would allow. Pipeline's own record of the request. */
export interface ApprovalDetails {
  type: "command" | "fileChange";
  reason: string | null;
  command: string | null;
  cwd: string | null;
  /** For example "example.org over HTTPS". */
  network: string | null;
  /** Permissions Allow once would add, as readable JSON. */
  extraPermissions: string | null;
  suggestedRule: string[];
  suggestedNetworkRules: string[];
  grantRoot: string | null;
  /** Allow once would grant something Pipeline can't describe. */
  unknownGrant: boolean;
  allowOffered: boolean;
  /** The decision that answers Decline. */
  declineDecision: string;
}

export interface PermissionDetails {
  type: "permission";
  reason: string | null;
  cwd: string | null;
  /** What is granted, returned unchanged when the researcher allows it. */
  permissions: Record<string, unknown> | null;
}

export interface RequestQuestion {
  id: string;
  header: string | null;
  question: string;
  options: Array<{ label: string; description: string | null }>;
  allowsOther: boolean;
  secret: boolean;
}

export interface QuestionDetails {
  type: "question";
  questions: RequestQuestion[];
}

export type RequestDetails =
  ApprovalDetails | PermissionDetails | QuestionDetails;

export type ChatApprovalMode = "ask" | "auto" | "sandbox";

export interface WorkbenchEvent {
  canRemember?: boolean;
  kind: string;
  epoch: number;
  /** The conversation behind a server request, or a failed `activeTurnChanged`. */
  sessionId?: string | null;
  sendError?: { code: string; message: string } | null;
  threadId?: string;
  turnId?: string;
  itemId?: string;
  itemKind?: string;
  delta?: string;
  requestId?: string | number;
  method?: string;
  params?: Record<string, unknown>;
  /** What a server request's card shows. */
  details?: RequestDetails | null;
  status?: string;
  success?: boolean;
  error?: string | null;
  [key: string]: unknown;
}

export interface ResolveServerRequest {
  epoch: number;
  requestId: string | number;
  method: string;
  result?: Record<string, unknown> | null;
  declineMessage?: string | null;
}
