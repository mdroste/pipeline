import type { Settings } from "../../lib/types";

import type { ThemePreference } from "../../lib/theme";

import { selectClass, SectionHeader, Field, Toggle } from "./controls";
export function GeneralSection({
  settings,
  setSettings,
  theme,
  onThemeChange,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
  theme: ThemePreference;
  onThemeChange: (theme: ThemePreference) => void;
}) {
  return (
    <>
      <SectionHeader title="Appearance & diagnostics" />

      <div className="space-y-5">
        <Field
          label="Appearance"
          help="Applied immediately, with no save needed. System follows your operating system's light or dark appearance."
        >
          <select
            aria-label="Appearance"
            value={theme}
            onChange={(event) =>
              onThemeChange(event.target.value as ThemePreference)
            }
            className={selectClass}
          >
            <option value="system">System</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
        </Field>
        <Toggle
          label="Verbose console logging"
          description="Show full LLM subprocess output in the console (commands, stdout, stderr). Useful for debugging."
          checked={settings.verbose_logging}
          onChange={(v) => setSettings({ ...settings, verbose_logging: v })}
        />
      </div>
    </>
  );
}
