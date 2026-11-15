import IconButton from "../ui/IconButton";
import { Icon } from "../ui/icons";
import { useEffect, useState } from "react";
import { harnessClient } from "../lib/workbench/harnessClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { EffectiveHarness } from "../lib/workbench/harnessTypes";
import Spinner from "../ui/Spinner";
import ReportIssueLink from "./ReportIssueLink";

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
    harnessClient
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
        <IconButton onClick={onClose} label="Close inspector">
          <Icon name="close" />
        </IconButton>
      </div>
      <p>{workspaceName ?? "Unfiled conversation"}</p>
      {!sessionId ? (
        <p className="text-ui-meta text-ink-muted">
          Start a conversation to inspect its context.
        </p>
      ) : error ? (
        <p role="alert" className="text-ui-label text-danger">
          {error}
          <ReportIssueLink />
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
              <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap rounded-ui-sm border border-line bg-sunken p-2 font-mono text-ui-meta leading-relaxed text-ink">
                {harness.contextPreview}
              </pre>
            ) : (
              <p className="mt-1 text-gray-500">
                No project material is attached to this conversation yet. Choose
                + (Add to message) in the message box to add some.
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
