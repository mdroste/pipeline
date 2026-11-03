import { useState } from "react";
import { inputClass } from "./controls";
import { useSettingsOperation } from "./SaveState";

/** `value` only says whether a key is saved. A saved key reaches the renderer as
 *  an opaque marker, which must never become part of what is typed here. */
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
  const [draft, setDraft] = useState("");
  useSettingsOperation({ pending: draft !== "" });
  const finish = (saved?: string) => {
    if (saved !== undefined) onSave(saved);
    setDraft("");
    setEditing(false);
  };
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
              placeholder={value ? "Enter a new key" : undefined}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setDraft(event.target.value)}
            />
            <button
              type="button"
              className="settings-button-primary"
              aria-label={`Save ${label}`}
              disabled={!draft.trim()}
              onClick={() => finish(draft.trim())}
            >
              Save
            </button>
            {(value || draft) && (
              <button
                type="button"
                className="settings-button"
                onClick={() => finish()}
              >
                Cancel
              </button>
            )}
            {value && (
              <button
                type="button"
                className="settings-button"
                aria-label={`Remove ${label}`}
                onClick={() => finish("")}
              >
                Remove
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
