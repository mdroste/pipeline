import { useState } from "react";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { fileLanguage } from "../../lib/fileLanguages";
import type { ArtifactEntry, ArtifactContent } from "../../lib/artifactTypes";
import HighlightedCode from "../file-workspace/HighlightedCode";
import { ext, formatBytes } from "./format";
const MAX_CSV_ROWS = 200;
export function CodeView({ text, relPath }: { text: string; relPath: string }) {
  return <HighlightedCode text={text} language={fileLanguage(relPath)} />;
}

export function CsvView({ text, relPath }: { text: string; relPath: string }) {
  const delim = ext(relPath) === "tsv" ? "\t" : ",";
  // Naive split — quoted delimiters aren't handled; fine for a preview.
  const lines = text.split(/\r?\n/).filter((l) => l.length > 0);
  const rows = lines.slice(0, MAX_CSV_ROWS).map((l) => l.split(delim));
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
      {lines.length > MAX_CSV_ROWS && (
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-2">
          Showing first {MAX_CSV_ROWS} of {lines.length} rows.
        </p>
      )}
    </div>
  );
}

export function BinaryCard({
  entry,
  content,
}: {
  entry: ArtifactEntry | null;
  content: ArtifactContent;
}) {
  const [openError, setOpenError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);

  const openInSystemViewer = async () => {
    setOpenError(null);
    setOpening(true);
    try {
      await openExternal(content.abs_path);
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
      {openError && (
        <p role="alert" className="mt-3 text-xs text-red-700 dark:text-red-400">
          {openError}
        </p>
      )}
    </div>
  );
}
