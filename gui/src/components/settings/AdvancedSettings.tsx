import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import type { Settings, UpdateInfo, UpdateStatus } from "../../lib/types";
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
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus | null>(null);
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
    invoke<UpdateStatus>("update_status")
      .then((v) => {
        if (live) setUpdateStatus(v ?? null);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);
  // A fresh manual result takes precedence over the saved one.
  const known = update ?? updateStatus?.info ?? null;
  const checkedAt = known?.checked_at ?? updateStatus?.last_checked_at ?? null;
  const openPage = (url: string) =>
    void open(url).catch(() => setError("Could not open the release page."));
  const saveAutomatic = async (enabled: boolean) => {
    setError("");
    try {
      setUpdateStatus(
        await invoke<UpdateStatus>("update_set_automatic", { enabled }),
      );
    } catch {
      setError("Could not save the update setting. Try again.");
    }
  };
  return (
    <>
      <SettingsCard id="advanced-logging">
        <SectionHeader title="Diagnostics" />
        <Toggle
          label="Verbose console logging"
          description="Show full commands, responses, and logs in the console for troubleshooting."
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
        {updateStatus && (
          <Toggle
            label="Check for updates automatically"
            description={
              updateStatus.automatic_available
                ? "Once a day, Pipeline asks GitHub whether a newer version is available. GitHub receives your network address and app version, nothing about your research."
                : "This copy of Pipeline was not installed from an official release, so it checks for updates only when you ask."
            }
            checked={updateStatus.automatic}
            onChange={(value) => void saveAutomatic(value)}
          />
        )}
        {updateStatus?.error && (
          <p role="alert" className="settings-error">
            Your update setting could not be read, so automatic checks are off.
            Choose a setting to replace it.
          </p>
        )}
        <div className="settings-actions mt-4">
          <button
            className="settings-button"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              setError("");
              try {
                setUpdate(await invoke<UpdateInfo>("check_for_update"));
              } catch (failure) {
                setError(
                  typeof failure === "string" && failure
                    ? failure
                    : "Could not check for updates. Check your connection and try again.",
                );
              } finally {
                setBusy(false);
              }
            }}
          >
            {busy ? "Checking…" : "Check for updates"}
          </button>
          {known?.update_available && (
            <>
              <button
                className="settings-button"
                onClick={() =>
                  openPage(known.download_url ?? known.release_url)
                }
              >
                Download version {known.latest}
              </button>
              <button
                className="settings-button"
                onClick={() => openPage(known.release_url)}
              >
                View release notes
              </button>
            </>
          )}
        </div>
        {(update || known?.update_available) && (
          <p role="status" className="settings-inline-status">
            {known?.update_available
              ? `Version ${known.latest} is available.`
              : "You’re up to date."}
          </p>
        )}
        {checkedAt && (
          <p className="settings-row-description">
            Last checked{" "}
            {new Date(checkedAt).toLocaleString(undefined, {
              dateStyle: "medium",
              timeStyle: "short",
            })}
            .
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
