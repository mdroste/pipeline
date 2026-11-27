import { useId, useState } from "react";
import {
  useAppPreferences,
  saveAppPreferences,
  type AppPreferences,
} from "../../lib/appPreferences";
import {
  THEME_OPTIONS,
  themeOption,
  type ThemePreference,
} from "../../lib/theme";
import { openGetStarted } from "../../lib/getStarted";
import { TYPEFACE_OPTIONS, typefaceOption } from "../../lib/typefaces";
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
  const fieldId = useId();
  const isCustom = themeOption(theme).colorTheme !== "default";
  const [customMenuOpen, setCustomMenuOpen] = useState(isCustom);
  const showCustomMenu = customMenuOpen || isCustom;
  const [themeError, setThemeError] = useState("");
  const changeTheme = (value: ThemePreference, closeMenu = false) => {
    try {
      onThemeChange(value);
      if (closeMenu) setCustomMenuOpen(false);
      setThemeError("");
    } catch {
      setThemeError("Could not save theme. Try again.");
    }
  };
  const scale = (
    key: "interfaceScale" | "reportScale",
    label: string,
    values: number[],
  ) => (
    <>
      <select
        id={`${fieldId}-${key}`}
        aria-describedby={
          key === "reportScale" ? `${fieldId}-${key}-help` : undefined
        }
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
      {feedback.key === key && <SaveFeedback error={feedback.error} />}
    </>
  );
  return (
    <>
      <SectionHeader title="Appearance & reading" />
      <SettingRow
        id="general-preferences"
        label="Appearance"
        description="System follows your device’s appearance using the default colors."
      >
        <div
          className="settings-segmented"
          role="radiogroup"
          aria-label="Appearance"
        >
          {(["system", "light", "dark", "custom"] as const).map((value) => (
            <label
              key={value}
              className={
                (showCustomMenu ? value === "custom" : theme === value)
                  ? "is-selected"
                  : ""
              }
            >
              <input
                type="radio"
                name={`${fieldId}-appearance`}
                value={value}
                checked={showCustomMenu ? value === "custom" : theme === value}
                onChange={() => {
                  if (value === "custom") {
                    setCustomMenuOpen(true);
                    setThemeError("");
                  } else {
                    changeTheme(value, true);
                  }
                }}
              />
              {value[0].toUpperCase() + value.slice(1)}
            </label>
          ))}
        </div>
        {showCustomMenu && (
          <div className="mt-2">
            <label className="sr-only" htmlFor={`${fieldId}-custom-theme`}>
              Custom color scheme
            </label>
            <select
              id={`${fieldId}-custom-theme`}
              className={selectClass}
              value={theme}
              onChange={(event) =>
                changeTheme(event.target.value as ThemePreference)
              }
            >
              {THEME_OPTIONS.filter(
                (option) => option.colorTheme === "default",
              ).map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
              <optgroup label="Custom themes">
                {THEME_OPTIONS.filter(
                  (option) => option.colorTheme !== "default",
                ).map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </optgroup>
            </select>
          </div>
        )}
        <SaveFeedback error={themeError} />
      </SettingRow>
      <SettingRow
        id="interface-typeface"
        controlId={`${fieldId}-typeface`}
        label="Typeface"
        description={typefaceOption(p.typeface).description}
      >
        <select
          id={`${fieldId}-typeface`}
          aria-describedby={`${fieldId}-typeface-help`}
          className={selectClass}
          value={p.typeface}
          onChange={(event) =>
            update("typeface", typefaceOption(event.target.value).value)
          }
        >
          {TYPEFACE_OPTIONS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
              {option.value === "inter" ? " · Default" : ""}
            </option>
          ))}
        </select>
        {feedback.key === "typeface" && <SaveFeedback error={feedback.error} />}
      </SettingRow>
      <SettingRow
        id="interface-scale"
        controlId={`${fieldId}-interfaceScale`}
        label="Interface text size"
      >
        {scale("interfaceScale", "Interface text size", [90, 100, 110, 125])}
      </SettingRow>
      <SettingRow
        id="report-scale"
        controlId={`${fieldId}-reportScale`}
        label="Default report text size"
        description="Default for new reports; adjustable while reading."
      >
        {scale(
          "reportScale",
          "Default report text size",
          [85, 100, 110, 125, 150],
        )}
      </SettingRow>
      <SettingRow
        id="interface-density"
        controlId={`${fieldId}-density`}
        label="Spacing"
      >
        <select
          id={`${fieldId}-density`}
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
        {feedback.key === "density" && <SaveFeedback error={feedback.error} />}
      </SettingRow>
    </>
  );
}

export function EditorSettings() {
  const { preferences: p, update, feedback } = usePreferenceEditor();
  return (
    <>
      <SectionHeader title="Editor" />
      <SettingRow id="editor-font" label="Editor font size">
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
          description="Default for new editors; adjustable in each editor."
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
      <SettingRow id="startup-page" label="When Pipeline opens">
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
      <SettingRow
        id="guided-setup"
        label="Guided setup"
        description="Check your connections and installed tools, and open an example review."
      >
        <button
          type="button"
          className="settings-button"
          onClick={() => openGetStarted("setup")}
        >
          Set up Pipeline
        </button>
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
        description="Shift+Enter starts a new line."
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
        {feedback.key === "sendShortcut" && <SaveFeedback {...feedback} />}
      </SettingRow>
      <div id="composer-controls" className="settings-anchor">
        <Toggle
          label="Combine model settings in one menu"
          description="Shows provider, model, thinking and tool approvals in a single menu in the message box instead of separate selectors."
          checked={p.composerControls === "combined"}
          onChange={(value) =>
            update("composerControls", value ? "combined" : "separate")
          }
        />
        {feedback.key === "composerControls" && <SaveFeedback {...feedback} />}
      </div>
    </>
  );
}
