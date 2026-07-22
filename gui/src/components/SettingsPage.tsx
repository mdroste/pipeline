import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import type { ModelCatalog, ModelSelection, Settings } from "../lib/types";
import EnginesPanel from "./EnginesPanel";

interface Props {
  onClose: () => void;
  dark: boolean;
  onDarkChange: (v: boolean) => void;
}

type Section = "llm" | "extraction" | "general";

export default function SettingsPage({ onClose, dark, onDarkChange }: Props) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [section, setSection] = useState<Section>("llm");
  const [loadError, setLoadError] = useState<string | null>(null);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [catalogs, setCatalogs] = useState<Record<string, ModelCatalog>>({});
  const [catalogLoading, setCatalogLoading] = useState<Record<string, boolean>>({});

  // Clear the "saved" indicator after 2 seconds, with proper cleanup
  useEffect(() => {
    if (!saved) return;
    const timer = setTimeout(() => setSaved(false), 2000);
    return () => clearTimeout(timer);
  }, [saved]);

  useEffect(() => {
    invoke<{ settings: Settings; warnings: string[] }>("get_settings")
      .then((resp) => {
        setSettings(resp.settings);
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
    setCatalogLoading((old) => ({ ...old, [provider]: true }));
    try {
      const catalog = await invoke<ModelCatalog>("get_model_catalog", {
        provider,
        settings: current,
        refresh,
      });
      setCatalogs((old) => ({ ...old, [provider]: catalog }));
    } catch (error) {
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
    } finally {
      setCatalogLoading((old) => ({ ...old, [provider]: false }));
    }
  };

  // Refetch only when the transport changes, not on every API-key keystroke.
  useEffect(() => {
    if (!settings) return;
    for (const provider of ["claude", "codex", "gemini", "local"]) {
      void loadCatalog(provider, settings);
    }
  }, [
    settings?.anthropic_api_key ? "api" : "cli",
    settings?.openai_api_key ? "api" : "cli",
    settings?.google_api_key ? "api" : "cli",
    settings?.local_base_url,
  ]);

  const handleSave = async () => {
    if (!settings) return;
    setSaving(true);
    setSaved(false);
    try {
      await invoke("save_settings", { settings });
      setWarnings([]); // Clear warnings after successful save
      setSaved(true);
    } catch (e) {
      console.error("Failed to save settings:", e);
      alert(`Failed to save: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSaving(false);
    }
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
          onClick={onClose}
          className="text-sm text-gray-600 dark:text-gray-300 hover:underline"
        >
          Go back
        </button>
      </div>
    );
  }

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
    <div className="flex h-full">
      {/* Sidebar nav */}
      <div className="w-48 shrink-0 border-r border-gray-200 dark:border-gray-700 bg-gray-50/50 dark:bg-gray-900/50 p-4 flex flex-col">
        <h2 className="text-sm font-semibold text-gray-400 dark:text-gray-500 uppercase tracking-wider mb-4 px-2">
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
        <button
          onClick={onClose}
          className="flex items-center gap-2 px-2.5 py-2 text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 transition-colors"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M10.5 19.5L3 12m0 0l7.5-7.5M3 12h18" />
          </svg>
          Back
        </button>
      </div>

      {/* Content area */}
      <div className="flex-1 overflow-y-auto">
        {warnings.length > 0 && (
          <div className="mx-8 mt-6 px-4 py-3 rounded-lg bg-amber-50 dark:bg-amber-950 border border-amber-200 dark:border-amber-800 text-sm text-amber-800 dark:text-amber-300">
            {warnings.map((w, i) => (
              <p key={i}>{w}</p>
            ))}
            <p className="mt-1 text-amber-600 dark:text-amber-400 text-xs">
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
              catalogLoading={catalogLoading}
              loadCatalog={loadCatalog}
            />
          )}
          {section === "extraction" && (
            <ExtractionSection settings={settings} setSettings={setSettings} />
          )}
          {section === "general" && (
            <GeneralSection settings={settings} setSettings={setSettings} dark={dark} onDarkChange={onDarkChange} />
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
              <span className="text-sm text-green-600 dark:text-green-400">Settings saved.</span>
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
  children,
}: {
  title: string;
  active: boolean;
  hasApiKey?: boolean;
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
  catalogLoading,
  loadCatalog,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
  catalogs: Record<string, ModelCatalog>;
  catalogLoading: Record<string, boolean>;
  loadCatalog: (provider: string, settings: Settings, refresh?: boolean) => Promise<void>;
}) {
  return (
    <>
      <SectionHeader
        title="Models"
        description="Configure models for each provider. The preferred provider is used for steps that don't specify an agent."
      />

      <div className="space-y-5">
        <Field label="Preferred Provider">
          <select
            value={settings.preferred_provider}
            onChange={(e) =>
              setSettings({ ...settings, preferred_provider: e.target.value })
            }
            className={selectClass}
          >
            <option value="claude">Claude</option>
            <option value="codex">Codex (OpenAI)</option>
            <option value="gemini">Gemini (Google)</option>
            <option value="local">Local (Ollama / OpenAI-compatible)</option>
          </select>
          <p className="text-xs text-gray-400 dark:text-gray-500 mt-1.5">
            Used when a pipeline step doesn't specify an explicit agent.
          </p>
        </Field>

        {/* Claude */}
        <ProviderGroup title="Claude" active={settings.preferred_provider === "claude"} hasApiKey={!!settings.anthropic_api_key}>
          <Field label="API Key">
            <input
              type="password"
              value={settings.anthropic_api_key}
              onChange={(e) =>
                setSettings({ ...settings, anthropic_api_key: e.target.value })
              }
              placeholder="sk-ant-... (optional, enables direct API)"
              className={`${inputClass} font-mono`}
              autoComplete="off"
            />
            <p className="text-xs text-gray-400 dark:text-gray-500 mt-1.5">
              Bypasses Claude CLI for faster calls. Leave empty to use CLI (subscription).
            </p>
          </Field>
          <Field label="Model">
            <ModelPicker
              provider="claude"
              settings={settings}
              catalog={catalogs.claude}
              loading={catalogLoading.claude}
              onChange={(selection) => setSettings(withProviderSelection(settings, "claude", selection))}
              onRefresh={() => loadCatalog("claude", settings, true)}
            />
          </Field>
          <Field label="Thinking Effort">
            <select
              value={settings.claude_effort}
              onChange={(e) =>
                setSettings({ ...settings, claude_effort: e.target.value })
              }
              className={selectClass}
            >
              <option value="">Default</option>
              {effortOptions(catalogs.claude, providerSelection(settings, "claude"), ["low", "medium", "high", "max"]).map((effort) => (
                <option key={effort} value={effort}>{effortLabel(effort)}</option>
              ))}
            </select>
          </Field>
        </ProviderGroup>

        {/* Codex / OpenAI */}
        <ProviderGroup title="Codex (OpenAI)" active={settings.preferred_provider === "codex"} hasApiKey={!!settings.openai_api_key}>
          <Field label="API Key">
            <input
              type="password"
              value={settings.openai_api_key}
              onChange={(e) =>
                setSettings({ ...settings, openai_api_key: e.target.value })
              }
              placeholder="sk-... (optional, enables direct API)"
              className={`${inputClass} font-mono`}
              autoComplete="off"
            />
            <p className="text-xs text-gray-400 dark:text-gray-500 mt-1.5">
              Bypasses Codex CLI for faster calls. Leave empty to use CLI.
            </p>
          </Field>
          <Field label="Model">
            <ModelPicker
              provider="codex"
              settings={settings}
              catalog={catalogs.codex}
              loading={catalogLoading.codex}
              onChange={(selection) => setSettings(withProviderSelection(settings, "codex", selection))}
              onRefresh={() => loadCatalog("codex", settings, true)}
            />
          </Field>
          <Field label="Reasoning Effort">
            <select
              value={settings.codex_effort}
              onChange={(e) =>
                setSettings({ ...settings, codex_effort: e.target.value })
              }
              className={selectClass}
            >
              <option value="">Default</option>
              {effortOptions(catalogs.codex, providerSelection(settings, "codex"), ["low", "medium", "high"]).map((effort) => (
                <option key={effort} value={effort}>{effortLabel(effort)}</option>
              ))}
            </select>
          </Field>
        </ProviderGroup>

        {/* Gemini */}
        <ProviderGroup title="Gemini (Google)" active={settings.preferred_provider === "gemini"} hasApiKey={!!settings.google_api_key}>
          <Field label="API Key">
            <input
              type="password"
              value={settings.google_api_key}
              onChange={(e) =>
                setSettings({ ...settings, google_api_key: e.target.value })
              }
              placeholder="AI... (optional, enables direct API)"
              className={`${inputClass} font-mono`}
              autoComplete="off"
            />
            <p className="text-xs text-gray-400 dark:text-gray-500 mt-1.5">
              Bypasses Gemini CLI for faster calls. Leave empty to use CLI.
            </p>
          </Field>
          <Field label="Model">
            <ModelPicker
              provider="gemini"
              settings={settings}
              catalog={catalogs.gemini}
              loading={catalogLoading.gemini}
              onChange={(selection) => setSettings(withProviderSelection(settings, "gemini", selection))}
              onRefresh={() => loadCatalog("gemini", settings, true)}
            />
          </Field>
        </ProviderGroup>

        {/* Local (Ollama / OpenAI-compatible) */}
        <ProviderGroup title="Local (Ollama)" active={settings.preferred_provider === "local"}>
          <p className="text-xs text-gray-500 dark:text-gray-400 -mt-1">
            Runs against any local OpenAI-compatible server. With{" "}
            <a
              href="https://ollama.com"
              onClick={(e) => {
                e.preventDefault();
                openUrl("https://ollama.com");
              }}
              className="underline cursor-pointer hover:text-gray-700 dark:hover:text-gray-300"
            >
              ollama.com
            </a>{" "}
            installed: <span className="font-mono">ollama pull llama3.3</span>, then enter the
            model name below. LM Studio, llama.cpp, and vLLM work by changing the URL.
            Local models are weaker than cloud models and may lack file-reading (tool) support.
          </p>
          <Field label="Server URL">
            <input
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
              value={settings.local_model}
              onChange={(e) => setSettings({ ...settings, local_model: e.target.value })}
              className={`${selectClass} font-mono`}
            >
              <option value="">Select a model…</option>
              {catalogs.local?.models.map((model) => (
                <option key={model.id} value={model.id}>{model.display_name || model.id}</option>
              ))}
              {settings.local_model && !catalogs.local?.models.some((model) => model.id === settings.local_model) && (
                <option value={settings.local_model}>{settings.local_model} (saved; not currently listed)</option>
              )}
            </select>
            <CatalogStatus catalog={catalogs.local} loading={catalogLoading.local} onRefresh={() => loadCatalog("local", settings, true)} />
          </Field>
          <Field label="API Key">
            <input
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

        <Field label="Step Timeout">
          <div className="flex items-center gap-3">
            <select
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
          <p className="text-xs text-gray-400 dark:text-gray-500 mt-1.5">
            Max time each LLM call can run before being killed. Orientation and extraction steps use half this value.
          </p>
        </Field>

        <Field label="Step Retries">
          <div className="flex items-center gap-3">
            <input
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
          <p className="text-xs text-gray-400 dark:text-gray-500 mt-1.5">
            Number of times to retry a failed step before giving up. Set to 0 for no retries.
          </p>
        </Field>
      </div>
    </>
  );
}

function ExtractionSection({
  settings,
  setSettings,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
}) {
  const useMarker = settings.pdf_extractor === "marker";

  return (
    <>
      <SectionHeader
        title="PDF Extraction"
        description="Configure how Pipeline extracts text from PDF papers. LaTeX source is always extracted natively."
      />

      <div className="space-y-5">
        <Field label="PDF Extraction Method">
          <div className="space-y-2">
            {(
              [
                ["llm", "LLM (default)", "Your configured provider reads the PDF and extracts it to Markdown, verified page-by-page. Best quality."],
                ["marker", "Local engine: marker-pdf", "Local extraction, no LLM cost. Install it below."],
                ["pdftotext", "pdftotext (basic)", "Fast, but equations are lost."],
              ] as const
            ).map(([value, label, desc]) => (
              <label
                key={value}
                className={`flex items-start gap-3 p-3 rounded-lg border cursor-pointer transition-colors ${
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

        {/* marker-pdf options — only shown when marker is active */}
        {useMarker && (
          <div className="pl-1 border-l-2 border-gray-200 dark:border-gray-700 ml-1">
            <p className="text-xs font-medium text-gray-400 dark:text-gray-500 uppercase tracking-wider mb-3 pl-4">
              marker-pdf options
            </p>
            <div className="space-y-2 pl-4">
              <Toggle
                label="Disable OCR"
                description="Skip optical character recognition. Much faster for PDFs with embedded text (most academic papers)."
                checked={settings.marker_disable_ocr}
                onChange={(v) =>
                  setSettings({ ...settings, marker_disable_ocr: v })
                }
              />
              <Toggle
                label="Disable image extraction"
                description="Skip extracting images from the PDF. Faster and uses less memory. When enabled, extracted figures appear in the run's artifact explorer."
                checked={settings.marker_disable_images}
                onChange={(v) =>
                  setSettings({ ...settings, marker_disable_images: v })
                }
              />
            </div>
          </div>
        )}

        <EnginesPanel />
      </div>
    </>
  );
}

function GeneralSection({
  settings,
  setSettings,
  dark,
  onDarkChange,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
  dark: boolean;
  onDarkChange: (v: boolean) => void;
}) {
  return (
    <>
      <SectionHeader
        title="General"
        description="Appearance and other application settings."
      />

      <div className="space-y-5">
        <Toggle
          label="Dark mode"
          description="Applied immediately, no save needed. Follows the system appearance until you set it here."
          checked={dark}
          onChange={onDarkChange}
        />
        <Toggle
          label="Verbose console logging"
          description="Show full LLM subprocess output in the console (commands, stdout, stderr). Useful for debugging."
          checked={settings.verbose_logging}
          onChange={(v) =>
            setSettings({ ...settings, verbose_logging: v })
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

  const loadUsage = () => {
    invoke<{ count: number; bytes: number }>("runs_disk_usage")
      .then(setUsage)
      .catch(() => setUsage(null));
  };
  useEffect(loadUsage, []);

  const fmtBytes = (n: number) =>
    n >= 1_000_000_000
      ? (n / 1_000_000_000).toFixed(1) + " GB"
      : n >= 1_000_000
        ? (n / 1_000_000).toFixed(0) + " MB"
        : (n / 1_000).toFixed(0) + " KB";

  const purgeNow = async () => {
    setPurging(true);
    try {
      // Passing the configured cap (0 keeps everything, so purge to a large
      // default only when unlimited) — here we honour the user's setting.
      await invoke("purge_runs", { keep: settings.max_saved_runs });
      loadUsage();
    } finally {
      setPurging(false);
    }
  };

  return (
    <div>
      <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
        Run history retention
      </label>
      <p className="text-sm text-gray-500 dark:text-gray-400 mb-2">
        Past runs are stored under <code>~/.pipeline/runs/</code> with their artifacts and page
        images, which add up. Keep at most this many; the oldest are removed after each run.
        0 keeps everything.
        {usage && (
          <>
            {" "}Currently {usage.count} run{usage.count === 1 ? "" : "s"}, {fmtBytes(usage.bytes)}.
          </>
        )}
      </p>
      <div className="flex items-center gap-2">
        <input
          type="number"
          min={0}
          value={settings.max_saved_runs}
          onChange={(e) =>
            setSettings({ ...settings, max_saved_runs: Math.max(0, parseInt(e.target.value, 10) || 0) })
          }
          className="w-24 py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-gray-900/20"
        />
        <button
          onClick={purgeNow}
          disabled={purging || settings.max_saved_runs === 0}
          className="px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          title={settings.max_saved_runs === 0 ? "Set a limit above 0 to purge" : "Delete runs beyond the limit now"}
        >
          {purging ? "Purging…" : "Purge now"}
        </button>
      </div>
    </div>
  );
}

/* ── Shared UI Components ────────────────────────────────────────── */

type CloudProvider = "claude" | "codex" | "gemini";

function providerTransport(settings: Settings, provider: string): "cli" | "api" {
  if (provider === "local") return "api";
  if (provider === "claude") return settings.anthropic_api_key ? "api" : "cli";
  if (provider === "codex") return settings.openai_api_key ? "api" : "cli";
  return settings.google_api_key ? "api" : "cli";
}

function providerSelection(settings: Settings, provider: CloudProvider): ModelSelection {
  const transport = providerTransport(settings, provider);
  const selection = provider === "claude"
    ? (transport === "cli" ? settings.claude_cli_model_selection : settings.claude_api_model_selection)
    : provider === "codex"
      ? (transport === "cli" ? settings.codex_cli_model_selection : settings.codex_api_model_selection)
      : (transport === "cli" ? settings.gemini_cli_model_selection : settings.gemini_api_model_selection);
  // Settings written before structured selection existed.
  if (selection) return selection;
  const legacy = provider === "claude" ? settings.claude_model : provider === "codex" ? settings.codex_model : settings.gemini_model;
  return legacy ? { mode: "pinned", model: legacy } : { mode: "automatic" };
}

function withProviderSelection(
  settings: Settings,
  provider: CloudProvider,
  selection: ModelSelection,
): Settings {
  const transport = providerTransport(settings, provider);
  const patch: Partial<Settings> = {};
  if (provider === "claude") {
    if (transport === "cli") patch.claude_cli_model_selection = selection;
    else patch.claude_api_model_selection = selection;
    patch.claude_model = "";
  } else if (provider === "codex") {
    if (transport === "cli") patch.codex_cli_model_selection = selection;
    else patch.codex_api_model_selection = selection;
    patch.codex_model = "";
  } else {
    if (transport === "cli") patch.gemini_cli_model_selection = selection;
    else patch.gemini_api_model_selection = selection;
    patch.gemini_model = "";
  }
  return { ...settings, ...patch };
}

function selectionValue(selection: ModelSelection): string {
  if (selection.mode === "automatic") return "automatic";
  if (selection.mode === "role") return `role:${selection.role}`;
  return `pinned:${selection.model}`;
}

function parseSelection(value: string): ModelSelection {
  if (value === "automatic") return { mode: "automatic" };
  if (value.startsWith("role:")) return { mode: "role", role: value.slice(5) };
  return { mode: "pinned", model: value.slice(7) };
}

function effortLabel(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

function effortOptions(
  catalog: ModelCatalog | undefined,
  selection: ModelSelection,
  fallback: string[],
): string[] {
  if (!catalog) return fallback;
  const id = selection.mode === "pinned"
    ? selection.model
    : selection.mode === "role"
      ? catalog.roles.find((role) => role.id === selection.role)?.model
      : catalog.default_model || catalog.recommended_model;
  const efforts = catalog.models.find((model) => model.id === id)?.supported_efforts;
  return efforts?.length ? efforts : fallback;
}

function ModelPicker({
  provider,
  settings,
  catalog,
  loading,
  onChange,
  onRefresh,
}: {
  provider: CloudProvider;
  settings: Settings;
  catalog?: ModelCatalog;
  loading?: boolean;
  onChange: (selection: ModelSelection) => void;
  onRefresh: () => void;
}) {
  const selection = providerSelection(settings, provider);
  const value = selectionValue(selection);
  const known = value === "automatic"
    || catalog?.roles.some((role) => value === `role:${role.id}`)
    || catalog?.models.some((model) => value === `pinned:${model.id}`);
  return (
    <>
      <select value={value} onChange={(event) => onChange(parseSelection(event.target.value))} className={selectClass}>
        <option value="automatic">
          Automatic — {catalog?.transport === "api" ? "recommended available model" : "installed CLI default"}
        </option>
        {!!catalog?.roles.length && (
          <optgroup label="Stable roles">
            {catalog.roles.map((role) => (
              <option key={role.id} value={`role:${role.id}`}>{role.label} — {role.model}</option>
            ))}
          </optgroup>
        )}
        {!!catalog?.models.length && (
          <optgroup label="Pin exact model">
            {catalog.models.map((model) => (
              <option key={model.id} value={`pinned:${model.id}`} disabled={model.deprecated}>
                {model.display_name || model.id}{model.is_default ? " (default)" : ""}{model.deprecated ? " (deprecated)" : ""}
              </option>
            ))}
          </optgroup>
        )}
        {!known && <option value={value}>{selection.mode === "pinned" ? selection.model : value} (saved; not currently listed)</option>}
      </select>
      <CatalogStatus catalog={catalog} loading={loading} onRefresh={onRefresh} />
    </>
  );
}

function CatalogStatus({ catalog, loading, onRefresh }: { catalog?: ModelCatalog; loading?: boolean; onRefresh: () => void }) {
  return (
    <div className="mt-1.5 flex items-start justify-between gap-3 text-[11px] text-gray-400 dark:text-gray-500">
      <span>
        {loading ? "Discovering models…" : catalog
          ? `${catalog.transport.toUpperCase()} · ${catalog.source_version || catalog.source}${catalog.stale ? " · stale" : ""}`
          : "Catalog not loaded"}
        {catalog?.warning && <span className="block text-amber-600 dark:text-amber-400">{catalog.warning}</span>}
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
}: {
  title: string;
  description: string;
}) {
  return (
    <div className="mb-6">
      <h3 className="text-lg font-semibold text-gray-900 dark:text-gray-100">
        {title}
      </h3>
      <p className="text-sm text-gray-500 dark:text-gray-400 mt-1">
        {description}
      </p>
    </div>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1.5">
        {label}
      </label>
      {children}
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
    <label className="flex items-start gap-3 p-3 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-800/40 cursor-pointer transition-colors">
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
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
        <div className="text-sm font-medium text-gray-900 dark:text-gray-100">
          {label}
        </div>
        <div className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
          {description}
        </div>
      </div>
    </label>
  );
}
