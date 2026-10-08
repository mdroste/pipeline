// Development-only full-app fixture shim. Installs a fake Tauri IPC layer so
// the complete App renders in an ordinary browser. No native runtime,
// credentials, files, commands, or model calls are used or reachable.
/* eslint-disable @typescript-eslint/no-explicit-any */

const now = "2026-09-29T17:20:00Z";
const params = new URLSearchParams(window.location.search);

const settings = {
  preferred_provider: "claude",
  default_parallel_agents: ["claude", "codex"],
  max_workers: 3,
  active_profile: "auto_paper_review",
  claude_model: "",
  claude_effort: "high",
  codex_model: "",
  codex_effort: "medium",
  antigravity_effort: "",
  pdf_extractor: "auto",
  paddle_page_concurrency: 0,
  paddle_mtmd_batch_tokens: 0,
  paddle_flash_attention: "auto",
  paddle_max_output_tokens: 2048,
  paddle_page_retries: 1,
  paddle_full_layout_detection: true,
  paddle_full_layout_threshold: 0.5,
  paddle_full_layout_nms: true,
  paddle_full_layout_merge_bboxes_mode: "large",
  paddle_full_merge_layout_blocks: true,
  paddle_full_ocr_image_blocks: true,
  paddle_full_format_block_content: true,
  paddle_full_merge_tables: true,
  paddle_full_relevel_titles: true,
  paddle_full_show_formula_numbers: false,
  pdf_extraction_timeout_secs: 900,
  reuse_pdf_extraction_cache: true,
  verbose_logging: false,
  step_timeout_secs: 1800,
  max_retries: 2,
  auto_revision_reconciliation: true,
  max_saved_runs: 0,
  max_saved_run_bytes: 0,
  claude_access_mode: "subscription",
  codex_backend: "app_server",
  codex_access_mode: "subscription",
  antigravity_access_mode: "api",
  anthropic_api_key: "",
  openai_api_key: "",
  google_api_key: "",
  local_base_url: "",
  local_model: "",
  local_api_key: "",
};

const steps = [
  {
    id: "first_pass", label: "First reading", prompt: "Read the manuscript as a generalist referee. Summarize the contribution and flag the three weakest links in the identification argument.",
    enabled: true, phase: "parallel", tools: [], agents: ["claude", "codex"],
    context: { include: [{ kind: "primary", parts: ["text"] }] }, run_if: null,
  },
  {
    id: "methods_pass", label: "Methods deep dive", prompt: "Assess the difference-in-differences design: parallel trends, inference, clustering choices, and robustness.",
    enabled: true, phase: "parallel", tools: [], agents: ["claude", "codex"],
    context: { include: [{ kind: "primary", parts: ["text", "structure"] }] }, run_if: null,
  },
  {
    id: "lit_check", label: "Literature check", prompt: "Verify that the paper's positioning against the cited minimum-wage literature is accurate and complete.",
    enabled: true, phase: "sequential", tools: ["web"], agents: ["claude"],
    context: { include: [{ kind: "primary", parts: ["text"] }, { kind: "step", step: "first_pass", parts: ["report"] }] }, run_if: null,
  },
  {
    id: "referee_report", label: "Referee report", prompt: "Draft a complete referee report from the prior passes: summary, major comments, minor comments, recommendation.",
    enabled: true, phase: "sequential", tools: [], agents: ["claude"],
    after: ["methods_pass"],
    context: { include: [{ kind: "step", step: "first_pass", parts: ["report"] }, { kind: "step", step: "methods_pass", parts: ["report"] }, { kind: "step", step: "lit_check", parts: ["report"] }] },
    run_if: null,
  },
  {
    id: "second_round", label: "Escalation pass", prompt: "If major issues were found, re-examine each flagged section in detail and propose concrete fixes.",
    enabled: true, phase: "sequential", tools: [], agents: ["claude"],
    context: { include: [{ kind: "step", step: "referee_report", parts: ["report"] }, { kind: "primary", parts: ["text"] }] },
    run_if: { kind: "output_matches", step: "referee_report", pattern: "major revision|reject" },
  },
];

