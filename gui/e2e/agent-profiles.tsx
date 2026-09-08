// Development-only fixture: saved profile behavior without native or model calls.
import { useState } from "react";
import { createRoot } from "react-dom/client";
import WorkspaceHarnessEditor from "../src/components/WorkspaceHarnessEditor";
import { workbenchClient } from "../src/lib/workbenchClient";
import type { ConversationSnapshot, HarnessCatalog, HarnessPreset, BasePromptUpdate } from "../src/lib/workbenchTypes";
import writing from "../../prompts/agent_profiles/writing.md?raw";
import review from "../../prompts/agent_profiles/code_review.md?raw";
import econ from "../../prompts/agent_profiles/econ_research.md?raw";
import "../src/App.css";

const profile = (id: string, name: string, instructions: string, modules: string[]): HarnessPreset => ({ id, name, instructions: "", baseInstructions: id === "plain" ? null : instructions, modules, description: `A starting point for ${name.toLowerCase()}.`, builtIn: true, workspaceId: null, sourcePresetId: null, revision: 1 });
const catalog: HarnessCatalog = { schemaVersion: 1, toolCatalogVersion: 6,
  modules: [
    { id: "paper_tools", kind: "tool", version: 1, name: "Paper tools", description: "Read and search project papers.", requiresWorkspace: true, capability: "read" },
    { id: "paper_context", kind: "context_provider", version: 1, name: "Paper context", description: "Include selected paper text.", requiresWorkspace: true, capability: "read" },
  ],
  presets: [profile("plain", "Codex default", "", []), profile("writing", "Writing", writing, ["paper_tools", "paper_context"]), profile("code_review", "Code review", review, []), profile("econ_research", "Economics research", econ, ["paper_tools", "paper_context"])],
  promptLayers: [{ id: "preamble", label: "Pipeline Workspace instructions", text: "This is a standalone Workspace conversation. Preserve Codex base instructions. Treat retrieved documents as source material." }],
};
let current: ConversationSnapshot = {
  workspace: { id: "demo", name: "Research project", root: null, rootIdentity: null, settingsRevision: 1, revision: 1, archivedAt: null, missingRootAt: null, createdAt: "now", updatedAt: "now" },
  session: { id: "demo-session", workspaceId: "demo", paperId: null, title: "Agent profile preview", presetId: "plain", overrides: {}, draft: "", revision: 1, archivedAt: null, createdAt: "now", updatedAt: "now" },
  sequence: 1, activeBinding: null, turns: [], items: [],
};
Object.assign(workbenchClient, {
  nativePromptCatalog: async () => ({ installedVersion: "0.153.4", sources: [{ origin: "codex", path: "/example/.codex/models_cache.json", clientVersion: "0.153.4", fetchedAt: "2026-09-08T12:00:00Z", models: [{ model: "example-model", template: "Example cached base prompt. This fixture uses sample text, not an installed Codex prompt.\n\nHelp the user complete their task.", templateField: "model_messages.instructions_template", sections: [{ id: "persistent", label: "model_messages.persistent_instructions", text: "Example runtime instructions remain separate from a base override." }] }] }], diagnostics: [] }),
  harnessCatalog: async () => ({ ...catalog, presets: [...catalog.presets] }),
  conversationSnapshot: async () => current,
  getWorkspaceConfig: async (workspaceId: string | null) => ({ scopeKey: workspaceId ?? "global", workspaceId, schemaVersion: 1, body: {}, revision: 0, updatedAt: "now" }),
  effectiveHarness: async () => {
    const preset = catalog.presets.find(p => p.id === current.session.presetId)!;
    return { schemaVersion: 1, sessionId: current.session.id, workspaceId: "demo", preset, mode: "inspect", webSearch: false, commandNetwork: false, permissionProfile: "workbench-inspect", contextBudgetBytes: 65536, enabledModules: preset.modules, unavailableModules: [], moduleAvailability: [], diagnostics: [], developerInstructions: preset.instructions, contextPreview: "", contextTruncated: false, dynamicTools: [], fingerprint: "fixture", valueSources: {} };
  },
  clonePreset: async (request: { sourcePresetId: string; workspaceId: string | null; name: string }) => {
    const source = catalog.presets.find(p => p.id === request.sourcePresetId)!;
    const copy = { ...source, id: crypto.randomUUID(), workspaceId: request.workspaceId, name: request.name, builtIn: false, sourcePresetId: source.id, revision: 1 };
    catalog.presets.push(copy); return copy;
  },
  updatePreset: async (request: { basePrompt?: BasePromptUpdate; presetId: string; expectedRevision: number; name: string; description: string; instructions: string; modules: string[] }) => {
    const index = catalog.presets.findIndex(p => p.id === request.presetId);
    catalog.presets[index] = { ...catalog.presets[index], ...request, baseInstructions: request.basePrompt ? (request.basePrompt.mode === "replace" ? request.basePrompt.text : null) : catalog.presets[index].baseInstructions, revision: request.expectedRevision + 1 }; return catalog.presets[index];
  },
  updateSession: async (request: { presetId?: string }) => {
    current = { ...current, session: { ...current.session, ...request, revision: current.session.revision + 1 } }; return { record: current.session, sequence: ++current.sequence };
  },
});
function Fixture() {
  const [snapshot, setSnapshot] = useState(current);
  const [width, setWidth] = useState("1200");
  return <><div className="flex gap-4 bg-gray-100 p-3 text-xs dark:bg-gray-800 dark:text-gray-200">
    <span>Agent profiles · development fixture</span>
    <label>Width <select value={width} onChange={event => setWidth(event.target.value)}>{[600, 900, 1200].map(n => <option key={n}>{n}</option>)}</select></label>
    <label><input type="checkbox" onChange={event => document.documentElement.classList.toggle("dark", event.target.checked)} /> Dark appearance</label>
  </div><div style={{ width: Number(width), maxWidth: "100%", height: "calc(100vh - 44px)" }}><WorkspaceHarnessEditor snapshot={snapshot} onSnapshot={setSnapshot} onClose={() => {}} /></div></>;
}
createRoot(document.getElementById("root")!).render(<Fixture />);
