import { useEffect, useState } from "react";
import type { Settings } from "../../lib/types";
import { Field, inputClass } from "./controls";
import { useSettingsOperation } from "./SaveState";

/**
 * A saved key belongs to one server, and the host erases it when the address
 * changes. Settings autosave, so the address is committed only when editing
 * ends, and a change that would erase a key has to be confirmed.
 */
export default function LocalServerUrl({
  settings,
  setSettings,
}: {
  settings: Settings;
  setSettings: (settings: Settings) => void;
}) {
  const saved = settings.local_base_url;
  const [draft, setDraft] = useState(saved);
  const [confirming, setConfirming] = useState(false);
  useEffect(() => {
    setDraft(saved);
    setConfirming(false);
  }, [saved]);
  const changed = draft !== saved;
  useSettingsOperation({ pending: changed });
  const commit = () => {
    if (!changed) return;
    if (settings.local_api_key) setConfirming(true);
    else setSettings({ ...settings, local_base_url: draft });
  };
  const revert = () => {
    setDraft(saved);
    setConfirming(false);
  };
  return (
    <Field label="Server URL">
      <input
        aria-label="Local Server URL"
        type="text"
        value={draft}
        onChange={(event) => {
          setDraft(event.target.value);
          setConfirming(false);
        }}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return;
          if (event.key === "Enter") commit();
          else if (event.key === "Escape") revert();
        }}
        placeholder="http://localhost:11434/v1"
        className={`${inputClass} font-mono`}
        autoComplete="off"
        spellCheck={false}
      />
      {confirming && (
        <div role="alert" className="mt-2 text-sm">
          <p>
            The saved API key belongs to the current server, so changing the
            address removes it. You can then add a key for the new server.
          </p>
          <div className="mt-2 flex flex-wrap gap-2">
            <button
              type="button"
              className="settings-button-primary"
              onClick={() =>
                setSettings({
                  ...settings,
                  local_base_url: draft,
                  local_api_key: "",
                })
              }
            >
              Change server and remove key
            </button>
            <button type="button" className="settings-button" onClick={revert}>
              Keep current server
            </button>
          </div>
        </div>
      )}
    </Field>
  );
}
