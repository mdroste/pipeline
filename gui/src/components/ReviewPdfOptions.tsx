import { useEffect, useState } from "react";
import { appClient } from "../lib/appClient";
import type { RunModelOverrides } from "../lib/reviewModelTypes";

const METHODS: Record<string, string> = {
  pdftotext: "Text layer · fast, local",
  "paddleocr-vl-full": "PaddleOCR · local, reads scans and layout",
  llm: "AI model · reads scans, equations, and tables",
};
export default function ReviewPdfOptions({
  path,
  value,
  onChange,
  disabled,
  refreshKey,
}: {
  path: string;
  value: RunModelOverrides | null;
  onChange: (value: RunModelOverrides | null) => void;
  disabled: boolean;
  refreshKey: number;
}) {
  const [options, setOptions] = useState<{ method: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let active = true;
    setOptions(null);
    setError(null);
    void appClient
      .reviewPdfOptions({
        runParallelOverrides: value,
      })
      .then((result) => {
        if (active) setOptions(result);
      })
      .catch((cause) => {
        if (active) setError(String(cause));
      });
    return () => {
      active = false;
    };
  }, [value, path, refreshKey, attempt]);
  return (
    <div className="mt-3 space-y-2 border-t border-line pt-3 text-ui-meta">
      <label className="block font-medium">
        Read this PDF with
        <select
          className="mt-2 block w-full rounded-lg border border-line bg-surface p-2 font-normal"
          value={value?.extraction_method ?? ""}
          disabled={disabled}
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
      {!options && !error && <p role="status">Checking PDF method…</p>}
      {error && (
        <div role="alert">
          <p>{error}</p>
          <button
            className="mt-1 underline"
            onClick={() => setAttempt((n) => n + 1)}
          >
            Retry PDF options
          </button>
        </div>
      )}
    </div>
  );
}
