import { useCallback, useEffect, useRef, useState } from "react";
import { SettingsNavigation } from "./settings/SettingsNavigation";
import { useSettingsDraft } from "./settings/useSettingsDraft";
import {
  type Section,
  type SettingsSection,
  sectionForTarget,
  SECTION_INFO,
  PROVIDER_LINKS,
  WORKFLOW_LINKS,
  focusSettingsTarget,
} from "./settings/navigation";
export type { SettingsSection } from "./settings/navigation";
import {
  SettingsCard,
  ProviderHeader,
  SectionHeader,
  Toggle,
} from "./settings/controls";
import {
  ModelDefaultsSection,
  WorkflowSection,
} from "./settings/WorkflowDefaults";
import {
  LocalProviderSection,
  ProvidersSection,
} from "./settings/ProviderSettings";
import { ExtractionSection } from "./settings/PdfExtractionSettings";
import { RunRetention } from "./settings/HistorySettings";
import {
  AppearanceSettings,
  EditorSettings,
  StartupSettings,
  ConversationBehavior,
} from "./settings/PreferenceSettings";
import ConversationTitleSettings from "./settings/ConversationTitleSettings";
import NotificationSettings from "./settings/NotificationSettings";
import AdvancedSettings from "./settings/AdvancedSettings";
import ConnectionStatus from "./settings/ConnectionStatus";
import {
  SettingsOperations,
  ReviewSaveState,
  ReviewSaveFeedback,
  type SaveState,
} from "./settings/SaveState";
import {
  searchSettings,
  settingsSearchEntries,
  type SettingsSearchEntry,
} from "./settings/search";
import { confirmDialog } from "./DialogService";
import type { DepsReport, Settings } from "../lib/types";
import type { ThemePreference } from "../lib/theme";
import { useAppPreferences } from "../lib/appPreferences";
import StorageSettings from "./StorageSettings";
import WorkspaceResearchDataSettings from "./WorkspaceResearchDataSettings";
import "./SettingsPage.css";

interface Props {
  onClose: () => void;
  onDirtyChange?: (dirty: boolean) => void;
  showBack?: boolean;
  theme: ThemePreference;
  onThemeChange: (theme: ThemePreference) => void;
  onSystemChange?: () => void;
  initialSection?: SettingsSection;
  targetId?: string;
  navigationKey?: number;
  dependencies?: DepsReport | null;
}

