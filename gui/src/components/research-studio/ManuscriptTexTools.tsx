// Discover once per open build sheet; explicit choices survive later detection.
import {
  useEffect,
  useState,
  useRef,
  type Dispatch,
  type SetStateAction,
} from "react";
import { executionClient } from "../../lib/workbench/executionClient";
import type { DetectedToolchain } from "../../lib/workbench/toolchains";
import type { BuildConfig } from "../../lib/studioClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { Text } from "./shared";
import { Fold } from "../../ui";

export default function ManuscriptTexTools({
  workspaceId,
  config,
  setConfig,
  saved,
}: {
  workspaceId: string;
  config: BuildConfig;
  setConfig: Dispatch<SetStateAction<BuildConfig>>;
  saved: boolean;
}) {
  const savedChoice = useRef(saved);
  savedChoice.current = saved;
  const [tools, setTools] = useState<DetectedToolchain[]>([]);
  const [status, setStatus] = useState("Looking for TeX…");
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let live = true;
    setStatus("Looking for TeX…");
    void executionClient
      .detectToolchains(workspaceId)
      .then((report) => {
        if (!live) return;
        const found = report.tools.filter((t) => t.adapter === "latex");
        setTools(found);
        setStatus(
          found.length
            ? "Installed TeX tools"
            : "No TeX installation found. Install TeX or enter its executable path below, then detect again.",
        );
        if (!savedChoice.current)
          setConfig((current) => {
            if (
              current.executable ||
              current.advancedArgv ||
              current.engine !== config.engine
            )
              return current;
            const matching =
              found.find((t) => engineName(t.path) === current.engine) ??
              (attempt === 0 && current.engine === "latexmk"
                ? found[0]
                : undefined);
            return matching
              ? {
                  ...current,
                  engine: engineName(matching.path) ?? current.engine,
                  executable: matching.path,
                }
              : current;
          });
      })
      .catch((error) => {
        if (live) setStatus(workbenchErrorMessage(error));
      });
    return () => {
      live = false;
    };
  }, [workspaceId, attempt]);
  const matching = tools.filter((t) => engineName(t.path) === config.engine);
  return (
    <div className="space-y-2">
      <p className="text-xs text-ink-muted" role="status">
        {matching.some((t) => t.path === config.executable)
          ? `${config.engine} detected · ${matching.find((t) => t.path === config.executable)?.version ?? "Ready"}`
          : status}
      </p>
      <Fold title="TeX installation">
        {matching.map((tool) => (
          <button
            type="button"
            className="file-action"
            key={tool.path}
            onClick={() => setConfig((c) => ({ ...c, executable: tool.path }))}
          >
            Use {tool.path}
            {tool.version ? ` (${tool.version})` : ""}
          </button>
        ))}
        <Text
          label="TeX executable path (optional)"
          value={config.executable ?? ""}
          onChange={(executable) =>
            setConfig((c) => ({ ...c, executable: executable || null }))
          }
        />
        <button
          type="button"
          className="file-action"
          onClick={() => setAttempt((n) => n + 1)}
        >
          Detect TeX again
        </button>
      </Fold>
    </div>
  );
}

function engineName(path: string) {
  return path
    .replaceAll("\\", "/")
    .split("/")
    .pop()
    ?.replace(/\.exe$/i, "")
    .toLowerCase();
}
