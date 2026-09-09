import { useState } from "react";
import {
  useAppPreferences,
  saveAppPreferences,
  type AppPreferences,
} from "../../lib/appPreferences";
import type { ThemePreference } from "../../lib/theme";
import { isMac } from "../../lib/platform";
import { SectionHeader, Toggle, selectClass } from "./controls";
import { SettingRow, SaveFeedback } from "./SaveState";

export function usePreferenceEditor() {
  const preferences = useAppPreferences();
  const [feedback, setFeedback] = useState<{
    key?: string;
    saved?: boolean;
    error?: string;
  }>({});
  const update = <K extends keyof Omit<AppPreferences, "version">>(
    key: K,
    value: AppPreferences[K],
  ) => {
    try {
      saveAppPreferences({ [key]: value });
      setFeedback({ key, saved: true });
    } catch {
      setFeedback({
        key,
        error: "Could not save this preference on this device. Try again.",
      });
    }
  };
  return { preferences, update, feedback };
}

export function AppearanceSettings({
  theme,
  onThemeChange,
}: {
  theme: ThemePreference;
  onThemeChange: (theme: ThemePreference) => void;
}) {
  const { preferences: p, update, feedback } = usePreferenceEditor();
  const [themeError, setThemeError] = useState("");
  const scale = (
    key: "interfaceScale" | "reportScale",
    label: string,
    values: number[],
  ) => (
    <>
      <select
        aria-label={label}
        className={selectClass}
        value={p[key]}
        onChange={(e) => update(key, Number(e.target.value))}
      >
        {[...new Set([...values, p[key]])]
          .sort((a, b) => a - b)
          .map((value) => (
            <option key={value} value={value}>
              {value}%{value === 100 ? " · Default" : ""}
            </option>
          ))}
      </select>
      {feedback.key === key && <SaveFeedback {...feedback} />}
    </>
  );
  return (
    <>
      <SectionHeader title="Appearance & reading" />
      <SettingRow
        id="general-preferences"
        label="Appearance"
        description="System follows your device’s light or dark appearance."
      >
        <div
          className="settings-segmented"
          role="radiogroup"
          aria-label="Appearance"
        >
          {(["system", "light", "dark"] as const).map((value) => (
            <label key={value} className={theme === value ? "is-selected" : ""}>
              <input
                type="radio"
                name="appearance"
                value={value}
                checked={theme === value}
                onChange={() => {
                  try {
                    onThemeChange(value);
                    setThemeError("");
                  } catch {
                    setThemeError("Could not save appearance. Try again.");
                  }
                }}
              />
              {value[0].toUpperCase() + value.slice(1)}
            </label>
          ))}
        </div>
        <SaveFeedback error={themeError} />
      </SettingRow>
      <SettingRow
        id="interface-scale"
        label="Interface text size"
        description="Scales interface text and spacing across Pipeline."
      >
        {scale("interfaceScale", "Interface text size", [90, 100, 110, 125])}
      </SettingRow>
      <SettingRow
        id="report-scale"
        label="Default report text size"
        description="New reports start at this size. The reader’s controls can adjust an individual view."
      >
        {scale(
          "reportScale",
          "Default report text size",
          [85, 100, 110, 125, 150],
        )}
      </SettingRow>
      <SettingRow
        id="interface-density"
        label="Spacing"
        description="Comfortable gives controls more room; Compact fits more rows on screen."
      >
        <select
          aria-label="Spacing"
          className={selectClass}
          value={p.density}
          onChange={(e) =>
            update("density", e.target.value as AppPreferences["density"])
          }
        >
          <option value="comfortable">Comfortable</option>
          <option value="compact">Compact</option>
        </select>
        {feedback.key === "density" && <SaveFeedback {...feedback} />}
      </SettingRow>
      <div className="settings-reading-preview" aria-label="Reading preview">
        <strong>A comfortable place to work</strong>
        <p>Read a paper, review the evidence, and return to your writing.</p>
      </div>
    </>
  );
}

export function EditorSettings() {
  const { preferences: p, update, feedback } = usePreferenceEditor();
  return (
    <>
      <SectionHeader
        title="Editor"
        description="Defaults for source files and manuscript editors."
      />
      <SettingRow
        id="editor-font"
        label="Editor font size"
        description="Applied immediately to open source editors."
      >
        <select
          aria-label="Editor font size"
          className={selectClass}
          value={p.editorFontSize}
          onChange={(e) => update("editorFontSize", Number(e.target.value))}
        >
          {Array.from({ length: 14 }, (_, i) => i + 11).map((n) => (
            <option key={n} value={n}>
              {n} px
            </option>
          ))}
        </select>
        {feedback.key === "editorFontSize" && <SaveFeedback {...feedback} />}
      </SettingRow>
      <div id="editor-wrap" className="settings-anchor">
        <Toggle
          label="Wrap long lines"
          description="New source editors wrap lines to fit. You can override this in each editor."
          checked={p.editorWrap}
          onChange={(value) => update("editorWrap", value)}
        />
        {feedback.key === "editorWrap" && <SaveFeedback {...feedback} />}
      </div>
    </>
  );
}

export function StartupSettings() {
  const { preferences: p, update, feedback } = usePreferenceEditor();
  return (
    <>
      <SectionHeader title="Startup" />
      <SettingRow
        id="startup-page"
        label="When Pipeline opens"
        description="Choose a fresh starting point or return to the last project or conversation."
      >
        <select
          aria-label="When Pipeline opens"
          className={selectClass}
          value={p.startup}
          onChange={(e) =>
            update("startup", e.target.value as AppPreferences["startup"])
          }
        >
          <option value="restore">Continue where I left off</option>
          <option value="home">Show Home overview</option>
        </select>
        <SaveFeedback {...feedback} />
      </SettingRow>
    </>
  );
}

export function ConversationBehavior() {
  const { preferences: p, update, feedback } = usePreferenceEditor();
  return (
    <>
      <SectionHeader title="Messages" />
      <SettingRow
        id="send-shortcut"
        label="Send message with"
        description="Shift+Enter always inserts a new line. The Send button is always available."
      >
        <select
          aria-label="Send message with"
          className={selectClass}
          value={p.sendShortcut}
          onChange={(e) =>
            update(
              "sendShortcut",
              e.target.value as AppPreferences["sendShortcut"],
            )
          }
        >
          <option value="enter">Enter</option>
          <option value="mod-enter">{isMac ? "Command" : "Ctrl"}+Enter</option>
        </select>
        <SaveFeedback {...feedback} />
      </SettingRow>
    </>
  );
}
