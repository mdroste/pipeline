# Workspace developer guide

Pipeline is an agent orchestration suite for doing and reviewing academic
research. Workspace is its persistent, interactive orchestration mode;
`workbench` is Workspace's compatibility implementation namespace. Reviews
(formerly Workflows) is the suite's deterministic orchestration mode. A workflow
remains a saved dependency graph; code and storage names are unchanged.

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

The research directory defaults to `~/.pipeline/workbench/` and follows
Settings → General → Research data folder after restart. Native credentials and
session files stay in local `~/.pipeline/workbench/codex/`. See
[storage selection](../storage.md) for scope and transfer limitations.

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

## Workspace layout

Workspace opens with a project working area and companion chat below one
compact bar. The project-name button opens navigation as a bounded drawer;
Keep navigation open pins it when space permits. Navigation is closed by
default. The current-tool button opens a searchable picker, also available
with Command/Ctrl K. It exposes every named tool, with the project's ordered
pins first. Navigation groups tools into Overview, Library, Analyses, Writing,
Notes & evidence, and Action items; Customize supports up to four shortcuts.
The Chat toggle hides/reopens chat, and the layout menu provides focus and
reset controls. Reset layout and shortcuts also restores pins and sidebar width.

Model, Thinking and the Assistant settings gear are below the message.
Settings opens preset, access, instructions, modules and recipes inside the
assistant pane. The conversation menu opens Outline, Context and Activity &
follow-ups in the same slot. Selected sources appear as a collapsed disclosure;
an empty source list occupies no space. Project setup and project notes also
start collapsed; Overview shows a brief and up to three open action items.
The full assistant editor
uses the project area and adapts to a single section picker at narrow widths.
Whole-store backups, retention and diagnostics are under Settings → General →
Research data; selective exchange and Send for review remain project tools.

Pinned navigation collapses when the remaining working area cannot fit both
panes. Below 848 pixels of working space, Project and Chat switch views while
retaining mounted drafts. Widening restores a preferred split. View, assistant
width, navigation pinning, project destination, last conversation and pins are local preferences;
they do not change project permissions. Research tools mount on first visit
and retain unsaved edits when switching destinations; polling stops for hidden
job, grid, check and execution views. These drafts remain scoped to the current
project/conversation and do not substitute for saving before leaving it.

See [the redesign plan and qualification record](workspace-ui-plan.md).

## Research desk

[NF-07–NF-14 implementation and usage](research-programs.md) covers specification grids, publication assets, theory checks, revision campaigns, explicit follow-ups, coauthor/replication materials, starter kits and app-open checks. It states the native branching/concurrency and persistent-service qualification boundaries.

[NF-01–NF-06 implementation and usage](research-desk.md) covers the persistent
object/assistant desk, exact-reference search and collections, decisions and
impact, literature acquisition, dataset/sample catalog, and captured execution.
The guide records service limits, sharing policy, archive recovery and observed
qualification. Migration 11 adds these records without replacing existing
research services or Workflow ownership.

## Implementation map

[File workspace](../file-workspace.md) covers the shared source editor, Markdown
links and previews, PDF navigation, source/PDF comparisons, scope boundaries,
and verification status.

### Frontend

- `gui/src/components/WorkspacePage.tsx` owns project/session navigation,
  the companion assistant, bounded transcript window, streaming, pending
  App Server requests and serialized composer saves. `WorkspaceDesk.tsx`
  owns the actual project/chat sizing; `WorkspaceProjectNavigation.tsx` and
  `lib/workspaceNavigation.ts` own named destinations and local pins.
  `WorkspaceToolPicker.tsx` provides searchable keyboard navigation and
  `WorkspaceMenu.tsx` provides viewport-bounded action popovers.
  `WorkspaceComposerControls.tsx` renders catalog-backed model/effort choices.
- `WorkspaceComposerMenu.tsx` exposes project navigation, dictation guidance,
  and the lazy `WorkspaceAttachmentsPanel.tsx` document importer/selector.
  `WorkspaceMessageActions.tsx` handles Markdown copy; the paged
  `WorkspaceConversationOutline.tsx` searches prompts/responses and asks the
  page to mount and focus the target transcript window.
- `WorkspaceConnectionSettings.tsx` owns isolated ChatGPT authentication,
  visible models/reasoning effort, and quota display.
- `WorkspaceResearchPanel.tsx` supplies the embedded assistant settings and
  the project tools for sources, notes, evidence, results, execution, review
  and exchange. `WorkspaceAssistantPresetForm.tsx` saves quick instruction and
  module edits as an explicit new preset copy for the current conversation.
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
- `WorkspaceExchangePanel.tsx` owns selective `.pwex` project exchange,
  import conflicts and Workflow drafts. `WorkspaceResearchDataSettings.tsx`
  and `WorkspaceStorageRetention.tsx` expose whole-store operations in Settings.
  See [project-exchange.md](project-exchange.md).
- `gui/src/lib/workbenchClient.ts`, `workbenchTypes.ts`, and
  `workbenchError.ts` are the typed Tauri boundary. Raw Codex protocol payloads
  must not cross it.

### Backend

- `gui/src-tauri/src/workbench/commands.rs` is the Tauri facade and owns bounded
  blocking database workers, the shared archive database gate, global turn
  serialization, active-turn identity, and the epoch-scoped event bridge.
- `store.rs` owns the versioned SQLite store, migrations, optimistic revisions,
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
  whole-store restore. Older supported archive databases are validated and migrated
  using the store migration runner on an isolated extracted copy before restore;
  archive migration failure leaves the destination unchanged.
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

Use `SidebarPanel` and `SidebarHeader` for Workspace sidebars, matching the
New Report surface and the workflow editor's shared panel shell. Keep title
spacing, neutral surfaces, and scroll regions consistent; scroll content inside
the shell so its divider remains reachable. Persist each panel's width with
`usePersistentPanelWidth`. Right-side inspectors put `ResizeHandle` on their
left edge; drag and arrow keys move that edge in the expected direction,
Shift uses a larger step, Home/End reach the bounds, and double-click resets.

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
