import { invoke } from "@tauri-apps/api/core";
import type {
  ClearWorkspaceRootRequest,
  CreateSessionRequest,
  CreateWorkspaceRequest,
  DeleteSessionRequest,
  MoveSessionRequest,
  Mutation,
  ReconcileResult,
  RegisterWorkspaceRootRequest,
  SessionList,
  SessionSnapshot,
  TitlePreferences,
  UpdateSessionRequest,
  UpdateWorkspaceRequest,
  WorkbenchSession,
  Workspace,
  WorkbenchCodexStatus,
  WorkspaceAccountState,
  WorkspaceList,
  WorkspaceModel,
  WorkspaceModelCatalog,
  WorkspaceRateLimits,
  LoginStart,
  ConversationSnapshot,
  ResolveServerRequest,
  SendTurnRequest,
  SendTurnResult,
  WorkbenchEvent,
  HarnessCatalog,
  BasePromptUpdate,
  NativePromptCatalog,
  EffectiveHarness,
  WorkspaceConfig,
  HarnessPreset,
  ResearchNote,
  PaperWithRevision,
  PaperReadResult,
  PaperSearchHit,
  SourceRecord,
  SourceImportResult,
  ResearchLedger,
  ClaimRecord,
  EvidenceRecord,
  ExecutionProfile,
  ResearchExecution,
  ResearchResultV1,
  ResultComparison,
  VerificationRecord,
  ResearchRecipe,
  RecipeInputCheck,
  RecipeRun,
  PerformanceBudget,
  ReviewHandoff,
  ResearchArchiveReport,
  ResearchArchiveInspection,
  ExchangeSelection,
  ExchangePreview,
  ExchangeReport,
  ExchangeInspection,
  ExchangeTarget,
  ExchangeImportPreview,
  ExchangeImportReport,
  ExchangeConflict,
  StorageReport,
  PrunePlan,
  TrashEntry,
  DraftWorkflow,
  ResearchEvaluation,
} from "./workbenchTypes";

