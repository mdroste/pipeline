import { useEffect, useState } from "react";
import { cliToolsClient, type CliToolsStatus } from "../../lib/cliToolsClient";
import { SectionHeader, SettingsCard } from "./controls";

export default function CommandLineSettings() {
  const [status, setStatus] = useState<CliToolsStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    void cliToolsClient.status().then(
      (next) => {
        if (live) setStatus(next);
      },
      (failure) => {
        if (live) setError(String(failure));
      },
    );
    return () => {
      live = false;
    };
  }, []);
  const perform = async (action: () => Promise<CliToolsStatus>) => {
    setBusy(true);
    setError("");
    try {
      setStatus(await action());
    } catch (failure) {
      setError(String(failure));
    } finally {
      setBusy(false);
    }
  };
  return (
    <SettingsCard id="command-line-tools">
      <SectionHeader
        title="Command-line tools"
        description="Run Pipeline from a terminal or script. Optional; the desktop app works without installing this command."
      />
      <p className="settings-row-description">
        Uses this app’s engine, PDF tools, and installed models. Installs
        immediately without another download and stays current with the app.
        Existing bundled command paths continue to work.
      </p>
      {error && (
        <p role="alert" className="settings-error">
          {error}
        </p>
      )}
      {status?.reason && (
        <p className="settings-row-description">{status.reason}</p>
      )}
      <div className="settings-actions mt-4">
        {!status ? (
          <button
            className="settings-button"
            disabled={busy}
            onClick={() => void perform(cliToolsClient.status)}
          >
            Check command status
          </button>
        ) : (
          <>
            <button
              className="settings-button"
              disabled={busy || !status.available}
              onClick={() => void perform(cliToolsClient.install)}
            >
              {busy
                ? "Working…"
                : status.installed
                  ? "Repair command"
                  : "Install command"}
            </button>
            {status.installed && (
              <button
                className="settings-button"
                disabled={busy}
                onClick={() => void perform(cliToolsClient.uninstall)}
              >
                Uninstall command
              </button>
            )}
          </>
        )}
      </div>
      {status?.installed && (
        <div className="mt-4 space-y-3">
          <p role="status" className="settings-row-description">
            Command installed. Your research data and models are shared with the
            app.
          </p>
          <label className="block text-sm">
            Try it in a terminal
            <input
              readOnly
              className="ui-input mt-1 w-full font-mono"
              value={status.command}
              onFocus={(event) => event.currentTarget.select()}
            />
          </label>
          <label className="block text-sm">
            Enable pipeline-cli in this terminal
            <input
              readOnly
              className="ui-input mt-1 w-full font-mono"
              value={status.terminal_setup}
              onFocus={(event) => event.currentTarget.select()}
            />
          </label>
          <p className="settings-row-description">
            To keep the command available in new terminals, add its folder to
            your user PATH. Uninstall removes this user command; existing
            app-bundled commands and research data stay available.
          </p>
        </div>
      )}
    </SettingsCard>
  );
}
