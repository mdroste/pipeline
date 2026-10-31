// Setup is automatic after enabling a tool; failures wait for an explicit retry.
import { useCallback, useEffect, useRef, useState } from "react";
import {
  toolSetupClient,
  type ToolSetupStatus,
} from "../../lib/workbench/toolSetupClient";

export default function ToolSetup({
  kind,
  disabled,
}: {
  kind: "browser" | "shell";
  disabled: boolean;
}) {
  const [status, setStatus] = useState<ToolSetupStatus | null>(null);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const attempted = useRef(false);
  const mounted = useRef(false);
  const label = kind === "browser" ? "Browser" : "Commands";
  useEffect(() => {
    mounted.current = true;
    void toolSetupClient
      .status(kind)
      .then((value) => {
        if (mounted.current) setStatus(value);
      })
      .catch((reason) => {
        if (mounted.current) setError(String(reason));
      });
    return () => {
      mounted.current = false;
    };
  }, [kind]);
  const install = useCallback(async () => {
    attempted.current = true;
    setInstalling(true);
    setError(null);
    try {
      const value = await toolSetupClient.install(kind);
      if (mounted.current) setStatus(value);
    } catch (reason) {
      if (mounted.current) setError(String(reason));
    } finally {
      if (mounted.current) setInstalling(false);
    }
  }, [kind]);
  useEffect(() => {
    if (
      status &&
      !status.installed &&
      status.platformAvailable !== false &&
      !disabled &&
      !attempted.current
    )
      void install();
  }, [status, disabled, install]);
  useEffect(() => {
    if (!installing) return;
    const timer = window.setInterval(() => {
      void toolSetupClient
        .status(kind)
        .then((value) => {
          if (mounted.current) setStatus(value);
        })
        .catch(() => {});
    }, 750);
    return () => window.clearInterval(timer);
  }, [installing, kind]);
  const unsupported = status?.platformAvailable === false;
  return (
    <div className="rounded border border-line p-3 text-ui-meta">
      <p role="status">
        {installing
          ? (status?.setupPhase ?? `Preparing ${label.toLowerCase()}…`)
          : status?.installed
            ? `${label} ready. Pipeline manages setup and updates.`
            : unsupported
              ? kind === "shell"
                ? "General commands require macOS, or Linux with bubblewrap and socat. Configured research commands remain available in Command settings."
                : "Browser setup is unavailable on this platform."
              : `Pipeline downloads and prepares ${label.toLowerCase()} automatically when needed.`}
      </p>
      {installing && (
        <button
          className="mt-2 rounded border px-2 py-1"
          onClick={() => {
            void toolSetupClient
              .cancel()
              .catch((reason) => setError(String(reason)));
          }}
        >
          Cancel setup
        </button>
      )}
      {error && (
        <>
          <p role="alert" className="mt-2 text-danger">
            {error}
          </p>
          <button
            className="mt-2 rounded border px-2 py-1"
            disabled={disabled || installing || unsupported}
            onClick={() => void install()}
          >
            Retry {label.toLowerCase()} setup
          </button>
        </>
      )}
    </div>
  );
}
