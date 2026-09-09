import { lazy, Suspense, useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type {
  ConversationSnapshot,
  PaperWithRevision,
  PerformanceBudget,
  ReviewHandoff,
} from "../lib/workbenchTypes";

const ExchangePanel = lazy(() => import("./WorkspaceExchangePanel"));

function operation(prefix: string) {
  return `${prefix}-${crypto.randomUUID()}`;
}

function parseRootMappings(value: string): Record<string, string | null> {
  const parsed: unknown = JSON.parse(value);
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("Workspace root mappings must be a JSON object.");
  }
  for (const [root, replacement] of Object.entries(parsed)) {
    if (!root || (replacement !== null && typeof replacement !== "string")) {
      throw new Error(
        "Each root mapping must use a nonempty path and a string or null value.",
      );
    }
  }
  return parsed as Record<string, string | null>;
}

interface WorkspaceReleasePanelProps {
  snapshot?: ConversationSnapshot | null;
  workspaceId?: string | null;
  view?: "all" | "share" | "review" | "backup" | "diagnostics";
  selectedPaper: PaperWithRevision | null;
  busy: boolean;
  onAction: (action: () => Promise<void>) => void;
  onError: (message: string) => void;
  onReviewHandoff?: (handoff: ReviewHandoff) => void;
}

export default function WorkspaceReleasePanel({
  snapshot,
  workspaceId: projectId,
  view = "all",
  selectedPaper,
  busy,
  onAction,
  onError,
  onReviewHandoff,
}: WorkspaceReleasePanelProps) {
  const workspaceId = projectId ?? snapshot?.session.workspaceId ?? null;
  const [budgets, setBudgets] = useState<PerformanceBudget[]>([]);

  useEffect(() => {
    if (view !== "all" && view !== "diagnostics") return;
    void workbenchClient
      .performanceBudgets()
      .then(setBudgets)
      .catch((cause) => onError(workbenchErrorMessage(cause)));
  }, [onError, view]);

  const exportArchive = async () => {
    const path = await save({
      defaultPath: "workspace-research.pwrx",
      filters: [{ name: "Workspace research archive", extensions: ["pwrx"] }],
    });
    if (!path) return;
    const report = await workbenchClient.exportResearchArchive(path);
    window.alert(
      `Exported ${report.workspaceCount} workspace(s), ${report.sessionCount} conversation(s), and ${report.blobCount} file(s).\n\n${report.portabilityNote}`,
    );
  };

  const restoreArchive = async () => {
    const path = await open({
      multiple: false,
      filters: [{ name: "Workspace research archive", extensions: ["pwrx"] }],
    });
    if (typeof path !== "string") return;
    const inspection = await workbenchClient.inspectResearchArchive(path);
    const roots = inspection.workspaceRoots.length
      ? inspection.workspaceRoots.map((root) => `• ${root}`).join("\n")
      : "• No registered Workspace roots";
    if (
      !window.confirm(
        `You can restore only when Workspace has no saved research data. Restoring does not run any tools. Conversations restart from saved context, and command profiles must be tested again.\n\nProject folders in this backup:\n${roots}\n\n${inspection.portabilityNote}\n\nContinue?`,
      )
    )
      return;
    const defaultMappings = Object.fromEntries(
      inspection.workspaceRoots.map((root) => [root, null]),
    );
    const mappingText = window.prompt(
      `Choose a new absolute path for every archived Workspace root, or leave its value null to detach it.\n\nArchived roots:\n${roots}`,
      JSON.stringify(defaultMappings, null, 2),
    );
    if (mappingText === null) return;
    const report = await workbenchClient.importResearchArchive(
      path,
      parseRootMappings(mappingText),
    );
    window.alert(
      `Restored ${report.workspaceCount} workspace(s) and ${report.sessionCount} conversation(s).\n\n${report.portabilityNote}`,
    );
    window.location.reload();
  };

  const handoffToReview = async () => {
    if (!workspaceId || !selectedPaper?.revision || !onReviewHandoff) return;
    const handoff = await workbenchClient.prepareReviewHandoff({
      workspaceId,
      sessionId: snapshot?.session.id ?? null,
      paperId: selectedPaper.paper.id,
      metadata: { paperTitle: selectedPaper.paper.title },
      operationId: operation("review-handoff"),
    });
    onReviewHandoff(handoff);
  };

  return (
    <div className="space-y-4">
      {(view === "all" || view === "backup") && (
        <section>
          <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
            Research backup
          </h3>
          <p className="mt-1 text-xs text-gray-500">
            Back up projects, research files, and conversations, including
            transcripts in Markdown and JSON. Sign-in details are excluded.
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            <button
              type="button"
              disabled={busy}
              onClick={() => onAction(exportArchive)}
              className="rounded border px-3 py-2 text-xs"
            >
              Export all research data
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => onAction(restoreArchive)}
              className="rounded border px-3 py-2 text-xs"
            >
              Restore archive
            </button>
          </div>
        </section>
      )}

      {(view === "all" || view === "share") && workspaceId && (
        <Suspense
          fallback={
            <p className="text-xs text-gray-500">Loading project exchange…</p>
          }
        >
          <ExchangePanel
            showStorage={view === "all"}
            workspaceId={workspaceId}
            sessionId={snapshot?.session.id ?? null}
            busy={busy}
            onAction={onAction}
            onError={onError}
          />
        </Suspense>
      )}

      {(view === "all" || view === "review") && (
        <section>
          <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
            Paper review
          </h3>
          <p className="mt-1 text-xs text-gray-500">
            Review a saved copy of this paper. Choose the workflow and models in
            the preview before starting.
          </p>
          <button
            type="button"
            disabled={
              busy ||
              !workspaceId ||
              !selectedPaper?.revision ||
              !onReviewHandoff
            }
            onClick={() => onAction(handoffToReview)}
            className="mt-3 rounded bg-gray-900 px-3 py-2 text-xs text-white disabled:opacity-40 dark:bg-neutral-100 dark:text-neutral-900"
          >
            Review this revision…
          </button>
          <p className="mt-2 text-[10px] text-gray-500">
            The review receives the paper and its selected metadata. It uses
            your Reviews settings and sign-in, and does not include project
            notes.
          </p>
        </section>
      )}

      {(view === "all" || view === "diagnostics") && (
        <details>
          <summary className="cursor-pointer text-xs font-semibold">
            Performance targets
          </summary>
          <div className="mt-2 space-y-2">
            {budgets.map((budget) => (
              <div
                key={budget.metric}
                className="rounded border bg-white p-2 text-[11px] dark:bg-neutral-950"
              >
                <span className="font-medium">
                  {budget.metric.replaceAll("_", " ")}: {budget.budgetValue}{" "}
                  {budget.unit}
                </span>
                <span className="block text-gray-500">{budget.rationale}</span>
              </div>
            ))}
          </div>
        </details>
      )}

      {(view === "all" || view === "backup") && (
        <p className="rounded border border-blue-200 bg-blue-50 p-2 text-xs text-blue-800 dark:border-blue-900 dark:bg-blue-950/30 dark:text-blue-200">
          After restoring, sign in to ChatGPT to continue. New replies use your
          reviewed notes and selected evidence; earlier messages remain in the
          saved transcript.
        </p>
      )}
    </div>
  );
}
