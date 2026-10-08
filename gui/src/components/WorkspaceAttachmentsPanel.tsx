import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import {
  importResearchFiles,
  isReadable,
  paperSource,
  RESEARCH_FILE_EXTENSIONS,
} from "../lib/workspaceAttachments";
import { addContextObject } from "./WorkspaceContextTray";
import Button from "../ui/Button";
import Skeleton from "../ui/Skeleton";
import type {
  ConversationSnapshot,
  EffectiveHarness,
  PaperWithRevision,
} from "../lib/workbenchTypes";

/**
 * The two ways to bring a document into a conversation. "import" adds files
 * from the computer and lists what was just added; "documents" lists what the
 * project already holds. Either way a document can become the one the
 * assistant reads from, or be attached as an exact source.
 */
export default function WorkspaceAttachmentsPanel({
  mode,
  snapshot,
  onSnapshot,
  onBusy,
  onClose,
}: {
  mode: "import" | "documents";
  snapshot: ConversationSnapshot;
  onSnapshot: (next: ConversationSnapshot) => void;
  onBusy: (busy: boolean) => void;
  onClose: () => void;
}) {
  const [papers, setPapers] = useState<PaperWithRevision[]>([]);
  const [loading, setLoading] = useState(mode === "documents");
  const [harness, setHarness] = useState<EffectiveHarness | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const busyRef = useRef(false);
  const workspaceId = snapshot.session.workspaceId;
  useEffect(() => {
    let disposed = false;
    if (workspaceId)
      void Promise.all([
        mode === "documents"
          ? workbenchClient.listPapers(workspaceId)
          : Promise.resolve([]),
        workbenchClient.effectiveHarness(snapshot.session.id),
      ])
        .then(([nextPapers, nextHarness]) => {
          if (!disposed) {
            setPapers(nextPapers);
            setHarness(nextHarness);
          }
        })
        .catch((cause) => {
          if (!disposed) setError(workbenchErrorMessage(cause));
        })
        .finally(() => {
          if (!disposed) setLoading(false);
        });
    return () => {
      disposed = true;
    };
  }, [mode, workspaceId, snapshot.session.id]);

  const act = async (fn: () => Promise<void>) => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    onBusy(true);
    setError(null);
    try {
      await fn();
    } catch (cause) {
      setError(workbenchErrorMessage(cause));
    } finally {
      busyRef.current = false;
      setBusy(false);
      onBusy(false);
    }
  };
  const importFiles = (directory: boolean) =>
    act(async () => {
      if (!workspaceId) return;
      const paths = await open(
        directory
          ? { directory: true, multiple: false }
          : {
              multiple: true,
              filters: [
                {
                  name: "Research files",
                  extensions: RESEARCH_FILE_EXTENSIONS,
                },
              ],
            },
      );
      if (!paths) return;
      const { failures } = await importResearchFiles(
        workspaceId,
        typeof paths === "string" ? [paths] : paths,
        (imported) =>
          setPapers((old) => [
            imported,
            ...old.filter((item) => item.paper.id !== imported.paper.id),
          ]),
      );
      if (failures.length) setError(failures.join("\n"));
    });
  const selectPaper = (paperId: string | null) =>
    act(async () => {
      const latest = await workbenchClient.conversationSnapshot(
        snapshot.session.id,
      );
      const effective = await workbenchClient.effectiveHarness(
        snapshot.session.id,
      );
      const enableResearch =
        paperId !== null && !effective.enabledModules.includes("paper_context");
      const updated = await workbenchClient.updateSession({
        sessionId: latest.session.id,
        expectedRevision: latest.session.revision,
        operationId: `composer-document-${crypto.randomUUID()}`,
        paperId: paperId ?? undefined,
        clearPaper: paperId === null,
        ...(enableResearch ? { presetId: "research_assistant" } : {}),
      });
      onSnapshot({
        ...latest,
        session: updated.record,
        sequence: updated.sequence,
      });
      onClose();
    });
  return (
    <div className="space-y-3">
      <p className="text-ui-meta text-ink-muted">
        {mode === "import"
          ? "Files you add are saved in this project. You can also drop files onto the message box."
          : "Documents saved in this project."}{" "}
        <strong className="font-medium">Use in conversation</strong> sets the
        one document the assistant reads from;{" "}
        <strong className="font-medium">Add as source</strong> attaches this
        exact version.
      </p>
      {harness && !harness.enabledModules.includes("paper_context") && (
        <p className="rounded-ui-sm bg-sunken p-2 text-ui-meta">
          Using a document in the conversation switches to the Research
          assistant profile so ChatGPT can read it.
          {snapshot.activeBinding &&
            " This changes the model context for your next message; the saved transcript remains here."}
        </p>
      )}
      {mode === "import" && (
        <div className="flex flex-wrap gap-2">
          <Button
            variant="primary"
            disabled={busy}
            onClick={() => void importFiles(false)}
          >
            {busy ? "Working…" : "Choose files…"}
          </Button>
          <Button disabled={busy} onClick={() => void importFiles(true)}>
            Add source folder…
          </Button>
        </div>
      )}
      {error && (
        <p
          role="alert"
          className="whitespace-pre-wrap text-ui-meta text-danger"
        >
          {error}
        </p>
      )}
      {loading ? (
        <Skeleton label="Loading documents" lines={3} />
      ) : (
        <div className="max-h-60 space-y-2 overflow-auto">
          {papers.map((item) => {
            const source = paperSource(item);
            return (
              <div
                key={item.paper.id}
                className="rounded-ui-sm border border-line p-2.5"
              >
                <p
                  className="truncate text-ui-label font-medium"
                  title={item.paper.title}
                >
                  {item.paper.title}
                </p>
                {item.revision?.extraction.status === "failed" && (
                  <p className="mt-1 text-ui-meta text-danger">
                    Text unavailable:{" "}
                    {String(
                      item.revision.extraction.error ?? "Extraction failed",
                    )}
                  </p>
                )}
                <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1">
                  <Button
                    variant="link"
                    disabled={busy || !harness || !isReadable(item)}
                    onClick={() => void selectPaper(item.paper.id)}
                  >
                    {snapshot.session.paperId === item.paper.id
                      ? "Selected · use in conversation"
                      : "Use in conversation"}
                  </Button>
                  <Button
                    variant="link"
                    disabled={busy || !source}
                    onClick={() => {
                      if (!workspaceId || !source) return;
                      addContextObject(workspaceId, source);
                      onClose();
                    }}
                  >
                    Add as source
                  </Button>
                </div>
              </div>
            );
          })}
          {!papers.length && mode === "documents" && (
            <p className="py-3 text-ui-meta text-ink-muted">
              No documents in this project yet. Add files to get started.
            </p>
          )}
        </div>
      )}
      {snapshot.session.paperId && (
        <Button
          variant="link"
          disabled={busy}
          onClick={() => void selectPaper(null)}
        >
          Use project default document
        </Button>
      )}
    </div>
  );
}
