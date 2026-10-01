import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import type { Settings, UpdateInfo } from "../../lib/types";
import {
  projectClient,
  type ProjectCapabilities,
} from "../../lib/projectClient";
import { SectionHeader, SettingsCard, Toggle } from "./controls";
import { ReviewSaveFeedback } from "./SaveState";

export default function AdvancedSettings({
  settings,
  setSettings,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
}) {
  const [version, setVersion] = useState("");
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [capabilities, setCapabilities] = useState<ProjectCapabilities | null>(
    null,
  );
  useEffect(() => {
    let live = true;
    getVersion()
      .then((v) => {
        if (live) setVersion(v);
      })
      .catch(() => {});
    projectClient
      .capabilities()
      .then((v) => {
        if (live) setCapabilities(v);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);
  return (
    <>
      <SettingsCard id="advanced-logging">
        <SectionHeader title="Diagnostics" />
        <Toggle
          label="Verbose console logging"
          description="Show complete model subprocess output, including commands and responses, in the console. Use when troubleshooting."
          checked={settings.verbose_logging}
          onChange={(value) =>
            setSettings({ ...settings, verbose_logging: value })
          }
        />
        <ReviewSaveFeedback />
      </SettingsCard>
      <SettingsCard id="workspace-diagnostics">
        <SectionHeader title="Research capabilities" />
        {capabilities ? (
          <>
            <p className="settings-row-description">
              {capabilities.qualification}
            </p>
            <dl className="settings-capabilities">
              {[
                ["Platform", capabilities.platform],
                ["Git", capabilities.git ?? "Not found"],
                ["LaTeX", capabilities.latex ?? "Not found"],
                [
                  "PDF tools",
                  capabilities.pdfPages ?? "No system PDF renderer found",
                ],
                ["Stata", capabilities.stataPolicy],
              ].map(([key, value]) => (
                <div key={key}>
                  <dt>{key}</dt>
                  <dd>{value}</dd>
                </div>
              ))}
            </dl>
          </>
        ) : (
          <p className="settings-row-description">
            Capability information is unavailable.
          </p>
        )}
      </SettingsCard>
      <SettingsCard id="app-version">
        <SectionHeader
          title="Pipeline"
          description={version ? `Version ${version}` : "Version unavailable"}
        />
        <p className="settings-row-description">
          A research workspace for conversations, reviews, and automations.
        </p>
        <div className="settings-actions mt-4">
          <button
            className="settings-button"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              setError("");
              try {
                setUpdate(await invoke<UpdateInfo>("check_for_update"));
              } catch {
                setError(
                  "Could not check for updates. Check your connection and try again.",
                );
              } finally {
                setBusy(false);
              }
            }}
          >
            {busy ? "Checking…" : "Check for updates"}
          </button>
          {update?.update_available && (
            <button
              className="settings-button"
              onClick={() =>
                void open(update.release_url).catch(() =>
                  setError("Could not open release notes."),
                )
              }
            >
              View release notes
            </button>
          )}
        </div>
        {update && (
          <p role="status" className="settings-inline-status">
            {update.update_available
              ? `Version ${update.latest} is available.`
              : "You’re up to date."}
          </p>
        )}
        {error && (
          <p role="alert" className="settings-error">
            {error}
          </p>
        )}
      </SettingsCard>
    </>
  );
}
