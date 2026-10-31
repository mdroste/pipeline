import { useEffect, useState } from "react";
import { inputClass } from "./controls";
import { useSettingsOperation } from "./SaveState";

export default function CredentialField({
  label,
  value,
  onSave,
  optional = false,
}: {
  label: string;
  value: string;
  onSave: (value: string) => void;
  optional?: boolean;
}) {
  const [editing, setEditing] = useState(!value);
  const [draft, setDraft] = useState(value);
  useEffect(() => {
    setDraft(value);
  }, [value]);
  useSettingsOperation({ pending: draft !== value });
  return (
    <div className="settings-credential">
      {editing ? (
        <>
          <label
            className="settings-row-label"
            htmlFor={`credential-${label.replaceAll(" ", "-")}`}
          >
            {label}
          </label>
          <div className="settings-credential-entry">
            <input
              id={`credential-${label.replaceAll(" ", "-")}`}
              aria-label={label}
              type="password"
              className={inputClass}
              value={draft}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setDraft(event.target.value)}
            />
            <button
              type="button"
              className="settings-button-primary"
              aria-label={`Save ${label}`}
              disabled={draft === value}
              onClick={() => {
                onSave(draft.trim());
                setDraft(draft.trim());
                setEditing(false);
              }}
            >
              Save
            </button>
            {(value || draft) && (
              <button
                type="button"
                className="settings-button"
                onClick={() => {
                  setDraft(value);
                  setEditing(false);
                }}
              >
                Cancel
              </button>
            )}
          </div>
          <p className="settings-row-description">
            {optional ? "Only needed if your server requires a key. " : ""}
            Stored encrypted.
          </p>
        </>
      ) : (
        <div className="settings-credential-summary">
          <span>
            {value
              ? "API key saved · encrypted"
              : optional
                ? "No API key · optional"
                : "No API key saved"}
          </span>
          <button
            type="button"
            className="settings-button"
            aria-label={`${value ? "Edit" : "Add"} ${label}`}
            onClick={() => setEditing(true)}
          >
            {value ? "Edit" : "Add"}
          </button>
        </div>
      )}
    </div>
  );
}
