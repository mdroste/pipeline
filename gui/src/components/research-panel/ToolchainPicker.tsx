import { useState } from "react";
import { executionClient } from "../../lib/workbench/executionClient";
import {
  toolsForAdapter,
  withDetectedProgram,
  type ExecutionAdapter,
  type ToolchainReport,
} from "../../lib/workbench/toolchains";
import { workbenchErrorMessage } from "../../lib/workbenchError";

type Props = {
  workspaceId: string;
  adapter: ExecutionAdapter;
  argv: string;
  setArgv: (value: string) => void;
  disabled: boolean;
  onError: (message: string) => void;
};

const NO_MATCH: Partial<Record<ExecutionAdapter, string>> = {
  stata:
    "No Stata installation was found in the standard folders or on PATH. Enter the path to Stata yourself, or use a custom launcher.",
  notebook:
    "Jupyter was not found. Install it in the project's environment, or enter its path yourself.",
};

/** Finds installed research tools and fills the program in the arguments. */
export default function ToolchainPicker({
  workspaceId,
  adapter,
  argv,
  setArgv,
  disabled,
  onError,
}: Props) {
  const [report, setReport] = useState<ToolchainReport | null>(null);
  const [detecting, setDetecting] = useState(false);
  if (adapter === "latex") return null;
  const matching = report ? toolsForAdapter(report, adapter) : [];
  const environmentFiles = report?.project?.files.map((file) => file.path);
  return (
    <div className="space-y-2">
      <button
        type="button"
        disabled={disabled || detecting}
        onClick={() => {
          setDetecting(true);
          executionClient
            .detectToolchains(workspaceId)
            .then(setReport)
            .catch((error) => onError(workbenchErrorMessage(error)))
            .finally(() => setDetecting(false));
        }}
        className="rounded border px-2 py-1 text-[11px]"
      >
        {detecting ? "Looking for installed tools…" : "Detect installed tools"}
      </button>
      {report && matching.length === 0 && (
        <p className="text-[11px] text-gray-500">
          {NO_MATCH[adapter] ??
            "No matching tool was found in the standard folders or on PATH. Enter its path yourself."}
        </p>
      )}
      {matching.length > 0 && (
        <ul aria-label="Detected tools" className="space-y-1">
          {matching.map((tool) => (
            <li
              key={`${tool.kind}:${tool.path}`}
              className="flex items-start justify-between gap-2 rounded border bg-white p-2 text-[11px] dark:bg-neutral-950"
            >
              <span className="min-w-0">
                <span className="font-medium">{tool.label}</span>
                {tool.version && (
                  <span className="text-gray-500"> · {tool.version}</span>
                )}
                <span className="block break-all font-mono text-[10px] text-gray-500">
                  {tool.path}
                </span>
                {tool.note && (
                  <span className="block text-[10px] text-gray-500">
                    {tool.note}
                  </span>
                )}
              </span>
              <button
                type="button"
                aria-label={`Use ${tool.label}`}
                onClick={() =>
                  setArgv(
                    withDetectedProgram(argv, adapter, tool, report!.platform),
                  )
                }
                className="shrink-0 rounded border px-2 py-1"
              >
                Use
              </button>
            </li>
          ))}
        </ul>
      )}
      {environmentFiles && environmentFiles.length > 0 && (
        <p className="text-[11px] text-gray-500">
          Environment files in this project: {environmentFiles.join(", ")}. Runs
          record their hashes so you can tell when the environment changed.
        </p>
      )}
      {report && <p className="text-[10px] text-gray-500">{report.boundary}</p>}
    </div>
  );
}