const pipelineConfig = {
  steps,
  merge: { enabled: true, prompt: "Merge the parallel referee perspectives into one coherent report; keep disagreements explicit.", agents: ["claude"] },
  outputs: { primary_step: "referee_report" },
  use_orientation: true,
  orientation_prompt: "",
  extraction: { method: "auto", input_mode: "document", extra_inputs: [] },
  parallel_context_template: "",
  variables: [
    { key: "journal", label: "Target journal", kind: "text", default: "AEJ: Applied" },
  ],
};

const profiles = [
  { id: "auto_paper_review", name: "Auto Paper Review", step_count: 12, builtin: true },
  { id: "draft_review_loop", name: "Draft → review → revise loop", step_count: 5, builtin: false },
  { id: "replication_audit", name: "Replication package audit", step_count: 7, builtin: false },
  { id: "referee_response", name: "Referee response letter", step_count: 4, builtin: false },
];

const runs = [
  {
    run_id: "run-2026-09-28-a", created: "2026-09-28T21:14:00Z", input_name: "minwage_draft_v7.pdf",
    input_path: "/Users/mike/Papers/minwage/minwage_draft_v7.pdf", input_mode: "document",
    profile_id: "auto_paper_review", profile_name: "Auto Paper Review", provider: "claude",
    status: "complete", duration_secs: 1841, input_tokens: 412_000, output_tokens: 58_200,
    cached_input_tokens: 156_000, cache_write_input_tokens: 61_000, step_count: 12, artifact_count: 19,
    failed_steps: [], resumable: false, title: "Minimum wage draft v7 — full review", tags: ["minwage", "revision"],
  },
  {
    run_id: "run-2026-09-27-b", created: "2026-09-27T16:03:00Z", input_name: "minwage_draft_v6.pdf",
    input_path: "/Users/mike/Papers/minwage/minwage_draft_v6.pdf", input_mode: "document",
    profile_id: "auto_paper_review", profile_name: "Auto Paper Review", provider: "codex",
    status: "complete", duration_secs: 2210, input_tokens: 388_000, output_tokens: 61_900,
    cached_input_tokens: 0, cache_write_input_tokens: 0, step_count: 12, artifact_count: 17,
    failed_steps: [], resumable: false, title: "Minimum wage draft v6", tags: ["minwage"],
  },
  {
    run_id: "run-2026-09-25-c", created: "2026-09-25T09:41:00Z", input_name: "replication_pkg",
    input_path: "/Users/mike/Papers/minwage/replication", input_mode: "folder",
    profile_id: "replication_audit", profile_name: "Replication package audit", provider: "claude",
    status: "partial", duration_secs: 3105, input_tokens: 96_000, output_tokens: 22_400,
    cached_input_tokens: 12_000, cache_write_input_tokens: 9_000, step_count: 7, artifact_count: 9,
    failed_steps: ["stata_rerun"], resumable: true, title: "Replication audit — county panel", tags: ["minwage"],
  },
];

