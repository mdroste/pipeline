import { useId } from "react";
import { muted, notice } from "../ui/classes";

export type AssistantAccess = "inspect" | "edit";

export const isAssistantAccess = (value: unknown): value is AssistantAccess =>
  value === "inspect" || value === "edit";

const choices: Array<[AssistantAccess, string, string]> = [
  [
    "inspect",
    "Read only",
    "Assistants read files in this folder and explain what they find.",
  ],
  ["edit", "Allow edits", "Assistants can create and change files here."],
];

/**
 * The project's default access for assistant conversations. A conversation
 * can still choose differently in its own Assistant settings.
 */
export default function AssistantAccessChoice({
  value,
  onChange,
  disabled = false,
  hint,
}: {
  value: AssistantAccess;
  onChange: (value: AssistantAccess) => void;
  disabled?: boolean;
  hint?: string;
}) {
  const group = useId();
  return (
    <fieldset className="space-y-1.5 text-xs" disabled={disabled}>
      <legend className="mb-1">Assistant access</legend>
      {choices.map(([id, label, description]) => (
        <label key={id} className="flex items-start gap-2">
          <input
            type="radio"
            name={group}
            value={id}
            checked={value === id}
            onChange={() => onChange(id)}
            className="mt-0.5"
          />
          <span>
            {label}
            <span className={`block ${muted}`}>{description}</span>
          </span>
        </label>
      ))}
      {value === "edit" && (
        <p role="note" className={notice}>
          Make sure this folder is backed up, for example with Git or Dropbox,
          so you can undo any changes an assistant makes to code or files.
        </p>
      )}
      {hint && <p className={muted}>{hint}</p>}
    </fieldset>
  );
}
