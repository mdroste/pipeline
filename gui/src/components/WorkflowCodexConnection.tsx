import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Status {
  epoch: number;
  unresolvedAttempts?: Array<{ id: string; label: string; state: string }>;
}

/** Review-specific recovery stays separate from the shared account controls. */
export default function WorkflowCodexConnection({
  onStatusChange,
}: {
  onStatusChange?: () => void;
}) {
  const [status, setStatus] = useState<Status | null>(null);
  const [reconciliation, setReconciliation] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const identity = useRef("");
  const refresh = useCallback(async () => {
    const next = await invoke<Status>("workflow_codex_status");
    setStatus(next);
  }, []);
  useEffect(() => {
    let live = true;
    const poll = () =>
      void invoke<Status>("workflow_codex_status")
        .then((next) => {
          if (live) {
            setStatus(next);
            setError(null);
          }
        })
        .catch((e) => {
          if (live) setError(String(e));
        });
    poll();
    const timer = window.setInterval(poll, 5000);
    return () => {
      live = false;
      window.clearInterval(timer);
    };
  }, []);
  useEffect(() => {
    if (!status) return;
    const next = JSON.stringify(
      (status.unresolvedAttempts ?? []).map((attempt) => attempt.id).sort(),
    );
    if (identity.current && identity.current !== next) onStatusChange?.();
    identity.current = next;
  }, [status, onStatusChange]);
  async function act(action: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await action();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  const button = "settings-button";
  if (!status?.unresolvedAttempts?.length && !error) return null;
  return (
    <div role="group" aria-label="Review recovery" className="space-y-2">
      {!!status?.unresolvedAttempts?.length && (
        <div className="space-y-2 text-xs">
          <p>
            An earlier attempt has no confirmed result. Check its saved result
            before allowing a new run. Previous usage still counts.
          </p>
          {status.unresolvedAttempts.map((attempt) => (
            <div key={attempt.id} className="space-y-1">
              <p>
                {attempt.label} — {attempt.state} ({attempt.id.slice(0, 8)})
              </p>
              <div className="flex gap-2">
                <button
                  type="button"
                  className={button}
                  disabled={busy}
                  onClick={() =>
                    void act(async () => {
                      const result = await invoke<{
                        status: string;
                        preview: string;
                        recordPath: string;
                      }>("workflow_codex_reconcile_attempt", {
                        id: attempt.id,
                        epoch: status.epoch,
                      });
                      setReconciliation(
                        `${result.status}\n${result.preview}\nSaved record: ${result.recordPath}`,
                      );
                    })
                  }
                >
                  Check saved result
                </button>
                <button
                  type="button"
                  className={button}
                  disabled={busy}
                  onClick={() =>
                    void act(async () => {
                      await invoke("workflow_codex_acknowledge_attempt", {
                        id: attempt.id,
                        epoch: status.epoch,
                      });
                    })
                  }
                >
                  Acknowledge and allow a new run
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
      {reconciliation && (
        <pre className="max-h-48 overflow-auto whitespace-pre-wrap text-xs">
          {reconciliation}
        </pre>
      )}
      {error && (
        <p role="alert" className="text-xs text-red-600 dark:text-red-400">
          {error}
        </p>
      )}
    </div>
  );
}
