import { useEffect, useMemo, useRef, useState } from "react";
import { applyFileHunks, fileHunks } from "../../lib/fileDiff";
import { fileLanguage } from "../../lib/fileLanguages";
import HighlightedCode from "./HighlightedCode";
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
  const hunks = useMemo(() => fileHunks(before, after), [before, after]);
  const [selected, setSelected] = useState<number[]>([]);
  const [current, setCurrent] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    setSelected(hunks.map((h) => h.id));
    setCurrent(0);
  }, [hunks]);
  const go = (index: number) => {
    const next = Math.max(0, Math.min(hunks.length - 1, index));
    setCurrent(next);
    root.current
      ?.querySelector(`[data-hunk="${next}"]`)
      ?.scrollIntoView({ block: "center" });
  };
  return (
    <div ref={root}>
      <div className="file-toolbar">
        <span>
          {hunks.length} change{hunks.length === 1 ? "" : "s"}
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
            <button onClick={() => setSelected(hunks.map((h) => h.id))}>
              Select all changes
            </button>
            <button onClick={() => setSelected([])}>Clear changes</button>
            <button
              disabled={busy || !selected.length}
              onClick={() => {
                setBusy(true);
                setError("");
                void onAccept(applyFileHunks(before, hunks, selected))
                  .catch((e) => setError(String(e)))
                  .finally(() => setBusy(false));
              }}
            >
              Accept {selected.length} selected change
              {selected.length === 1 ? "" : "s"}
            </button>
          </>
        )}
      </div>
      {error && (
        <p role="alert" className="file-error">
          {error}
        </p>
      )}
      {hunks.map((h) => (
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
              <span className="text-xs text-red-600">Before</span>
              <HighlightedCode text={h.before} language={fileLanguage(path)} />
            </div>
            <div>
              <span className="text-xs text-green-600">After</span>
              <HighlightedCode text={h.after} language={fileLanguage(path)} />
            </div>
          </div>
        </section>
      ))}
    </div>
  );
}
