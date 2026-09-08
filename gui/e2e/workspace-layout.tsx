// Development-only geometry fixture. All IPC is replaced before mounting; no
// native runtime, credentials, files, commands, or model calls are used.
import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import WorkspacePage from "../src/components/WorkspacePage";
import { workbenchClient } from "../src/lib/workbenchClient";
import { projectClient } from "../src/lib/projectClient";
import { deskClient } from "../src/lib/deskClient";
import { programClient } from "../src/lib/programClient";
import type { ConversationSnapshot, Workspace } from "../src/lib/workbenchTypes";
import "../src/App.css";

const now = "2026-09-08T12:00:00Z";
const workspace: Workspace = { id: "layout-fixture", name: "Production networks", root: null, rootIdentity: null, revision: 1, settingsRevision: 1, archivedAt: null, missingRootAt: null, createdAt: now, updatedAt: now };
let snapshot: ConversationSnapshot = { workspace, sequence: 1, activeBinding: null,
  session: { id: "layout-conversation", title: "Aggregation assumptions and the role of network propagation", workspaceId: workspace.id, paperId: null, presetId: "research_assistant", overrides: {}, draft: "", revision: 1, archivedAt: null, createdAt: now, updatedAt: now }, turns: [],
  items: [{ id: "layout-message", turnId: null, providerItemId: "example", itemKind: "agentMessage", payload: { text: "Open a project tool alongside this conversation. Your draft stays here when you change destinations.\n\nUse the **gear beside your message** to adjust the assistant. Find any project tool from the workspace bar, or press Command/Ctrl K." }, isFinal: true, createdAt: now, updatedAt: now }] };
const modules = [{ id: "paper_context", name: "Paper context", description: "Read the selected document.", kind: "context_provider" }, { id: "paper_tools", name: "Paper tools", description: "Search project papers.", kind: "tool" }];
const preset = { id: "research_assistant", workspaceId: null, name: "Research assistant", description: "Work with project papers and notes.", instructions: "Preserve substantive claims and equations.", modules: ["paper_context", "paper_tools"], builtIn: true, sourcePresetId: null, revision: 1 };
const catalog = { schemaVersion: 1, toolCatalogVersion: 4, presets: [preset], modules };
const home = { settings: { id: "home", workspaceId: workspace.id, kind: "home", revision: 1, updatedAt: now, body: { manuscriptRevisionId: null, baselineExecutionId: null, briefNoteIds: [], excludedNoteIds: [], ignoredPaths: [], layout: "reading" } }, notes: [], noteHistory: [], tasks: [], anchors: [], papers: [], executions: [], ledger: { claims: [], evidence: [], staleClaims: [], unsupportedAcceptedClaims: [] }, inventory: null, changes: [], applications: [], workingCopyStatus: "No folder attached", fileAcceptance: false, contextPreview: "" };
Object.assign(window, { __TAURI_INTERNALS__: { transformCallback: () => 1, unregisterCallback: () => {}, invoke: async (command: string) => {
  if (command.startsWith("plugin:event|")) return 1;
  throw new Error(`Unavailable in the layout fixture: ${command}`);
} } });
Object.assign(workbenchClient, {
  listWorkspaces: async () => ({ workspaces: [workspace] }), getWorkspace: async () => workspace,
  pendingRequests: async () => [], connectCodex: async () => ({}), accountState: async () => ({ status: "chatgpt" }),
  modelCatalog: async () => ({ models: [{ id: "fixture-model", model: "fixture-model", displayName: "Example model", description: "Geometry fixture", isDefault: true, defaultReasoningEffort: "medium", supportedReasoningEfforts: [{ reasoningEffort: "low", description: "Low" }, { reasoningEffort: "medium", description: "Medium" }, { reasoningEffort: "high", description: "High" }] }] }),
  listSessions: async () => ({ sessions: [snapshot.session] }), conversationSnapshot: async () => snapshot, reconcileSession: async () => false,
  updateSession: async (request: Record<string, unknown>) => { snapshot = { ...snapshot, session: { ...snapshot.session, ...request, revision: snapshot.session.revision + 1 } }; return { record: snapshot.session, sequence: ++snapshot.sequence }; },
  harnessCatalog: async () => catalog,
  effectiveHarness: async () => ({ schemaVersion: 1, sessionId: snapshot.session.id, workspaceId: workspace.id, preset, mode: "inspect", webSearch: false, commandNetwork: false, permissionProfile: "workbench-inspect", contextBudgetBytes: 65536, enabledModules: [], unavailableModules: preset.modules, moduleAvailability: modules.map(module => ({ id: module.id, available: false, reasons: ["Select a paper."] })), diagnostics: [], developerInstructions: preset.instructions, contextPreview: "", contextTruncated: false, dynamicTools: [], fingerprint: "fixture", valueSources: {} }),
  listPapers: async () => [], listSources: async () => [], listNotes: async () => [], listExecutions: async () => [], listExecutionProfiles: async () => [], researchLedger: async () => home.ledger,
  listRecipes: async () => [], listRecipeRuns: async () => [], performanceBudgets: async () => [],
  sendTurn: async () => { throw new Error("No messages are sent by this layout fixture."); },
});
Object.assign(projectClient, { home: async () => home });
Object.assign(deskClient, { context: async () => ({ revision: 0, items: [] }) });
Object.assign(programClient, { queue: async () => [], call: async () => [] });
localStorage.setItem("pipeline.workspace.workspaceId", workspace.id);
localStorage.setItem("pipeline.workspace.sessionId", snapshot.session.id);
localStorage.setItem("pipeline.workspace.sidebarWidth", "224");

