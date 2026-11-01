// A read-only view of a saved Jupyter notebook. Saved outputs can hold anything,
// so HTML and JavaScript outputs are never run: their plain text is shown instead.
import { useEffect, useMemo, useState } from "react";
import ReportViewer from "../../ReportViewer";
import HighlightedCode from "../HighlightedCode";

type Multiline = string | string[];
interface NotebookOutput {
  output_type?: string;
  text?: Multiline;
  name?: string;
  ename?: string;
  evalue?: string;
  traceback?: string[];
  data?: Record<string, Multiline>;
}
interface NotebookCell {
  cell_type?: string;
  source?: Multiline;
  outputs?: NotebookOutput[];
  execution_count?: number | null;
}
interface Notebook {
  cells?: NotebookCell[];
  metadata?: {
    kernelspec?: { language?: string };
    language_info?: { name?: string };
  };
}
const PAGE = 100;
const joined = (value: Multiline | undefined) =>
  Array.isArray(value) ? value.join("") : (value ?? "");
// eslint-disable-next-line no-control-regex
const stripAnsi = (text: string) =>
  text.replace(/\u001b\[[0-9;]*[A-Za-z]/g, "");

export function parseNotebook(text: string): Notebook | null {
  try {
    const value = JSON.parse(text) as Notebook;
    return Array.isArray(value?.cells) ? value : null;
  } catch {
    return null;
  }
}
function Output({ output }: { output: NotebookOutput }) {
  const data = output.data;
  const image = (mime: string) => joined(data?.[mime]).replace(/\s/g, "");
  if (data?.["image/png"] || data?.["image/jpeg"]) {
    const mime = data["image/png"] ? "image/png" : "image/jpeg";
    return (
      <img
        alt="Notebook output"
        className="file-notebook-image"
        src={`data:${mime};base64,${image(mime)}`}
      />
    );
  }
  if (data?.["image/svg+xml"])
    return (
      <img
        alt="Notebook output"
        className="file-notebook-image"
        src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(joined(data["image/svg+xml"]))}`}
      />
    );
  if (output.output_type === "error")
    return (
      <pre className="file-notebook-error">
        {stripAnsi(
          output.traceback?.join("\n") ??
            `${output.ename ?? "Error"}: ${output.evalue ?? ""}`,
        )}
      </pre>
    );
  const text = data ? joined(data["text/plain"]) : joined(output.text);
  if (!text) return null;
  return <pre>{stripAnsi(text)}</pre>;
}
export default function NotebookViewer({
  text,
  load,
  path,
}: {
  /** The whole file when it fits in the editor. */
  text: string | null;
  /** Reads the whole file when `text` is only its beginning. */
  load?: () => Promise<string>;
  path: string;
}) {
  const [loaded, setLoaded] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [shown, setShown] = useState(PAGE);
  useEffect(() => {
    if (text !== null || !load) return;
    let live = true;
    load().then(
      (value) => live && setLoaded(value),
      (e) => live && setError(String(e?.message ?? e)),
    );
    return () => {
      live = false;
    };
  }, [text, load]);
  const notebook = useMemo(
    () => parseNotebook(text ?? loaded ?? ""),
    [text, loaded],
  );
  if (error)
    return (
      <p role="alert" className="file-error">
        {error}
      </p>
    );
  if (text === null && loaded === null)
    return <p className="file-status">Reading notebook…</p>;
  if (!notebook)
    return (
      <p role="alert" className="file-error">
        This notebook could not be read. Use Source to inspect it.
      </p>
    );
  const language =
    notebook.metadata?.language_info?.name ??
    notebook.metadata?.kernelspec?.language ??
    "python";
  const cells = notebook.cells ?? [];
  return (
    <div className="file-notebook" aria-label={`Notebook: ${path}`}>
      {cells.slice(0, shown).map((cell, index) =>
        cell.cell_type === "markdown" ? (
          <div key={index} className="file-notebook-cell">
            <ReportViewer markdown={joined(cell.source)} />
          </div>
        ) : cell.cell_type === "code" ? (
          <div key={index} className="file-notebook-cell">
            <span className="file-notebook-count">
              [{cell.execution_count ?? " "}]
            </span>
            <HighlightedCode text={joined(cell.source)} language={language} />
            {cell.outputs?.map((output, outputIndex) => (
              <div key={outputIndex} className="file-notebook-output">
                <Output output={output} />
              </div>
            ))}
          </div>
        ) : (
          <pre key={index} className="file-notebook-cell">
            {joined(cell.source)}
          </pre>
        ),
      )}
      {cells.length > shown && (
        <button
          className="file-notebook-more"
          onClick={() => setShown((old) => old + PAGE)}
        >
          Show more cells ({cells.length - shown} remaining)
        </button>
      )}
    </div>
  );
}
