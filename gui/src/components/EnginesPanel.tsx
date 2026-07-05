import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type { EngineStatus } from "../lib/types";

type PhaseStatus = "running" | "done" | "failed";

const PHASES: { id: string; label: string }[] = [
  { id: "runtime", label: "Runtime" },
  { id: "packages", label: "Packages" },
  { id: "models", label: "Models" },
];

/** Cap the streamed install log kept in memory. */
const MAX_LOG_LINES = 200;

/**
 * Installable local extraction engines. Everything lands under ~/.pipeline/
 * and is removed by Uninstall; nothing touches system Python.
 */
export default function EnginesPanel() {
  const [engines, setEngines] = useState<EngineStatus[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [phases, setPhases] = useState<Record<string, PhaseStatus>>({});
  const [logLines, setLogLines] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const logRef = useRef<HTMLPreElement>(null);

  const refresh = useCallback(() => {
    invoke<EngineStatus[]>("list_engines")
      .then(setEngines)
      .catch((e) => console.error("Failed to list engines:", e));
  }, []);

  const openPipelineDir = useCallback(async () => {
    try {
      await invoke("open_pipeline_dir");
    } catch (e) {
      console.error("Failed to open ~/.pipeline:", e);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    // Catch immediately so a non-Tauri environment (tests) can't produce
    // unhandled rejections; cleanup tolerates the null.
    const unlistens: Promise<UnlistenFn | null>[] = [
      listen<{ engine: string; phase: string; status: string }>(
        "engines:phase",
        (e) => {
          setPhases((p) => ({
            ...p,
            [e.payload.phase]: e.payload.status as PhaseStatus,
          }));
        },
      ).catch(() => null),
      listen<{ line: string }>("engines:log", (e) => {
        setLogLines((l) => [...l.slice(-(MAX_LOG_LINES - 1)), e.payload.line]);
      }).catch(() => null),
    ];
    return () => {
      unlistens.forEach((u) => u.then((f) => f && f()));
    };
  }, []);

  useEffect(() => {
    if (logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
    }
  }, [logLines]);

  const install = async (id: string) => {
    setBusy(id);
    setError(null);
    setPhases({});
    setLogLines([]);
    try {
      await invoke("install_engine", { engineId: id });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
      refresh();
    }
  };

  const uninstall = async (id: string) => {
    if (!window.confirm("Uninstall this engine? Downloaded model weights will be removed.")) {
      return;
    }
    setBusy(id);
    setError(null);
    setPhases({});
    setLogLines([]);
    try {
      await invoke("uninstall_engine", { engineId: id });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
      refresh();
    }
  };

  const cancel = () => {
    invoke("cancel_engine_install").catch(() => {});
  };

  // An install started elsewhere (e.g. before this panel mounted).
  const backendBusy = engines.some((e) => e.installing);

  return (
    <div className="mt-8">
      <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100 mb-1">
        Local Engines
      </h4>
      <p className="text-xs text-gray-500 dark:text-gray-400 mb-3">
        Installs into{" "}
        <button
          type="button"
          onClick={openPipelineDir}
          className="font-mono text-blue-600 dark:text-blue-400 hover:underline"
          title="Open this folder"
        >
          ~/.pipeline/
        </button>
      </p>

      <div className="space-y-3">
        {engines.map((engine) => {
          const isBusy = busy === engine.id || (backendBusy && busy === null);
          return (
            <div
              key={engine.id}
              className="rounded-lg border border-gray-200 dark:border-gray-700 p-4 space-y-3"
            >
              <div className="flex items-center justify-between gap-3">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-semibold text-gray-900 dark:text-gray-100">
                    {engine.label}
                  </span>
                  {engine.installed ? (
                    <span className="text-[10px] uppercase tracking-wider font-medium px-1.5 py-0.5 rounded bg-green-700 text-white dark:bg-green-600">
                      installed{engine.version ? ` v${engine.version}` : ""}
                    </span>
                  ) : (
                    <span className="text-[10px] uppercase tracking-wider font-medium px-1.5 py-0.5 rounded bg-gray-200 text-gray-600 dark:bg-gray-700 dark:text-gray-300">
                      not installed
                    </span>
                  )}
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  {isBusy ? (
                    <button
                      onClick={cancel}
                      className="text-xs py-1.5 px-3 rounded-lg border border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
                    >
                      Cancel
                    </button>
                  ) : engine.installed ? (
                    <button
                      onClick={() => uninstall(engine.id)}
                      className="text-xs py-1.5 px-3 rounded-lg border border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
                    >
                      Uninstall
                    </button>
                  ) : (
                    <button
                      onClick={() => install(engine.id)}
                      className="text-xs py-1.5 px-3 rounded-lg bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 hover:bg-gray-800 dark:hover:bg-gray-200 transition-colors"
                    >
                      Install (~{(engine.est_download_mb / 1000).toFixed(1)} GB)
                    </button>
                  )}
                </div>
              </div>

              <p className="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">
                {engine.description}
              </p>

              {!engine.installed && engine.system_path && (
                <p className="text-xs text-amber-700 dark:text-amber-400 bg-amber-50 dark:bg-amber-950/50 border border-amber-200 dark:border-amber-900 rounded-lg px-2.5 py-2">
                  A system copy was found at{" "}
                  <code className="font-mono break-all">{engine.system_path}</code>.
                  Extraction uses it until you install a managed copy here.
                </p>
              )}
              {engine.installed && engine.system_path && (
                <p className="text-xs text-gray-400 dark:text-gray-500">
                  A system copy at{" "}
                  <code className="font-mono break-all">{engine.system_path}</code>{" "}
                  is ignored in favor of this managed install.
                </p>
              )}

              {engine.installed && engine.managed_stack_mb > 0 && (
                <p className="text-xs text-gray-400 dark:text-gray-500">
                  Disk usage: {(engine.managed_stack_mb / 1000).toFixed(1)} GB
                </p>
              )}

              {isBusy && (
                <div className="flex items-center gap-2">
                  {PHASES.map((phase) => {
                    const status = phases[phase.id];
                    const chip =
                      status === "done"
                        ? "bg-green-100 text-green-700 dark:bg-green-900/50 dark:text-green-300"
                        : status === "running"
                          ? "bg-blue-100 text-blue-700 dark:bg-blue-900/50 dark:text-blue-300 animate-pulse"
                          : status === "failed"
                            ? "bg-red-100 text-red-700 dark:bg-red-900/50 dark:text-red-300"
                            : "bg-gray-100 text-gray-400 dark:bg-gray-800 dark:text-gray-500";
                    return (
                      <span
                        key={phase.id}
                        className={`text-[10px] uppercase tracking-wider font-medium px-1.5 py-0.5 rounded ${chip}`}
                      >
                        {phase.label}
                      </span>
                    );
                  })}
                </div>
              )}

              {(isBusy || error) && logLines.length > 0 && (
                <pre
                  ref={logRef}
                  className="text-[11px] leading-relaxed font-mono bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-700 rounded-lg p-2.5 max-h-40 overflow-y-auto whitespace-pre-wrap"
                >
                  {logLines.join("\n")}
                </pre>
              )}
            </div>
          );
        })}
      </div>

      {error && (
        <p className="mt-3 text-xs text-red-600 dark:text-red-400" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
