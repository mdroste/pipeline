import { useState } from "react";

import { open as openUrl } from "@tauri-apps/plugin-shell";
import type { ModelCatalog, Settings } from "../../lib/types";

import InfoButton from "../InfoButton";

import WorkflowCodexConnection from "../WorkflowCodexConnection";
import WorkspaceConnectionSettings from "../WorkspaceConnectionSettings";

import {
  SettingsCard,
  ProviderHeader,
  CatalogStatus,
  selectClass,
  inputClass,
  Field,
} from "./controls";
export function LocalProviderSection({
  settings,
  setSettings,
  catalogs,
  catalogBlocked,
  catalogLoading,
  discoveryInputChanged,
  loadCatalog,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
  catalogs: Record<string, ModelCatalog>;
  catalogBlocked: Record<string, boolean>;
  catalogLoading: Record<string, boolean>;
  discoveryInputChanged: Record<string, boolean>;
  loadCatalog: (
    provider: string,
    settings: Settings,
    refresh?: boolean,
  ) => Promise<void>;
}) {
  const localCatalog = catalogBlocked.local ? undefined : catalogs.local;
  const [externalLinkError, setExternalLinkError] = useState<string | null>(
    null,
  );

  const openOllamaSite = async () => {
    setExternalLinkError(null);
    try {
      await openUrl("https://ollama.com");
    } catch (error) {
      setExternalLinkError(
        error instanceof Error ? error.message : String(error),
      );
    }
  };

  return (
    <section className="space-y-5">
      <div className="flex items-center gap-1.5">
        <h3 className="text-sm font-semibold text-gray-900 dark:text-neutral-100">
          Local server
        </h3>
        <InfoButton label="Local server">
          Connect to an OpenAI-compatible server. With{" "}
          <a
            href="https://ollama.com"
            onClick={(e) => {
              e.preventDefault();
              void openOllamaSite();
            }}
            className="underline cursor-pointer hover:text-gray-700 dark:hover:text-neutral-300"
          >
            ollama.com
          </a>{" "}
          installed, run <span className="font-mono">ollama pull llama3.3</span>
          , then select the model below. LM Studio, llama.cpp, and vLLM work by
          changing the URL. Local models may be less capable than cloud models
          and may lack file-reading support.
        </InfoButton>
      </div>
      {externalLinkError && (
        <p role="alert" className="text-xs text-red-700 dark:text-red-300">
          Could not open ollama.com: {externalLinkError}
        </p>
      )}
      <Field label="Server URL">
        <input
          aria-label="Local Server URL"
          type="text"
          value={settings.local_base_url}
          onChange={(e) =>
            setSettings({ ...settings, local_base_url: e.target.value })
          }
          placeholder="http://localhost:11434/v1"
          className={`${inputClass} font-mono`}
          autoComplete="off"
          spellCheck={false}
        />
      </Field>
      <Field
        label="API Key"
        help="Optional bearer token for this endpoint. Stored encrypted."
      >
        <input
          aria-label="Local API Key"
          type="password"
          value={settings.local_api_key}
          onChange={(e) =>
            setSettings({ ...settings, local_api_key: e.target.value })
          }
          placeholder="Usually empty for local servers"
          className={`${inputClass} font-mono`}
          autoComplete="off"
        />
      </Field>
      <Field label="Model">
        <select
          aria-label="Local Model"
          value={settings.local_model}
          disabled={catalogBlocked.local}
          onChange={(e) =>
            setSettings({ ...settings, local_model: e.target.value })
          }
          className={`${selectClass} font-mono disabled:cursor-not-allowed disabled:opacity-50`}
        >
          <option value="">Select a model…</option>
          {localCatalog?.models.map((model) => (
            <option key={model.id} value={model.id}>
              {model.display_name || model.id}
            </option>
          ))}
          {settings.local_model &&
            !discoveryInputChanged.local &&
            !localCatalog?.models.some(
              (model) => model.id === settings.local_model,
            ) && (
              <option value={settings.local_model}>
                {settings.local_model} (saved; not currently listed)
              </option>
            )}
        </select>
        <CatalogStatus
          blocked={catalogBlocked.local}
          catalog={localCatalog}
          loading={catalogLoading.local}
          onRefresh={() => loadCatalog("local", settings, true)}
        />
      </Field>
    </section>
  );
}