const workspace = {
  id: "ws-minwage", name: "Minimum wage revision", root: "/Users/mike/Papers/minwage",
  rootIdentity: null, revision: 4, settingsRevision: 2, archivedAt: null, missingRootAt: null,
  createdAt: "2026-08-30T12:00:00Z", updatedAt: now,
};
const session = {
  id: "sess-aggregation", title: "Interpreting the county-pair estimates",
  workspaceId: workspace.id, paperId: null, presetId: "research_assistant", overrides: {},
  draft: "", revision: 3, archivedAt: null, createdAt: "2026-09-27T12:00:00Z", updatedAt: now,
};
const snapshot = {
  workspace, sequence: 8, activeBinding: null, session,
  turns: [],
  items: [
    { id: "m1", turnId: null, providerItemId: "u1", itemKind: "userMessage", payload: { text: "Summarize how the border-county design handles spillovers, and whether Table 4 supports the no-anticipation claim." }, isFinal: true, createdAt: now, updatedAt: now },
    { id: "m2", turnId: null, providerItemId: "a1", itemKind: "agentMessage", payload: { text: "The design compares contiguous county pairs across state lines, so spillovers that cross the border would *attenuate* the estimated employment effect rather than inflate it.\n\nOn **Table 4**: the event-study leads are jointly insignificant, which is consistent with no anticipation, but the confidence bands widen two quarters before treatment — worth flagging in the referee response rather than claiming a clean pre-trend." }, isFinal: true, createdAt: now, updatedAt: now },
  ],
};
const modules = [
  { id: "paper_context", name: "Paper context", description: "Read the selected document.", kind: "context_provider" },
  { id: "paper_tools", name: "Paper tools", description: "Search project papers.", kind: "tool" },
];
const preset = { id: "research_assistant", workspaceId: null, name: "Research assistant", description: "Work with project papers and notes.", instructions: "Preserve substantive claims and equations.", modules: ["paper_context", "paper_tools"], builtIn: true, sourcePresetId: null, revision: 1 };
const harnessCatalog = { schemaVersion: 1, toolCatalogVersion: 4, presets: [preset], modules };
const effectiveHarness = {
  schemaVersion: 1, sessionId: session.id, workspaceId: workspace.id, preset, mode: "inspect",
  webSearch: false, commandNetwork: false, permissionProfile: "workbench-inspect",
  contextBudgetBytes: 65536, enabledModules: ["paper_context"], unavailableModules: ["paper_tools"],
  moduleAvailability: modules.map((m) => ({ id: m.id, available: m.id === "paper_context", reasons: m.id === "paper_context" ? [] : ["Select a paper."] })),
  diagnostics: [], developerInstructions: preset.instructions, contextPreview: "", contextTruncated: false,
  dynamicTools: [], fingerprint: "fixture", valueSources: {},
};
// `?project=thin` shows a project with nothing but conversations; `?project=bare`
// shows the folder-only project this fixture used before the overview derived state.
const projectKind = params.get("project") ?? "rich";
if (projectKind === "thin") workspace.root = null as unknown as string;
const note = (id: string, kind: string, body: string, state = "accepted") => ({
  id, workspaceId: workspace.id, paperId: null, kind, body, state, origin: state === "proposed" ? "assistant" : "user",
  pinned: kind === "question", revision: 1, createdAt: "2026-09-28T09:00:00Z", updatedAt: "2026-09-28T09:00:00Z",
});
const record = (id: string, kind: string, body: unknown, updatedAt = now) => ({ id, workspaceId: workspace.id, kind, revision: 1, body, updatedAt });
const file = (path: string, hash: string) => ({ path, hash, size: 2048, status: "current", executable: false });
const manuscript = {
  paper: { id: "paper-minwage", workspaceId: workspace.id, title: "Minimum wages and county-pair employment", role: "manuscript", currentRevisionId: "rev-v8", createdAt: "2026-08-30T12:00:00Z", updatedAt: "2026-09-29T15:00:00Z" },
  revision: { id: "rev-v8", paperId: "paper-minwage", inputKind: "source_tree", entrypoint: "/Users/mike/Papers/minwage/paper", dependencyManifest: {}, contentHash: "hash-v8", textReference: null, compiledArtifactId: null, extraction: { status: "complete" }, captureComplete: true, capturedAt: "2026-09-29T15:00:00Z" },
};
const emptyHome = {
  settings: { id: "home", workspaceId: workspace.id, kind: "home", revision: 1, updatedAt: now, body: { manuscriptRevisionId: null, baselineExecutionId: null, briefNoteIds: [], excludedNoteIds: [], ignoredPaths: [], layout: "reading" } },
  notes: [] as unknown[], noteHistory: [], tasks: [] as unknown[], anchors: [], papers: [] as unknown[], executions: [] as unknown[],
  ledger: { claims: [], evidence: [], staleClaims: [] as string[], unsupportedAcceptedClaims: [] as string[] },
  inventory: null as unknown, changes: [], applications: [], workingCopyStatus: "Folder attached", workingCopyChanged: 0, fileAcceptance: false, contextPreview: "",
};
const projectHome =
  projectKind === "bare"
    ? emptyHome
    : projectKind === "thin"
      ? { ...emptyHome, notes: [note("q1", "question", "Do county-pair minimum wage comparisons identify employment effects under spillovers?")] }
      : {
          ...emptyHome,
          settings: { ...emptyHome.settings, body: { ...emptyHome.settings.body, manuscriptRevisionId: "rev-v8", targetDate: "2026-11-15", targetLabel: "AEJ: Applied resubmission" } },
          notes: [
            note("q1", "question", "Do county-pair minimum wage comparisons identify employment effects under spillovers?"),
            note("n1", "next_step", "Re-estimate Table 4 with commuting-zone clusters before answering Referee 2."),
            note("p1", "assumption", "No anticipation in the two quarters before treatment.", "proposed"),
            note("p2", "decision", "Report the border-pair estimate as the headline number.", "proposed"),
          ],
          tasks: [record("task-1", "task", { objective: "Add the commuting-zone robustness table to the appendix", anchorId: null, expectedOutputs: [], expectedChecks: [], status: "open" })],
          papers: [manuscript],
          executions: [
            { id: "exec-1", workspaceId: workspace.id, sessionId: session.id, outcome: "completed", adapter: "stata", createdAt: "2026-09-29T16:00:00Z", startedAt: "2026-09-29T16:00:00Z", endedAt: "2026-09-29T16:20:00Z" },
            { id: "exec-2", workspaceId: workspace.id, sessionId: session.id, outcome: "failed", adapter: "stata", createdAt: "2026-09-29T16:30:00Z", startedAt: "2026-09-29T16:30:00Z", endedAt: "2026-09-29T16:31:00Z" },
          ],
          ledger: { claims: [], evidence: [], staleClaims: ["claim-1", "claim-2"], unsupportedAcceptedClaims: [] },
          inventory: record("inventory", "inventory", {
            rootIdentity: "root", capturedAt: now, complete: true, warnings: [],
            files: [file("paper/main.tex", "a2aaaaaaaaaa"), file("analysis/event_study.do", "b2bbbbbbbbbb"), file("analysis/tables.do", "cccccccccccc"), file("output/table4.tex", "dddddddddddd"), file("data/county_pairs.dta", "eeeeeeeeeeee")],
          }),
          workingCopyChanged: 2,
        };
