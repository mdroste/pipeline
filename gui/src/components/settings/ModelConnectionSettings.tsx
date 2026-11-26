import { useId } from "react";
import {
  connectionAvailability,
  connectionDescription,
  connections,
  reviewConnectionId,
  type ConnectionId,
  type ProviderFamily,
} from "../../lib/connections";
import { withAccessMode } from "../../lib/providers";
import type { DepsReport, Settings } from "../../lib/settingsTypes";
import { selectClass } from "./controls";
import { focusSettingsTarget } from "./navigation";

export const SETTINGS_PROVIDERS: Record<
  ProviderFamily,
  { label: string; target: string }
> = {
  claude: { label: "Anthropic", target: "anthropic-provider" },
  codex: { label: "OpenAI", target: "openai-provider" },
  antigravity: { label: "Google", target: "google-provider" },
  local: { label: "Local / custom", target: "local-provider" },
};

const PRESENTATION: Record<
  ConnectionId,
  { label: string; description: string }
> = {
  claude: {
    label: "Claude Code · Subscription",
    description: "Uses your Claude Code subscription sign-in.",
  },
  "claude-code-api": {
    label: "Claude Code · Console billing",
    description:
      "Uses your Console sign-in in Claude Code. API usage is billed to that account.",
  },
  "anthropic-api": {
    label: "Direct API · API key",
    description: "Uses the Anthropic API key saved in Pipeline.",
  },
  codex: {
    label: "ChatGPT account",
    description: "Uses your ChatGPT account connection.",
  },
  "openai-api": {
    label: "OpenAI API",
    description:
      "Uses the OpenAI API key saved in Pipeline. API usage is billed separately.",
  },
  "google-api": {
    label: "Gemini API",
    description:
      "Uses the Google API key saved in Pipeline. API usage is billed separately.",
  },
  local: {
    label: "Local / custom server",
    description: "Uses the server and model configured below.",
  },
};

/** Settings-only labels; saved connection IDs and runtime names stay unchanged. */
export function ModelConnectionField({
  settings,
  provider,
  onChange,
  dependencies,
  ariaLabel,
  showSetup = false,
}: {
  settings: Settings;
  provider: ProviderFamily;
  onChange: (connection: ConnectionId) => void;
  dependencies?: DepsReport | null;
  ariaLabel: string;
  showSetup?: boolean;
}) {
  const id = useId();
  const selected = reviewConnectionId(settings, provider);
  const { label, description } = PRESENTATION[selected];
  const choices = connections.filter(
    (connection) => connection.family === provider,
  );
  const missing = (connection: ConnectionId) =>
    dependencies !== undefined &&
    connectionAvailability(connection, settings, dependencies) === "missing";
  return (
    <div className="min-w-0 text-xs">
      {choices.length > 1 ? (
        <label className="flex min-w-0 flex-col gap-1 text-gray-600 dark:text-neutral-400">
          Connection
          <select
            aria-label={ariaLabel}
            aria-describedby={`${id}-description`}
            title={label}
            className={selectClass}
            value={selected}
            onChange={(event) => onChange(event.target.value as ConnectionId)}
          >
            {choices.map((connection) => (
              <option key={connection.id} value={connection.id}>
                {PRESENTATION[connection.id].label}
                {missing(connection.id) ? " (not connected)" : ""}
              </option>
            ))}
          </select>
        </label>
      ) : (
        <p className="settings-row-label">{label}</p>
      )}
      <p id={`${id}-description`} className="settings-row-description mt-1">
        {description}
        {missing(selected) && (
          <span className="block text-warning">Not connected.</span>
        )}
      </p>
      {showSetup && (
        <button
          type="button"
          className="mt-1 underline"
          onClick={() =>
            focusSettingsTarget(SETTINGS_PROVIDERS[provider].target)
          }
        >
          Set up connection
        </button>
      )}
    </div>
  );
}

/** The default owns its connection picker; other providers remain configurable. */
export function ProviderConnectionSettings({
  provider,
  settings,
  setSettings,
  dependencies,
}: {
  provider: "claude" | "codex";
  settings: Settings;
  setSettings: (settings: Settings) => void;
  dependencies?: DepsReport | null;
}) {
  const selected = reviewConnectionId(settings, provider);
  if (
    settings.default_model_chosen &&
    settings.preferred_provider === provider
  ) {
    return (
      <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
        <span>{PRESENTATION[selected].label}</span>
        <button
          type="button"
          className="underline"
          onClick={() => focusSettingsTarget("Default connection")}
        >
          Change default connection
        </button>
      </div>
    );
  }
  return (
    <ModelConnectionField
      provider={provider}
      settings={settings}
      dependencies={dependencies}
      ariaLabel={`${SETTINGS_PROVIDERS[provider].label} connection`}
      onChange={(id) =>
        setSettings(
          withAccessMode(settings, provider, connectionDescription(id).access),
        )
      }
    />
  );
}