export const workbenchClient = {
  createWorkspace(request: CreateWorkspaceRequest) {
    return invoke<Mutation<Workspace>>("workbench_create_workspace", { request });
  },
  registerWorkspaceRoot(request: RegisterWorkspaceRootRequest) {
    return invoke<Mutation<Workspace>>("workbench_register_workspace_root", { request });
  },
  clearWorkspaceRoot(request: ClearWorkspaceRootRequest) {
    return invoke<Mutation<Workspace>>("workbench_clear_workspace_root", { request });
  },
  getWorkspace(workspaceId: string) {
    return invoke<Workspace>("workbench_get_workspace", { workspaceId });
  },
  listWorkspaces(includeArchived = false) {
    return invoke<WorkspaceList>("workbench_list_workspaces", {
      includeArchived,
    });
  },
  updateWorkspace(request: UpdateWorkspaceRequest) {
    return invoke<Mutation<Workspace>>("workbench_update_workspace", { request });
  },
  createSession(request: CreateSessionRequest) {
    return invoke<Mutation<WorkbenchSession>>("workbench_create_session", { request });
  },
  updateSession(request: UpdateSessionRequest) {
    return invoke<Mutation<WorkbenchSession>>("workbench_update_session", { request });
  },
  moveSession(request: MoveSessionRequest) {
    return invoke<Mutation<WorkbenchSession>>("workbench_move_session", { request });
  },
  deleteSession(request: DeleteSessionRequest) {
    return invoke<number>("workbench_delete_session", { request });
  },
  titlePreferences() {
    return invoke<TitlePreferences>("workbench_get_title_preferences");
  },
  saveTitlePreferences(preferences: TitlePreferences) {
    return invoke<TitlePreferences>("workbench_save_title_preferences", { preferences });
  },
  generateSessionTitle(sessionId: string) {
    return invoke<string>("workbench_generate_session_title", { sessionId });
  },
  listSessions(workspaceId: string | null, includeArchived = false) {
    return invoke<SessionList>("workbench_list_sessions", { workspaceId, includeArchived });
  },
  sessionSnapshot(sessionId: string) {
    return invoke<SessionSnapshot>("workbench_session_snapshot", { sessionId });
  },
  conversationSnapshot(sessionId: string) {
    return invoke<ConversationSnapshot>("workbench_conversation_snapshot", { sessionId });
  },
  harnessCatalog(workspaceId: string | null) {
    return invoke<HarnessCatalog>("workbench_harness_catalog", { workspaceId });
  },
  effectiveHarness(sessionId: string) {
    return invoke<EffectiveHarness>("workbench_effective_harness", { sessionId });
  },
  getWorkspaceConfig(workspaceId: string | null) {
    return invoke<WorkspaceConfig>("workbench_get_workspace_config", { workspaceId });
  },
  saveWorkspaceConfig(request: { workspaceId: string | null; expectedRevision: number; body: Record<string, unknown>; operationId: string }) {
    return invoke<WorkspaceConfig>("workbench_save_workspace_config", { request });
  },
  clonePreset(request: { workspaceId: string | null; sourceWorkspaceId?: string | null; sourcePresetId: string; name: string; operationId: string }) {
    return invoke<HarnessPreset>("workbench_clone_preset", { request });
  },
  nativePromptCatalog() {
    return invoke<NativePromptCatalog>("workbench_native_prompt_catalog");
  },
  updatePreset(request: { basePrompt?: BasePromptUpdate; presetId: string; expectedRevision: number; name: string; description: string; instructions: string; modules: string[]; operationId: string }) {
    return invoke<HarnessPreset>("workbench_update_preset", { request });
  },
  createNote(request: { workspaceId: string; paperId: string | null; kind: ResearchNote["kind"]; body: string; state?: ResearchNote["state"]; origin: string; pinned: boolean; operationId: string }) {
    return invoke<ResearchNote>("workbench_create_note", { request });
  },
  updateNote(request: { noteId: string; expectedRevision: number; body?: string; state?: ResearchNote["state"]; pinned?: boolean; operationId: string }) {
    return invoke<ResearchNote>("workbench_update_note", { request });
  },
  listNotes(workspaceId: string, includeRejected = false) {
    return invoke<ResearchNote[]>("workbench_list_notes", { workspaceId, includeRejected });
  },
  importPaper(request: { workspaceId: string; paperId: string | null; title: string; role: "manuscript" | "appendix" | "other"; path: string; operationId: string }) {
    return invoke<PaperWithRevision>("workbench_import_paper", { request });
  },
  listPapers(workspaceId: string) {
    return invoke<PaperWithRevision[]>("workbench_list_papers", { workspaceId });
  },
  paperRead(request: { workspaceId: string; revisionId: string; start?: number; length?: number }) {
    return invoke<PaperReadResult>("workbench_paper_read", { request });
  },
  paperSearch(workspaceId: string, revisionId: string, query: string, limit = 20) {
    return invoke<PaperSearchHit[]>("workbench_paper_search", { workspaceId, revisionId, query, limit });
  },
  importSource(request: { workspaceId: string; title: string; citationKey: string | null; identifiers: Record<string, unknown>; versionLabel: string | null; path: string | null; locator: string | null; accessState: SourceRecord["accessState"]; acquiredVia: string; operationId: string }) {
    return invoke<SourceImportResult>("workbench_import_source", { request });
  },
  listSources(workspaceId: string) {
    return invoke<SourceRecord[]>("workbench_list_sources", { workspaceId });
  },
  proposeClaim(request: { workspaceId: string; paperId: string | null; claim: string; kind: string; origin: string; operationId: string }) {
    return invoke<ClaimRecord>("workbench_propose_claim", { request });
  },
  setClaimState(request: { claimId: string; state: ClaimRecord["workflowState"]; operationId: string }) {
    return invoke<ClaimRecord>("workbench_set_claim_state", { request });
  },
  proposeEvidence(request: { workspaceId: string; claimVersionId: string; targetType: string; targetId: string; locator: Record<string, unknown> | null; relation: EvidenceRecord["relation"]; assessor: string; operationId: string }) {
    return invoke<EvidenceRecord>("workbench_propose_evidence", { request });
  },
  confirmEvidence(request: { evidenceId: string; operationId: string }) {
    return invoke<EvidenceRecord>("workbench_confirm_evidence", { request });
  },
  researchLedger(workspaceId: string) {
    return invoke<ResearchLedger>("workbench_research_ledger", { workspaceId });
  },
  saveExecutionProfile(request: { profileId: string | null; workspaceId: string; name: string; adapter: ExecutionProfile["adapter"]; argv: string[]; cwd: string; environment: Record<string, string>; inputs: string[]; timeoutSeconds: number; outputs: string[]; expectedRevision: number | null; operationId: string }) {
    return invoke<ExecutionProfile>("workbench_save_execution_profile", { request });
  },
  listExecutionProfiles(workspaceId: string) {
    return invoke<ExecutionProfile[]>("workbench_list_execution_profiles", { workspaceId });
  },
  runExecution(request: { planId?: string; profileId: string; sessionId: string | null; testOnly: boolean; operationId: string }) {
    return invoke<ResearchExecution>("workbench_run_execution", { request });
  },
  listExecutions(workspaceId: string) {
    return invoke<ResearchExecution[]>("workbench_list_executions", { workspaceId });
  },
  cancelExecution(executionId: string) {
    return invoke<void>("workbench_cancel_execution", { request: { executionId } });
  },
  compareResults(left: ResearchResultV1, right: ResearchResultV1, absoluteTolerance: number, rationale: string | null) {
    return invoke<ResultComparison>("workbench_compare_results", { request: { left, right, absoluteTolerance, rationale } });
  },
  recordStructuredResult(workspaceId: string, result: ResearchResultV1, operationId: string) {
    return invoke<ResearchResultV1>("workbench_record_structured_result", { request: { workspaceId, result, operationId } });
  },
  listStructuredResults(workspaceId: string) {
    return invoke<ResearchResultV1[]>("workbench_list_structured_results", { workspaceId });
  },
  verifyEvidenceResults(request: { evidenceId: string; leftResultId: string; rightResultId: string; absoluteTolerance: number; rationale: string | null; operationId: string }) {
    return invoke<VerificationRecord>("workbench_verify_evidence_results", { request });
  },
  listRecipes(workspaceId: string | null) {
    return invoke<ResearchRecipe[]>("workbench_list_recipes", { workspaceId });
  },
  cloneRecipe(request: { workspaceId: string; sourceRecipeId: string; name: string }) {
    return invoke<ResearchRecipe>("workbench_clone_recipe", { request });
  },
  updateRecipe(request: { recipeId: string; expectedRevision: number; name: string; description: string; instructions: string; requiredInputs: string[]; requiredTools: string[]; suggestedPermissionMode: "inspect" | "edit"; expectedChecks: string[] }) {
    return invoke<ResearchRecipe>("workbench_update_recipe", { request });
  },
  checkRecipeInputs(sessionId: string, recipeId: string) {
    return invoke<RecipeInputCheck[]>("workbench_check_recipe_inputs", { sessionId, recipeId });
  },
  startRecipe(request: { sessionId: string; recipeId: string; operationId: string }) {
    return invoke<RecipeRun>("workbench_start_recipe", { request });
  },
  completeRecipe(request: { recipeRunId: string; artifacts: Array<Record<string, unknown>>; checks: Array<Record<string, unknown>>; unresolvedIssues: string[]; missingEvidence: string[] }) {
    return invoke<RecipeRun>("workbench_complete_recipe", { request });
  },
  listRecipeRuns(sessionId: string) {
    return invoke<RecipeRun[]>("workbench_list_recipe_runs", { sessionId });
  },
  performanceBudgets() {
    return invoke<PerformanceBudget[]>("workbench_performance_budgets");
  },
  recordPerformance(metric: string, observedValue: number, details: Record<string, unknown>) {
    return invoke<Record<string, unknown>>("workbench_record_performance", { request: { metric, observedValue, details } });
  },
  recordResearchEvaluation(request: { fixtureId: string; fixtureVersion: number; variant: ResearchEvaluation["variant"]; model: string | null; settings: Record<string, unknown>; outcome: Record<string, unknown>; latencyMs: number | null; inputTokens: number | null; outputTokens: number | null }) {
    return invoke<ResearchEvaluation>("workbench_record_research_evaluation", { request });
  },
  listResearchEvaluations(fixtureId: string | null = null) {
    return invoke<ResearchEvaluation[]>("workbench_list_research_evaluations", { fixtureId });
  },
  prepareReviewHandoff(request: { workspaceId: string; sessionId: string | null; paperId: string; metadata: Record<string, unknown>; operationId: string }) {
    return invoke<ReviewHandoff>("workbench_prepare_review_handoff", { request });
  },
  linkReviewHandoff(handoffId: string, externalReference: string) {
    return invoke<ReviewHandoff>("workbench_link_review_handoff", { request: { handoffId, externalReference } });
  },
  exportResearchArchive(path: string) {
    return invoke<ResearchArchiveReport>("workbench_export_research_archive", { request: { path } });
  },
  inspectResearchArchive(path: string) {
    return invoke<ResearchArchiveInspection>("workbench_inspect_research_archive", { request: { path } });
  },
  importResearchArchive(path: string, rootMappings: Record<string, string | null> = {}) {
    return invoke<ResearchArchiveReport>("workbench_import_research_archive", { request: { path, rootMappings } });
  },
  previewProjectExchange(workspaceId: string, selection: ExchangeSelection) {
    return invoke<ExchangePreview>("workbench_preview_project_exchange", { workspaceId, selection });
  },
  exportProjectExchange(workspaceId: string, path: string, selection: ExchangeSelection) {
    return invoke<ExchangeReport>("workbench_export_project_exchange", { request: { workspaceId, path, selection } });
  },
  inspectProjectExchange(path: string) {
    return invoke<ExchangeInspection>("workbench_inspect_project_exchange", { path });
  },
  previewProjectExchangeImport(path: string, target: ExchangeTarget) {
    return invoke<ExchangeImportPreview>("workbench_preview_project_exchange_import", { path, target });
  },
  importProjectExchange(path: string, target: ExchangeTarget, operationId: string) {
    return invoke<ExchangeImportReport>("workbench_import_project_exchange", { request: { path, target, operationId } });
  },
  listExchangeConflicts(workspaceId: string) {
    return invoke<ExchangeConflict[]>("workbench_list_exchange_conflicts", { workspaceId });
  },
  resolveExchangeConflict(workspaceId: string, conflictId: string, takeImported: boolean) {
    return invoke<ExchangeConflict>("workbench_resolve_exchange_conflict", { workspaceId, conflictId, takeImported });
  },
  storageReport() {
    return invoke<StorageReport>("workbench_storage_report");
  },
  pruneStorage(categories: string[], apply: boolean, expectedPreviewToken: string | null = null) {
    return invoke<PrunePlan>("workbench_prune_storage", { request: { categories, apply, expectedPreviewToken } });
  },
  restoreTrash(trashId: string) {
    return invoke<TrashEntry>("workbench_restore_trash", { trashId });
  },
  emptyTrash() {
    return invoke<number>("workbench_empty_trash");
  },
  draftWorkflow(request: { workspaceId: string; name: string; recipeRunIds: string[]; taskIds: string[]; theoryIds: string[] }) {
    return invoke<DraftWorkflow>("workbench_draft_workflow", { request });
  },
  exportConversation(sessionId: string, path: string) {
    return invoke<void>("workbench_export_conversation", { sessionId, path });
  },
  reconcileWorkspaceRoots(operationId: string) {
    return invoke<ReconcileResult>("workbench_reconcile_workspace_roots", { operationId });
  },
  connectCodex() {
    return invoke<WorkbenchCodexStatus>("workbench_codex_connect");
  },
  accountState(refreshToken = false) {
    return invoke<WorkspaceAccountState>("workbench_codex_account_state", { refreshToken });
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
  validateModelSelection(model: string, effort: string | null) {
    return invoke<WorkspaceModel>("workbench_codex_validate_model_selection", {
      model,
      effort,
    });
  },
  rateLimits() {
    return invoke<WorkspaceRateLimits>("workbench_codex_rate_limits");
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
