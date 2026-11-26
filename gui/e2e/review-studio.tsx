// Development-only fixture: real setup components, deterministic providers, no model calls.
import { useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import RunSetupPanel from "../src/components/RunSetupPanel";
import {
  ReviewSetupReads,
  createReviewSetupReads,
} from "../src/components/reviewSetupReads";
import { settingsClient } from "../src/lib/settingsClient";
import { appClient } from "../src/lib/appClient";
import { workflowProfileClient } from "../src/lib/workflowProfileClient";
import type { Settings } from "../src/lib/settingsTypes";
import type {
  ModelCatalog,
  RosterEntry,
  RunModelOverrides,
} from "../src/lib/reviewModelTypes";
import type {
  PipelineConfig,
  PrimaryInputSelection,
} from "../src/lib/workflowTypes";
import "../src/App.css";

const query = new URLSearchParams(location.search);
document.documentElement.classList.toggle("dark", query.has("dark"));
const settings = {
  preferred_provider: query.has("live-defaults") ? "codex" : "claude",
  default_model: query.has("live-defaults") ? "" : "claude-exact",
  default_effort: query.has("live-defaults") ? "" : "high",
  default_parallel_agents: [],
  default_parallel_model_overrides: {},
  default_parallel_effort_overrides: {},
  claude_access_mode: "subscription",
  codex_access_mode: "subscription",
  local_model: "local-test",
  anthropic_api_key: "",
  openai_api_key: "",
  google_api_key: "",
  local_base_url: "http://localhost:11434/v1",
} as Settings;
const config = {
  steps: [
    "Claims and contribution",
    "Technical correctness",
    "Empirical strategy",
    "Internal consistency",
    "Exposition and framing",
    "Consolidate findings",
    "Validate feedback",
  ].map((label, index) => ({
    id: `step-${index}`,
    label,
    prompt: "Review the document.",
    enabled: true,
    phase: index < 5 ? "parallel" : "sequential",
    agents: [],
    tools: [],
    context: { include: [] },
  })),
  merge: { enabled: true, agents: [], prompt: "Combine reviews." },
  extraction: { input_mode: "document", method: "pdftotext" },
  use_orientation: true,
} as PipelineConfig;
const names: Record<string, string> = {
  claude: "Claude Exact",
  codex: query.has("live-defaults") ? "GPT-5.6 Sol" : "GPT Exact",
  antigravity: "Gemini Exact",
  local: "Local model",
};
const catalog = (provider: string): ModelCatalog => ({
  provider,
  transport: provider === "antigravity" || provider === "local" ? "api" : "cli",
  source: "fixture",
  source_version: "1",
  fetched_at: "2026-11-23",
  stale: false,
  default_model: `${provider}-exact`,
  recommended_model: `${provider}-exact`,
  roles: [],
  models: [
    {
      id: `${provider}-exact`,
      display_name: names[provider],
      description: "Fixture model",
      is_default: true,
      default_effort: provider === "codex" ? "medium" : null,
      deprecated: false,
      capabilities: [],
      supported_efforts: ["low", "medium", "high", "max"],
    },
  ],
});
Object.assign(settingsClient, {
  load: async () => ({ settings, warnings: [] }),
  modelCatalog: async ({ provider }: { provider: string }) => catalog(provider),
});
Object.assign(workflowProfileClient, {
  config: async () => config,
  active: async () => "auto-paper-review",
  profiles: async () =>
    [
      "Paper Review",
      "Paper Review (Fast)",
      "Slide Review",
      "Grant Review",
      "Custom Review",
    ].map((name, index) => ({
      id: index === 0 ? "auto-paper-review" : `profile-${index}`,
      name,
      builtin: true,
      step_count: 7,
    })),
  switch: async () => config,
});
Object.assign(appClient, {
  reviewPdfOptions: async () => ({ method: "pdftotext" }),
  modelRoster: async ({
    runParallelOverrides: value,
  }: {
    runParallelOverrides: RunModelOverrides | null;
  }): Promise<RosterEntry[]> => {
    const agents = value?.agents.length
      ? value.agents
      : [value?.review?.provider || "claude"];
    return [
      ...agents.map((provider) => ({
        stage: "parallel" as const,
        label: "Parallel steps",
        step_ids: ["step-0"],
        step_labels: ["Specialist reviews"],
        provider,
        transport: provider === "antigravity" ? "api" : "cli",
        model: value?.model_overrides[`${provider}:cli`] ?? {
          mode: "pinned" as const,
          model: `${provider}-exact`,
        },
        effort: "high",
        source: value ? ("review" as const) : ("default" as const),
        note: null,
      })),
      {
        stage: "merge",
        label: "Combine results",
        step_ids: [],
        step_labels: [],
        provider: value?.merge_agent || "claude",
        transport: "cli",
        model: { mode: "pinned", model: "claude-exact" },
        effort: "high",
        source: value?.override_workflow_pins ? "review" : "workflow",
        note: null,
      },
      {
        stage: "fallback",
        label: "Usage-limit backup",
        step_ids: [],
        step_labels: [],
        provider: "codex",
        transport: "cli",
        model: { mode: "pinned", model: "codex-exact" },
        effort: "high",
        source: "settings",
        note: "Only after a usage limit",
      },
    ];
  },
});
const fixtureWindow = window as unknown as {
  __TAURI_INTERNALS__: { invoke: (cmd: string) => Promise<unknown> };
  nextFiles?: string[];
};
fixtureWindow.__TAURI_INTERNALS__ = {
  invoke: async (command) => {
    if (command === "plugin:dialog|open")
      return fixtureWindow.nextFiles ?? ["G:/Research/Working paper.pdf"];
    throw new Error(`Native call unavailable in fixture: ${command}`);
  },
};

function Fixture() {
  const [selection, setSelection] = useState<PrimaryInputSelection | null>(
    query.has("empty")
      ? null
      : {
          paths: query.has("folder")
            ? ["G:/Research/Working paper"]
            : query.has("batch")
              ? ["G:/Research/Working paper.pdf", "G:/Research/Appendix.docx"]
              : ["G:/Research/Working paper.pdf"],
          selectionKind: query.has("folder") ? "folder" : "file",
          interpretation: query.has("folder")
            ? "latex_project"
            : query.has("batch")
              ? "batch"
              : "document",
        },
  );
  const [note, setNote] = useState(
    "Focus on identification assumptions and robustness checks.",
  );
  const [overrides, setOverrides] = useState<RunModelOverrides | null>(null);
  const [version, setVersion] = useState(0);
  const reads = useMemo(createReviewSetupReads, [version]);
  return (
    <div
      style={{
        height: "100vh",
        display: "grid",
        gridTemplateRows: "36px minmax(0,1fr)",
        gridTemplateColumns: `${query.has("rail") ? 56 : 216}px minmax(0,1fr)`,
        background: "var(--ui-surface)",
      }}
    >
      <header
        style={{
          gridColumn: "1/-1",
          padding: "8px 18px",
          borderBottom: "1px solid var(--ui-border)",
          fontSize: 13,
        }}
      >
        Pipeline
      </header>
      <aside
        style={{
          borderRight: "1px solid var(--ui-border)",
          padding: "18px 10px",
          color: "var(--ui-muted)",
          fontSize: 13,
        }}
      >
        {!query.has("rail") && (
          <>
            <p>Workspace</p>
            <p style={{ marginTop: 20, fontWeight: 600 }}>Reviews</p>
            <p style={{ marginTop: 12, color: "var(--ui-text)" }}>New review</p>
            <p style={{ marginTop: 12 }}>History</p>
            <p style={{ marginTop: 12 }}>Designer</p>
          </>
        )}
      </aside>
      <main
        data-testid="fixture-content"
        style={{ overflow: "auto", minHeight: 0 }}
      >
        <ReviewSetupReads.Provider value={reads}>
          <RunSetupPanel
            authorNote={note}
            onAuthorNoteChange={setNote}
            configVersion={version}
            configError={null}
            configLoading={false}
            dependenciesError={null}
            dependencies={
              query.has("disconnected")
                ? {
                    ready: false,
                    deps: [
                      {
                        name: "Claude CLI",
                        found: true,
                        cli_auth_status: "signed_out",
                        version: "",
                        path: "",
                        required: false,
                        hint: "",
                      },
                      {
                        name: "ChatGPT (reviews)",
                        found: true,
                        authenticated: false,
                        version: "",
                        path: "",
                        required: false,
                        hint: "",
                      },
                    ],
                  }
                : undefined
            }
            inputMode="document"
            inputSelection={selection}
            listenersReady
            localLlmActive={!query.has("disconnected")}
            paperPath={selection?.paths[0] ?? null}
            preparingRun={false}
            parallelOverrides={overrides}
            selectionKey={0}
            runActivity={
              query.has("busy")
                ? { running: 2, queued: 1, limit: 2 }
                : undefined
            }
            onConfigureWorkflow={() => {}}
            onGenerate={() => {}}
            onConnectModel={() => {}}
            onPaperPathChange={() => {}}
            onInputSelectionChange={setSelection}
            onPrivacyDetails={() => {}}
            onParallelOverridesChange={setOverrides}
            onProfileChange={() => setVersion((n) => n + 1)}
            onRetryConfig={() => {}}
            onRetryDependencies={() => {}}
          />
        </ReviewSetupReads.Provider>
      </main>
    </div>
  );
}
createRoot(document.getElementById("root")!).render(<Fixture />);
