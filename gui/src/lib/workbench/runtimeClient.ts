import { invoke } from "@tauri-apps/api/core";
import type {
  ClaudeRateLimit,
  WorkbenchCodexStatus,
  WorkspaceAccountState,
  WorkspaceModel,
  WorkspaceModelCatalog,
  WorkspaceRateLimits,
  LoginStart,
  ResolveServerRequest,
  SendTurnRequest,
  SendTurnResult,
  WorkbenchEvent,
} from "../workbenchTypes";

export const runtimeClient = {
  connectCodex() {
    return invoke<WorkbenchCodexStatus>("workbench_codex_connect");
  },
  accountState(refreshToken = false) {
    return invoke<WorkspaceAccountState>("workbench_codex_account_state", {
      refreshToken,
    });
  },
  loginStart() {
    return invoke<LoginStart>("workbench_codex_login_start");
  },
  loginCancel(loginId: string) {
    return invoke<boolean>("workbench_codex_login_cancel", { loginId });
  },
  logout() {
    return invoke<void>("workbench_codex_logout");
  },
  modelCatalog() {
    return invoke<WorkspaceModelCatalog>("workbench_codex_model_catalog");
  },
  apiModelCatalog(runtime: string) {
    return invoke<WorkspaceModelCatalog>("workbench_api_model_catalog", {
      runtime,
    });
  },
  claudeModelCatalog() {
    return invoke<WorkspaceModelCatalog>("workbench_claude_model_catalog");
  },
  validateModelSelection(model: string, effort: string | null) {
    return invoke<WorkspaceModel>("workbench_codex_validate_model_selection", {
      model,
      effort,
    });
  },
  rateLimits() {
    return invoke<WorkspaceRateLimits>("workbench_codex_rate_limits");
  },
  claudeRateLimits() {
    return invoke<ClaudeRateLimit | null>("workbench_claude_rate_limits");
  },
  sendTurn(request: SendTurnRequest) {
    return invoke<SendTurnResult>("workbench_codex_send_turn", { request });
  },
  interruptTurn(threadId: string, turnId: string) {
    return invoke<void>("workbench_codex_interrupt_turn", { threadId, turnId });
  },
  reconcileSession(sessionId: string) {
    return invoke<boolean>("workbench_codex_reconcile_session", { sessionId });
  },
  resolveServerRequest(request: ResolveServerRequest) {
    return invoke<void>("workbench_codex_resolve_server_request", { request });
  },
  pendingRequests() {
    return invoke<WorkbenchEvent[]>("workbench_codex_pending_requests");
  },
};
