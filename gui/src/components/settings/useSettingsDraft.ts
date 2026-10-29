import { settingsClient } from "../../lib/settingsClient";
import { useState, useEffect, useRef } from "react";

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
  const [undecryptableKeys, setUndecryptableKeys] = useState<string[]>([]);
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

  const load = () =>
    settingsClient
      .load()
      .then((resp) => {
        providerCatalogs.initialize(resp.settings);
        setSettings(resp.settings);
        const snapshot = JSON.stringify(resp.settings);
        latestQueuedSnapshotRef.current = snapshot;
        setSavedSettingsSnapshot(snapshot);
        setWarnings(resp.warnings);
        setUndecryptableKeys(resp.undecryptable_keys ?? []);
        setLoading(false);
      })
      .catch((e) => {
        console.error("Failed to load settings:", e);
        setLoadError(e instanceof Error ? e.message : String(e));
        setLoading(false);
      });
  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // A stored key that no longer decrypts blocks every run and cannot be
  // cleared by saving an empty field; discarding it is an explicit action.
  const discardStoredKey = async (provider: string) => {
    try {
      // Share the save queue, but refresh diagnostics only. Reloading the
      // complete settings object would overwrite edits made during this request.
      const operation = saveChainRef.current
        .catch(() => undefined)
        .then(async () => {
          await settingsClient.discardSecret({ provider });
          const response = await settingsClient.load();
          setWarnings(response.warnings);
          setUndecryptableKeys(response.undecryptable_keys ?? []);
        });
      saveChainRef.current = operation.catch(() => undefined);
      await operation;
    } catch (cause) {
      setSaveError(cause instanceof Error ? cause.message : String(cause));
    }
  };

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
          await settingsClient.save({ settings: settingsToSave });
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
    undecryptableKeys,
    discardStoredKey,
    retrySave,
    providerCatalogs,
  };
}
