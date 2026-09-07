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
      throw new Error("Each root mapping must use a nonempty path and a string or null value.");
    }
  }
  return parsed as Record<string, string | null>;
}

interface WorkspaceReleasePanelProps {
  snapshot: ConversationSnapshot;
  selectedPaper: PaperWithRevision | null;
  busy: boolean;
  onAction: (action: () => Promise<void>) => void;
  onError: (message: string) => void;
  onReviewHandoff?: (handoff: ReviewHandoff) => void;
}

export default function WorkspaceReleasePanel({
  snapshot,
  selectedPaper,
  busy,
  onAction,
  onError,
  onReviewHandoff,
}: WorkspaceReleasePanelProps) {
  const workspaceId = snapshot.session.workspaceId;
  const [budgets, setBudgets] = useState<PerformanceBudget[]>([]);

  useEffect(() => {
    void workbenchClient.performanceBudgets()
      .then(setBudgets)
      .catch((cause) => onError(workbenchErrorMessage(cause)));
  }, [onError]);

  const exportArchive = async () => {
    const path = await save({
      defaultPath: "workspace-research.pwrx",
      filters: [{ name: "Workspace research archive", extensions: ["pwrx"] }],
    });
    if (!path) return;
    const report = await workbenchClient.exportResearchArchive(path);
    window.alert(
      `Exported ${report.workspaceCount} workspace(s), ${report.sessionCount} conversation(s), and ${report.blobCount} immutable blob(s).\n\n${report.portabilityNote}`,
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
    if (!window.confirm(
      `Restore is allowed only into an empty Workspace research store. Imported tools will not run, native thread bindings will be retired, and execution profiles must be retested.\n\nArchived roots:\n${roots}\n\n${inspection.portabilityNote}\n\nContinue?`,
    )) return;
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
      sessionId: snapshot.session.id,
      paperId: selectedPaper.paper.id,
      metadata: { paperTitle: selectedPaper.paper.title },
      operationId: operation("review-handoff"),
    });
    onReviewHandoff(handoff);
  };

  return <div className="space-y-4">
    <section>
      <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">Portable research archive</h3>
      <p className="mt-1 text-xs text-gray-500">
        Exports use a consistent SQLite snapshot, immutable blobs, and readable Markdown plus structured JSON transcripts. Credentials and private runtime state are excluded.
      </p>
      <div className="mt-3 flex flex-wrap gap-2">
        <button type="button" disabled={busy} onClick={() => onAction(exportArchive)} className="rounded border px-3 py-2 text-xs">Export all research data</button>
        <button type="button" disabled={busy} onClick={() => onAction(restoreArchive)} className="rounded border px-3 py-2 text-xs">Restore archive</button>
      </div>
    </section>

    {workspaceId && <Suspense fallback={<p className="text-xs text-gray-500">Loading project exchange…</p>}>
      <ExchangePanel workspaceId={workspaceId} sessionId={snapshot.session.id} busy={busy} onAction={onAction} onError={onError}/>
    </Suspense>}

    <section>
      <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">Review bridge</h3>
      <p className="mt-1 text-xs text-gray-500">
        Optional and explicit. Workspace stages the selected immutable revision; Review keeps its current workflow, models, providers, authentication, scheduling, and cancellation.
      </p>
      <button
        type="button"
        disabled={busy || !workspaceId || !selectedPaper?.revision || !onReviewHandoff}
        onClick={() => onAction(handoffToReview)}
        className="mt-3 rounded bg-gray-900 px-3 py-2 text-xs text-white disabled:opacity-40 dark:bg-neutral-100 dark:text-neutral-900"
      >
        Review this revision…
      </button>
      <p className="mt-2 text-[10px] text-gray-500">
        Only a staged artifact and selected metadata cross the boundary. Workspace credentials, model choice, tool catalog, permissions, and notes do not.
      </p>
    </section>

    <details>
      <summary className="cursor-pointer text-xs font-semibold">Recorded performance budgets</summary>
      <div className="mt-2 space-y-2">
        {budgets.map((budget) => <div key={budget.metric} className="rounded border bg-white p-2 text-[11px] dark:bg-neutral-950">
          <span className="font-medium">{budget.metric.replaceAll("_", " ")}: {budget.budgetValue} {budget.unit}</span>
          <span className="block text-gray-500">{budget.rationale}</span>
        </div>)}
      </div>
    </details>

    <p className="rounded border border-blue-200 bg-blue-50 p-2 text-xs text-blue-800 dark:border-blue-900 dark:bg-blue-950/30 dark:text-blue-200">
      Portable records do not guarantee resumable native Codex threads on another machine or account. Restored bindings start a new native session from reviewed Workspace context.
    </p>
  </div>;
}