if (projectKind === "rich")
  localStorage.setItem(
    `pipeline.project.visit.${workspace.id}`,
    JSON.stringify({
      version: 1, since: null, baseline: null, lastActive: "2026-09-28T18:00:00Z",
      latest: { "paper/main.tex": "aaaaaaaaaaaa", "analysis/event_study.do": "bbbbbbbbbbbb", "analysis/tables.do": "cccccccccccc", "data/county_pairs.dta": "eeeeeeeeeeee" },
    }),
  );
else localStorage.removeItem(`pipeline.project.visit.${workspace.id}`);
const response = (number: string, disposition: string, draft = "", resolvingCheck = "") =>
  record(`resp-${number}`, "response", {
    source: { kind: "report", revisionId: "rev-v7" }, flags: [],
    decision: { number, category: "identification", severity: "medium", disposition, intendedResponse: "", taskId: null, manuscriptRevisionId: null, applicationId: null, executionId: null, evidenceAnchorIds: [], draft, rationale: "", disputedPremise: "", counterargument: "", resolvingCheck, reportsAnalysisAdded: false },
  });
const studioRecords: Record<string, unknown[]> = {
  build: [
    record("build-receipt", "build", {
      recordType: "receipt", executionId: "exec-build", outcome: "failed",
      diagnostics: [
        { severity: "error", message: "Undefined control sequence \\tnote in output/table4.tex", path: "output/table4.tex", line: 18 },
        { severity: "warning", message: "Reference `tab:cz' undefined", path: "paper/main.tex", line: 412 },
      ],
      pdf: null, mappingArtifactId: null, pageInspection: "", synchronization: "", sourceManifest: null,
    }, "2026-09-29T15:40:00Z"),
  ],
  response: [
    response("R1.1", "addressed", "We now report the event-study leads."),
    response("R1.2", "addressed", "Clustering is at the commuting-zone level."),
    response("R1.3", "investigating", "We agree and are re-estimating.", "Re-run Table 4 with commuting-zone clusters"),
    response("R2.1", "open"),
    response("R2.2", "open"),
    response("R2.3", "deferred", "Outside the scope of this revision."),
  ],
};
const coverage = (printed: string, state: string, reasons: string[]) => ({ record: record(`bind-${printed}`, "binding", { printed }), state, reasons, numericPassed: state === "current", expected: null, dependency: { state }, result: null, anchor: {} });

const efforts = (...levels: string[]) =>
  levels.map((reasoningEffort) => ({
    reasoningEffort,
    description:
      { low: "Fastest replies for simple questions", medium: "Balances speed and depth for everyday work", high: "Works through hard problems more carefully", xhigh: "Longest deliberation for the hardest problems" }[reasoningEffort] ?? reasoningEffort,
  }));