export default function SettingsPage({
  onClose,
  onDirtyChange,
  showBack = true,
  theme,
  onThemeChange,
  onSystemChange,
  initialSection = "general",
  targetId,
  navigationKey = 0,
  dependencies,
}: Props) {
  const draft = useSettingsDraft(onSystemChange);
  const {
    settings,
    setSettings: setReviewSettings,
    loading,
    loadError,
    saving,
    saved,
    dirty,
    saveError,
    warnings,
    retrySave,
    providerCatalogs,
  } = draft;
  const {
    catalogs,
    catalogLoading,
    catalogBlocked,
    discoveryInputChanged,
    loadCatalog,
  } = providerCatalogs;
  const [storageSaving, setStorageSaving] = useState(false);
  const [researchDataSaving, setResearchDataSaving] = useState(false);
  const [operations, setOperations] = useState<Record<string, SaveState>>({});
  const reportOperation = useCallback(
    (id: string, state: SaveState | null) =>
      setOperations((old) => {
        const next = { ...old };
        if (state) next[id] = state;
        else delete next[id];
        return next;
      }),
    [],
  );
  const [section, setSection] = useState<Section>(
    sectionForTarget(initialSection, targetId),
  );
  const [query, setQuery] = useState("");
  const [destination, setDestination] = useState<string | undefined>();
  const [focusKey, setFocusKey] = useState(0);
  const [saveScope, setSaveScope] = useState("");
  const lastFocusedCard = useRef("");
  const setSettings = (next: Settings) => {
    const card = document.activeElement?.closest(".settings-card");
    setSaveScope(card?.id || lastFocusedCard.current);
    setReviewSettings(next);
  };
  const contentRef = useRef<HTMLDivElement>(null);
  const preferences = useAppPreferences();
  const operationPending = Object.values(operations).some(
    (state) => state.pending,
  );
  const savePending =
    dirty || saving || storageSaving || researchDataSaving || operationPending;
  useEffect(() => {
    onDirtyChange?.(savePending);
  }, [onDirtyChange, savePending]);
  useEffect(() => () => onDirtyChange?.(false), [onDirtyChange]);
  useEffect(() => {
    setSection(sectionForTarget(initialSection, targetId));
    setQuery("");
    setDestination(
      targetId ??
        (
          {
            extraction: "workflow-extraction",
            llm: "workflow-models",
            workspace: "workspace-provider",
            "api-keys": "anthropic-provider",
          } as Partial<Record<SettingsSection, string>>
        )[initialSection],
    );
    setFocusKey((key) => key + 1);
  }, [initialSection, targetId, navigationKey]);
  useEffect(() => {
    if (loading || !destination) return;
    const frame = requestAnimationFrame(() => focusSettingsTarget(destination));
    return () => cancelAnimationFrame(frame);
  }, [loading, section, destination, focusKey]);
  const canLeaveSection = async () =>
    !(storageSaving || researchDataSaving || operationPending) ||
    (await confirmDialog(
      "This section has unfinished changes. Leave this section anyway?",
      { confirmLabel: "Leave section" },
    ));
  const selectSection = async (next: Section, target?: string) => {
    if (next !== section && !(await canLeaveSection())) return;
    setSection(next);
    setQuery("");
    setDestination(target);
    setFocusKey((key) => key + 1);
    if (contentRef.current) contentRef.current.scrollTop = 0;
  };
  const handleClose = async () => {
    if (
      savePending &&
      !(await confirmDialog(
        "Settings changes have not finished saving. Leave anyway?",
      ))
    )
      return;
    onClose();
  };
  if (loading)
    return (
      <div className="p-8" role="status">
        Loading settings…
      </div>
    );
  if (loadError || !settings)
    return (
      <div className="p-8">
        <p>Failed to load settings{loadError ? `: ${loadError}` : "."}</p>
        <button onClick={handleClose}>Go back</button>
      </div>
    );
  const pageInfo = SECTION_INFO[section];
  const results = searchSettings(
    settingsSearchEntries(settings, preferences, theme),
    query,
  );
  const openResult = (entry: SettingsSearchEntry) =>
    void selectSection(entry.section, entry.target);
  return (
    <SettingsOperations.Provider value={reportOperation}>
      <ReviewSaveState.Provider
        value={{
          pending: dirty || saving,
          error: saveError ? `Could not save settings: ${saveError}` : null,
          saved,
          retry: retrySave,
          scope: saveScope,
        }}
      >
        <div data-testid="settings-page" className="settings-shell">
          <SettingsNavigation
            section={section}
            selectSection={(next) => void selectSection(next)}
            showBack={showBack}
            handleClose={handleClose}
            status={
              saveError ? (
                <span className="settings-error">
                  Some Review settings could not be saved.{" "}
                  <button onClick={retrySave}>Retry</button>
                </span>
              ) : savePending ? (
                "Changes pending…"
              ) : (
                "Preferences save automatically."
              )
            }
          />
          <div
            ref={contentRef}
            className="settings-content"
            onFocusCapture={(event) => {
              const card = event.target.closest(".settings-card");
              if (card?.id) lastFocusedCard.current = card.id;
            }}
          >
            <div className="settings-search-bar">
              <label className="sr-only" htmlFor="settings-search">
                Search settings
              </label>
              <input
                id="settings-search"
                type="search"
                value={query}
                placeholder="Search settings…"
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Escape") setQuery("");
                }}
              />
              <span className="settings-search-hint">Find a preference</span>
            </div>
            {query.trim() ? (
              <div className="settings-page-body settings-search-results">
                <h1>Search results</h1>
                <p role="status" className="settings-row-description">
                  {results.length}{" "}
                  {results.length === 1 ? "setting" : "settings"} found
                </p>
                {results.map((entry) => (
                  <button
                    type="button"
                    key={`${entry.section}:${entry.target}`}
                    onClick={() => openResult(entry)}
                    className="settings-search-result"
                  >
                    <span>
                      <strong>{entry.label}</strong>
                      <span className="block settings-row-description">
                        {SECTION_INFO[entry.section].title} ·{" "}
                        {entry.description}
                      </span>
                    </span>
                    <span className="settings-search-value">{entry.value}</span>
                  </button>
                ))}
                {!results.length && (
                  <p>
                    No matching settings. Try “API key”, “font”, “backup”, or
                    “OCR”.
                  </p>
                )}
              </div>
            ) : null}
            <div hidden={Boolean(query.trim())}>
              <header className="settings-page-header">
                <div>
                  <h1>{pageInfo.title}</h1>
                  <p className="settings-page-description">
                    {pageInfo.description}
                  </p>
                </div>
              </header>
              {(section === "providers" || section === "workflow") && (
                <nav
                  className="settings-jump-nav"
                  aria-label={
                    section === "workflow"
                      ? "Review and workflow sections"
                      : "Provider sections"
                  }
                >
                  {(section === "workflow"
                    ? WORKFLOW_LINKS
                    : PROVIDER_LINKS
                  ).map((link) => (
                    <button
                      type="button"
                      key={link.id}
                      onClick={() => {
                        if (link.id === "workflow-history")
                          void selectSection("storage", link.id);
                        else focusSettingsTarget(link.id);
                      }}
                    >
                      {link.label}
                    </button>
                  ))}
                </nav>
              )}
              {warnings.length > 0 && (
                <div role="alert" className="settings-load-warning">
                  {warnings.map((warning, i) => (
                    <p key={i}>{warning}</p>
                  ))}
                  <p>
                    Changing a setting will overwrite the current file with
                    these values.
                  </p>
                </div>
              )}
              <div className="settings-page-body">
                {section === "general" && (
                  <>
                    <SettingsCard id="appearance-settings">
                      <AppearanceSettings
                        theme={theme}
                        onThemeChange={onThemeChange}
                      />
                    </SettingsCard>
                    <SettingsCard id="editor-settings">
                      <EditorSettings />
                    </SettingsCard>
                    <SettingsCard id="startup-settings">
                      <StartupSettings />
                    </SettingsCard>
                  </>
                )}
                {section === "conversations" && (
                  <>
                    <SettingsCard id="conversation-behavior">
                      <ConversationBehavior />
                    </SettingsCard>
                    <SettingsCard id="conversation-naming">
                      <ConversationTitleSettings />
                    </SettingsCard>
                  </>
                )}
                {section === "providers" && (
                  <>
                    <ProvidersSection
                      settings={settings}
                      setSettings={setSettings}
                      onCodexAccountChange={() =>
                        void loadCatalog("codex", settings, true)
                      }
                      onCodexStatusChange={onSystemChange}
                      catalogs={catalogs}
                      catalogLoading={catalogLoading}
                      catalogBlocked={catalogBlocked}
                      loadCatalog={loadCatalog}
                      dependencies={dependencies}
                    />
                    <SettingsCard id="local-provider">
                      <ProviderHeader
                        name="OpenAI-compatible"
                        description="Ollama, LM Studio, llama.cpp, or another compatible server."
                      />
                      <ConnectionStatus
                        provider="local"
                        settings={settings}
                        catalog={catalogs.local}
                        loading={catalogLoading.local}
                        blocked={catalogBlocked.local}
                        onCheck={() => loadCatalog("local", settings, true)}
                      />
                      <LocalProviderSection
                        settings={settings}
                        setSettings={setSettings}
                        catalogs={catalogs}
                        catalogBlocked={catalogBlocked}
                        catalogLoading={catalogLoading}
                        discoveryInputChanged={discoveryInputChanged}
                        loadCatalog={loadCatalog}
                      />
                      <ReviewSaveFeedback />
                    </SettingsCard>
                  </>
                )}
                {section === "workflow" && (
                  <>
                    <SettingsCard id="workflow-models">
                      <ModelDefaultsSection
                        settings={settings}
                        setSettings={setSettings}
                        catalogs={catalogs}
                      />
                      <button
                        type="button"
                        className="settings-text-link"
                        onClick={() => void selectSection("providers")}
                      >
                        Manage provider connections →
                      </button>
                      <ReviewSaveFeedback />
                    </SettingsCard>
                    <SettingsCard id="workflow-execution">
                      <WorkflowSection
                        settings={settings}
                        setSettings={setSettings}
                      />
                      <ReviewSaveFeedback />
                    </SettingsCard>
                    <SettingsCard id="workflow-extraction">
                      <ExtractionSection
                        settings={settings}
                        setSettings={setSettings}
                        onSystemChange={onSystemChange}
                      />
                      <ReviewSaveFeedback />
                    </SettingsCard>
                    <SettingsCard id="review-comparison">
                      <SectionHeader title="Revision comparison" />
                      <Toggle
                        label="Automatic revision reconciliation"
                        description="Add an AI comparison of addressed, remaining, and new concerns when a previous report exists. This adds a model call."
                        checked={settings.auto_revision_reconciliation}
                        onChange={(value) =>
                          setSettings({
                            ...settings,
                            auto_revision_reconciliation: value,
                          })
                        }
                      />
                      <ReviewSaveFeedback />
                    </SettingsCard>
                  </>
                )}
                {section === "storage" && (
                  <>
                    <StorageSettings onSavingChange={setStorageSaving} />
                    <SettingsCard id="workflow-history">
                      <SectionHeader
                        title="Review history"
                        description="Retention applies to completed Reviews and their artifacts."
                      />
                      <RunRetention
                        settings={settings}
                        setSettings={setSettings}
                      />
                      <ReviewSaveFeedback />
                    </SettingsCard>
                    <SettingsCard id="storage-cache">
                      <SectionHeader title="Extraction cache" />
                      <Toggle
                        label="Reuse verified extraction cache"
                        description="Reuse exact source-and-settings matches to avoid repeating PDF extraction. Existing research records and reports are unaffected."
                        checked={settings.reuse_pdf_extraction_cache}
                        onChange={(value) =>
                          setSettings({
                            ...settings,
                            reuse_pdf_extraction_cache: value,
                          })
                        }
                      />
                      <ReviewSaveFeedback />
                    </SettingsCard>
                    <WorkspaceResearchDataSettings
                      expanded
                      onSavingChange={setResearchDataSaving}
                    />
                  </>
                )}
                {section === "notifications" && (
                  <SettingsCard id="notifications-preferences">
                    <NotificationSettings />
                  </SettingsCard>
                )}
                {section === "advanced" && (
                  <AdvancedSettings
                    settings={settings}
                    setSettings={setSettings}
                  />
                )}
              </div>
            </div>
          </div>
        </div>
      </ReviewSaveState.Provider>
    </SettingsOperations.Provider>
  );
}
