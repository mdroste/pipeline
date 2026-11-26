import { useEffect, useId, useState } from "react";
import { useReviewSetupEnvironment } from "./reviewSetupEnvironment";
import type { RunModelOverrides } from "../lib/reviewModelTypes";
import Select from "../ui/Select";

const METHODS: { id: string; label: string; description: string }[] = [
  {
    id: "pdftotext",
    label: "Text layer",
    description: "Fast; uses the text already in the PDF",
  },
  {
    id: "paddleocr-vl-full",
    label: "PaddleOCR",
    description: "Runs locally; reads scans and page layout",
  },
  {
    id: "llm",
    label: "AI model",
    description: "Reads scans, equations, and tables",
  },
];

/** The run's overrides without its own PDF choice, so the workflow's default shows through. */
function withoutPdfMethod(value: RunModelOverrides | null) {
  if (!value) return null;
  const { extraction_method: _ignored, ...rest } = value;
  return !rest.review && !rest.agents.length && !rest.merge_agent ? null : rest;
}

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
  const environment = useReviewSetupEnvironment();
  const labelId = useId();
  const base = withoutPdfMethod(value);
  const baseKey = JSON.stringify(base);
  useEffect(() => {
    let active = true;
    setOptions(null);
    setError(null);
    void environment
      .pdfOptions({
        runParallelOverrides: base,
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
    // `base` is captured through `baseKey`; picking a PDF method doesn't refetch.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [baseKey, path, refreshKey, attempt, environment]);
  const defaultMethod = options?.method;
  const choose = (method: string) => {
    const next = {
      ...(value ?? {
        agents: [],
        model_overrides: {},
        effort_overrides: {},
      }),
      // Choosing the workflow's own default follows the workflow again.
      extraction_method: method === defaultMethod ? undefined : method,
    };
    onChange(
      !next.review &&
        !next.agents.length &&
        !next.merge_agent &&
        !next.extraction_method
        ? null
        : next,
    );
  };
  return (
    <div className="review-pdf-options">
      <div className="review-pdf-options-row">
        <span id={labelId}>Read this PDF with</span>
        <Select
          labelledBy={labelId}
          value={value?.extraction_method ?? defaultMethod ?? ""}
          placeholder="Workflow default"
          disabled={disabled}
          align="end"
          className="w-full"
          options={METHODS.map((method) => ({
            value: method.id,
            label: method.label,
            description: method.description,
            badge: method.id === defaultMethod ? "Default" : undefined,
          }))}
          onChange={choose}
        />
      </div>
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