const workspaceModelCatalog = {
  models: [
    { id: "gpt-5-codex", model: "gpt-5-codex", displayName: "GPT-5 Codex", description: "Best for analysis, code, and long research tasks", isDefault: true, defaultReasoningEffort: "medium", supportedReasoningEfforts: efforts("low", "medium", "high", "xhigh") },
    { id: "gpt-5", model: "gpt-5", displayName: "GPT-5", description: "General reasoning and writing", isDefault: false, defaultReasoningEffort: "medium", supportedReasoningEfforts: efforts("low", "medium", "high") },
    { id: "gpt-5-mini", model: "gpt-5-mini", displayName: "GPT-5 mini", description: "Quick answers at lower usage", isDefault: false, defaultReasoningEffort: "low", supportedReasoningEfforts: efforts("low", "medium") },
  ],
};
// `?sources=N` attaches N sources to the conversation; `?model=` and
// `?effort=` preselect a saved model choice (use an unknown id to see the
// unavailable state).
const sourceTitles: Record<string, string> = {
  "rev-v8": "Minimum wages and county-pair employment",
  "rev-ref2": "Referee 2 report.pdf",
  "ds-qcew": "QCEW county panel 2001-2019",
  "rev-v7": "minwage_draft_v7.pdf",
  "res-t4": "Table 4 event study",
};
const sourceItems = [
  { role: "main", object: { kind: "paper", id: "rev-v8", revision: "hash-v8" } },
  { role: "referee_report", object: { kind: "paper", id: "rev-ref2", revision: "hash-r2" } },
  { role: "data_dictionary", object: { kind: "dataset", id: "ds-qcew", revision: "hash-q" } },
  { role: "prior_draft", object: { kind: "paper", id: "rev-v7", revision: "hash-v7" } },
  { role: "result", object: { kind: "result", id: "res-t4", revision: "hash-t4" } },
].slice(0, Number(params.get("sources") ?? 0));
let contextSelection = { revision: 1, items: sourceItems as unknown[] };
if (params.get("model")) Object.assign(session.overrides, { model: params.get("model"), effort: params.get("effort") ?? null });

const modelCatalog = {
  provider: "claude", transport: "cli", source: "bundled", source_version: "2026-09-01",
  fetched_at: now, stale: false, default_model: "claude-opus-5",
  models: [
    { id: "claude-opus-5", display_name: "Claude Opus 5", description: "Most capable", is_default: true, supported_efforts: ["low", "medium", "high"], capabilities: [], deprecated: false },
    { id: "claude-sonnet-5", display_name: "Claude Sonnet 5", description: "Balanced", is_default: false, supported_efforts: ["low", "medium", "high"], capabilities: [], deprecated: false },
  ],
  roles: [],
};

const depsReport = {
  ready: true,
  deps: [
    { name: "claude", found: true, version: "2.1.14", path: "/usr/local/bin/claude", required: false, hint: "", authenticated: true, cli_auth_status: "signed_in" },
    { name: "codex", found: true, version: "0.153.4", path: "/usr/local/bin/codex", required: false, hint: "", authenticated: true, cli_auth_status: "signed_in" },
    { name: "poppler", found: true, version: "24.04", path: "(bundled)", required: true, hint: "" },
  ],
};

const executionPlan = {
  profileId: "auto_paper_review",
  profileConfigSnapshotId: "snap-1",
  profileSnapshotId: "snap-1",
  inputMode: "document",
  inputInterpretation: "document",
  variables: pipelineConfig.variables,
  inputSlots: [],
  readiness: depsReport,
  stages: [
    { id: "extract", kind: "extraction", label: "Extract document", stepIds: [] },
    { id: "orientation", kind: "orientation", label: "Orientation map", stepIds: [] },
    { id: "wave-1", kind: "parallel", label: "Parallel review", stepIds: ["first_pass", "methods_pass"], stepLabels: ["First reading", "Methods deep dive"] },
    { id: "merge", kind: "merge", label: "Merge perspectives", stepIds: [] },
    { id: "wave-2", kind: "sequential", label: "Literature check", stepIds: ["lit_check"], stepLabels: ["Literature check"] },
    { id: "wave-3", kind: "sequential", label: "Referee report", stepIds: ["referee_report"], stepLabels: ["Referee report"] },
    { id: "wave-4", kind: "sequential", label: "Escalation pass", stepIds: ["second_round"], stepLabels: ["Escalation pass"] },
  ],
  parallelAgents: ["claude", "codex"],
  mergeAgent: "claude",
};

