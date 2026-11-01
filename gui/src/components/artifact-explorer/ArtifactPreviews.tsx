import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { fileLanguage } from "../../lib/fileLanguages";
import type { ArtifactEntry, ArtifactContent } from "../../lib/artifactTypes";
import HighlightedCode from "../file-workspace/HighlightedCode";
import { parseResearchTable } from "../ResearchTablePreview";
import { ext, formatBytes } from "./format";
const MAX_CSV_ROWS = 200;
export function CodeView({ text, relPath }: { text: string; relPath: string }) {
  return <HighlightedCode text={text} language={fileLanguage(relPath)} />;
}

export function CsvView({ text, relPath }: { text: string; relPath: string }) {
  const delim = ext(relPath) === "tsv" ? "\t" : ",";
  const { rows, truncated, error } = parseResearchTable(
    text,
    delim,
    MAX_CSV_ROWS,
  );
  if (error)
    return (
      <div className="space-y-2">
        <p className="text-xs text-amber-700 dark:text-amber-300">{error}</p>
        <CodeView text={text} relPath={relPath} />
      </div>
    );
  return (
    <div className="overflow-auto">
      <table className="text-xs border-collapse">
        <tbody>
          {rows.map((cells, i) => (
            <tr
              key={i}
              className={
                i === 0 ? "font-semibold bg-gray-50 dark:bg-gray-800" : ""
              }
            >
              {cells.map((c, j) => (
                <td
                  key={j}
                  className="border border-gray-200 dark:border-gray-700 px-2 py-1 whitespace-nowrap"
                >
                  {c}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {truncated && (
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-2">
          Showing the first {MAX_CSV_ROWS} rows.
        </p>
      )}
    </div>
  );
}

export function BinaryCard({
  runId,
  entry,
  content,
}: {
  runId: string;
  entry: ArtifactEntry | null;
  content: ArtifactContent;
}) {
  const [openError, setOpenError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  // A model-written program (exe, jar, pkg, ...) must never be one click away
  // from running with the researcher's privileges.
  const executable = content.kind === "executable";

  const openInSystemViewer = async () => {
    setOpenError(null);
    setOpening(true);
    try {
      await invoke("open_run_artifact", { runId, relPath: entry?.rel_path });
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setOpenError(
        `Could not open this artifact in the system viewer: ${message}`,
      );
    } finally {
      setOpening(false);
    }
  };

  return (
    <div className="max-w-sm mx-auto mt-12 p-5 rounded-lg border border-gray-200 dark:border-gray-700 text-center">
      <p className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
        {entry?.label ?? "Binary file"}
      </p>
      <p className="text-xs text-gray-500 dark:text-gray-400 mb-4">
        {formatBytes(content.bytes)} — not previewable in the app.
      </p>
      {executable ? (
        <p className="text-xs text-gray-600 dark:text-gray-300">
          The review created this program file. Open its folder to inspect the
          code. Pipeline will not run it from this preview.
        </p>
      ) : (
        <button
          type="button"
          onClick={openInSystemViewer}
          disabled={opening}
          className="py-1.5 px-4 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                   text-gray-700 dark:text-gray-200 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors
                   disabled:cursor-wait disabled:opacity-50"
        >
          {opening ? "Opening…" : "Open in system viewer"}
        </button>
      )}
      {openError && (
        <p role="alert" className="mt-3 text-xs text-red-700 dark:text-red-400">
          {openError}
        </p>
      )}
    </div>
  );
}
