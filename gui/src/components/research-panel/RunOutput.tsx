import type { ResearchExecution } from "../../lib/workbenchTypes";

const streams = [
  ["stdout", "Standard output"],
  ["stderr", "Errors and warnings"],
] as const;

/** Both captured streams of a run, each labelled and scrollable. */
export default function RunOutput({
  run,
}: {
  run: Pick<ResearchExecution, "stdout" | "stderr" | "validation">;
}) {
  const shown = streams.filter(([stream]) => run[stream]);
  if (shown.length === 0) {
    return (
      <pre className="whitespace-pre-wrap rounded bg-gray-100 p-2 dark:bg-neutral-900">
        No captured output
      </pre>
    );
  }
  return (
    <>
      {shown.map(([stream, label]) => (
        <div key={stream} className="space-y-1">
          <p className="font-medium text-gray-600 dark:text-gray-400">
            {label}
          </p>
          <pre
            aria-label={label}
            className="max-h-40 overflow-auto whitespace-pre-wrap rounded bg-gray-100 p-2 dark:bg-neutral-900"
          >
            {run[stream]}
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