const commandData: Record<string, unknown> = {
  mark_smoke_ready: true,
  get_execution_plan: executionPlan,
  get_settings: settings,
  save_settings: true,
  check_for_update: { current: "1.0.1", latest: "1.0.1", update_available: false, release_url: "", release_name: "", published_at: null },
  get_batch_status: [],
  get_run_setup: { profileId: "auto_paper_review", profileConfigSnapshotId: "snap-1", inputMode: "document", variables: pipelineConfig.variables, inputSlots: [] },
  get_pipeline_config: pipelineConfig,
  save_pipeline_config: true,
  list_profiles: profiles,
  get_active_profile: "auto_paper_review",
  get_model_catalog: modelCatalog,
  list_runs: runs,
  runs_disk_usage: { count: 38, bytes: 6_400_000_000 },
  list_trashed_runs: [],
  list_projects: { projects: [{ schema_version: 1, id: "proj-minwage", name: "Minimum wage revision", description: "AEJ: Applied second round", created: "2026-08-30T12:00:00Z", updated: now, run_ids: ["run-2026-09-28-a", "run-2026-09-27-b"] }], warnings: [] },
  list_trashed_projects: [],
  check_deps: depsReport,
  get_deps: depsReport,
  list_input_files: [],
  workbench_list_workspaces: { workspaces: [workspace] },
  workbench_get_workspace: workspace,
  workbench_list_sessions: { sessions: [session] },
  workbench_conversation_snapshot: snapshot,
  workbench_session_snapshot: snapshot,
  workbench_harness_catalog: harnessCatalog,
  workbench_effective_harness: effectiveHarness,
  workbench_codex_account_state: { status: "chatgpt", email: "mike@example.edu", planType: "plus", unsupportedAccountType: null, requiresOpenaiAuth: false },
  workbench_codex_rate_limits: { source: "perBucket", buckets: [{ limitId: "codex", limitName: null, planType: "plus", primary: { usedPercent: 38, remainingPercent: 62, windowDurationMins: 300, resetsAt: Math.floor(Date.now() / 1000) + 2 * 3600 }, secondary: null }] },
  workbench_codex_pending_requests: [],
  workbench_codex_connect: {},
  workbench_codex_model_catalog: workspaceModelCatalog,
  workbench_project_home: projectHome,
  workbench_project_index: [
    {
      id: workspace.id, name: workspace.name, root: workspace.root, missingRootAt: null, updatedAt: now,
      brief: { id: "q1", title: "Do county-pair minimum wage comparisons identify employment effects under spillovers?", kind: "question", updatedAt: now },
      conversation: { id: session.id, title: session.title, kind: "conversation", updatedAt: now },
      activity: { id: session.id, title: session.title, kind: "conversation", updatedAt: now },
      nextTask: null,
      proposedNotes: 2, interruptedEdits: 0,
    },
    {
      id: "ws-inventory", name: "Inventory dynamics survey", root: null, missingRootAt: null, updatedAt: "2026-09-18T10:00:00Z",
      brief: { id: "q2", title: "Survey chapter on inventory adjustment and aggregate fluctuations.", kind: "question", updatedAt: "2026-09-18T10:00:00Z" },
      conversation: null, activity: null, nextTask: null, proposedNotes: 0, interruptedEdits: 1,
    },
  ],
  workbench_project_tasks: [],
  workbench_project_mutate: {},
  workbench_project_capabilities: { fileAcceptance: false, hostExecution: false },
  workbench_list_papers: projectKind === "rich" ? [
    manuscript,
    { paper: { ...manuscript.paper, id: "paper-ref2", title: "Referee 2 report.pdf", role: "other" }, revision: { ...manuscript.revision, id: "rev-ref2", paperId: "paper-ref2", contentHash: "hash-r2" } },
    { paper: { ...manuscript.paper, id: "paper-scan", title: "Scanned appendix.pdf", role: "other" }, revision: { ...manuscript.revision, id: "rev-scan", paperId: "paper-scan", contentHash: "hash-s", extraction: { status: "failed", error: "No readable text" } } },
  ] : [],
  workbench_list_sources: [],
  workbench_list_notes: [],
  workbench_list_executions: [],
  workbench_list_execution_profiles: [],
  workbench_research_ledger: projectHome.ledger,
  workbench_list_recipes: [],
  workbench_list_recipe_runs: [],
  workbench_performance_budgets: [],
  workbench_storage_report: { totalBytes: 0, categories: [] },
  workflow_codex_status: { state: "ready" },
  workbench_research_activity: {
    turns: [],
    jobs: [],
    pendingRequests: [],
    researchAttention: 0,
  },
  task_sessions: [
    { id: "sess-aggregation", title: "Interpreting the county-pair estimates", workspaceName: "Minimum wage revision" },
  ],
  workbench_codex_reconcile_session: false,
  sync_project_issue_ledger: {
    schema_version: 1, project_id: "proj-minwage", updated: now, warnings: [],
    issues: [
      {
        id: "iss-1", title: "Pre-trend widening two quarters before treatment", severity: "high", section: "Identification",
        status: "open", note: "", created: "2026-09-27T16:40:00Z", updated: now, decision_updated: now,
        occurrences: [{ key: "occ-1", run_id: "run-2026-09-28-a", issue_id: "iss-1", observed_at: now, profile_id: "auto_paper_review", profile_name: "Auto Paper Review", input_name: "minwage_draft_v7.pdf", input_mode: "document", input_interpretation: "document", step_id: "auto_validate", step_label: "Validate Feedback", title: "Pre-trend widening two quarters before treatment", severity: "high", section: "Identification", body: "Event-study confidence bands widen before treatment; the no-anticipation claim in Section 5.2 overstates the evidence.", evidence: [] }],
      },
      {
        id: "iss-2", title: "Clustering level inconsistent between Tables 3 and 4", severity: "medium", section: "Inference",
        status: "addressed", note: "Re-estimated with commuting-zone clusters.", created: "2026-09-25T10:00:00Z", updated: now, decision_updated: now,
        occurrences: [{ key: "occ-2", run_id: "run-2026-09-27-b", issue_id: "iss-2", observed_at: "2026-09-27T16:40:00Z", profile_id: "auto_paper_review", profile_name: "Auto Paper Review", input_name: "minwage_draft_v6.pdf", input_mode: "document", input_interpretation: "document", step_id: "auto_validate", step_label: "Validate Feedback", title: "Clustering level inconsistent between Tables 3 and 4", severity: "medium", section: "Inference", body: "Table 3 clusters by state; Table 4 by county pair.", evidence: [] }],
      },
    ],
  },
  workbench_update_session: (args: { request: { overrides?: Record<string, unknown> } }) => {
    if (args.request.overrides) session.overrides = args.request.overrides;
    session.revision += 1;
    return { record: session, sequence: ++snapshot.sequence };
  },
  workbench_import_paper: (args: { request: { title: string } }) => ({
    paper: { ...manuscript.paper, id: `paper-${args.request.title}`, title: args.request.title, role: "other" },
    revision: { ...manuscript.revision, id: `rev-${args.request.title}`, contentHash: `hash-${args.request.title}` },
  }),
  workbench_context_selection: () => contextSelection,
  workbench_save_context_selection: (args: { items: unknown[] }) => (contextSelection = { revision: contextSelection.revision + 1, items: args.items }),
  workbench_research_object: (args: { object: { id: string; kind: string } }) => ({ object: args.object, title: sourceTitles[args.object.id] ?? args.object.id, text: "", provenance: "", access: "", completeness: "", truncated: false }),
  ...(projectKind === "rich"
    ? {
        workbench_studio_records: (args: { kind: string }) => studioRecords[args.kind] ?? [],
        workbench_binding_coverage: [
          coverage("-0.042", "stale", ["Declared execution inputs changed"]),
          coverage("0.118", "stale", ["Printed value or sign disagrees at the declared precision"]),
          coverage("1,204", "current", []),
          coverage("0.31", "unknown", ["Proposed link requires explicit confirmation"]),
        ],
        workbench_change_impact: {
          impacts: [
            { object: { kind: "execution", id: "exec-1", revision: "1" }, status: "input_changed", reason: "Declared input analysis/event_study.do changed since this execution", path: [{}] },
            { object: { kind: "result", id: "res-1", revision: "1" }, status: "review_needed", reason: "Input changed via Result adopted from this execution", path: [{}, {}] },
            { object: { kind: "record", id: "bind-1", revision: "1" }, status: "review_needed", reason: "Input changed via Result binding execution", path: [{}, {}, {}] },
          ],
          relations: [], complete: true, limitations: [],
        },
        workbench_repository_status: {
          branch: "main", head: "4f8fb37c0de", upstream: "origin/main", ahead: 1, behind: 2,
          fetchedAt: "2026-09-29T09:00:00Z", changed: 3, untracked: 1, conflicts: 0,
          changedPaths: ["paper/main.tex", "analysis/event_study.do", "output/table4.tex", "notes/scratch.md"],
          remote: { name: "origin", host: "github.com", owner: "mdroste", repo: "minwage", webUrl: "https://github.com/mdroste/minwage" },
          commits: [
            { sha: "4f8fb37c0de", author: "Michael Droste", date: "2026-09-29T14:10:00Z", subject: "Re-estimate Table 4 with commuting-zone clusters" },
            { sha: "46b4ae9", author: "Jane Doe", date: "2026-09-29T08:30:00Z", subject: "Rewrite the identification section" },
            { sha: "481e17a", author: "Michael Droste", date: "2026-09-20T12:00:00Z", subject: "Add event-study figure" },
          ],
          incoming: [
            { sha: "aaa1111", author: "Jane Doe", date: "2026-09-29T08:50:00Z", subject: "Respond to Referee 2 on spillovers" },
            { sha: "bbb2222", author: "Sam Lee", date: "2026-09-29T08:40:00Z", subject: "Update county-pair sample" },
          ],
        },
        workbench_program: (args: { action: { action: string } }) =>
          args.action.action === "monitors"
            ? { checks: [], lifecycle: "running", attention: [{ id: "att-1", body: { title: "FRED series CES7072200001 was revised", outcome: null }, createdAt: "2026-09-29T08:00:00Z", acknowledgedAt: null }] }
            : null,
      }
    : {}),
  task_list: projectKind === "rich"
    ? [{ id: "task-rr", revision: 2, name: "Re-review after Table 4 changes", state: "finished", reason: null, createdAt: 1790000000, updatedAt: Date.parse("2026-09-29T11:00:00Z") / 1000, dueAt: null, sessionId: session.id, scheduleId: null }]
    : [],
  task_schedules: [],
  task_saved_chains: [],
  task_proposals: [],
  mission_list: [],
  discovery_list: [],
  get_auto_review_catalog: { passes: [] },
};

