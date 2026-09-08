import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

import type { Settings } from "../../lib/types";

import { useProviderCatalogs } from "./useProviderCatalogs";
const AUTOSAVE_DELAY_MS = 400;
export function useSettingsDraft(onSystemChange?: () => void) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [savedSettingsSnapshot, setSavedSettingsSnapshot] = useState<
    string | null
  >(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [warnings, setWarnings] = useState<string[]>([]);
  const settingsRef = useRef<Settings | null>(settings);
  const saveChainRef = useRef<Promise<void>>(Promise.resolve());
  const latestQueuedSnapshotRef = useRef<string | null>(null);
  const latestSaveSequenceRef = useRef(0);
  settingsRef.current = settings;
  const dirty =
    settings !== null &&
    savedSettingsSnapshot !== null &&
    JSON.stringify(settings) !== savedSettingsSnapshot;
  const providerCatalogs = useProviderCatalogs(settings);
  // Clear the "saved" indicator after 2 seconds, with proper cleanup
  useEffect(() => {
    if (!saved) return;
    const timer = setTimeout(() => setSaved(false), 2000);
    return () => clearTimeout(timer);
  }, [saved]);

  useEffect(() => {
    invoke<{ settings: Settings; warnings: string[] }>("get_settings")
      .then((resp) => {
        providerCatalogs.initialize(resp.settings);
        setSettings(resp.settings);
        const snapshot = JSON.stringify(resp.settings);
        latestQueuedSnapshotRef.current = snapshot;
        setSavedSettingsSnapshot(snapshot);
        setWarnings(resp.warnings);
        setLoading(false);
      })
      .catch((e) => {
        console.error("Failed to load settings:", e);
        setLoadError(e instanceof Error ? e.message : String(e));
        setLoading(false);
      });
  }, []);

  const enqueueSave = (settingsToSave: Settings, force = false) => {
    const savedSnapshot = JSON.stringify(settingsToSave);
    if (!force && latestQueuedSnapshotRef.current === savedSnapshot) return;
    latestQueuedSnapshotRef.current = savedSnapshot;
    const saveSequence = latestSaveSequenceRef.current + 1;
    latestSaveSequenceRef.current = saveSequence;
    setSaving(true);
    setSaved(false);
    setSaveError(null);

    // Serialize saves so a slower, older request can never overwrite a newer
    // edit. Each queued request still captures an immutable settings snapshot.
    saveChainRef.current = saveChainRef.current
      .catch(() => undefined)
      .then(async () => {
        try {
          await invoke("save_settings", { settings: settingsToSave });
          setSavedSettingsSnapshot(savedSnapshot);
          setWarnings([]);
          setSaved(JSON.stringify(settingsRef.current) === savedSnapshot);
          providerCatalogs.didSave(settingsToSave);
          onSystemChange?.();
        } catch (e) {
          console.error("Failed to save settings:", e);
          if (latestSaveSequenceRef.current === saveSequence) {
            setSaveError(e instanceof Error ? e.message : String(e));
          }
        } finally {
          if (latestSaveSequenceRef.current === saveSequence) {
            setSaving(false);
          }
        }
      });
  };

  useEffect(() => {
    if (!settings || !dirty) return;
    setSaved(false);
    setSaveError(null);
    const snapshot = JSON.stringify(settings);
    if (latestQueuedSnapshotRef.current === snapshot) return;
    const timer = window.setTimeout(
      () => enqueueSave(settings),
      AUTOSAVE_DELAY_MS,
    );
    return () => window.clearTimeout(timer);
  }, [dirty, settings]);

  const retrySave = () => {
    if (settings) enqueueSave(settings, true);
  };

  return {
    settings,
    setSettings,
    loading,
    loadError,
    saving,
    saved,
    dirty,
    saveError,
    warnings,
    retrySave,
    providerCatalogs,
  };
}
