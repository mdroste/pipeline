import { useState } from "react";
import type { Json, Receipt } from "../../lib/taskTypes";
import { taskClient } from "../../lib/taskClient";
import { router } from "../../lib/router";

export default function Output({
  receipt,
  runId,
  onError,
}: {
  receipt: Receipt;
  runId: string;
  onError: (error: string) => void;
}) {
  const [full, setFull] = useState<{ value: Json } | null>(null);
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);
  const v = full ? full.value : receipt.output;
  const object = v && typeof v === "object" && !Array.isArray(v) ? v : null;
  const result =
    object?.result &&
    typeof object.result === "object" &&
    !Array.isArray(object.result)
      ? object.result
      : object;
  const artifact =
    result?.reviewedArtifact &&
    typeof result.reviewedArtifact === "object" &&
    !Array.isArray(result.reviewedArtifact)
      ? result.reviewedArtifact
      : result;
  const filename =
    typeof artifact?.filename === "string" ? artifact.filename : null;
  const text =
    typeof v === "string"
      ? v
      : typeof result?.answer === "string"
        ? result.answer
        : typeof result?.summary === "string"
          ? result.summary
          : null;
  async function load() {
    if (full || loading) return;
    setLoading(true);
    try {
      setFull({
        value: await taskClient.stepOutput(
          runId,
          receipt.address,
          receipt.operation,
        ),
      });
    } catch (e) {
      onError(String(e));
    } finally {
      setLoading(false);
    }
  }
  return (
    <>
      {text && (
        <p className="task-output-summary">
          {text.length > 700 ? `${text.slice(0, 700)}…` : text}
        </p>
      )}
      {filename && (
        <p>
          <strong>{filename}</strong>
        </p>
      )}
      {result &&
        (result.kind === "artifact" ||
          result.kind === "artifactTree" ||
          result.reviewedArtifact) && (
          <button
            type="button"
            onClick={() =>
              void taskClient
                .artifact(runId, receipt.address, receipt.operation)
                .catch((e) => onError(String(e)))
            }
          >
            Save paper…
          </button>
        )}
      {typeof result?.runId === "string" && (
        <button
          type="button"
          onClick={() =>
            router.navigate({ page: "history", runId: result.runId as string })
          }
        >
          Open review · {receipt.label}
        </button>
      )}
      {result?.highPriorityCount !== undefined && (
        <p>
          {String(result.highPriorityCount)} high-priority findings ·{" "}
          {result.complete === true
            ? "Complete review"
            : "Review has limitations"}
        </p>
      )}
      <details
        onToggle={(e) => {
          if (e.currentTarget.open) void load();
        }}
      >
        <summary>Output details</summary>
        <p>Step: {receipt.address}</p>
        <p>Operation: {receipt.operation}</p>
        {typeof result?.runId === "string" && <p>Review ID: {result.runId}</p>}
        {loading && <p role="status">Loading output…</p>}
        <button
          type="button"
          onClick={async () => {
            try {
              await navigator.clipboard.writeText(
                JSON.stringify(
                  {
                    step: receipt.address,
                    operation: receipt.operation,
                    output: v,
                  },
                  null,
                  2,
                ),
              );
              setCopied(true);
            } catch {
              onError("Could not copy the output. Select it below to copy.");
            }
          }}
        >
          {copied ? "Copied" : "Copy details"}
        </button>
        <pre className="task-result">
          {typeof v === "string" ? v : JSON.stringify(v, null, 2)}
        </pre>
      </details>
    </>
  );
}
