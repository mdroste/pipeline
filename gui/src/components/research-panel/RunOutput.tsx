import { useEffect, useState } from "react";
import { executionClient } from "../../lib/workbench/executionClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import type { ResearchExecution } from "../../lib/workbenchTypes";

const streams = [
  ["stdout", "Standard output"],
  ["stderr", "Errors and warnings"],
] as const;

/**
 * Both captured streams of a run, each labelled and scrollable. Run lists carry
 * no output, so this loads it for the one run that is open.
 */
export default function RunOutput({
  workspaceId,
  run,
}: {
  workspaceId: string;
  run: Pick<ResearchExecution, "id" | "outcome" | "validation">;
}) {
  const [output, setOutput] = useState<Pick<
    ResearchExecution,
    "stdout" | "stderr"
  > | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    executionClient.executionOutput(workspaceId, run.id).then(
      (value) => {
        if (live) setOutput(value);
      },
      (cause) => {
        if (live) setError(workbenchErrorMessage(cause));
      },
    );
    return () => {
      live = false;
    };
  }, [workspaceId, run.id, run.outcome]);
  if (error) return <p role="alert">{error}</p>;
  if (!output) return <p className="text-gray-500">Loading output…</p>;
  const shown = streams.filter(([stream]) => output[stream]);
  const imported = run.validation.importedFrom != null && (
    <p className="text-amber-700 dark:text-amber-300">
      Imported from a package. This run didn't happen on this computer.
    </p>
  );
  if (shown.length === 0) {
    return (
      <>
        {imported}
        <pre className="whitespace-pre-wrap rounded bg-gray-100 p-2 dark:bg-neutral-900">
          No captured output
        </pre>
      </>
    );
  }
  return (
    <>
      {imported}
      {shown.map(([stream, label]) => (
        <div key={stream} className="space-y-1">
          <p className="font-medium text-gray-600 dark:text-gray-400">
            {label}
          </p>
          <pre
            aria-label={label}
            className="max-h-40 overflow-auto whitespace-pre-wrap rounded bg-gray-100 p-2 dark:bg-neutral-900"
          >
            {output[stream]}
          </pre>
          {run.validation[`${stream}Truncated`] === true && (
            <p className="text-gray-500">
              Only the first 4 MiB of this output was saved.
            </p>
          )}
        </div>
      ))}
    </>
  );
}
