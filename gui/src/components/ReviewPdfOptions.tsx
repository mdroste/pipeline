import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { RunModelOverrides } from "../lib/types";
import { PROVIDER_LABELS, type Provider } from "../lib/providers";

const METHODS: Record<string, string> = {
  pdftotext: "Text layer · fast, local",
  "paddleocr-vl-full": "PaddleOCR · local, reads scans and layout",
  llm: "AI model · reads scans, equations, and tables",
};
interface Sample {
  text: string;
  image: string;
  method: string;
  pageCount: number;
  notes: string[];
}
export default function ReviewPdfOptions({
  path,
  value,
  onChange,
  disabled,
  refreshKey,
  onBusy,
}: {
  path: string;
  value: RunModelOverrides | null;
  onChange: (value: RunModelOverrides | null) => void;
  disabled: boolean;
  refreshKey: number;
  onBusy: (value: boolean) => void;
}) {
  const [options, setOptions] = useState<{
    method: string;
    provider: string;
    snapshotId: string;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [page, setPage] = useState(1);
  const [sample, setSample] = useState<Sample | null>(null);
  const [busy, setBusy] = useState(false);
  const generation = useRef(0);
  useEffect(() => {
    const key = ++generation.current;
    setOptions(null);
    setSample(null);
    setError(null);
    void invoke<{ method: string; provider: string; snapshotId: string }>(
      "review_pdf_options",
      { runParallelOverrides: value },
    )
      .then((result) => {
        if (key === generation.current) setOptions(result);
      })
      .catch((cause) => {
        if (key === generation.current) setError(String(cause));
      });
    return () => {
      generation.current++;
    };
  }, [value, path, refreshKey, attempt]);
  const preview = async () => {
    if (!options || busy) return;
    const key = generation.current;
    setBusy(true);
    onBusy(true);
    setError(null);
    setSample(null);
    try {
      const result = await invoke<Sample>("preview_review_pdf", {
        paperPath: path,
        page,
        method: options.method,
        allowModelCall: options.method === "llm",
        expectedSnapshotId: options.snapshotId,
        runParallelOverrides: value,
      });
      if (key === generation.current) setSample(result);
    } catch (cause) {
      if (key === generation.current) setError(String(cause));
    } finally {
      setBusy(false);
      onBusy(false);
    }
  };
  return (
    <div className="mt-4 space-y-3 border-t border-line pt-4 text-ui-meta">
      <label className="block font-medium">
        Read this PDF with
        <select
          className="mt-2 block w-full rounded-lg border border-line bg-surface p-2 font-normal"
          value={value?.extraction_method ?? ""}
          disabled={disabled || busy}
          onChange={(event) => {
            const next = {
              ...(value ?? {
                agents: [],
                model_overrides: {},
                effort_overrides: {},
              }),
              extraction_method: event.target.value || undefined,
            };
            onChange(
              !next.review &&
                !next.agents.length &&
                !next.merge_agent &&
                !next.extraction_method
                ? null
                : next,
            );
          }}
        >
          <option value="">
            Workflow default
            {options && !value?.extraction_method
              ? " · " + METHODS[options.method]
              : ""}
          </option>
          {Object.entries(METHODS).map(([id, label]) => (
            <option key={id} value={id}>
              {label}
            </option>
          ))}
        </select>
      </label>
      <p className="text-ink-muted">
        Applies to this review. Check a page with equations, tables, or scanned
        text before you start.
      </p>
      {!options && !error && <p role="status">Checking PDF method…</p>}
      <details>
        <summary className="cursor-pointer font-medium">
          Check a sample page
        </summary>
        <div className="mt-3 space-y-3">
          {options?.method === "llm" ? (
            <p>
              This sends a one-page PDF to{" "}
              {PROVIDER_LABELS[options.provider as Provider] ??
                options.provider}{" "}
              and uses your model allowance. Failed reads may be retried.
            </p>
          ) : (
            <p>The sample runs on this computer.</p>
          )}
          <div className="flex flex-wrap items-end gap-3">
            <label>
              Page{" "}
              <input
                type="number"
                min={1}
                max={sample?.pageCount}
                value={page}
                disabled={busy}
                onChange={(event) => {
                  setPage(
                    Math.max(1, Math.floor(Number(event.target.value) || 1)),
                  );
                  setSample(null);
                }}
                className="ml-2 w-20 rounded border border-line bg-surface p-2"
              />
            </label>
            <button
              className="ui-page-button"
              disabled={!options || busy || disabled}
              onClick={() => void preview()}
            >
              {busy
                ? "Reading sample…"
                : options?.method === "llm"
                  ? "Preview page with AI"
                  : "Preview page"}
            </button>
          </div>
          {sample && (
            <div className="space-y-3">
              <p>
                Page {page} of {sample.pageCount} · {METHODS[sample.method]}
              </p>
              <details>
                <summary className="cursor-pointer">Original page</summary>
                <img
                  src={sample.image}
                  alt={`Original PDF page ${page}`}
                  className="mt-2 max-h-[32rem] w-full object-contain"
                />
              </details>
              <p className="font-medium">Extracted text</p>
              <pre
                tabIndex={0}
                className="max-h-80 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-sunken p-3 font-mono text-xs"
              >
                {sample.text ||
                  "No text found. Try OCR or an AI model for a scanned page."}
              </pre>
              {sample.notes.map((note) => (
                <p key={note} className="text-ink-muted">
                  {note}
                </p>
              ))}
            </div>
          )}
        </div>
      </details>
      {error && (
        <div role="alert">
          <p>{error}</p>
          <button
            className="mt-1 underline"
            disabled={busy}
            onClick={() => setAttempt((n) => n + 1)}
          >
            Retry PDF options
          </button>
        </div>
      )}
    </div>
  );
}
