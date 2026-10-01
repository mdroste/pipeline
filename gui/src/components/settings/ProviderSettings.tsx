import { useState } from "react";

import { open as openUrl } from "@tauri-apps/plugin-shell";
import type { DepsReport, ModelCatalog, Settings } from "../../lib/types";

import InfoButton from "../InfoButton";
import CredentialField from "./CredentialField";
import ConnectionStatus from "./ConnectionStatus";
import { ReviewSaveFeedback } from "./SaveState";

import WorkflowCodexConnection from "../WorkflowCodexConnection";
import ChatgptConnection from "../ChatgptConnection";

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
      <CredentialField
        label="Local API Key"
        value={settings.local_api_key}
        optional
        onSave={(value) => setSettings({ ...settings, local_api_key: value })}
      />
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
  catalogs,
  catalogLoading,
  catalogBlocked,
  loadCatalog,
  dependencies,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
  onCodexAccountChange: () => void;
  onCodexStatusChange?: () => void;
  catalogs: Record<string, ModelCatalog>;
  catalogLoading: Record<string, boolean>;
  catalogBlocked: Record<string, boolean>;
  loadCatalog: (
    provider: string,
    settings: Settings,
    refresh?: boolean,
  ) => Promise<void>;
  dependencies?: DepsReport | null;
}) {
  const check = (provider: string) => (
    <ConnectionStatus
      provider={provider}
      settings={settings}
      catalog={catalogs[provider]}
      loading={catalogLoading[provider]}
      blocked={catalogBlocked[provider]}
      onCheck={() => loadCatalog(provider, settings, true)}
      dependencies={dependencies}
    />
  );
  return (
    <>
      <SettingsCard id="anthropic-provider">
        <ProviderHeader name="Anthropic" />
        {check("claude")}
        <AccessModeSelector
          provider="Claude"
          value={settings.claude_access_mode}
          onChange={(mode) =>
            setSettings({ ...settings, claude_access_mode: mode })
          }
        />
        {settings.claude_access_mode === "api" ? (
          <CredentialField
            label="Claude API Key"
            value={settings.anthropic_api_key}
            onSave={(value) =>
              setSettings({ ...settings, anthropic_api_key: value })
            }
          />
        ) : (
          settings.anthropic_api_key && (
            <p className="settings-row-description mt-3">
              A saved API key is inactive in subscription mode.
            </p>
          )
        )}
        <ReviewSaveFeedback />
      </SettingsCard>
      <SettingsCard id="openai-provider">
        <ProviderHeader name="OpenAI" />
        <section
          id="workspace-provider"
          tabIndex={-1}
          className="settings-anchor settings-provider-scope"
        >
          <h3>ChatGPT account</h3>
          <ChatgptConnection
            onAccountChange={onCodexAccountChange}
            onStatusChange={onCodexStatusChange}
          />
        </section>
        <section className="settings-provider-scope">
          <h3>Reviews</h3>
          <AccessModeSelector
            provider="ChatGPT"
            value={settings.codex_access_mode}
            onChange={(mode) =>
              setSettings({ ...settings, codex_access_mode: mode })
            }
          />
          {settings.codex_access_mode !== "api" ? (
            <>
              {settings.codex_backend === "legacy_cli" ? (
                check("codex")
              ) : (
                <WorkflowCodexConnection onStatusChange={onCodexStatusChange} />
              )}
              <details className="settings-disclosure mt-4">
                <summary>
                  Advanced connection settings <span>Compatibility</span>
                </summary>
                <div className="pt-4">
                  <Field
                    label="Reviews connection"
                    help="Use the legacy CLI only for compatibility troubleshooting. Pipeline never switches to it automatically."
                  >
                    <select
                      aria-label="Reviews Codex backend"
                      className={selectClass}
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
                    <p className="settings-row-description mt-3">
                      This uses your terminal’s account. Run{" "}
                      <code>codex login</code> in a terminal to sign in.
                    </p>
                  )}
                </div>
              </details>
              {settings.openai_api_key && (
                <p className="settings-row-description mt-3">
                  A saved API key is inactive in subscription mode.
                </p>
              )}
            </>
          ) : (
            <>
              {check("codex")}
              <CredentialField
                label="OpenAI API Key"
                value={settings.openai_api_key}
                onSave={(value) =>
                  setSettings({ ...settings, openai_api_key: value })
                }
              />
            </>
          )}
          <ReviewSaveFeedback />
        </section>
      </SettingsCard>
      <SettingsCard id="google-provider">
        <ProviderHeader name="Google" />
        {check("antigravity")}
        <AccessModeSelector
          provider="Gemini"
          value={settings.antigravity_access_mode}
          onChange={(mode) =>
            setSettings({ ...settings, antigravity_access_mode: mode })
          }
          subscriptionDisabledNote="Gemini connects through the Google API. A Google API key is required; subscription sign-in is unavailable."
        />
        <CredentialField
          label="Gemini API Key"
          value={settings.google_api_key}
          onSave={(value) =>
            setSettings({ ...settings, google_api_key: value })
          }
        />
        <ReviewSaveFeedback />
      </SettingsCard>
    </>
  );
}
