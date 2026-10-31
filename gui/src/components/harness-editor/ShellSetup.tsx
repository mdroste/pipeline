import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Status {
  installed: boolean;
  nodeAvailable: boolean;
  version: string;
  platformAvailable: boolean;
}
export default function ShellSetup({ disabled }: { disabled: boolean }) {
  const [status, setStatus] = useState<Status | null>(null);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    void invoke<Status>("workbench_shell_status")
      .then((value) => {
        if (live) setStatus(value);
      })
      .catch((reason) => {
        if (live) setError(String(reason));
      });
    return () => {
      live = false;
    };
  }, []);
  const install = async () => {
    setInstalling(true);
    setError(null);
    try {
      setStatus(await invoke<Status>("workbench_shell_install"));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setInstalling(false);
    }
  };
  return (
    <div className="rounded border border-line p-3 text-ui-meta">
      <div>
        {status?.installed
          ? "Command engine installed. Commands follow this conversation’s access settings."
          : "Install the command engine to run sandboxed commands with API and local models. This downloads a small package to this device."}
      </div>
      {status && !status.nodeAvailable && (
        <p>Install Node.js 22.12 or later, then restart Pipeline.</p>
      )}
      {status && !status.platformAvailable && (
        <p>Commands require macOS, or Linux with bubblewrap and socat.</p>
      )}
      {!status?.installed && (
        <button
          className="mt-2 rounded border px-2 py-1"
          disabled={
            disabled ||
            installing ||
            !status?.nodeAvailable ||
            !status?.platformAvailable
          }
          onClick={() => void install()}
        >
          {installing ? "Installing command engine…" : "Install command engine"}
        </button>
      )}
      {error && (
        <p role="alert" className="mt-2 text-danger">
          {error}
        </p>
      )}
    </div>
  );
}
