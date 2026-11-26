import { useEffect, useRef, useState } from "react";
import { applyFileHunks, type FileHunk } from "../../lib/fileDiff";
import { startFileDiff } from "../../lib/fileDiffClient";
import { fileLanguage } from "../../lib/fileLanguages";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import DiffPreview from "./DiffPreview";
import ReportIssueLink from "../ReportIssueLink";
const EMPTY_HUNKS: FileHunk[] = [];
const HUNKS_PER_PAGE = 40;
export default function SourceDiff({
  before,
  after,
  path,
  onAccept,
}: {
  before: string;
  after: string;
  path: string;
  onAccept?: (content: string) => Promise<void>;
}) {
  const [comparison, setComparison] = useState<{
    before: string;
    after: string;
    hunks: FileHunk[];
  } | null>(null);
  const ready = comparison?.before === before && comparison?.after === after;
  const hunks = ready ? comparison.hunks : EMPTY_HUNKS;
  const [selected, setSelected] = useState<number[]>([]);
  const [current, setCurrent] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [accepted, setAccepted] = useState("");
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let live = true;
    const job = startFileDiff(before, after);
    setSelected([]);
    setCurrent(0);
    void job.result
      .then((hunks) => {
        if (!live) return;
        setComparison({ before, after, hunks });
        setSelected(hunks.map((h) => h.id));
      })
      .catch((error) => {
        if (live) setError(workbenchErrorMessage(error));
      });
    return () => {
      live = false;
      job.cancel();
    };
  }, [before, after]);
  const pageStart = Math.floor(current / HUNKS_PER_PAGE) * HUNKS_PER_PAGE;
  useEffect(() => {
    root.current
      ?.querySelector(`[data-hunk="${current}"]`)
      ?.scrollIntoView?.({ block: "center" });
  }, [current, pageStart]);
  const go = (index: number) => {
    const next = Math.max(0, Math.min(hunks.length - 1, index));
    setCurrent(next);
  };
  return (
    <div ref={root}>
      <div className="file-toolbar">
        <span>
          {ready
            ? `${hunks.length} change${hunks.length === 1 ? "" : "s"}`
            : "Comparing…"}
        </span>
        <button disabled={current <= 0} onClick={() => go(current - 1)}>
          Previous change
        </button>
        <button
          disabled={current >= hunks.length - 1}
          onClick={() => go(current + 1)}
        >
          Next change
        </button>
        {onAccept && (
          <>
            <button
              disabled={!ready || busy}
              onClick={() => setSelected(hunks.map((h) => h.id))}
            >
              Select all changes
            </button>
            <button onClick={() => setSelected([])}>Clear changes</button>
            <button
              disabled={!ready || busy || !selected.length}
              onClick={() => {
                const count = selected.length;
                setBusy(true);
                setError("");
                setAccepted("");
                void Promise.resolve()
                  .then(() => onAccept(applyFileHunks(before, hunks, selected)))
                  .then(() =>
                    setAccepted(
                      `Accepted ${count} change${count === 1 ? "" : "s"}.`,
                    ),
                  )
                  .catch((e) => setError(workbenchErrorMessage(e)))
                  .finally(() => setBusy(false));
              }}
            >
              Accept {selected.length} selected change
              {selected.length === 1 ? "" : "s"}
            </button>
            {accepted && <span role="status">{accepted}</span>}
          </>
        )}
      </div>
      {error && (
        <p role="alert" className="file-error">
          {error}
          <ReportIssueLink />
        </p>
      )}
      {hunks.length > HUNKS_PER_PAGE && (
        <div className="file-toolbar">
          <button
            disabled={pageStart === 0}
            onClick={() => go(pageStart - HUNKS_PER_PAGE)}
          >
            Previous changes
          </button>
          <span>
            Showing {pageStart + 1}–
            {Math.min(pageStart + HUNKS_PER_PAGE, hunks.length)} of{" "}
            {hunks.length}
          </span>
          <button
            disabled={pageStart + HUNKS_PER_PAGE >= hunks.length}
            onClick={() => go(pageStart + HUNKS_PER_PAGE)}
          >
            Next changes
          </button>
        </div>
      )}
      {hunks.slice(pageStart, pageStart + HUNKS_PER_PAGE).map((h) => (
        <section
          key={h.id}
          data-hunk={h.id}
          className="mb-4 rounded border p-2"
        >
          <div className="file-toolbar">
            {onAccept && (
              <input
                type="checkbox"
                aria-label={`Accept change at line ${h.line}`}
                checked={selected.includes(h.id)}
                onChange={(e) =>
                  setSelected((old) =>
                    e.target.checked
                      ? [...old, h.id]
                      : old.filter((id) => id !== h.id),
                  )
                }
              />
            )}
            <span>Line {h.line}</span>
          </div>
          <div className="grid gap-2 lg:grid-cols-2">
            <div>
              <span className="text-xs text-danger">Before</span>
              <DiffPreview
                text={h.before}
                language={fileLanguage(path)}
                label={`Line ${h.line} before`}
              />
            </div>
            <div>
              <span className="text-xs text-success">After</span>
              <DiffPreview
                text={h.after}
                language={fileLanguage(path)}
                label={`Line ${h.line} after`}
              />
            </div>
          </div>
        </section>
      ))}
    </div>
  );
}