type AccessMode = "subscription" | "api";

export function AccessModeSelector({
  provider,
  value,
  onChange,
  subscriptionDisabledNote,
}: {
  provider: string;
  value: AccessMode;
  onChange: (mode: AccessMode) => void;
  /** When set, the Subscription option is shown but not selectable and this
      note replaces the mode description. */
  subscriptionDisabledNote?: string;
}) {
  const effectiveValue = subscriptionDisabledNote ? "api" : value;
  return (
    <div>
      <div className="mb-1.5 text-sm font-medium text-gray-700 dark:text-neutral-300">
        Connection mode
      </div>
      <div
        role="radiogroup"
        aria-label={`${provider} connection mode`}
        className="inline-flex rounded-lg border border-gray-300 bg-gray-50 p-0.5 dark:border-neutral-600 dark:bg-neutral-900"
      >
        {(["subscription", "api"] as const).map((mode) => {
          const disabled =
            mode === "subscription" && !!subscriptionDisabledNote;
          return (
            <label
              key={mode}
              className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors focus-within:ring-2 focus-within:ring-blue-500 ${
                disabled
                  ? "cursor-not-allowed text-gray-300 dark:text-neutral-600"
                  : effectiveValue === mode
                    ? "cursor-pointer bg-white text-gray-900 shadow-sm dark:bg-neutral-700 dark:text-neutral-100"
                    : "cursor-pointer text-gray-500 hover:text-gray-800 dark:text-neutral-400 dark:hover:text-neutral-200"
              }`}
            >
              <input
                type="radio"
                name={`${provider.toLowerCase().replaceAll(" ", "-")}-access-mode`}
                value={mode}
                checked={effectiveValue === mode}
                disabled={disabled}
                onChange={() => onChange(mode)}
                className="sr-only"
              />
              {mode === "subscription" ? "Subscription" : "API"}
            </label>
          );
        })}
      </div>
      <p className="mt-1.5 text-[11px] leading-4 text-gray-500 dark:text-neutral-400">
        {subscriptionDisabledNote
          ? subscriptionDisabledNote
          : effectiveValue === "subscription"
            ? "Uses the provider’s native connection and signed-in subscription. The saved API key is not used."
            : "Uses direct, token-metered API calls with the key below. The provider CLI is not required."}
      </p>
    </div>
  );
}

export function ProvidersSection({
  settings,
  setSettings,
  onCodexAccountChange,
  onCodexStatusChange,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
  onCodexAccountChange: () => void;
  onCodexStatusChange?: () => void;
}) {
  const [workspaceLoaded, setWorkspaceLoaded] = useState(false);
  return (
    <>
      <div className="space-y-5">
        <SettingsCard id="anthropic-provider">
          <ProviderHeader
            name="Anthropic"
            description="Claude through a subscription or the Anthropic API."
            badge="Reviews"
          />
          <div className="space-y-4">
            <AccessModeSelector
              provider="Claude"
              value={settings.claude_access_mode}
              onChange={(mode) =>
                setSettings({ ...settings, claude_access_mode: mode })
              }
            />
            <Field
              label={
                settings.claude_access_mode === "api"
                  ? "API Key"
                  : "API Key · inactive in subscription mode"
              }
              help="Stored encrypted and used only when Claude is in API mode."
            >
              <input
                aria-label="Claude API Key"
                type="password"
                value={settings.anthropic_api_key}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    anthropic_api_key: e.target.value,
                  })
                }
                placeholder="sk-ant-…"
                className={`${inputClass} font-mono`}
                autoComplete="off"
              />
            </Field>
            {settings.claude_access_mode === "api" &&
              !settings.anthropic_api_key.trim() && (
                <p className="text-xs text-amber-700 dark:text-amber-300">
                  Enter an Anthropic API key before running Claude in API mode.
                </p>
              )}
          </div>
        </SettingsCard>

        <SettingsCard id="openai-provider">
          <ProviderHeader
            name="OpenAI"
            description="ChatGPT subscriptions and OpenAI API access."
            badge="Workspace · Reviews"
          />
          <div className="space-y-4">
            <h3 className="text-sm font-medium">Reviews</h3>
            <AccessModeSelector
              provider="ChatGPT"
              value={settings.codex_access_mode}
              onChange={(mode) =>
                setSettings({ ...settings, codex_access_mode: mode })
              }
            />
            {settings.codex_access_mode !== "api" && (
              <>
                {settings.codex_backend === "legacy_cli" ? (
                  <p className="text-xs text-gray-600 dark:text-neutral-400">
                    The legacy Codex CLI connection is enabled. Manage it in
                    Advanced connection settings.
                  </p>
                ) : (
                  <WorkflowCodexConnection
                    onAccountChange={onCodexAccountChange}
                    onStatusChange={onCodexStatusChange}
                  />
                )}
                <details className="settings-disclosure">
                  <summary>
                    Advanced connection settings <span>Compatibility</span>
                  </summary>
                  <div className="space-y-3 pt-4">
                    <Field
                      label="Reviews connection"
                      help="Use the legacy CLI only for compatibility troubleshooting. Pipeline never switches to it automatically."
                    >
                      <select
                        aria-label="Reviews Codex backend"
                        className={inputClass}
                        value={settings.codex_backend ?? "app_server"}
                        onChange={(e) =>
                          setSettings({
                            ...settings,
                            codex_backend: e.target.value as
                              "legacy_cli" | "app_server",
                            codex_backend_preference_version: 1,
                          })
                        }
                      >
                        <option value="app_server">
                          Codex App Server (recommended)
                        </option>
                        <option value="legacy_cli">Legacy Codex CLI</option>
                      </select>
                    </Field>
                    {settings.codex_backend === "legacy_cli" && (
                      <p className="text-xs text-gray-600 dark:text-neutral-400">
                        This uses your terminal's Codex account. Run{" "}
                        <code>codex login</code> in a terminal to sign in. App
                        Server keeps its own sign-in and can be restored above.
                      </p>
                    )}
                  </div>
                </details>
              </>
            )}
            <Field
              label={
                settings.codex_access_mode === "api"
                  ? "API Key"
                  : "API Key · inactive in subscription mode"
              }
              help="Stored encrypted and used only when ChatGPT is in API mode."
            >
              <input
                aria-label="OpenAI API Key"
                type="password"
                value={settings.openai_api_key}
                onChange={(e) =>
                  setSettings({ ...settings, openai_api_key: e.target.value })
                }
                placeholder="sk-…"
                className={`${inputClass} font-mono`}
                autoComplete="off"
              />
            </Field>
            {settings.codex_access_mode === "api" &&
              !settings.openai_api_key.trim() && (
                <p className="text-xs text-amber-700 dark:text-amber-300">
                  Enter an OpenAI API key before running ChatGPT in API mode.
                </p>
              )}
          </div>
          <details
            id="workspace-provider"
            tabIndex={-1}
            className="settings-disclosure settings-anchor mt-5"
            onToggle={(event) => {
              if (event.currentTarget.open) setWorkspaceLoaded(true);
            }}
          >
            <summary>
              Workspace ChatGPT <span>Separate sign-in, models & usage</span>
            </summary>
            {workspaceLoaded && (
              <div className="pt-5">
                <WorkspaceConnectionSettings embedded />
              </div>
            )}
          </details>
        </SettingsCard>

        <SettingsCard id="google-provider">
          <ProviderHeader
            name="Google"
            description="Gemini models through the Google API."
            badge="Reviews"
          />
          <div className="space-y-4">
            <AccessModeSelector
              provider="Gemini"
              value={settings.antigravity_access_mode}
              onChange={(mode) =>
                setSettings({ ...settings, antigravity_access_mode: mode })
              }
              subscriptionDisabledNote="Gemini connects through the Google API. A Google API key is required; subscription sign-in is unavailable."
            />
            <Field
              label="Gemini API Key"
              help="Stored encrypted. Required for Gemini API calls."
            >
              <input
                aria-label="Gemini API Key"
                type="password"
                value={settings.google_api_key}
                onChange={(e) =>
                  setSettings({ ...settings, google_api_key: e.target.value })
                }
                placeholder="AI..."
                className={`${inputClass} font-mono`}
                autoComplete="off"
              />
            </Field>
            {!settings.google_api_key.trim() && (
              <p className="text-xs text-amber-700 dark:text-amber-300">
                Enter a Google AI API key before running Gemini.
              </p>
            )}
          </div>
        </SettingsCard>
      </div>
    </>
  );
}
