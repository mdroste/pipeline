import type { PipelineState, PassStatus } from "../hooks/usePipeline";

interface Props {
  state: PipelineState;
}

const BASE_STAGES = [
  { key: "extracting", label: "Extract text" },
  { key: "orienting", label: "Build orientation map" },
  { key: "dispatching", label: "Run parallel steps" },
  { key: "merging", label: "Merge cross-agent reports" },
  { key: "synthesizing", label: "Run sequential steps" },
  { key: "done", label: "Complete" },
];

function formatPassName(key: string): string {
  // Handle "merge/stepId" keys
  if (key.startsWith("merge/")) {
    const base = key.slice(6);
    return `Merge: ${base.charAt(0).toUpperCase() + base.slice(1)}`;
  }
  // Handle "stepId/agentName" keys
  const slash = key.indexOf("/");
  if (slash === -1) {
    // Capitalize the step ID as a fallback label
    return key.charAt(0).toUpperCase() + key.slice(1).replace(/_/g, " ");
  }
  const base = key.slice(0, slash);
  const agent = key.slice(slash + 1);
  const label = base.charAt(0).toUpperCase() + base.slice(1).replace(/_/g, " ");
  return `${label} (${agent.charAt(0).toUpperCase() + agent.slice(1)})`;
}

function stageIndex(kind: string, stages: typeof BASE_STAGES): number {
  const idx = stages.findIndex((s) => s.key === kind);
  return idx === -1 ? -1 : idx;
}

function StatusDot({ status }: { status: "done" | "active" | "pending" | "failed" }) {
  if (status === "done") {
    return (
      <div className="w-5 h-5 rounded-full bg-green-500 flex items-center justify-center">
        <svg className="w-3 h-3 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={3} d="M5 13l4 4L19 7" />
        </svg>
      </div>
    );
  }
  if (status === "failed") {
    return (
      <div className="w-5 h-5 rounded-full bg-red-500 flex items-center justify-center">
        <svg className="w-3 h-3 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={3} d="M6 18L18 6M6 6l12 12" />
        </svg>
      </div>
    );
  }
  if (status === "active") {
    return (
      <div className="w-5 h-5 rounded-full border-2 border-gray-600 flex items-center justify-center">
        <div className="w-2 h-2 rounded-full bg-gray-600 animate-pulse" />
      </div>
    );
  }
  return <div className="w-5 h-5 rounded-full border-2 border-gray-300" />;
}

function PassStatusIcon({ status }: { status: PassStatus }) {
  switch (status) {
    case "done":
      return <span className="text-green-500 text-xs">done</span>;
    case "running":
      return <span className="text-gray-500 dark:text-gray-400 text-xs animate-pulse">running</span>;
    case "error":
      return <span className="text-red-500 text-xs">failed</span>;
    default:
      return <span className="text-gray-300 dark:text-gray-600 text-xs">pending</span>;
  }
}

export default function PipelineProgress({ state }: Props) {
  const isError = state.kind === "error";
  const isCancelled = isError && state.message?.toLowerCase().includes("cancelled");
  const failedAtKey = isError ? (state.failedAt ?? "done") : state.kind;

  const mergeStageOrder = stageIndex("merging", BASE_STAGES);
  const currentBaseIdx = stageIndex(isError ? failedAtKey : state.kind, BASE_STAGES);
  const STAGES = BASE_STAGES.filter((s) => {
    if (s.key !== "merging") return true;
    // Show merge stage if we're currently merging or have passed it
    return state.kind === "merging" || currentBaseIdx > mergeStageOrder;
  });

  const currentIdx = stageIndex(isError ? failedAtKey : state.kind, STAGES);
  const failedIdx = isError ? stageIndex(failedAtKey, STAGES) : -1;

  return (
    <div className="space-y-1">
      <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">Progress</h3>
      <div className="space-y-3">
        {STAGES.map((stage, i) => {
          let status: "done" | "active" | "pending" | "failed";
          if (isError && i === failedIdx) {
            status = "failed";
          } else if (state.kind === "done" && stage.key === "done") {
            status = "done";
          } else if (i < currentIdx) {
            status = "done";
          } else if (i === currentIdx && !isError) {
            status = "active";
          } else {
            status = "pending";
          }

          return (
            <div key={stage.key}>
              <div className="flex items-center gap-2.5">
                <StatusDot status={status} />
                <span
                  className={`text-sm ${
                    status === "failed"
                      ? "text-red-600 dark:text-red-400 font-medium"
                      : status === "active"
                        ? "text-gray-900 dark:text-gray-100 font-medium"
                        : status === "done"
                          ? "text-gray-500 dark:text-gray-400"
                          : "text-gray-400 dark:text-gray-600"
                  }`}
                >
                  {stage.label}
                </span>
                {status === "failed" && (
                  <span className="text-red-500 dark:text-red-400 text-xs ml-auto">
                    {isCancelled ? "cancelled" : "failed"}
                  </span>
                )}
                {status === "active" && (stage.key === "extracting" || stage.key === "orienting" || stage.key === "merging") && (
                  <span className="text-gray-500 dark:text-gray-400 text-xs animate-pulse ml-auto">running...</span>
                )}
              </div>

              {/* Show individual pass statuses during dispatch, merge, or synthesize */}
              {((stage.key === "dispatching" && state.kind === "dispatching") ||
                (stage.key === "merging" && state.kind === "merging") ||
                (stage.key === "synthesizing" && state.kind === "synthesizing")) &&
                "passes" in state && Object.keys(state.passes).length > 0 && (
                  <div className="ml-7 mt-1.5 space-y-1">
                    {Object.entries(state.passes).map(([name, passStatus]) => (
                      <div
                        key={name}
                        className="flex items-center justify-between text-xs text-gray-500 dark:text-gray-400"
                      >
                        <span>{formatPassName(name)}</span>
                        <PassStatusIcon status={passStatus} />
                      </div>
                    ))}
                  </div>
                )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
