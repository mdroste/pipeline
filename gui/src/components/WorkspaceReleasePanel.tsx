import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { releaseClient } from "../lib/workbench/releaseClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import { confirmDialog, notify } from "./DialogService";
import WorkspaceRestoreRootsDialog, {
  type RootMappings,
} from "./WorkspaceRestoreRootsDialog";
import type {
  ConversationSnapshot,
  PaperWithRevision,
  PerformanceBudget,
  ReviewHandoff,
} from "../lib/workbenchTypes";

const ExchangePanel = lazy(() => import("./WorkspaceExchangePanel"));

const plural = (count: number, noun: string) =>
  `${count} ${noun}${count === 1 ? "" : "s"}`;

function operation(prefix: string) {
  return `${prefix}-${crypto.randomUUID()}`;
}

/** Long enough to recognize the damage, short enough to fit in a dialog. */
const LISTED_FILES = 12;
function fileList(names: string[]) {
  const listed = names.slice(0, LISTED_FILES).map((name) => `• ${name}`);
  if (names.length > LISTED_FILES) {
    listed.push(`• and ${names.length - LISTED_FILES} more`);
  }
  return listed.join("\n");
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
  const [restoreRoots, setRestoreRoots] = useState<string[] | null>(null);
  const rootMappingAnswer = useRef<
    ((value: RootMappings | null) => void) | null
  >(null);

  useEffect(() => () => rootMappingAnswer.current?.(null), []);

  const chooseRootMappings = (roots: string[]) =>
    new Promise<RootMappings | null>((resolve) => {
      rootMappingAnswer.current = resolve;
      setRestoreRoots(roots);
    });

  const finishRootMappings = (mappings: RootMappings | null) => {
    const answer = rootMappingAnswer.current;
    rootMappingAnswer.current = null;
    setRestoreRoots(null);
    answer?.(mappings);
  };

  useEffect(() => {
    if (view !== "all" && view !== "diagnostics") return;
    void releaseClient
      .performanceBudgets()
      .then(setBudgets)
      .catch((cause) => onError(workbenchErrorMessage(cause)));
  }, [onError, view]);

  const exportArchive = async () => {
    const missing = await releaseClient.missingBackupFiles();
    if (
      missing.length &&
      !(await confirmDialog(
        `The files behind ${plural(missing.length, "saved item")} are missing from your research data folder, so a backup can't include them:\n${fileList(missing.map((file) => file.description))}\n\nEverything else will be saved. After restoring, these items stay in your projects and show as unavailable. If you still have the files, cancel and copy them back first.`,
        {
          title: "Back up without these files?",
          confirmLabel: "Back up without them",
        },
      ))
    )
      return;
    const path = await save({
      defaultPath: "workspace-research.pwrx",
      filters: [{ name: "Research backup", extensions: ["pwrx"] }],
    });
    if (!path) return;
    const report = await releaseClient.exportResearchArchive(
      path,
      missing.map((file) => file.reference),
    );
    const summary = `Exported ${plural(report.workspaceCount, "project")}, ${plural(report.sessionCount, "conversation")}, and ${plural(report.blobCount, "file")}.`;
    if (report.omittedFiles.length) {
      await confirmDialog(
        `${summary}\n\nLeft out because their files were missing:\n${fileList(report.omittedFiles)}\n\n${report.portabilityNote}`,
        {
          title: "Backup saved without some files",
          confirmLabel: "OK",
          acknowledgeOnly: true,
        },
      );
      return;
    }
    notify(`${summary}\n\n${report.portabilityNote}`, "success");
  };

  const restoreArchive = async () => {
    const path = await open({
      multiple: false,
      filters: [{ name: "Research backup", extensions: ["pwrx"] }],
    });
    if (typeof path !== "string") return;
    const inspection = await releaseClient.inspectResearchArchive(path);
    const roots = inspection.workspaceRoots.length
      ? inspection.workspaceRoots.map((root) => `• ${root}`).join("\n")
      : "• None";
    const leftOut = inspection.omittedFileCount
      ? `This backup was made without ${plural(inspection.omittedFileCount, "file")} that had gone missing. The items that used them come back as unavailable.\n\n`
      : "";
    if (
      !(await confirmDialog(
        `You can restore only if Pipeline has no saved projects or conversations. Conversations restart from saved context. Test your command profiles again after restoring.\n\nProject folders in this backup:\n${roots}\n\n${leftOut}${inspection.portabilityNote}`,
        { title: "Restore research backup", confirmLabel: "Continue" },
      ))
    )
      return;
    const mappings = inspection.workspaceRoots.length
      ? await chooseRootMappings(inspection.workspaceRoots)
      : {};
    if (!mappings) return;
    const report = await releaseClient.importResearchArchive(path, mappings);
    const unavailable = report.omittedFiles.length
      ? `Unavailable because their files were missing from this backup:\n${fileList(report.omittedFiles)}\n\n`
      : "";
    await confirmDialog(
      `Restored ${plural(report.workspaceCount, "project")} and ${plural(report.sessionCount, "conversation")}.\n\n${unavailable}${report.portabilityNote}`,
      {
        title: "Research backup restored",
        confirmLabel: "OK",
        acknowledgeOnly: true,
      },
    );
    window.location.reload();
  };

  const handoffToReview = async () => {
    if (!workspaceId || !selectedPaper?.revision || !onReviewHandoff) return;
    const handoff = await releaseClient.prepareReviewHandoff({
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
      {restoreRoots && (
        <WorkspaceRestoreRootsDialog
          roots={restoreRoots}
          onDone={finishRootMappings}
        />
      )}
      {(view === "all" || view === "backup") && (
        <section>
          <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
            Research backup
          </h3>
          <p className="mt-1 text-xs text-gray-500">
            Saves your projects, research files, and conversations to one file.
            It leaves out attached project folders, reviews, automations, Trash,
            and sign-ins. Back up attached folders separately.
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            <button
              type="button"
              disabled={busy}
              onClick={() => onAction(exportArchive)}
              className="rounded border px-3 py-2 text-xs"
            >
              Export research backup
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => onAction(restoreArchive)}
              className="rounded border px-3 py-2 text-xs"
            >
              Restore research backup
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
            Review a saved copy of this paper. Choose the review and models in
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
            Send the paper and selected document details using your Reviews
            settings and account. Project notes stay out of the review.
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
          After restoring, sign in to ChatGPT. Conversations restart from your
          notes and evidence. You can still read earlier messages.
        </p>
      )}
    </div>
  );
}
