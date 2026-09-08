import { SettingsNavigation } from "./settings/SettingsNavigation";
import { useSettingsDraft } from "./settings/useSettingsDraft";
import {
  type Section,
  type SettingsSection,
  resolveSection,
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
import { GeneralSection } from "./settings/GeneralSettings";
import { RunRetention } from "./settings/HistorySettings";
import { useState, useEffect, useRef } from "react";

import { confirmDialog } from "./DialogService";

import type { DepsReport } from "../lib/types";

import type { ThemePreference } from "../lib/theme";

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
  initialSection = "workflow",
  targetId,
  navigationKey = 0,
}: Props) {
  const {
    settings,
    setSettings,
    loading,
    loadError,
    saving,
    saved,
    dirty,
    saveError,
    warnings,
    retrySave,
    providerCatalogs,
  } = useSettingsDraft(onSystemChange);
  const {
    catalogs,
    catalogLoading,
    catalogBlocked,
    discoveryInputChanged,
    loadCatalog,
  } = providerCatalogs;
  const [storageSaving, setStorageSaving] = useState(false);
  const [researchDataSaving, setResearchDataSaving] = useState(false);
  const [section, setSection] = useState<Section>(
    resolveSection(initialSection),
  );
  const contentRef = useRef<HTMLDivElement>(null);
  const savePending = dirty || saving || storageSaving || researchDataSaving;
  useEffect(() => {
    onDirtyChange?.(savePending);
  }, [onDirtyChange, savePending]);

  useEffect(() => {
    setSection(resolveSection(initialSection));
  }, [initialSection, navigationKey]);

  useEffect(() => {
    if (loading || section !== resolveSection(initialSection)) return;
    const destination =
      targetId ??
      (
        {
          extraction: "workflow-extraction",
          llm: "workflow-models",
          workspace: "workspace-provider",
        } as Partial<Record<SettingsSection, string>>
      )[initialSection];
    if (!destination) return;
    const frame = requestAnimationFrame(() => focusSettingsTarget(destination));
    return () => cancelAnimationFrame(frame);
  }, [initialSection, loading, navigationKey, section, targetId]);

  useEffect(() => () => onDirtyChange?.(false), [onDirtyChange]);

  const handleClose = async () => {
    if (
      savePending &&
      !(await confirmDialog(
        "Settings changes have not finished saving. Leave anyway?",
      ))
    ) {
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
      <div className="p-8 flex flex-col items-center justify-center h-full gap-4 text-gray-500 dark:text-neutral-400">
        <p className="text-sm">
          Failed to load settings{loadError ? `: ${loadError}` : "."}
        </p>
        <button
          onClick={handleClose}
          className="text-sm text-gray-600 dark:text-neutral-300 hover:underline"
        >
          Go back
        </button>
      </div>
    );
  }

  const selectSection = (next: Section) => {
    setSection(next);
    if (contentRef.current) contentRef.current.scrollTop = 0;
  };
  const pageInfo = SECTION_INFO[section];

  return (
    <div data-testid="settings-page" className="settings-shell">
      <SettingsNavigation
        section={section}
        selectSection={selectSection}
        showBack={showBack}
        handleClose={handleClose}
        status={
          <>
            {" "}
            {saveError ? (
              <div
                role="alert"
                className="flex flex-col items-start gap-2 text-red-700 dark:text-red-400"
              >
                <span>Could not save settings: {saveError}</span>
                <button
                  type="button"
                  onClick={retrySave}
                  className="font-medium underline underline-offset-2"
                >
                  Retry
                </button>
              </div>
            ) : saving || dirty ? (
              <span className="text-gray-500 dark:text-neutral-400">
                Saving changes…
              </span>
            ) : saved ? (
              <span className="text-green-700 dark:text-green-400">
                All changes saved.
              </span>
            ) : (
              <span className="text-gray-500 dark:text-neutral-400">
                Changes save automatically.
              </span>
            )}
          </>
        }
      />
      {/* Content area */}
      <div ref={contentRef} className="settings-content">
        <header className="settings-page-header">
          <div>
            <p className="settings-eyebrow">Settings</p>
            <h1>{pageInfo.title}</h1>
            <p className="settings-page-description">{pageInfo.description}</p>
          </div>
        </header>
        {section !== "general" && (
          <nav
            aria-label={
              section === "workflow"
                ? "Review and workflow sections"
                : "Provider sections"
            }
            className="settings-jump-nav"
          >
            {(section === "workflow" ? WORKFLOW_LINKS : PROVIDER_LINKS).map(
              (link) => (
                <button
                  type="button"
                  key={link.id}
                  onClick={() => focusSettingsTarget(link.id)}
                >
                  {link.label}
                </button>
              ),
            )}
          </nav>
        )}
        {warnings.length > 0 && (
          <div className="mx-8 mt-6 px-4 py-3 rounded-lg bg-amber-50 dark:bg-amber-950 border border-amber-200 dark:border-amber-800 text-sm text-amber-800 dark:text-amber-300">
            {warnings.map((w, i) => (
              <p key={i}>{w}</p>
            ))}
            <p className="mt-1 text-amber-700 dark:text-amber-300 text-xs">
              Changing a setting will overwrite the current file with these
              values.
            </p>
          </div>
        )}
        <div className="settings-page-body">
          {section === "providers" && (
            <>
              <ProvidersSection
                settings={settings}
                setSettings={setSettings}
                onCodexAccountChange={() =>
                  void loadCatalog("codex", settings, true)
                }
                onCodexStatusChange={onSystemChange}
              />
              <SettingsCard id="local-provider">
                <ProviderHeader
                  name="OpenAI-compatible"
                  description="Ollama, LM Studio, llama.cpp, or another compatible server."
                  badge="Reviews"
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
                  onClick={() => selectSection("providers")}
                >
                  Manage provider connections →
                </button>
              </SettingsCard>
              <SettingsCard id="workflow-execution">
                <WorkflowSection
                  settings={settings}
                  setSettings={setSettings}
                />
              </SettingsCard>
              <SettingsCard id="workflow-extraction">
                <ExtractionSection
                  settings={settings}
                  setSettings={setSettings}
                  onSystemChange={onSystemChange}
                />
              </SettingsCard>
              <SettingsCard id="workflow-history">
                <SectionHeader
                  title="Reports & history"
                  description="Compare revisions and manage saved reports and their artifacts."
                />
                <div className="space-y-5">
                  <Toggle
                    label="Automatic revision reconciliation"
                    description="When a matching completed report exists, add an AI comparison of addressed, remaining, and new concerns. This adds an LLM call to the report."
                    checked={settings.auto_revision_reconciliation}
                    onChange={(v) =>
                      setSettings({
                        ...settings,
                        auto_revision_reconciliation: v,
                      })
                    }
                  />
                  <RunRetention settings={settings} setSettings={setSettings} />
                </div>
              </SettingsCard>
            </>
          )}
          {section === "general" && (
            <>
              <SettingsCard id="general-preferences">
                <GeneralSection
                  settings={settings}
                  setSettings={setSettings}
                  theme={theme}
                  onThemeChange={onThemeChange}
                />
              </SettingsCard>
              <StorageSettings onSavingChange={setStorageSaving} />
              <WorkspaceResearchDataSettings onSavingChange={setResearchDataSaving} />
            </>
          )}
        </div>
      </div>
    </div>
  );
}
