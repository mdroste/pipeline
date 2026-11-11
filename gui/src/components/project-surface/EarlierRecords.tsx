import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  projectClient,
  type EarlierRecords as Summary,
  type EarlierRecordsExport,
} from "../../lib/projectClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { Card, button, muted } from "./shared";

const records = (n: number) => (n === 1 ? "1 record" : `${n} records`);

/**
 * Theory notes, experiments, claims and the other records whose views were
 * retired. Shown only to a project that has some, so they can be saved as
 * Markdown files. Nothing is deleted.
 */
export default function EarlierRecords({
  workspaceId,
  root,
  reportError,
}: {
  workspaceId: string;
  root: string | null;
  reportError: (message: string) => void;
}) {
  const [summary, setSummary] = useState<Summary | null>(null);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<EarlierRecordsExport | null>(null);
  useEffect(() => {
    let stale = false;
    setSummary(null);
    setDone(null);
    void Promise.resolve()
      .then(() => projectClient.earlierRecords(workspaceId))
      .then((next) => {
        if (!stale) setSummary(next);
      })
      // An unreadable summary only hides this card.
      .catch(() => undefined);
    return () => {
      stale = true;
    };
  }, [workspaceId]);
  if (!summary?.total) return null;
  const exportRecords = async () => {
    const destination = await open({
      directory: true,
      multiple: false,
      defaultPath: root ?? undefined,
      title: "Choose where to save earlier research records",
    });
    if (typeof destination !== "string") return;
    setBusy(true);
    try {
      setDone(
        await projectClient.exportEarlierRecords(workspaceId, destination),
      );
    } catch (cause) {
      reportError(workbenchErrorMessage(cause));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card
      title="Earlier research records"
      action={
        <button
          className={button}
          disabled={busy}
          onClick={() => void exportRecords()}
        >
          {busy ? "Exporting…" : "Export as Markdown…"}
        </button>
      }
    >
      <p className={muted}>
        This project has {records(summary.total)} from views Pipeline no longer
        has. Export saves them as Markdown files in a new folder, one file per
        kind. The records stay in Pipeline either way.
      </p>
      <ul className="text-xs">
        {summary.kinds.map((kind) => (
          <li key={kind.kind}>
            {kind.label} · {kind.count}
          </li>
        ))}
      </ul>
      {done && (
        <p role="status" className="text-xs">
          Saved {done.files.length} files to{" "}
          <span className="break-all font-mono">{done.directory}</span>.
          {done.notesCreated > 0 &&
            ` ${done.notesCreated === 1 ? "1 abandoned approach is" : `${done.notesCreated} abandoned approaches are`} now a note.`}
        </p>
      )}
    </Card>
  );
}
