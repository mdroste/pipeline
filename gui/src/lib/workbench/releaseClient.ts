import { invoke } from "@tauri-apps/api/core";
import type {
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
} from "../workbenchTypes";

export const releaseClient = {
  performanceBudgets() {
    return invoke<PerformanceBudget[]>("workbench_performance_budgets");
  },
  recordPerformance(
    metric: string,
    observedValue: number,
    details: Record<string, unknown>,
  ) {
    return invoke<Record<string, unknown>>("workbench_record_performance", {
      request: { metric, observedValue, details },
    });
  },
  recordResearchEvaluation(request: {
    fixtureId: string;
    fixtureVersion: number;
    variant: ResearchEvaluation["variant"];
    model: string | null;
    settings: Record<string, unknown>;
    outcome: Record<string, unknown>;
    latencyMs: number | null;
    inputTokens: number | null;
    outputTokens: number | null;
  }) {
    return invoke<ResearchEvaluation>("workbench_record_research_evaluation", {
      request,
    });
  },
  listResearchEvaluations(fixtureId: string | null = null) {
    return invoke<ResearchEvaluation[]>("workbench_list_research_evaluations", {
      fixtureId,
    });
  },
  prepareReviewHandoff(request: {
    workspaceId: string;
    sessionId: string | null;
    paperId: string;
    metadata: Record<string, unknown>;
    operationId: string;
  }) {
    return invoke<ReviewHandoff>("workbench_prepare_review_handoff", {
      request,
    });
  },
  linkReviewHandoff(handoffId: string, externalReference: string) {
    return invoke<ReviewHandoff>("workbench_link_review_handoff", {
      request: { handoffId, externalReference },
    });
  },
  exportResearchArchive(path: string) {
    return invoke<ResearchArchiveReport>("workbench_export_research_archive", {
      request: { path },
    });
  },
  inspectResearchArchive(path: string) {
    return invoke<ResearchArchiveInspection>(
      "workbench_inspect_research_archive",
      { request: { path } },
    );
  },
  importResearchArchive(
    path: string,
    rootMappings: Record<string, string | null> = {},
  ) {
    return invoke<ResearchArchiveReport>("workbench_import_research_archive", {
      request: { path, rootMappings },
    });
  },
  previewProjectExchange(workspaceId: string, selection: ExchangeSelection) {
    return invoke<ExchangePreview>("workbench_preview_project_exchange", {
      workspaceId,
      selection,
    });
  },
  exportProjectExchange(
    workspaceId: string,
    path: string,
    selection: ExchangeSelection,
  ) {
    return invoke<ExchangeReport>("workbench_export_project_exchange", {
      request: { workspaceId, path, selection },
    });
  },
  inspectProjectExchange(path: string) {
    return invoke<ExchangeInspection>("workbench_inspect_project_exchange", {
      path,
    });
  },
  previewProjectExchangeImport(path: string, target: ExchangeTarget) {
    return invoke<ExchangeImportPreview>(
      "workbench_preview_project_exchange_import",
      { path, target },
    );
  },
  importProjectExchange(
    path: string,
    target: ExchangeTarget,
    operationId: string,
  ) {
    return invoke<ExchangeImportReport>("workbench_import_project_exchange", {
      request: { path, target, operationId },
    });
  },
  listExchangeConflicts(workspaceId: string) {
    return invoke<ExchangeConflict[]>("workbench_list_exchange_conflicts", {
      workspaceId,
    });
  },
  resolveExchangeConflict(
    workspaceId: string,
    conflictId: string,
    takeImported: boolean,
  ) {
    return invoke<ExchangeConflict>("workbench_resolve_exchange_conflict", {
      workspaceId,
      conflictId,
      takeImported,
    });
  },
  storageReport() {
    return invoke<StorageReport>("workbench_storage_report");
  },
  pruneStorage(
    categories: string[],
    apply: boolean,
    expectedPreviewToken: string | null = null,
  ) {
    return invoke<PrunePlan>("workbench_prune_storage", {
      request: { categories, apply, expectedPreviewToken },
    });
  },
  restoreTrash(trashId: string) {
    return invoke<TrashEntry>("workbench_restore_trash", { trashId });
  },
  emptyTrash() {
    return invoke<number>("workbench_empty_trash");
  },
  draftWorkflow(request: {
    workspaceId: string;
    name: string;
    recipeRunIds: string[];
    taskIds: string[];
    theoryIds: string[];
  }) {
    return invoke<DraftWorkflow>("workbench_draft_workflow", { request });
  },
};
