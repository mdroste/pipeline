export default function ReportNotices({
  notes,
  truncated,
  fullLoading,
  fullError,
  onLoadFull,
  onSources,
  preamble,
  raw,
  onRaw,
}: {
  notes: string[];
  truncated: boolean;
  fullLoading: boolean;
  fullError: string | null;
  onLoadFull: () => void;
  onSources?: () => void;
  preamble: boolean;
  raw: boolean;
  onRaw: (raw: boolean) => void;
}) {
  if (!notes.length && !truncated && !preamble) return null;
  return (
    <section
      aria-label="Report notices"
      className="shrink-0 border-b border-line bg-sunken px-6 py-3 text-xs text-ink"
    >
      {notes.length > 0 && (
        <details open>
          <summary className="cursor-pointer font-medium">
            Report limitations · {notes.length}{" "}
            {notes.length === 1 ? "notice" : "notices"}
          </summary>
          <ul className="mt-2 max-h-48 list-disc space-y-2 overflow-auto whitespace-pre-wrap break-words pl-4">
            {notes.map((note) => (
              <li key={note}>{note}</li>
            ))}
          </ul>
          {onSources && (
            <button className="mt-2 font-medium underline" onClick={onSources}>
              Open saved outputs
            </button>
          )}
        </details>
      )}
      {preamble && (
        <div className="mt-1 flex flex-wrap items-center gap-3">
          <p>
            {raw
              ? "Showing the full model output."
              : "Model process notes are hidden from the report."}
          </p>
          <button className="underline" onClick={() => onRaw(!raw)}>
            {raw ? "Clean view" : "Raw output"}
          </button>
        </div>
      )}
      {truncated && (
        <div className="mt-1 flex flex-wrap items-center gap-3">
          <p>The first part of this report is loaded.</p>
          <button
            disabled={fullLoading}
            className="font-medium underline disabled:opacity-50"
            onClick={onLoadFull}
          >
            {fullLoading ? "Loading full report…" : "Load full report"}
          </button>
        </div>
      )}
      {fullError && (
        <p role="alert" className="mt-2 whitespace-pre-wrap break-words">
          Couldn't load the full report. {fullError} Try again with Load full
          report.
        </p>
      )}
    </section>
  );
}
