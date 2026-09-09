import { useEffect, useState } from "react";
import { inputClass } from "./controls";
import { useSettingsOperation, ReviewSaveFeedback } from "./SaveState";

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
  const [submitted, setSubmitted] = useState(false);
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
          <p className="settings-row-description">
            {optional ? "Optional bearer token. " : ""}Stored encrypted. Changes
            are sent only after you save.
          </p>
          <div className="settings-actions">
            <button
              type="button"
              className="settings-button-primary"
              disabled={draft === value}
              onClick={() => {
                onSave(draft.trim());
                setDraft(draft.trim());
                setSubmitted(true);
                setEditing(false);
              }}
            >
              Save {label}
            </button>
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
          </div>
        </>
      ) : (
        <div className="settings-credential-summary">
          <span>
            {value
              ? "API key configured · encrypted"
              : optional
                ? "No API key · optional"
                : "API key not configured"}
          </span>
          <button
            type="button"
            className="settings-button"
            onClick={() => setEditing(true)}
          >
            {value ? "Edit" : "Add"} {label}
          </button>
        </div>
      )}
      {submitted && <ReviewSaveFeedback />}
    </div>
  );
}
