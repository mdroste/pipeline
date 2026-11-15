// Native command availability is checked without downloading a runtime.
import { useCallback, useEffect, useState } from "react";
import {
  toolSetupClient,
  type ToolSetupStatus,
} from "../../lib/workbench/toolSetupClient";
import ReportIssueLink from "../ReportIssueLink";

export default function ShellSetup({ disabled }: { disabled: boolean }) {
  const [status, setStatus] = useState<ToolSetupStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const check = useCallback(async () => {
    setChecking(true);
    setError(null);
    try {
      setStatus(await toolSetupClient.status());
    } catch (reason) {
      setError(String(reason));
    } finally {
      setChecking(false);
    }
  }, []);
  useEffect(() => {
    void check();
  }, [check]);
  return (
    <div className="rounded border border-line p-3 text-ui-meta">
      <p role="status">
        {checking
          ? "Checking commands…"
          : status?.available
            ? "Commands ready. Included with Pipeline."
            : (status?.reason ?? "Checking command availability…")}
      </p>
      {error && (
        <p role="alert" className="mt-2 text-danger">
          {error}
          <ReportIssueLink />
        </p>
      )}
      {(error || status?.available === false) && (
        <button
          className="mt-2 rounded border px-2 py-1"
          disabled={disabled || checking}
          onClick={() => void check()}
        >
          Check again
        </button>
      )}
    </div>
  );
}
