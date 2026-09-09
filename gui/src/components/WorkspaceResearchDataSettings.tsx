import { lazy, Suspense, useState } from "react";
import { workbenchErrorMessage } from "../lib/workbenchError";
const Release = lazy(() => import("./WorkspaceReleasePanel"));
const Retention = lazy(() => import("./WorkspaceStorageRetention"));

/** Whole-store operations are available without choosing a project or chat. */
export default function WorkspaceResearchDataSettings({
  onSavingChange,
  expanded = false,
  mode = "data",
}: {
  onSavingChange?: (saving: boolean) => void;
  expanded?: boolean;
  mode?: "data" | "diagnostics";
}) {
  const [open, setOpen] = useState(expanded);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const action = async (operation: () => Promise<void>) => {
    if (busy) return;
    setBusy(true);
    onSavingChange?.(true);
    setError(null);
    try {
      await operation();
    } catch (cause) {
      setError(workbenchErrorMessage(cause));
    } finally {
      setBusy(false);
      onSavingChange?.(false);
    }
  };
  return (
    <section
      id={mode === "data" ? "workspace-research-data" : "research-diagnostics"}
      tabIndex={-1}
      className={mode === "data" ? "settings-card settings-anchor" : "mt-5"}
    >
      <h2 className="text-base font-semibold">
        {mode === "data"
          ? "Research backups & cleanup"
          : "Research diagnostics"}
      </h2>
      <p className="mt-2 text-sm text-gray-500">
        Backups and retention apply to all projects and conversations on this
        device.
      </p>
      {!expanded && (
        <button
          type="button"
          disabled={busy}
          className="mt-3 rounded border px-3 py-2 text-sm"
          aria-expanded={open}
          onClick={() => setOpen((value) => !value)}
        >
          Manage research data
        </button>
      )}
      {error && (
        <p role="alert" className="mt-3 text-sm text-red-600">
          {error}
        </p>
      )}
      {open && (
        <Suspense
          fallback={
            <p className="mt-4 text-sm">Opening research data settings…</p>
          }
        >
          <div className="mt-5 space-y-6">
            {mode === "data" && (
              <>
                <Release
                  view="backup"
                  selectedPaper={null}
                  busy={busy}
                  onAction={(operation) => void action(operation)}
                  onError={setError}
                />
                <Retention
                  busy={busy}
                  onAction={(operation) => void action(operation)}
                />
              </>
            )}
            {mode === "diagnostics" && (
              <div>
                <Release
                  view="diagnostics"
                  selectedPaper={null}
                  busy={busy}
                  onAction={(operation) => void action(operation)}
                  onError={setError}
                />
              </div>
            )}
          </div>
        </Suspense>
      )}
    </section>
  );
}
