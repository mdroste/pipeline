import { useState } from "react";

import { open as openUrl } from "@tauri-apps/plugin-shell";
import type { DepsReport, Settings } from "../../lib/settingsTypes";
import type { ModelCatalog } from "../../lib/reviewModelTypes";

import InfoButton from "../InfoButton";
import CredentialField from "./CredentialField";
import LocalServerUrl from "./LocalServerUrl";
import ConnectionStatus from "./ConnectionStatus";
import { ProviderConnectionSettings } from "./ModelConnectionSettings";
import { ReviewSaveFeedback } from "./SaveState";

import WorkflowCodexConnection from "../WorkflowCodexConnection";
import ChatgptConnection from "../ChatgptConnection";

import {
  SettingsCard,
  ProviderHeader,
  CatalogStatus,
  selectClass,
  Field,
} from "./controls";
import { workbenchErrorMessage } from "../../lib/workbenchError";
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
      setExternalLinkError(workbenchErrorMessage(error));
    }
  };

  return (
    <section className="mt-3 space-y-3">
      <div className="flex items-center gap-1.5">
        <h3 className="text-sm font-semibold text-gray-900 dark:text-neutral-100">
          Server setup
        </h3>
        <InfoButton label="Server setup">
          Connect to an OpenAI-compatible server. With{" "}
          <a
            href="https://ollama.com"
            onClick={(e) => {
              e.preventDefault();
              void openOllamaSite();
            }}
            className="underline cursor-pointer hover:text-gray-700 dark:hover:text-neutral-300"
          >
            Ollama
          </a>{" "}
          installed, run <span className="font-mono">ollama pull llama3.3</span>
          , then select the model below. For LM Studio, llama.cpp, or vLLM,
          enter that server’s URL. Choose a model that can use tools to work
          with files and search the web.
        </InfoButton>
      </div>
      {externalLinkError && (
        <p role="alert" className="ui-notice ui-notice--error">
          Could not open ollama.com: {externalLinkError}
        </p>
      )}
      <LocalServerUrl settings={settings} setSettings={setSettings} />
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
                {settings.local_model} (saved, not available)
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
        <ProviderHeader name="Anthropic">{check("claude")}</ProviderHeader>
        <ProviderConnectionSettings
          provider="claude"
          settings={settings}
          setSettings={setSettings}
          dependencies={dependencies}
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
          <p className="settings-row-description mt-3">
            {settings.claude_access_mode === "code_api" ? (
              <>
                Sign in through Claude Code with{" "}
                <code>claude auth login --console</code> in Terminal. If
                offered, choose “Create an API key.” Usage is billed to your
                Console account. Pipeline’s saved API key is only used by the
                direct API connection.
              </>
            ) : (
              <>
                Sign in through <code>claude auth login</code> and choose your
                Claude subscription. If that sign-in is unavailable, requests
                stop; Pipeline never switches to API billing. Your saved API key
                isn’t used in subscription mode.
              </>
            )}
          </p>
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
          <div className="settings-provider-header">
            <h3>Reviews</h3>
            {(settings.codex_access_mode === "api" ||
              settings.codex_backend === "legacy_cli") &&
              check("codex")}
          </div>
          <ProviderConnectionSettings
            provider="codex"
            settings={settings}
            setSettings={setSettings}
            dependencies={dependencies}
          />
          {settings.codex_access_mode !== "api" ? (
            <>
              {settings.codex_backend !== "legacy_cli" && (
                <WorkflowCodexConnection onStatusChange={onCodexStatusChange} />
              )}
              <details className="settings-disclosure">
                <summary>Advanced connection settings</summary>
                <div className="pt-4">
                  <Field
                    label="Reviews connection"
                    help="Use the legacy CLI only if Codex App Server doesn’t work."
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
                  Your saved API key isn’t used in subscription mode.
                </p>
              )}
            </>
          ) : (
            <CredentialField
              label="OpenAI API Key"
              value={settings.openai_api_key}
              onSave={(value) =>
                setSettings({ ...settings, openai_api_key: value })
              }
            />
          )}
          <ReviewSaveFeedback />
        </section>
      </SettingsCard>
      <SettingsCard id="google-provider">
        <ProviderHeader name="Google">{check("antigravity")}</ProviderHeader>
        <p className="settings-row-description">
          Gemini uses the Google API key saved in Pipeline. API usage is billed
          separately.
        </p>
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