const misses = new Set<string>();
let callbackId = 1;

(window as any).__TAURI_INTERNALS__ = {
  metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main", windowLabel: "main" } },
  transformCallback: () => callbackId++,
  unregisterCallback: () => {},
  convertFileSrc: (p: string) => p,
  invoke: async (command: string, _args?: unknown) => {
    if (command.startsWith("plugin:event|")) return callbackId++;
    if (command.startsWith("plugin:")) return null;
    if (command in commandData) {
      const value = commandData[command];
      return structuredClone(typeof value === "function" ? value(_args) : value);
    }
    if (!misses.has(command)) {
      misses.add(command);
      console.warn(`FIXTURE-MISS ${command}`);
    }
    if (/list|_all$|s$/.test(command)) return [];
    return null;
  },
};

if (params.get("theme") === "dark") localStorage.setItem("theme", "dark");
else localStorage.setItem("theme", "light");
const page = params.get("page");
const routePaths: Record<string, string> = {
  home: "#/home",
  main: "#/reviews/new",
  workspace: "#/project",
  "project-index": "#/projects",
  tasks: "#/automations",
  pipeline: "#/reviews/designer",
  gallery: "#/reviews/gallery",
  batch: "#/reviews/batch",
  projects: "#/reviews/collections",
  history: "#/reviews/history",
  settings: "#/settings",
  help: "#/help",
  activity: "#/activity",
};
if (page) localStorage.setItem("pipeline.ui.route", routePaths[page] ?? "#/home");
localStorage.setItem("pipeline.workspace.workspaceId", workspace.id);
localStorage.setItem("pipeline.workspace.sessionId", session.id);

export {};
