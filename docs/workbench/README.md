# Workspace developer guide

Pipeline is an agent orchestration suite for doing and reviewing academic
research. Workspace is its persistent, interactive orchestration mode;
`workbench` is Workspace's compatibility implementation namespace. Workflows
are the suite's deterministic orchestration mode.

| Term | Meaning |
|---|---|
| Workflow | A deterministic dependency graph of isolated model calls for repeatable research or review tasks |
| Workspace | Persistent ChatGPT conversations plus optional research capabilities |
| Harness preset | Resolved instructions, context providers, tools, inspectors, and permissions for a Workspace turn |
| Recipe | Optional versioned conversation instructions, input requirements, and completion checks; not a scheduler |
| Review handoff | Explicit copy of one immutable Workspace paper revision into the normal Workflow launch preview |

The product plan and current WB-step status live in
[`../../workbench_plan.md`](../../workbench_plan.md). Canonical repository-wide
architecture remains in [`../../CLAUDE.md`](../../CLAUDE.md). This page is the
short implementation map.

## Product boundary

Workspace works without a Pipeline project, Workflow run, profile, provider
selection, or paper. It owns its SQLite store, blobs, Codex home, native
process tree, cancellation, conversations, and research records under
`~/.pipeline/workbench/`. Unfiled conversations are valid.

Do not share Workflow credentials, settings, run state, PID registries, or
writable artifact roots with Workspace. Shared use of the Tauri shell and
carefully selected low-level utilities does not merge the domains. The Review
handoff and selected finding exchanges are explicit immutable copies. The
finding bridge previews selected canonical findings and retains Workspace
decisions separately from the Workflow issue ledger; neither side imports the
other's mutable runtime state.

## Implementation map

### Frontend

- `gui/src/components/WorkspacePage.tsx` owns the normal-chat route,
  workspace/session navigation, bounded transcript window, streaming, pending
  App Server requests, and draft composer.
- `WorkspaceComposerMenu.tsx` exposes project navigation, dictation guidance,
  and the lazy `WorkspaceAttachmentsPanel.tsx` document importer/selector.
  `WorkspaceMessageActions.tsx` handles Markdown copy; the paged
  `WorkspaceConversationOutline.tsx` searches prompts/responses and asks the
  page to mount and focus the target transcript window.
- `WorkspaceConnectionSettings.tsx` owns isolated ChatGPT authentication,
  visible models/reasoning effort, and quota display.
- `WorkspaceResearchPanel.tsx` owns the lazy, tab-scoped research inspector;
  its Setup tab is a read-only harness summary that opens the editor.
  `WorkspaceRecipesPanel.tsx` and `WorkspaceReleasePanel.tsx` isolate recipe and
  release/archive concerns.
- `WorkspaceHarnessEditor.tsx` is the lazy two-pane harness editor (preset
  navigator, Access & inheritance table, effective preview). The panels and
  pure helpers live under `harness-editor/`; presets are edited as a local
  draft and saved with the optimistic revision, built-ins stay read-only, and
  clones can be restored from their source preset.
- `WorkspaceResearchStudio.tsx` lazy-loads project Manuscript, Responses,
  Experiments, Result links, Literature and Theory panels, with a shared
  local-job drawer. See [research-studio.md](research-studio.md) and `studioClient.ts`.
- `WorkspaceExchangePanel.tsx` (inside the Release tab) owns selective `.pwex`
  project exchange, import conflicts, storage retention, and Workflow drafts.
  See [project-exchange.md](project-exchange.md).
- `gui/src/lib/workbenchClient.ts`, `workbenchTypes.ts`, and
  `workbenchError.ts` are the typed Tauri boundary. Raw Codex protocol payloads
  must not cross it.

### Backend

- `gui/src-tauri/src/workbench/commands.rs` is the Tauri facade and owns bounded
  blocking database workers, the shared archive database gate, global turn
  serialization, active-turn identity, and the epoch-scoped event bridge.
- `store.rs` owns the schema-10 SQLite store, migrations, optimistic revisions,
  operation IDs, conversations, native bindings, turns/items, and root
  reconciliation. `store/tests.rs` contains persistence and migration tests.
- `codex/` owns Workspace configuration, App Server supervision, native thread
  bindings, and durable event projection. Shared JSONL transport, compatibility,
  process ownership, account/model/quota operations, and simulation live in
  `gui/src-tauri/src/agent_runtime/codex/`. Sharing these primitives with the
  [Workflow backend](../workflow-codex.md) does not share runtime state or
  credentials. The development-only `workbench_probe` exercises the no-model
  native boundary.
- `research.rs` owns harness resolution, immutable paper/source revisions,
  dynamic research tools, notes, claims, and evidence.
  `research/execution.rs` owns tested execution profiles, process isolation,
  artifacts, structured results, comparisons, and verification receipts.
