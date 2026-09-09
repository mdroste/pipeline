import { useEffect, useState } from "react";
import { useSettingsOperation } from "./SaveState";

/** Keep incomplete input local. Only a committed, valid number reaches autosave. */
export default function ValidatedNumber({
  value,
  onChange,
  label,
  min = 0,
  max = Number.MAX_SAFE_INTEGER,
  step = 1,
}: {
  value: number;
  onChange: (value: number) => void;
  label: string;
  min?: number;
  max?: number;
  step?: number;
}) {
  const [draft, setDraft] = useState(String(value));
  const [error, setError] = useState("");
  useEffect(() => {
    setDraft(String(value));
  }, [value]);
  useSettingsOperation({ pending: draft !== String(value) });
  const commit = () => {
    if (draft === String(value)) return;
    const number = Number(draft);
    if (
      !draft.trim() ||
      !Number.isFinite(number) ||
      number < min ||
      number > max ||
      Math.abs((number - min) / step - Math.round((number - min) / step)) > 1e-7
    ) {
      setError(
        `Enter a number from ${min} to ${max}${step === 1 ? " without decimals" : ""}.`,
      );
      return;
    }
    setError("");
    setDraft(String(number));
    onChange(number);
  };
  return (
    <div>
      <input
        aria-label={label}
        aria-invalid={Boolean(error)}
        type="number"
        min={min}
        max={max}
        step={step}
        value={draft}
        onChange={(event) => {
          setDraft(event.target.value);
          setError("");
        }}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") commit();
          if (event.key === "Escape") {
            setDraft(String(value));
            setError("");
          }
        }}
        className="settings-number"
      />
      {error && (
        <p role="alert" className="settings-error">
          {error}
        </p>
      )}
    </div>
  );
}
