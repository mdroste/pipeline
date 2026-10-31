import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirmDialog } from "./DialogService";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type { EngineDownloadProgress, EngineStatus } from "../lib/types";
import EngineInstallStatus, { type PhaseStatus } from "./EngineInstallStatus";
import InfoButton from "./InfoButton";

/** Cap the streamed install log kept in memory. */
const MAX_LOG_LINES = 200;

/** Status poll while an install started before this panel mounted runs. */
const BACKGROUND_POLL_MS = 2500;

/**
 * Installable native extraction engines. Everything lands under
 * ~/.pipeline/native/ and is removed by Uninstall.
 */
export default function EnginesPanel({
  onSystemChange,
  onEngineStatusChange,
}: {
  onSystemChange?: () => void;
  onEngineStatusChange?: (engines: EngineStatus[]) => void;
}) {
  const [engines, setEngines] = useState<EngineStatus[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [phases, setPhases] = useState<Record<string, PhaseStatus>>({});
  const [download, setDownload] = useState<EngineDownloadProgress | null>(null);
  const [logLines, setLogLines] = useState<string[]>([]);
  const [uninstalling, setUninstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [listenersReady, setListenersReady] = useState(false);
  const [listenerError, setListenerError] = useState<string | null>(null);
  const [listenerAttempt, setListenerAttempt] = useState(0);
  const [engineStatusError, setEngineStatusError] = useState<string | null>(
    null,
  );
  const [refreshingStatus, setRefreshingStatus] = useState(true);
  const [openDirError, setOpenDirError] = useState<string | null>(null);
  const [openingPipelineDir, setOpeningPipelineDir] = useState(false);
  const refreshGeneration = useRef(0);
  const backgroundInstalls = useRef(new Set<string>());
  const cancelRequested = useRef(false);

  const refresh = useCallback(() => {
    const generation = ++refreshGeneration.current;
    setRefreshingStatus(true);
    setEngineStatusError(null);

    const engineRequest = invoke<EngineStatus[]>("list_engines");

    void engineRequest.then(
      (next) => {
        if (refreshGeneration.current !== generation) return;
        setEngines(next);
        onEngineStatusChange?.(next);
        const activeProgress = next.find(
          (engine) => engine.installing && engine.install_progress,
        )?.install_progress;
        if (activeProgress) {
          setPhases(activeProgress.phases as Record<string, PhaseStatus>);
          setDownload(activeProgress.download ?? null);
          setLogLines(activeProgress.log_lines.slice(-MAX_LOG_LINES));
        }
        setEngineStatusError(null);
      },
      (reason) => {
        if (refreshGeneration.current !== generation) return;
        setEngines([]);
        onEngineStatusChange?.([]);
        const detail =
          reason instanceof Error ? reason.message : String(reason);
        setEngineStatusError(
          `Local engine status could not be loaded: ${detail}`,
        );
      },
    );
    void Promise.allSettled([engineRequest]).then(() => {
      if (refreshGeneration.current === generation) setRefreshingStatus(false);
    });
  }, [onEngineStatusChange]);

  const openPipelineDir = useCallback(async () => {
    setOpenDirError(null);
    setOpeningPipelineDir(true);
    try {
      await invoke("open_pipeline_dir");
    } catch (e) {
      const detail = e instanceof Error ? e.message : String(e);
      setOpenDirError(`Could not open ~/.pipeline/: ${detail}`);
    } finally {
      setOpeningPipelineDir(false);
    }
  }, []);

  useEffect(() => {
    refresh();
    return () => {
      refreshGeneration.current += 1;
    };
  }, [refresh]);

  useEffect(() => {
    let live = true;
    let unlistens: UnlistenFn[] = [];
    setListenersReady(false);
    setListenerError(null);
    void Promise.allSettled([
      listen<{ engine: string; phase: string; status: string }>(
        "engines:phase",
        (e) => {
          setPhases((p) => ({
            ...p,
            [e.payload.phase]: e.payload.status as PhaseStatus,
          }));
        },
      ),
      listen<{ download: EngineDownloadProgress | null }>(
        "engines:download",
        (e) => setDownload(e.payload.download),
      ),
      listen<{ line: string }>("engines:log", (e) => {
        setLogLines((l) => [...l.slice(-(MAX_LOG_LINES - 1)), e.payload.line]);
      }),
    ]).then((registrations) => {
      const registered = registrations.flatMap((registration) =>
        registration.status === "fulfilled" ? [registration.value] : [],
      );
      const failed = registrations.find(
        (registration): registration is PromiseRejectedResult =>
          registration.status === "rejected",
      );
      if (!live || failed) {
        registered.forEach((unlisten) => unlisten());
        if (live && failed) {
          setListenerError(
            `Engine progress connection failed: ${
              failed.reason instanceof Error
                ? failed.reason.message
                : String(failed.reason)
            }. Retry before installing or removing an engine.`,
          );
        }
        return;
      }
      unlistens = registered;
      setListenersReady(true);
    });
    return () => {
      live = false;
      unlistens.forEach((unlisten) => unlisten());
    };
  }, [listenerAttempt]);

  const install = async (id: string) => {
    cancelRequested.current = false;
    setBusy(id);
    setError(null);
    setPhases({});
    setDownload(null);
    setLogLines([]);
    try {
      await invoke("install_engine", { engineId: id });
      onSystemChange?.();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
      setDownload(null);
      refresh();
    }
  };

  const uninstall = async (id: string) => {
    if (
      !(await confirmDialog(
        "Uninstall this parser and remove its installed software and model files?",
        {
          title: "Uninstall engine",
          confirmLabel: "Uninstall",
          destructive: true,
        },
      ))
    ) {
      return;
    }
    setBusy(id);
    setUninstalling(true);
    setError(null);
    setPhases({});
    setDownload(null);
    setLogLines([]);
    try {
      await invoke("uninstall_engine", { engineId: id });
      onSystemChange?.();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
      setUninstalling(false);
      refresh();
    }
  };

  const cancel = async () => {
    setError(null);
    cancelRequested.current = true;
    try {
      await invoke("cancel_engine_install");
    } catch (caught) {
      const detail = caught instanceof Error ? caught.message : String(caught);
      setError(`Engine cancellation failed: ${detail}`);
    }
  };

  // An install started elsewhere (e.g. before this panel mounted).
  const backendBusy = engines.some((e) => e.installing);
  const watchingBackground = backendBusy && busy === null;

  // The backend reports no completion event, so a remounted panel polls the
  // engine list until the install it inherited ends.
  useEffect(() => {
    if (!watchingBackground) return;
    const timer = window.setInterval(refresh, BACKGROUND_POLL_MS);
    return () => window.clearInterval(timer);
  }, [refresh, watchingBackground]);

  useEffect(() => {
    if (busy !== null) return;
    const finished: EngineStatus[] = [];
    for (const engine of engines) {
      if (engine.installing) backgroundInstalls.current.add(engine.id);
      else if (backgroundInstalls.current.delete(engine.id))
        finished.push(engine);
    }
    if (!finished.length) return;
    onSystemChange?.();
    const failed = finished.find((engine) => !engine.installed);
    if (failed && !cancelRequested.current)
      setError(
        `${failed.label} was not installed. The installation stopped before it finished; install again to retry.`,
      );
    cancelRequested.current = false;
  }, [busy, engines, onSystemChange]);

  return (
    <div>
      <div className="mb-3 flex items-center gap-1.5">
        <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">
          Local Engines
        </h4>
        <InfoButton label="Local Engines">
          Managed engines are installed under{" "}
          <button
            type="button"
            onClick={openPipelineDir}
            disabled={openingPipelineDir}
            aria-busy={openingPipelineDir || undefined}
            className="font-mono text-blue-600 hover:underline dark:text-blue-400"
          >
            ~/.pipeline/
          </button>
          .
        </InfoButton>
      </div>

      {openDirError && (
        <p role="alert" className="mb-3 text-xs text-red-700 dark:text-red-400">
          {openDirError}
        </p>
      )}

      {engineStatusError && (
        <div
          role="alert"
          className="mb-3 flex items-start gap-3 rounded-lg border border-red-200 bg-red-50 p-3 text-xs text-red-700 dark:border-red-900 dark:bg-red-950/40 dark:text-red-300"
        >
          <span className="flex-1">{engineStatusError}</span>
          <button
            type="button"
            onClick={refresh}
            className="shrink-0 rounded border border-red-300 px-2 py-1 font-medium dark:border-red-800"
          >
            Retry status
          </button>
        </div>
      )}

      {listenerError && (
        <div
          role="alert"
          className="mb-3 flex items-start gap-3 rounded-lg border border-red-200 bg-red-50 p-3 text-xs text-red-700 dark:border-red-900 dark:bg-red-950/40 dark:text-red-300"
        >
          <span className="flex-1">{listenerError}</span>
          <button
            type="button"
            onClick={() => setListenerAttempt((attempt) => attempt + 1)}
            className="shrink-0 rounded border border-red-300 px-2 py-1 font-medium dark:border-red-800"
          >
            Retry connection
          </button>
        </div>
      )}

      <div className="space-y-3">
        {engines.map((engine) => {
          const isBusy = busy === engine.id || (backendBusy && busy === null);
          const available = engine.available !== false;
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
                  {!available ? (
                    <span className="text-[10px] uppercase tracking-wider font-medium px-1.5 py-0.5 rounded bg-amber-100 text-amber-800 dark:bg-amber-900/50 dark:text-amber-200">
                      unavailable
                    </span>
                  ) : engine.installed ? (
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
                      type="button"
                      onClick={cancel}
                      className="text-xs py-1.5 px-3 rounded-lg border border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
                    >
                      Cancel
                    </button>
                  ) : engine.installed ? (
                    <button
                      type="button"
                      onClick={() => uninstall(engine.id)}
                      disabled={!listenersReady}
                      className="text-xs py-1.5 px-3 rounded-lg border border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
                    >
                      Uninstall
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() => install(engine.id)}
                      disabled={!listenersReady || !available}
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

              {!available && engine.unavailable_reason && (
                <p className="text-xs text-amber-700 dark:text-amber-300 leading-relaxed">
                  {engine.unavailable_reason}
                </p>
              )}

              {engine.installed && engine.managed_stack_mb > 0 && (
                <p className="text-xs text-gray-500 dark:text-gray-400">
                  Disk usage: {(engine.managed_stack_mb / 1000).toFixed(1)} GB
                </p>
              )}

              {uninstalling && isBusy ? (
                <p
                  className="text-xs text-gray-500 dark:text-gray-400"
                  role="status"
                >
                  Removing {engine.label}…
                </p>
              ) : (
                <>
                  {isBusy && (
                    <p
                      className="text-xs text-blue-700 dark:text-blue-300"
                      role="status"
                    >
                      Installation continues in the background if you leave
                      Settings.
                    </p>
                  )}
                  {(isBusy || (error && logLines.length > 0)) && (
                    <EngineInstallStatus
                      active={isBusy}
                      phases={phases}
                      download={download}
                      logLines={logLines}
                    />
                  )}
                </>
              )}
            </div>
          );
        })}
      </div>

      {refreshingStatus && engines.length === 0 && !engineStatusError && (
        <p role="status" className="text-xs text-gray-500 dark:text-gray-400">
          Loading local engine status…
        </p>
      )}
      {!refreshingStatus && !engineStatusError && engines.length === 0 && (
        <p className="text-xs text-gray-500 dark:text-gray-400">
          No local parsers are available to install.
        </p>
      )}

      {error && (
        <p className="mt-3 text-xs text-red-600 dark:text-red-400" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
