import { useCallback, useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import type { EngineStatus, ModelCatalog, ModelSelection, Settings } from "../lib/types";
import {
  decodeModelSelection,
  effortOptions,
  encodeModelSelection,
  type CloudProvider,
  providerSelection,
  providerTransport,
  PROVIDERS,
  withProviderSelection,
} from "../lib/providers";
import EnginesPanel from "./EnginesPanel";
import InfoButton from "./InfoButton";
import ResizeHandle from "./ResizeHandle";
import usePersistentPanelWidth from "../hooks/usePersistentPanelWidth";
import type { ThemePreference } from "../lib/theme";

interface Props {
  onClose: () => void;
  onDirtyChange?: (dirty: boolean) => void;
  showBack?: boolean;
  theme: ThemePreference;
  onThemeChange: (theme: ThemePreference) => void;
  onSystemChange?: () => void;
  initialSection?: Section;
  targetId?: string;
  navigationKey?: number;
}

type Section = "llm" | "extraction" | "general";

function catalogDiscoveryInputs(settings: Settings): Record<string, string> {
  return {
    claude: settings.anthropic_api_key,
    codex: settings.openai_api_key,
    gemini: settings.google_api_key,
    local: `${settings.local_base_url}\u0000${settings.local_api_key}`,
  };
}

export default function SettingsPage({
  onClose,
  onDirtyChange,
  showBack = true,
  theme,
  onThemeChange,
  onSystemChange,
  initialSection = "llm",
  targetId,
  navigationKey = 0,
}: Props) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [savedSettingsSnapshot, setSavedSettingsSnapshot] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [section, setSection] = useState<Section>(initialSection);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [catalogs, setCatalogs] = useState<Record<string, ModelCatalog>>({});
  const [catalogLoading, setCatalogLoading] = useState<Record<string, boolean>>({});
  const [catalogInputKeys, setCatalogInputKeys] = useState<Record<string, string>>({});
  const [savedCatalogInputs, setSavedCatalogInputs] = useState<Record<string, string> | null>(null);
  const [navWidth, setNavWidth] = usePersistentPanelWidth(
    "pipeline.ui.settingsNavWidth",
    192,
    160,
    320,
  );
  const settingsRef = useRef<Settings | null>(settings);
  const catalogRequestsRef = useRef<Record<string, number>>({});
  const previousDraftDiscoveryInputsRef = useRef<Record<string, string> | null>(null);
  const initialCatalogDiscoveryStartedRef = useRef(false);
  settingsRef.current = settings;
  const dirty = settings !== null &&
    savedSettingsSnapshot !== null &&
    JSON.stringify(settings) !== savedSettingsSnapshot;

  useEffect(() => {
    onDirtyChange?.(dirty);
  }, [dirty, onDirtyChange]);

  useEffect(() => {
    setSection(initialSection);
  }, [initialSection, navigationKey]);

  useEffect(() => {
    if (loading || !targetId || section !== initialSection) return;
    const frame = requestAnimationFrame(() => {
      const target = document.getElementById(targetId);
      target?.focus({ preventScroll: true });
      target?.scrollIntoView({ block: "start" });
    });
    return () => cancelAnimationFrame(frame);
  }, [initialSection, loading, navigationKey, section, targetId]);

  useEffect(() => () => onDirtyChange?.(false), [onDirtyChange]);

  // Clear the "saved" indicator after 2 seconds, with proper cleanup
  useEffect(() => {
    if (!saved) return;
    const timer = setTimeout(() => setSaved(false), 2000);
    return () => clearTimeout(timer);
  }, [saved]);

  useEffect(() => {
    invoke<{ settings: Settings; warnings: string[] }>("get_settings")
      .then((resp) => {
        const discoveryInputs = catalogDiscoveryInputs(resp.settings);
        previousDraftDiscoveryInputsRef.current = discoveryInputs;
        setSavedCatalogInputs(discoveryInputs);
        setSettings(resp.settings);
        setSavedSettingsSnapshot(JSON.stringify(resp.settings));
        setWarnings(resp.warnings);
        setLoading(false);
      })
      .catch((e) => {
        console.error("Failed to load settings:", e);
        setLoadError(e instanceof Error ? e.message : String(e));
        setLoading(false);
      });
  }, []);

  const loadCatalog = async (provider: string, current: Settings, refresh = false) => {
    const request = (catalogRequestsRef.current[provider] ?? 0) + 1;
    const inputKey = catalogDiscoveryInputs(current)[provider];
    catalogRequestsRef.current[provider] = request;
    setCatalogLoading((old) => ({ ...old, [provider]: true }));
    try {
      const catalog = await invoke<ModelCatalog>("get_model_catalog", {
        provider,
        settings: current,
        refresh,
      });
      if (catalogRequestsRef.current[provider] === request) {
        setCatalogs((old) => ({ ...old, [provider]: catalog }));
        setCatalogInputKeys((old) => ({ ...old, [provider]: inputKey }));
      }
    } catch (error) {
      if (catalogRequestsRef.current[provider] === request) {
        setCatalogs((old) => ({
          ...old,
          [provider]: {
            provider,
            transport: provider === "local" || providerTransport(current, provider) === "api" ? "api" : "cli",
            source: "unavailable",
            source_version: "",
            fetched_at: "",
            stale: true,
            warning: String(error),
            models: [],
            roles: [],
          },
        }));
        setCatalogInputKeys((old) => ({ ...old, [provider]: inputKey }));
      }
    } finally {
      if (catalogRequestsRef.current[provider] === request) {
        setCatalogLoading((old) => ({ ...old, [provider]: false }));
      }
    }
  };

  // Load catalogs once from settings that are already persisted. Draft
  // credentials never trigger automatic authenticated discovery.
  useEffect(() => {
    if (!settings || !savedCatalogInputs || initialCatalogDiscoveryStartedRef.current) return;
    initialCatalogDiscoveryStartedRef.current = true;
    for (const provider of PROVIDERS) {
      void loadCatalog(provider, settings);
    }
  }, [savedCatalogInputs, settings]);

  // Invalidate both visible data and in-flight requests as soon as a discovery
  // input changes. The user must Save or explicitly Refresh before models for
  // the draft credential/server become available.
  useEffect(() => {
    if (!settings) return;
    const current = catalogDiscoveryInputs(settings);
    const previous = previousDraftDiscoveryInputsRef.current;
    previousDraftDiscoveryInputsRef.current = current;
    if (!previous) return;
    const changed = PROVIDERS.filter(
      (provider) => previous[provider] !== current[provider],
    );
    if (changed.length === 0) return;
    for (const provider of changed) {
      catalogRequestsRef.current[provider] =
        (catalogRequestsRef.current[provider] ?? 0) + 1;
    }
    setCatalogs((old) => {
      const next = { ...old };
      changed.forEach((provider) => delete next[provider]);
      return next;
    });
    setCatalogInputKeys((old) => {
      const next = { ...old };
      changed.forEach((provider) => delete next[provider]);
      return next;
    });
    setCatalogLoading((old) => ({
      ...old,
      ...Object.fromEntries(changed.map((provider) => [provider, false])),
    }));
  }, [
    settings?.anthropic_api_key,
    settings?.openai_api_key,
    settings?.google_api_key,
    settings?.local_base_url,
    settings?.local_api_key,
  ]);

  const handleSave = async () => {
    if (!settings) return;
    const settingsToSave = settings;
    const savedSnapshot = JSON.stringify(settingsToSave);
    setSaving(true);
    setSaved(false);
    try {
      await invoke("save_settings", { settings: settingsToSave });
      const nextCatalogInputs = catalogDiscoveryInputs(settingsToSave);
      const previousCatalogInputs =
        savedCatalogInputs ?? nextCatalogInputs;
      const changedProviders = PROVIDERS.filter(
        (provider) =>
          previousCatalogInputs[provider] !== nextCatalogInputs[provider],
      );
      setSavedSettingsSnapshot(savedSnapshot);
      setSavedCatalogInputs(nextCatalogInputs);
      setWarnings([]); // Clear warnings after successful save
      setSaved(JSON.stringify(settingsRef.current) === savedSnapshot);
      for (const provider of changedProviders) {
        // Cloud catalog caches do not include account identity, so a changed
        // saved credential must bypass them. Local discovery is uncached.
        void loadCatalog(
          provider,
          settingsToSave,
          provider !== "local",
        );
      }
      onSystemChange?.();
    } catch (e) {
      console.error("Failed to save settings:", e);
      alert(`Failed to save: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSaving(false);
    }
  };

  const handleClose = () => {
    if (dirty && !window.confirm("You have unsaved settings changes. Leave and discard them?")) {
      return;
    }
    onClose();
  };

  if (loading) {
    return (
      <div className="p-8 flex items-center justify-center h-full text-gray-400">
        <div className="animate-spin w-6 h-6 border-2 border-gray-300 border-t-gray-600 rounded-full" />
      </div>
    );
  }

  if (loadError || !settings) {
    return (
      <div className="p-8 flex flex-col items-center justify-center h-full gap-4 text-gray-500 dark:text-gray-400">
        <p className="text-sm">Failed to load settings{loadError ? `: ${loadError}` : "."}</p>
        <button
          onClick={handleClose}
          className="text-sm text-gray-600 dark:text-gray-300 hover:underline"
        >
          Go back
        </button>
      </div>
    );
  }

  const draftCatalogInputs = catalogDiscoveryInputs(settings);
  const catalogBlocked = Object.fromEntries(
    PROVIDERS.map((provider) => [
      provider,
      catalogInputKeys[provider] !== draftCatalogInputs[provider],
    ]),
  ) as Record<string, boolean>;
  const discoveryInputChanged = Object.fromEntries(
    PROVIDERS.map((provider) => [
      provider,
      savedCatalogInputs?.[provider] !== draftCatalogInputs[provider],
    ]),
  ) as Record<string, boolean>;

  const navItems: { id: Section; label: string; icon: React.ReactNode }[] = [
    {
      id: "llm",
      label: "Models",
      icon: (
        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09zM18.259 8.715L18 9.75l-.259-1.035a3.375 3.375 0 00-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 002.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 002.455 2.456L21.75 6l-1.036.259a3.375 3.375 0 00-2.455 2.456z" />
        </svg>
      ),
    },
    {
      id: "extraction",
      label: "PDF Extraction",
      icon: (
        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
        </svg>
      ),
    },
    {
      id: "general",
      label: "General",
      icon: (
        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.324.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 011.37.49l1.296 2.247a1.125 1.125 0 01-.26 1.431l-1.003.827c-.293.24-.438.613-.431.992a6.759 6.759 0 010 .255c-.007.378.138.75.43.99l1.005.828c.424.35.534.954.26 1.43l-1.298 2.247a1.125 1.125 0 01-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.57 6.57 0 01-.22.128c-.331.183-.581.495-.644.869l-.213 1.28c-.09.543-.56.941-1.11.941h-2.594c-.55 0-1.02-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 01-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 01-1.369-.49l-1.297-2.247a1.125 1.125 0 01.26-1.431l1.004-.827c.292-.24.437-.613.43-.992a6.932 6.932 0 010-.255c.007-.378-.138-.75-.43-.99l-1.004-.828a1.125 1.125 0 01-.26-1.43l1.297-2.247a1.125 1.125 0 011.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.087.22-.128.332-.183.582-.495.644-.869l.214-1.281z" />
          <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
      ),
    },
  ];

  return (
    <div data-testid="settings-page" className="flex h-full">
      {/* Sidebar nav */}
      <div
        style={{ width: navWidth }}
        className="relative flex shrink-0 flex-col border-r border-gray-200 bg-gray-50/50 p-4 dark:border-gray-700 dark:bg-gray-900/50"
      >
        <h2 className="text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-4 px-2">
          Settings
        </h2>
        <nav className="space-y-1 flex-1">
          {navItems.map((item) => (
            <button
              key={item.id}
              onClick={() => setSection(item.id)}
              className={`w-full flex items-center gap-2.5 px-2.5 py-2 rounded-lg text-sm transition-colors ${
                section === item.id
                  ? "bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900"
                  : "text-gray-600 dark:text-gray-400 hover:bg-gray-200/60 dark:hover:bg-gray-800/60"
              }`}
            >
              {item.icon}
              {item.label}
            </button>
          ))}
        </nav>
        {showBack && (
          <button
            onClick={handleClose}
            className="flex items-center gap-2 px-2.5 py-2 text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 transition-colors"
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M10.5 19.5L3 12m0 0l7.5-7.5M3 12h18" />
            </svg>
            Back
          </button>
        )}
        <ResizeHandle
          currentWidth={navWidth}
          defaultWidth={192}
          label="Resize settings navigation"
          min={160}
          max={320}
          onResize={setNavWidth}
        />
      </div>

      {/* Content area */}
      <div className="flex-1 overflow-y-auto">
        {warnings.length > 0 && (
          <div className="mx-8 mt-6 px-4 py-3 rounded-lg bg-amber-50 dark:bg-amber-950 border border-amber-200 dark:border-amber-800 text-sm text-amber-800 dark:text-amber-300">
            {warnings.map((w, i) => (
              <p key={i}>{w}</p>
            ))}
            <p className="mt-1 text-amber-700 dark:text-amber-300 text-xs">
              Saving will overwrite the current file with these values.
            </p>
          </div>
        )}
        <div className="p-8 max-w-xl">
          {section === "llm" && (
            <LLMSection
              settings={settings}
              setSettings={setSettings}
              catalogs={catalogs}
              catalogBlocked={catalogBlocked}
              catalogLoading={catalogLoading}
              discoveryInputChanged={discoveryInputChanged}
              loadCatalog={loadCatalog}
            />
          )}
          {section === "extraction" && (
            <ExtractionSection
              settings={settings}
              setSettings={setSettings}
              onSystemChange={onSystemChange}
            />
          )}
          {section === "general" && (
            <GeneralSection
              settings={settings}
              setSettings={setSettings}
              theme={theme}
              onThemeChange={onThemeChange}
            />
          )}

          {/* Save bar */}
          <div className="mt-10 pt-6 border-t border-gray-200 dark:border-gray-700 flex items-center gap-3">
            <button
              onClick={handleSave}
              disabled={saving}
              className="py-2 px-6 bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 rounded-lg text-sm font-medium
                         hover:bg-gray-800 dark:hover:bg-gray-200 disabled:bg-gray-400 dark:disabled:bg-gray-700 transition-colors"
            >
              {saving ? "Saving..." : "Save"}
            </button>
            {saved && (
              <span className="text-sm text-green-700 dark:text-green-400">Settings saved.</span>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

/* ── Section Components ──────────────────────────────────────────── */

function ProviderGroup({
  title,
  active,
  hasApiKey,
  help,
  children,
}: {
  title: string;
  active: boolean;
  hasApiKey?: boolean;
  help?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className={`rounded-lg border p-4 space-y-4 ${
      active
        ? "border-gray-900 dark:border-gray-300 bg-gray-50/50 dark:bg-gray-800/30"
        : "border-gray-200 dark:border-gray-700"
    }`}>
      <div className="flex items-center gap-2">
        <span className="text-sm font-semibold text-gray-900 dark:text-gray-100">{title}</span>
        {help && <InfoButton label={title}>{help}</InfoButton>}
        {active && (
          <span className="text-[10px] uppercase tracking-wider font-medium px-1.5 py-0.5 rounded bg-gray-900 text-white dark:bg-gray-200 dark:text-gray-900">
            preferred
          </span>
        )}
        {hasApiKey && (
          <span className="text-[10px] uppercase tracking-wider font-medium px-1.5 py-0.5 rounded bg-green-700 text-white dark:bg-green-600">
            api
          </span>
        )}
      </div>
      {children}
    </div>
  );
}

function LLMSection({
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
  loadCatalog: (provider: string, settings: Settings, refresh?: boolean) => Promise<void>;
}) {
  const localCatalog = catalogBlocked.local ? undefined : catalogs.local;
  const [externalLinkError, setExternalLinkError] = useState<string | null>(null);

  const openOllamaSite = async () => {
    setExternalLinkError(null);
    try {
      await openUrl("https://ollama.com");
    } catch (error) {
      setExternalLinkError(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <>
      <SectionHeader
        title="Models"
        help="Configure models for each provider. The preferred provider is used for steps that do not specify an agent."
      />

      <div className="space-y-5">
        <Field
          label="Preferred Provider"
          help="Provider used when workflow does not specify explicit agent(s)."
        >
          <select
            aria-label="Preferred Provider"
            value={settings.preferred_provider}
            onChange={(e) =>
              setSettings({ ...settings, preferred_provider: e.target.value })
            }
            className={selectClass}
          >
            <option value="claude">Claude (Anthropic)</option>
            <option value="codex">ChatGPT (OpenAI)</option>
            <option value="gemini">Gemini (Google)</option>
            <option value="local">Local (Ollama / OpenAI-compatible)</option>
          </select>
        </Field>

        {/* Claude */}
        <ProviderGroup title="Claude (Anthropic)" active={settings.preferred_provider === "claude"} hasApiKey={!!settings.anthropic_api_key}>
          <Field
            label="API Key"
            help="Bypasses Claude CLI for faster calls. Leave empty to use the CLI with your subscription."
          >
            <input
              aria-label="Claude API Key"
              type="password"
              value={settings.anthropic_api_key}
              onChange={(e) =>
                setSettings({ ...settings, anthropic_api_key: e.target.value })
              }
              placeholder="sk-ant-... (optional, enables direct API)"
              className={`${inputClass} font-mono`}
              autoComplete="off"
            />
          </Field>
          <Field label="Model">
            <ModelPicker
              provider="claude"
              settings={settings}
              catalog={catalogBlocked.claude ? undefined : catalogs.claude}
              blocked={catalogBlocked.claude}
              allowSavedUnknown={!discoveryInputChanged.claude}
              loading={catalogLoading.claude}
              onChange={(selection) => setSettings(withProviderSelection(settings, "claude", selection))}
              onRefresh={() => loadCatalog("claude", settings, true)}
            />
          </Field>
          <Field label="Thinking Effort">
            <select
              aria-label="Claude Thinking Effort"
              value={settings.claude_effort}
              onChange={(e) =>
                setSettings({ ...settings, claude_effort: e.target.value })
              }
              className={selectClass}
            >
              <option value="">Default</option>
              {effortOptions(
                catalogBlocked.claude ? undefined : catalogs.claude,
                providerSelection(settings, "claude"),
                ["low", "medium", "high", "max"],
              ).map((effort) => (
                <option key={effort} value={effort}>{effortLabel(effort)}</option>
              ))}
            </select>
          </Field>
        </ProviderGroup>

        {/* ChatGPT / OpenAI */}
        <ProviderGroup title="ChatGPT (OpenAI)" active={settings.preferred_provider === "codex"} hasApiKey={!!settings.openai_api_key}>
          <Field
            label="API Key"
            help="Bypasses Codex CLI for faster calls. Leave empty to use the CLI."
          >
            <input
              aria-label="OpenAI API Key"
              type="password"
              value={settings.openai_api_key}
              onChange={(e) =>
                setSettings({ ...settings, openai_api_key: e.target.value })
              }
              placeholder="sk-... (optional, enables direct API)"
              className={`${inputClass} font-mono`}
              autoComplete="off"
            />
          </Field>
          <Field label="Model">
            <ModelPicker
              provider="codex"
              settings={settings}
              catalog={catalogBlocked.codex ? undefined : catalogs.codex}
              blocked={catalogBlocked.codex}
              allowSavedUnknown={!discoveryInputChanged.codex}
              loading={catalogLoading.codex}
              onChange={(selection) => setSettings(withProviderSelection(settings, "codex", selection))}
              onRefresh={() => loadCatalog("codex", settings, true)}
            />
          </Field>
          <Field label="Reasoning Effort">
            <select
              aria-label="OpenAI Reasoning Effort"
              value={settings.codex_effort}
              onChange={(e) =>
                setSettings({ ...settings, codex_effort: e.target.value })
              }
              className={selectClass}
            >
              <option value="">Default</option>
              {effortOptions(
                catalogBlocked.codex ? undefined : catalogs.codex,
                providerSelection(settings, "codex"),
                ["low", "medium", "high"],
              ).map((effort) => (
                <option key={effort} value={effort}>{effortLabel(effort)}</option>
              ))}
            </select>
          </Field>
        </ProviderGroup>

        {/* Gemini */}
        <ProviderGroup title="Gemini (Google)" active={settings.preferred_provider === "gemini"} hasApiKey={!!settings.google_api_key}>
          <Field
            label="API Key"
            help="Bypasses Gemini CLI for faster calls. Leave empty to use the CLI."
          >
            <input
              aria-label="Gemini API Key"
              type="password"
              value={settings.google_api_key}
              onChange={(e) =>
                setSettings({ ...settings, google_api_key: e.target.value })
              }
              placeholder="AI... (optional, enables direct API)"
              className={`${inputClass} font-mono`}
              autoComplete="off"
            />
          </Field>
          <Field label="Model">
            <ModelPicker
              provider="gemini"
              settings={settings}
              catalog={catalogBlocked.gemini ? undefined : catalogs.gemini}
              blocked={catalogBlocked.gemini}
              allowSavedUnknown={!discoveryInputChanged.gemini}
              loading={catalogLoading.gemini}
              onChange={(selection) => setSettings(withProviderSelection(settings, "gemini", selection))}
              onRefresh={() => loadCatalog("gemini", settings, true)}
            />
          </Field>
        </ProviderGroup>

        {/* Local (Ollama / OpenAI-compatible) */}
        <ProviderGroup
          title="Local (Ollama)"
          active={settings.preferred_provider === "local"}
          help={<>Runs against any local OpenAI-compatible server. With{" "}
            <a
              href="https://ollama.com"
              onClick={(e) => {
                e.preventDefault();
                void openOllamaSite();
              }}
              className="underline cursor-pointer hover:text-gray-700 dark:hover:text-gray-300"
            >
              ollama.com
            </a>{" "}
            installed, run <span className="font-mono">ollama pull llama3.3</span>, then select
            the model below. LM Studio, llama.cpp, and vLLM work by changing the URL. Local
            models may be less capable than cloud models and may lack file-reading support.</>}
        >
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
          <Field label="Model">
            <select
              aria-label="Local Model"
              value={settings.local_model}
              disabled={catalogBlocked.local}
              onChange={(e) => setSettings({ ...settings, local_model: e.target.value })}
              className={`${selectClass} font-mono disabled:cursor-not-allowed disabled:opacity-50`}
            >
              <option value="">Select a model…</option>
              {localCatalog?.models.map((model) => (
                <option key={model.id} value={model.id}>{model.display_name || model.id}</option>
              ))}
              {settings.local_model &&
                !discoveryInputChanged.local &&
                !localCatalog?.models.some((model) => model.id === settings.local_model) && (
                <option value={settings.local_model}>{settings.local_model} (saved; not currently listed)</option>
              )}
            </select>
            <CatalogStatus
              blocked={catalogBlocked.local}
              catalog={localCatalog}
              loading={catalogLoading.local}
              onRefresh={() => loadCatalog("local", settings, true)}
            />
          </Field>
          <Field label="API Key">
            <input
              aria-label="Local API Key"
              type="password"
              value={settings.local_api_key}
              onChange={(e) =>
                setSettings({ ...settings, local_api_key: e.target.value })
              }
              placeholder="usually empty for local servers"
              className={`${inputClass} font-mono`}
              autoComplete="off"
            />
          </Field>
        </ProviderGroup>

        <Field label="Max Concurrent Referee Passes">
          <div className="flex items-center gap-3">
            <input
              aria-label="Max Concurrent Referee Passes"
              type="range"
              min={1}
              max={10}
              value={settings.max_workers}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  max_workers: parseInt(e.target.value, 10),
                })
              }
              className="flex-1 accent-gray-900 dark:accent-gray-300"
            />
            <span className="text-sm font-mono text-gray-700 dark:text-gray-300 w-6 text-center">
              {settings.max_workers}
            </span>
          </div>
        </Field>

        <Field
          label="Step Timeout"
          help="Maximum time for each LLM call. Orientation and extraction calls use half this value."
        >
          <div className="flex items-center gap-3">
            <select
              aria-label="Step Timeout"
              value={settings.step_timeout_secs}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  step_timeout_secs: parseInt(e.target.value, 10),
                })
              }
              className={selectClass}
            >
              <option value={600}>10 minutes</option>
              <option value={1200}>20 minutes (default)</option>
              <option value={1800}>30 minutes</option>
              <option value={2700}>45 minutes</option>
              <option value={3600}>60 minutes</option>
            </select>
          </div>
        </Field>

        <Field
          label="Step Retries"
          help="Number of times to retry a failed step before giving up. Set to 0 for no retries."
        >
          <div className="flex items-center gap-3">
            <input
              aria-label="Step Retries"
              type="range"
              min={0}
              max={5}
              value={settings.max_retries}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  max_retries: parseInt(e.target.value, 10),
                })
              }
              className="flex-1 accent-gray-900 dark:accent-gray-300"
            />
            <span className="text-sm font-mono text-gray-700 dark:text-gray-300 w-6 text-center">
              {settings.max_retries}
            </span>
          </div>
        </Field>
      </div>
    </>
  );
}

function ExtractionSection({
  settings,
  setSettings,
  onSystemChange,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
  onSystemChange?: () => void;
}) {
  const [paddleInstalled, setPaddleInstalled] = useState(false);
  const handleEngineStatusChange = useCallback((engines: EngineStatus[]) => {
    setPaddleInstalled(
      engines.some(
        (engine) => engine.id === "paddleocr-vl-parser" && engine.installed,
      ),
    );
  }, []);

  return (
    <>
      <SectionHeader
        title="PDF Extraction"
        help="These options apply to PDFs. Pipeline always extracts LaTeX source natively."
      />

      <div className="space-y-4">
        <Field label="PDF Extraction Method">
          <div className="space-y-1.5">
            {settings.pdf_extractor === "marker" && (
              <div
                role="alert"
                className="rounded-lg border border-amber-300 bg-amber-50 p-3 text-sm text-amber-900 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-200"
              >
                <div className="font-medium">Marker is unavailable in Pipeline 0.9.0</div>
                <p className="mt-1 text-xs leading-relaxed">
                  Its compatible Python dependency closure contains known security vulnerabilities.
                  Choose PaddleOCR-VL, LLM extraction, or pdftotext below, then save Settings.
                  Pipeline will not run a previously installed Marker executable.
                </p>
              </div>
            )}
            {(
              [
                ["llm", "LLM", "Your configured provider reads the PDF and extracts it to Markdown in bounded page ranges. Most faithful, but slower and potentially costly."],
                ["paddleocr-vl-full", "Local engine: PaddleOCR-VL 1.6 Full Parser", "Layout-aware extraction with reading order, structured blocks, title hierarchy, formula metadata, and cross-page table reconstruction. Reuses the managed Q8 model."],
                ["pdftotext", "pdftotext (basic)", "Fast, but equations are lost."],
              ] as const
            ).map(([value, label, desc]) => (
              <label
                key={value}
                className={`flex items-start gap-3 rounded-lg border px-3 py-2.5 cursor-pointer transition-colors ${
                  settings.pdf_extractor === value
                    ? "border-gray-900 dark:border-gray-300 bg-gray-50 dark:bg-gray-800/50"
                    : "border-gray-200 dark:border-gray-700 hover:border-gray-300 dark:hover:border-gray-600"
                }`}
              >
                <input
                  type="radio"
                  name="pdf_extractor"
                  value={value}
                  checked={settings.pdf_extractor === value}
                  onChange={(e) =>
                    setSettings({ ...settings, pdf_extractor: e.target.value })
                  }
                  className="mt-0.5 accent-gray-900 dark:accent-gray-300"
                />
                <div>
                  <div className="text-sm font-medium text-gray-900 dark:text-gray-100">
                    {label}
                  </div>
                  <div className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
                    {desc}
                  </div>
                </div>
              </label>
            ))}
          </div>
        </Field>

        <div
          id="paddleocr-local-engine"
          tabIndex={-1}
          className="scroll-mt-4 rounded-lg focus:outline-none focus-visible:ring-2 focus-visible:ring-gray-400"
        >
          <EnginesPanel
            onSystemChange={onSystemChange}
            onEngineStatusChange={handleEngineStatusChange}
          />
        </div>

        {paddleInstalled && (
          <>
            <div className="pl-1 border-l-2 border-gray-200 dark:border-gray-700 ml-1">
            <SubsectionHeader
              label="PaddleOCR-VL recognition server"
              help="The Full Parser uses Pipeline's managed llama.cpp server for recognition; no separate llama.cpp installation is needed."
            />
            <div className="space-y-3 pl-4">
              <Field
                label="Concurrent pages"
                help="Automatic uses two slots on Apple Silicon and one elsewhere. Each slot receives a full 16K context."
              >
                <select
                  aria-label="PaddleOCR-VL concurrent pages"
                  value={settings.paddle_page_concurrency}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      paddle_page_concurrency: parseInt(e.target.value, 10),
                    })
                  }
                  className={selectClass}
                >
                  <option value={0}>Automatic (recommended)</option>
                  <option value={1}>1 page — lowest memory</option>
                  <option value={2}>2 pages — higher throughput</option>
                  <option value={3}>3 pages — high-memory workstation</option>
                  <option value={4}>4 pages — maximum throughput</option>
                </select>
              </Field>

              <Field
                label="Vision encoder batch"
                help="Larger batches can speed image encoding when enough GPU memory is available. They do not reduce OCR resolution."
              >
                <select
                  aria-label="PaddleOCR-VL vision encoder batch"
                  value={settings.paddle_mtmd_batch_tokens}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      paddle_mtmd_batch_tokens: parseInt(e.target.value, 10),
                    })
                  }
                  className={selectClass}
                >
                  <option value={0}>Automatic (recommended)</option>
                  <option value={512}>512 tokens — lower memory</option>
                  <option value={1024}>1,024 tokens — conservative</option>
                  <option value={2048}>2,048 tokens — faster prefill</option>
                  <option value={4096}>4,096 tokens — highest peak memory</option>
                </select>
              </Field>

              <Field label="Maximum page output">
                <select
                  aria-label="PaddleOCR-VL maximum page output"
                  value={settings.paddle_max_output_tokens}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      paddle_max_output_tokens: parseInt(e.target.value, 10),
                    })
                  }
                  className={selectClass}
                >
                  <option value={2048}>2,048 tokens — shorter pages</option>
                  <option value={4096}>4,096 tokens — recommended</option>
                  <option value={8192}>8,192 tokens — unusually dense pages</option>
                </select>
              </Field>

              <Field
                label="Page retries"
                help="Suspicious layout-aware pages are retried before the Full Parser records an extraction failure."
              >
                <select
                  aria-label="PaddleOCR-VL page retries"
                  value={settings.paddle_page_retries}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      paddle_page_retries: parseInt(e.target.value, 10),
                    })
                  }
                  className={selectClass}
                >
                  <option value={0}>No retries</option>
                  <option value={1}>1 retry — recommended</option>
                  <option value={2}>2 retries</option>
                  <option value={3}>3 retries</option>
                </select>
              </Field>

              <Field
                label="Flash Attention"
                help="Automatic is safest across platforms. Force it on when benchmarking a supported GPU; turn it off for compatibility troubleshooting."
              >
                <select
                  aria-label="PaddleOCR-VL Flash Attention"
                  value={settings.paddle_flash_attention}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      paddle_flash_attention: e.target.value,
                    })
                  }
                  className={selectClass}
                >
                  <option value="auto">Automatic (default)</option>
                  <option value="on">On</option>
                  <option value="off">Off</option>
                </select>
              </Field>
            </div>
            </div>

            <div className="pl-1 border-l-2 border-gray-200 dark:border-gray-700 ml-1">
              <SubsectionHeader
                label="Full parser structure"
                help="These options are included in the Full Parser cache fingerprint, so changing one creates a distinct cached result."
              />
              <div className="space-y-3 pl-4">
            <Toggle
              label="Layout detection and reading order"
              description="Run PP-DocLayoutV3 before recognition and retain semantic regions, coordinates, and reading order."
              checked={settings.paddle_full_layout_detection}
              onChange={(v) => setSettings({ ...settings, paddle_full_layout_detection: v })}
            />
            <Field label="Layout confidence threshold">
              <select
                aria-label="PaddleOCR-VL layout confidence threshold"
                value={settings.paddle_full_layout_threshold}
                onChange={(e) => setSettings({
                  ...settings,
                  paddle_full_layout_threshold: parseFloat(e.target.value),
                })}
                className={selectClass}
              >
                <option value={0.3}>0.30 — retain more regions</option>
                <option value={0.5}>0.50 — recommended</option>
                <option value={0.7}>0.70 — higher precision</option>
              </select>
            </Field>
            <Toggle
              label="Layout NMS"
              description="Suppress overlapping layout detections before region recognition."
              checked={settings.paddle_full_layout_nms}
              onChange={(v) => setSettings({ ...settings, paddle_full_layout_nms: v })}
            />
            <Field label="Overlapping layout boxes">
              <select
                aria-label="PaddleOCR-VL overlapping layout boxes"
                value={settings.paddle_full_layout_merge_bboxes_mode}
                onChange={(e) => setSettings({
                  ...settings,
                  paddle_full_layout_merge_bboxes_mode: e.target.value,
                })}
                className={selectClass}
              >
                <option value="large">Keep outer region — recommended</option>
                <option value="small">Keep inner region</option>
                <option value="union">Keep both</option>
              </select>
            </Field>
            <Toggle
              label="Merge layout blocks"
              description="Join cross-column or vertically staggered regions before producing reading-order blocks."
              checked={settings.paddle_full_merge_layout_blocks}
              onChange={(v) => setSettings({ ...settings, paddle_full_merge_layout_blocks: v })}
            />
            <Toggle
              label="OCR text inside images"
              description="Recognize labels and other text within image regions."
              checked={settings.paddle_full_ocr_image_blocks}
              onChange={(v) => setSettings({ ...settings, paddle_full_ocr_image_blocks: v })}
            />
            <Toggle
              label="Format block content"
              description="Retain block-level Markdown for tables, formulas, lists, and other semantic regions."
              checked={settings.paddle_full_format_block_content}
              onChange={(v) => setSettings({ ...settings, paddle_full_format_block_content: v })}
            />
            <Toggle
              label="Merge tables across pages"
              description="Reconstruct a continuing table as one logical table when page boundaries divide it."
              checked={settings.paddle_full_merge_tables}
              onChange={(v) => setSettings({ ...settings, paddle_full_merge_tables: v })}
            />
            <Toggle
              label="Relevel titles"
              description="Reconstruct a consistent multi-level heading hierarchy across the document."
              checked={settings.paddle_full_relevel_titles}
              onChange={(v) => setSettings({ ...settings, paddle_full_relevel_titles: v })}
            />
            <Toggle
              label="Retain formula numbers"
              description="Keep equation numbers in the Markdown and structured formula evidence."
              checked={settings.paddle_full_show_formula_numbers}
              onChange={(v) => setSettings({ ...settings, paddle_full_show_formula_numbers: v })}
            />
              </div>
            </div>
          </>
        )}

        <Toggle
          label="Reuse verified extraction cache"
          description="Reuse exact source-and-settings matches. PaddleOCR-VL Full Parser reuses its validated structure and image cache; verified LLM transcriptions avoid another provider call."
          checked={settings.reuse_pdf_extraction_cache}
          onChange={(v) =>
            setSettings({ ...settings, reuse_pdf_extraction_cache: v })
          }
        />

        <Field
          label="Extraction time budget"
          help="Applies to the complete extraction stage, including retries. An incomplete document fails before orientation instead of silently continuing."
        >
          <select
            aria-label="PDF extraction time budget"
            value={settings.pdf_extraction_timeout_secs}
            onChange={(e) =>
              setSettings({
                ...settings,
                pdf_extraction_timeout_secs: parseInt(e.target.value, 10),
              })
            }
            className={selectClass}
          >
            <option value={300}>5 minutes</option>
            <option value={600}>10 minutes</option>
            <option value={900}>15 minutes</option>
            <option value={1800}>30 minutes — recommended</option>
            <option value={3600}>60 minutes</option>
          </select>
        </Field>
      </div>
    </>
  );
}

function GeneralSection({
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
      <SectionHeader
        title="General"
      />

      <div className="space-y-5">
        <Field
          label="Appearance"
          help="Applied immediately, with no save needed. System follows your operating system's light or dark appearance."
        >
          <select
            aria-label="Appearance"
            value={theme}
            onChange={(event) => onThemeChange(event.target.value as ThemePreference)}
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
          onChange={(v) =>
            setSettings({ ...settings, verbose_logging: v })
          }
        />
        <Toggle
          label="Automatic revision reconciliation"
          description="When a matching completed run exists, add an AI comparison of addressed, remaining, and new concerns. This adds an LLM call to the run."
          checked={settings.auto_revision_reconciliation}
          onChange={(v) =>
            setSettings({ ...settings, auto_revision_reconciliation: v })
          }
        />
        <RunRetention settings={settings} setSettings={setSettings} />
      </div>
    </>
  );
}

function RunRetention({
  settings,
  setSettings,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
}) {
  const [usage, setUsage] = useState<{ count: number; bytes: number } | null>(null);
  const [purging, setPurging] = useState(false);
  const [purgeError, setPurgeError] = useState<string | null>(null);
  const [purgeResult, setPurgeResult] = useState<string | null>(null);

  const loadUsage = async () => {
    try {
      setUsage(await invoke<{ count: number; bytes: number }>("runs_disk_usage"));
    } catch {
      setUsage(null);
    }
  };
  useEffect(() => {
    void loadUsage();
  }, []);

  const fmtBytes = (n: number) =>
    n >= 1_000_000_000
      ? (n / 1_000_000_000).toFixed(1) + " GB"
      : n >= 1_000_000
        ? (n / 1_000_000).toFixed(0) + " MB"
        : (n / 1_000).toFixed(0) + " KB";

  const purgeNow = async () => {
    setPurging(true);
    setPurgeError(null);
    setPurgeResult(null);
    let purgeStarted = false;
    try {
      const preview = await invoke<{
        delete_count: number;
        delete_bytes: number;
        remaining_count: number;
        remaining_bytes: number;
        preview_token: string;
      }>("preview_purge_runs", {
        keep: settings.max_saved_runs,
        maxBytes: settings.max_saved_run_bytes,
      });
      if (preview.delete_count === 0) {
        setPurgeResult("No completed runs were beyond the configured limits.");
        await loadUsage();
        return;
      }
      const confirmed = window.confirm(
        "Purge run history?\n\n" +
        `This will permanently delete ${preview.delete_count} completed run${
          preview.delete_count === 1 ? "" : "s"
        } (${fmtBytes(preview.delete_bytes)}). ` +
        `${preview.remaining_count} run${preview.remaining_count === 1 ? "" : "s"} ` +
        `(${fmtBytes(preview.remaining_bytes)}) will remain.\n\n` +
        "Deleted run artifacts cannot be recovered.",
      );
      if (!confirmed) return;

      purgeStarted = true;
      const removed = await invoke<number>("purge_runs", {
        keep: settings.max_saved_runs,
        maxBytes: settings.max_saved_run_bytes,
        previewToken: preview.preview_token,
      });
      setPurgeResult(
        removed === 0
          ? "No completed runs were beyond the configured limits."
          : `Removed ${removed} completed run${removed === 1 ? "" : "s"}.`,
      );
      await loadUsage();
    } catch (error) {
      setPurgeError(
        `${purgeStarted
          ? "Run history could not be purged"
          : "The purge preview could not be loaded; no runs were deleted"}: ${
          error instanceof Error ? error.message : String(error)
        }`,
      );
    } finally {
      setPurging(false);
    }
  };

  return (
    <div>
      <div className="mb-1.5 flex items-center gap-1.5">
        <span className="text-sm font-medium text-gray-700 dark:text-gray-300">
          Run history retention
        </span>
        <InfoButton label="Run history retention">
          Past runs and their artifacts are stored under <code>~/.pipeline/runs/</code>. After
          each run, Pipeline removes the oldest completed runs until both limits hold. Set a
          limit to 0 to disable it.
        </InfoButton>
        {usage && (
          <span className="ml-auto text-xs font-normal text-gray-500 dark:text-gray-400">
            {usage.count} run{usage.count === 1 ? "" : "s"} · {fmtBytes(usage.bytes)}
          </span>
        )}
      </div>
      <div className="flex items-center gap-2">
        <input
          aria-label="Maximum saved runs"
          type="number"
          min={0}
          value={settings.max_saved_runs}
          onChange={(e) =>
            setSettings({ ...settings, max_saved_runs: Math.max(0, parseInt(e.target.value, 10) || 0) })
          }
          className="w-24 py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-gray-900/20"
        />
        <span className="text-xs text-gray-500">runs and</span>
        <input
          aria-label="Run history size limit in GB"
          type="number"
          min={0}
          max={1000}
          step={1}
          value={Math.round((settings.max_saved_run_bytes ?? 5_000_000_000) / 1_000_000_000)}
          onChange={(e) =>
            setSettings({
              ...settings,
              max_saved_run_bytes:
                Math.max(0, parseInt(e.target.value, 10) || 0) * 1_000_000_000,
            })
          }
          className="w-24 py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-gray-900/20"
        />
        <span className="text-xs text-gray-500">GB</span>
        <button
          onClick={purgeNow}
          disabled={
            purging ||
            (settings.max_saved_runs === 0 && settings.max_saved_run_bytes === 0)
          }
          className="px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          title={
            settings.max_saved_runs === 0 && settings.max_saved_run_bytes === 0
              ? "Set a limit above 0 to purge"
              : "Delete runs beyond the limits now"
          }
        >
          {purging ? "Purging…" : "Purge now"}
        </button>
      </div>
      {purgeError && (
        <p role="alert" className="mt-2 text-sm text-red-600 dark:text-red-400">
          {purgeError}
        </p>
      )}
      {purgeResult && (
        <p role="status" className="mt-2 text-sm text-green-700 dark:text-green-400">
          {purgeResult}
        </p>
      )}
    </div>
  );
}

/* ── Shared UI Components ────────────────────────────────────────── */

function effortLabel(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

function ModelPicker({
  provider,
  settings,
  catalog,
  blocked,
  allowSavedUnknown,
  loading,
  onChange,
  onRefresh,
}: {
  provider: CloudProvider;
  settings: Settings;
  catalog?: ModelCatalog;
  blocked: boolean;
  allowSavedUnknown: boolean;
  loading?: boolean;
  onChange: (selection: ModelSelection) => void;
  onRefresh: () => void;
}) {
  const selection = providerSelection(settings, provider);
  const value = encodeModelSelection(selection);
  const known = value === "automatic"
    || catalog?.models.some((model) => value === `pinned:${model.id}`);
  const selectValue = known || allowSavedUnknown ? value : "automatic";
  return (
    <>
      <select
        aria-label={`${provider} model`}
        value={selectValue}
        disabled={blocked}
        onChange={(event) => onChange(decodeModelSelection(event.target.value)!)}
        className={`${selectClass} disabled:cursor-not-allowed disabled:opacity-50`}
      >
        <option value="automatic">
          Automatic — {catalog?.transport === "api" ? "recommended available model" : "installed CLI default"}
        </option>
        {!!catalog?.models.length && (
          <optgroup label="Available models">
            {catalog.models.map((model) => (
              <option key={model.id} value={`pinned:${model.id}`} disabled={model.deprecated}>
                {model.display_name || model.id}{model.is_default ? " (default)" : ""}{model.deprecated ? " (deprecated)" : ""}
              </option>
            ))}
          </optgroup>
        )}
        {!known && allowSavedUnknown && (
          <option value={value}>
            {selection.mode === "role"
              ? `${effortLabel(selection.role)} (saved role; choose a model)`
              : selection.mode === "pinned"
                ? `${selection.model} (saved; not currently listed)`
                : "Automatic"}
          </option>
        )}
      </select>
      <CatalogStatus
        blocked={blocked}
        catalog={catalog}
        loading={loading}
        onRefresh={onRefresh}
      />
    </>
  );
}

function CatalogStatus({
  blocked = false,
  catalog,
  loading,
  onRefresh,
}: {
  blocked?: boolean;
  catalog?: ModelCatalog;
  loading?: boolean;
  onRefresh: () => void;
}) {
  return (
    <div className="mt-1.5 flex items-start justify-between gap-3 text-[11px] text-gray-500 dark:text-gray-400">
      <span>
        {loading ? "Discovering models…" : blocked
          ? "Save settings or Refresh to discover models for these values"
          : catalog
          ? `${catalog.transport.toUpperCase()} · ${catalog.source_version || catalog.source}${catalog.stale ? " · stale" : ""}`
          : "Catalog not loaded"}
        {catalog?.warning && <span className="block text-amber-700 dark:text-amber-300">{catalog.warning}</span>}
      </span>
      <button type="button" onClick={onRefresh} disabled={loading} className="shrink-0 underline disabled:opacity-40">
        Refresh
      </button>
    </div>
  );
}

const selectClass =
  "w-full py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-gray-900/20 dark:focus:ring-gray-100/20 transition-[box-shadow,color,background-color,border-color]";

const inputClass =
  "w-full py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-gray-900/20 dark:focus:ring-gray-100/20 transition-[box-shadow,color,background-color,border-color]";

function SectionHeader({
  title,
  description,
  help,
}: {
  title: string;
  description?: string;
  help?: React.ReactNode;
}) {
  return (
    <div className="mb-6">
      <div className="flex items-center gap-1.5">
        <h3 className="text-lg font-semibold text-gray-900 dark:text-gray-100">
          {title}
        </h3>
        {help && <InfoButton label={title}>{help}</InfoButton>}
      </div>
      {description && (
        <p className="text-sm text-gray-500 dark:text-gray-400 mt-1">
          {description}
        </p>
      )}
    </div>
  );
}

function Field({
  label,
  help,
  children,
}: {
  label: string;
  help?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div>
      <div className="mb-1.5 flex items-center gap-1.5 text-sm font-medium text-gray-700 dark:text-gray-300">
        <span>{label}</span>
        {help && <InfoButton label={label}>{help}</InfoButton>}
      </div>
      {children}
    </div>
  );
}

function SubsectionHeader({
  label,
  help,
}: {
  label: string;
  help?: React.ReactNode;
}) {
  return (
    <div className="mb-3 flex items-center gap-1.5 pl-4 text-xs font-medium uppercase tracking-wider text-gray-500 dark:text-gray-400">
      <span>{label}</span>
      {help && <InfoButton label={label}>{help}</InfoButton>}
    </div>
  );
}

function Toggle({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div
      onClick={() => onChange(!checked)}
      className="flex cursor-pointer items-start gap-3 rounded-lg p-3 transition-colors hover:bg-gray-50 dark:hover:bg-gray-800/40"
    >
      <button
        type="button"
        role="switch"
        aria-label={label}
        aria-checked={checked}
        onClick={(event) => {
          event.stopPropagation();
          onChange(!checked);
        }}
        className={`relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors mt-0.5 ${
          checked
            ? "bg-gray-900 dark:bg-gray-200"
            : "bg-gray-300 dark:bg-gray-600"
        }`}
      >
        <span
          className={`inline-block h-3.5 w-3.5 rounded-full bg-white dark:bg-gray-900 transition-transform ${
            checked ? "translate-x-[18px]" : "translate-x-[3px]"
          }`}
        />
      </button>
      <div className="flex-1">
        <div className="flex items-center gap-1.5 text-sm font-medium text-gray-900 dark:text-gray-100">
          <span>{label}</span>
          <InfoButton label={label}>{description}</InfoButton>
        </div>
      </div>
    </div>
  );
}
