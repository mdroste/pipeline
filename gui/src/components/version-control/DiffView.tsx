import { useEffect, useMemo, useState } from "react";
import { Spinner } from "../../ui";
import {
  conflictRows,
  newFileRows,
  parseUnifiedDiff,
  type DiffRow,
} from "../../lib/unifiedDiff";
import {
  versionControlClient,
  type DiffRequest,
  type FileDiff,
} from "../../lib/versionControlClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";

/** A long regenerated file stays readable and the page stays responsive. */
const MAX_ROWS = 4000;

const MESSAGES: Partial<Record<FileDiff["kind"], string>> = {
  binary: "This is not a text file, so there are no lines to compare.",
  tooLarge: "This change is too large to show here.",
  folder: "This is a new folder. Everything inside it is included.",
  empty: "No changes to show.",
};

function rowsFor(diff: FileDiff): DiffRow[] {
  if (diff.kind === "diff") return parseUnifiedDiff(diff.text);
  if (diff.kind === "new") return newFileRows(diff.text);
  if (diff.kind === "conflict") return conflictRows(diff.text);
  return [];
}

function Rows({ rows }: { rows: DiffRow[] }) {
  const visible = rows.slice(0, MAX_ROWS);
  return (
    <div className="vc-diff-table" role="table" aria-label="Changed lines">
      {visible.map((row, index) => (
        <div key={index} role="row" className={`vc-diff-row is-${row.kind}`}>
          <span className="vc-diff-number" aria-hidden="true">
            {row.oldLine ?? ""}
          </span>
          <span className="vc-diff-number" aria-hidden="true">
            {row.newLine ?? ""}
          </span>
          <span className="vc-diff-mark" aria-hidden="true">
            {row.kind === "add" ? "+" : row.kind === "del" ? "−" : ""}
          </span>
          <span role="cell" className="vc-diff-text">
            {row.text || " "}
          </span>
        </div>
      ))}
      {rows.length > MAX_ROWS && (
        <p className="vc-diff-message">
          Showing the first {MAX_ROWS.toLocaleString()} of{" "}
          {rows.length.toLocaleString()} lines.
        </p>
      )}
    </div>
  );
}

export default function DiffView({
  workspaceId,
  request,
  stamp,
}: {
  workspaceId: string;
  request: DiffRequest;
  /** Changes when the file may have changed, so the diff reloads. */
  stamp: unknown;
}) {
  const [loaded, setLoaded] = useState<{ key: string; diff: FileDiff } | null>(
    null,
  );
  const [error, setError] = useState("");
  const key = JSON.stringify(request);
  // A refresh of this file keeps its diff up; another file never shows it.
  const diff = loaded?.key === key ? loaded.diff : null;
  useEffect(() => {
    let stale = false;
    setError("");
    versionControlClient
      .diff(workspaceId, JSON.parse(key) as DiffRequest)
      .then((next) => {
        if (!stale) setLoaded({ key, diff: next });
      })
      .catch((cause) => {
        if (!stale) {
          setLoaded(null);
          setError(workbenchErrorMessage(cause));
        }
      });
    return () => {
      stale = true;
    };
  }, [workspaceId, key, stamp]);
  const rows = useMemo(() => (diff ? rowsFor(diff) : []), [diff]);
  if (error) return <p className="vc-diff-message">{error}</p>;
  if (!diff)
    return (
      <div className="vc-diff-message">
        <Spinner label="Loading" />
      </div>
    );
  const message = MESSAGES[diff.kind];
  if (message) return <p className="vc-diff-message">{message}</p>;
  if (!rows.length)
    return <p className="vc-diff-message">No changes to show.</p>;
  return <Rows rows={rows} />;
}