- `research/jobs.rs` owns the bounded local queue, turn/detached ownership,
  log cursors and adoption reconciliation. `project/studio/` implements the
  editor/build, response, experiment/binding and literature services.
- `release.rs` owns recipes, evaluation/performance records, and Review
  handoffs. `release/archive.rs` owns bounded `.pwrx` inspection, export, and
  whole-store restore.
- `migrations/` is append-only. Never edit an applied migration; add the next
  numbered migration and preserve backup, transaction, integrity, and
  newer-schema rejection behavior.

## Concurrency and durability

All production store access, including supervisor projections, goes through the
helpers in `commands.rs`. Ordinary work shares the database read gate and a
four-permit `spawn_blocking` pool. Whole-store archive export/restore takes the
exclusive gate; restore is rejected during turn setup and while a turn is
active; export and restore also reject active local jobs. Process waits use
separate blocking tasks and hold neither a database worker nor the archive gate.

Only one Workspace turn may be active process-wide. The permit spans setup,
durable submission bookkeeping, and the matching terminal event. A terminal
event may release it only when the connection epoch, thread, and turn match the
active state. Drafts are persisted before transport and cleared only after the
turn start is acknowledged. Ambiguous sends are never retried automatically.

App Server events are projections, not the sole source of truth. On lag,
reconnect, or restart, reconcile from `thread/read`; projections and replay must
remain idempotent. Pending approvals/questions are scoped by epoch, request ID,
and method so a stale connection cannot authorize a new one.

## Research trust model

- Harness configuration resolves global → Workspace → conversation and is
  snapshotted immutably for every turn. The effective harness also carries
  derived per-module availability reasons and labelled instruction sections;
  they are attached after fingerprinting so display-only additions never retire
  a native binding.
- Papers, source versions, execution outputs, and structured results are
  immutable or content-addressed. Locators always identify an exact revision.
- Dynamic tools derive Workspace scope from the active binding, enforce bounded
  inputs/results/call counts, and record a running receipt before side effects.
- Model-created notes, claims, evidence, and assessments remain proposals.
  Acceptance and human confirmation require explicit UI commands.
- Execution uses locally configured profiles with explicit host authorization,
  including profile tests. These commands run outside the conversation sandbox;
  command/launch/input changes invalidate their grants. LaTeX forbids enabling
  shell escape. On this Mac, Stata profiles must invoke `/bin/zsh -lic` with the
  `oldstata` wrapper; direct Stata binaries are rejected.

## Performance and UI rules

Normal conversation must not initialize document, evidence, computation, or
Workflow services. `WorkspacePage` and its research inspector are separate lazy
chunks. Research data loads only for the active tab. Stream deltas are coalesced
into 50 ms UI updates, and transcripts render a fixed 200-message window with
older/newer paging. Preserve these boundaries when adding modules.

## Archive and release rules

`.pwrx` export snapshots SQLite through the online backup API and includes only
the validated database, referenced immutable blobs, and Markdown/JSON
transcripts. It excludes the isolated Codex home and credentials. Import uses a
bounded inspection and exact manifest allowlist, rejects unsafe or inconsistent
archives, requires an empty destination store, and requires an explicit
remap-or-detach choice for every archived root. Imported native bindings are
retired and imported tools never run.

Protocol support is capability-gated rather than exact-version-pinned. Read
[`protocol/compatibility-policy.md`](protocol/compatibility-policy.md) and the
applicable compatibility record before changing wire behavior. Release status
is governed by [`release-qualification.md`](release-qualification.md): unit and
simulator coverage must never be described as authenticated, real-tool,
packaged-app, or cross-platform qualification.

## Focused verification

Workspace tests are part of the ordinary frontend and Rust suites:

```bash
cd gui
npm test -- --run
npm run build

cd src-tauri
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo run --locked --bin workbench_probe  # opt-in; no model turn
```

The probe needs a compatible native Codex CLI. Authenticated model/tool tests,
real LaTeX and Stata fixtures, packaged crash exercises, and platform checks are
manual release gates.

## Project home and reversible research tasks

[Project surface](project-surface.md) documents PI-00–PI-03 and the UI's
plain-language label map: accepted manuscript
and computational baselines, curated context, immutable document selections,
regular-file inventories, isolated local/Git tasks, diff review, recoverable
acceptance, and undo. `project.rs`/`project/` own these app records and file
contracts; `WorkspaceProjectSurface` and `WorkspaceDocumentReader` are lazy
frontend surfaces. Tool catalog 2 adds scoped read-only project-context and
anchor retrieval using the same services as the UI. Plain chat remains separate.