function Fixture() {
  const [width, setWidth] = useState(1400);
  const [dark, setDark] = useState(false);
  const [metrics, setMetrics] = useState("");
  useEffect(() => { document.documentElement.classList.toggle("dark", dark); }, [dark]);
  useEffect(() => {
    const timer = setInterval(() => {
      const shell = document.querySelector<HTMLElement>(".workspace-chat-shell");
      if (!shell) return;
      const box = shell.getBoundingClientRect();
      const panels = Array.from(shell.querySelectorAll<HTMLElement>(".workspace-assistant-inspector, .workspace-desk-assistant, .workspace-desk-project"))
        .filter(el => el.getClientRects().length && getComputedStyle(el).display !== "none")
        .map(el => { const r = el.getBoundingClientRect(); return `${el.className.split(" ")[0]}: ${Math.round(r.width)}px ${r.left >= box.left - 1 && r.right <= box.right + 1 ? "in bounds" : "OUT OF BOUNDS"}`; });
      setMetrics(`Shell ${Math.round(box.width)}px; overflow ${shell.scrollWidth - shell.clientWidth}px. ${panels.join("; ")}`);
    }, 300);
    return () => clearInterval(timer);
  }, []);
  return <>
    <div style={{ padding: 12, display: "flex", gap: 18, flexWrap: "wrap", background: dark ? "#222" : "#eee", color: dark ? "#eee" : "#222" }}>
      <label>Window width <select aria-label="Test window width" value={width} onChange={event => setWidth(Number(event.target.value))}>{[1024, 1280, 1400, 1600].map(value => <option key={value}>{value}</option>)}</select></label>
      <label><input type="checkbox" checked={dark} onChange={event => setDark(event.target.checked)} /> Dark appearance</label>
      <span>Development fixture · no model or native calls</span>
    </div>
    <div style={{ display: "flex", width, maxWidth: "100%", height: "calc(100vh - 100px)", minHeight: 600, border: "1px solid #888" }}>
      <nav style={{ width: 176, flexShrink: 0, padding: 20, color: dark ? "#ddd" : "#333", background: dark ? "#202020" : "#f5f5f5" }}>Pipeline<br /><br />Workspace</nav>
      <div style={{ flex: 1, minWidth: 0 }}><WorkspacePage onOpenSettings={() => {}} /></div>
    </div>
    <output aria-label="Layout measurements" style={{ display: "block", padding: 12, fontSize: 12 }}>{metrics}</output>
  </>;
}
createRoot(document.getElementById("root")!).render(<Fixture />);
