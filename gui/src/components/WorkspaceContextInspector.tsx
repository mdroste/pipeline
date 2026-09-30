import { useEffect, useState } from "react";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { EffectiveHarness } from "../lib/workbenchTypes";
import Spinner from "../ui/Spinner";

/** The honesty surface: exactly what the assistant will see for this
 * conversation — instructions, enabled tools, and the assembled context. */
export default function WorkspaceContextInspector({
  sessionId,
  workspaceName,
  onClose,
  onOpenSettings,
}: {
  sessionId: string | null;
  workspaceName: string | null;
  onClose: () => void;
  onOpenSettings: () => void;
}) {
  const [harness, setHarness] = useState<EffectiveHarness | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!sessionId) return;
    let stale = false;
    setHarness(null);
    workbenchClient
      .effectiveHarness(sessionId)
      .then((loaded) => {
        if (!stale) setHarness(loaded);
      })
      .catch((cause) => {
        if (!stale) setError(workbenchErrorMessage(cause));
      });
    return () => {
      stale = true;
    };
  }, [sessionId]);
  const enabled = harness?.moduleAvailability?.filter((m) => m.available) ?? [];
  const unavailable =
    harness?.moduleAvailability?.filter((m) => !m.available) ?? [];
  return (
    <section
      className="workspace-simple-inspector"
      aria-label="Conversation context"
    >
      <div className="workspace-inspector-heading">
        <h2>What the assistant sees</h2>
        <button type="button" onClick={onClose} aria-label="Close inspector">
          ×
        </button>
      </div>
      <p>{workspaceName ?? "Unfiled conversation"}</p>
      {!sessionId ? (
        <p className="text-xs text-gray-500">
          Start a conversation to inspect its context.
        </p>
      ) : error ? (
        <p role="alert" className="text-xs text-red-600 dark:text-red-400">
          {error}
        </p>
      ) : !harness ? (
        <Spinner inline label="Assembling context…" />
      ) : (
        <div className="space-y-3 overflow-y-auto text-xs">
          {enabled.length > 0 && (
            <div>
              <h3 className="font-semibold">Enabled tools</h3>
              <ul className="mt-1 list-disc pl-4 text-gray-600 dark:text-gray-300">
                {enabled.map((m) => (
                  <li key={m.id}>{m.id}</li>
                ))}
              </ul>
            </div>
          )}
          {unavailable.length > 0 && (
            <div>
              <h3 className="font-semibold">Not available</h3>
              <ul className="mt-1 list-disc pl-4 text-gray-500 dark:text-gray-400">
                {unavailable.map((m) => (
                  <li key={m.id}>
                    {m.id}
                    {m.reasons?.length ? ` — ${m.reasons.join("; ")}` : ""}
                  </li>
                ))}
              </ul>
            </div>
          )}
          <div>
            <h3 className="font-semibold">
              Assembled context
              {harness.contextTruncated ? " (truncated to budget)" : ""}
            </h3>
            {harness.contextPreview ? (
              <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap rounded border border-gray-200 bg-gray-50 p-2 font-mono text-[11px] leading-4 text-gray-700 dark:border-gray-800 dark:bg-gray-900 dark:text-gray-300">
                {harness.contextPreview}
              </pre>
            ) : (
              <p className="mt-1 text-gray-500">
                No project material is attached to this conversation yet. Use
                Sources above your message to add some.
              </p>
            )}
          </div>
          <button type="button" onClick={onOpenSettings}>
            Instructions and tools
          </button>
        </div>
      )}
    </section>
  );
}
