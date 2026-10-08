# Workspace developer guide

Pipeline is an agent orchestration suite for doing and reviewing academic
research. Workspace is its persistent, interactive orchestration mode;
`workbench` is Workspace's compatibility implementation namespace. Reviews
(formerly Workflows) is the suite's deterministic orchestration mode. A workflow
remains a saved dependency graph; code and storage names are unchanged.

| Term                                      | Meaning                                                                                                  |
| ----------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Workflow                                  | A deterministic dependency graph of isolated model calls for repeatable research or review tasks         |
| Workspace                                 | Persistent ChatGPT conversations plus optional research capabilities                                     |
| Agent profile (harness preset internally) | Resolved instructions, context providers, tools, inspectors, and permissions for a Workspace turn        |
| Recipe                                    | Optional versioned conversation instructions, input requirements, and completion checks; not a scheduler |
| Review handoff                            | Explicit copy of one immutable Workspace paper revision into the normal Workflow launch preview          |

This page is the current implementation map. The older WB milestone plan is
retained as historical context in
[`../../notes/workbench_plan.md`](../../notes/workbench_plan.md). Canonical
repository-wide architecture remains in [`../../CLAUDE.md`](../../CLAUDE.md).

## Product boundary

The research directory defaults to `~/.pipeline/workbench/` and follows
Settings → Data & Storage → Research data folder after restart. Native session files stay in local `~/.pipeline/workbench/codex/`. Managed
ChatGPT credentials belong to the [shared account service](../chatgpt-account.md). See
[storage selection](../storage.md) for scope and transfer limitations.

Workspace works without a Pipeline project, Workflow run, profile, provider
selection, or paper. It owns its SQLite store, blobs, Codex home, native
process tree, cancellation, conversations, and research records under
`~/.pipeline/workbench/`. Unfiled conversations are valid.

Do not share Workflow API keys, settings, run state, PID registries, or
writable artifact roots with Workspace. The application-owned ChatGPT account
provides in-memory authentication to both isolated runtimes. Shared use of the Tauri shell and
carefully selected low-level utilities does not merge the domains. The Review
handoff and selected finding exchanges are explicit immutable copies. The
finding bridge previews selected canonical findings and retains Workspace
decisions separately from the Workflow issue ledger; neither side imports the
other's mutable runtime state.

## Workspace layout

The suite opens on a project-centered Home that routes users by outcome:
assistant work, paper review, or an automation. Home and Projects are the
primary destinations; recent projects appear directly in the suite rail, while
lower-level Review and automation controls remain under Tools. This navigation
layer does not merge the underlying Workspace, Review, or Tasks runtimes.

Projects opens a searchable index with local pins, recent activity, and direct
conversation resume actions. Selecting a project opens its Overview, with a
research brief, pending decisions, and recent work. Setup controls live in
Project settings. See [Projects index and overview](projects-home.md) for
behavior, ownership, and validation.

Inside a project, the working area and companion chat share one compact bar.
The project-name button opens navigation as a bounded drawer;
Keep navigation open pins it when space permits. Navigation is closed by
default. The current-view button opens a searchable picker, also available
with Command/Ctrl K. It exposes specialized project views without making them
peer sections. Navigation has six stable areas: Overview, Library, Analyze,
Write, Automate, and Activity.
The Chat toggle hides/reopens chat, and the layout menu provides focus and
reset controls.

Below the message sit the attach button, one model-and-thinking chip, and the
Assistant settings gear, with a single Send control that becomes Stop while a
response runs. The chip opens a picker with the connected account, remaining
usage, each model's description, and the thinking levels that model supports;
a saved choice that is no longer offered stays visible as a warning. The
message field grows with its text. The attach menu adds files, a project
document, or an automation; files dropped on the conversation follow the same
import path.
Settings opens preset, access, instructions, modules and recipes inside the
assistant pane. The conversation menu opens Outline, Context and Activity &
follow-ups in the same slot. Selected sources appear as a row of chips above the message, each with a
readable role and a remove button, with the overflow behind "+N more";
an empty source list occupies no space. Project notes start collapsed; Overview
shows resume, brief, attention, and recent-work sections. Project settings holds
the setup controls.
The full assistant editor
uses the project area and adapts to a single section picker at narrow widths.
Whole-store backups and retention are under Settings → Data & Storage;
diagnostics are under Settings → Advanced & About; selective exchange and Send for review remain project tools.

Pinned navigation collapses when the remaining working area cannot fit both
panes. Below 848 pixels of working space, Project and Chat switch views while
retaining mounted drafts. Widening restores a preferred split. View, assistant
width, navigation pinning, project destination, and last conversation are local preferences;
they do not change project permissions. Research tools mount on first visit
and retain unsaved edits when switching destinations; polling stops for hidden
job, grid, check and execution views. These drafts remain scoped to the current
project/conversation and do not substitute for saving before leaving it.

See [the redesign plan and qualification record](workspace-ui-plan.md).

## Agent profiles

Open **Assistant settings → Manage agent profiles** to create, find, duplicate,
select, or edit an agent profile. **New profile** starts with an empty added prompt
and no Workspace modules. The creation dialog offers this project or all
Workspaces; duplication copies the saved prompt and capabilities independently.
The System prompt tab edits the profile’s standing instructions. Tools & context
controls the Workspace capability allowlist, with unavailable tools explained.
Native command/file permissions remain under Access & inheritance for the
conversation. Profile prompts do not grant permissions.

Codex default, Writing, Code review, Economics research, and the existing research
starters are read-only. Duplicate one to customize it. Editing a custom profile
changes future turns for every conversation using it. Changes are saved with
optimistic revisions; navigating away prompts before discarding a draft.

The System prompt tab separates **Use Codex default** / **Replace base prompt**
from **Supplemental instructions**. Writing, Code review, and Economics research
start with domain-specific replacement base prompts; the older research starters
continue to inherit Codex. Custom replacements are sent verbatim through stable
`thread/start.baseInstructions`. Supplemental profile instructions, Pipeline's
preamble, and recipe/context layers use `developerInstructions`. Native runtime
instructions, tools, permission enforcement, and project instructions remain
separate; a base override does not replace the entire assembled turn.

**View Codex default prompt** reads model templates from local `models_cache.json`
files. Pipeline's isolated runtime cache is offered first when available; the
regular `~/.codex` cache is also available, explicitly marked as reference only.
The viewer includes the source path, model, cache client version, fetch timestamp,
installed CLI version, and additional cached instruction sections. Unknown cache
formats, missing files, and version mismatches are visible. No cached text is
silently installed as a prompt. A custom profile can explicitly copy the selected
template into its replacement editor; template variables are not expanded and
should be reviewed before saving. Cache contents are neither a live prompt readback
nor proof of the exact default selected by the running server. Reading the viewer
uses no model call and reads no session histories or credentials.

On the inspected Codex 0.153.4 installation, the package contains compiled binaries;
the model cache contains the plaintext templates. Parsing strings out of the binary
would be brittle, so Pipeline uses the structured cache and reports when it is
unavailable. The App Server contract is documented in the
[official OpenAI documentation](https://learn.chatgpt.com/docs/app-server), and the
[file-based base override](https://learn.chatgpt.com/docs/config-file/config-sample#instruction-overrides)
is an alternative for standalone CLI configuration. Pipeline uses per-thread
fields so profiles do not modify the global Codex installation.

Schema 14 adds nullable `presets.base_instructions`: existing profiles inherit the
native default. Legacy JSON snapshots omit this field, preserving their hashes;
old update requests preserve a stored override. New requests use
`basePrompt: { mode: "codexDefault" }` or `{ mode: "replace", text: "…" }`.
Replacement text is bounded to 256 KiB and cannot be blank or contain NUL.
Cloning, restoring from source, and full research archive round trips retain the
base prompt. Selective project exchanges continue to exclude all harness presets.
Changing a saved base prompt changes the harness fingerprint and starts a fresh
native binding on the next turn; resuming an unchanged binding retains its native
instructions. Renaming Plain conversation to Codex default preserves its existing
binding fingerprint.

### Agent profile verification

Tests cover read-only built-ins, base replacement/reset and supplemental separation,
legacy migration/update/snapshot compatibility, archive round trips, native request
payloads, bounded modern/legacy cache parsing, cache provenance and error states.
The browser fixture at `gui/e2e/agent-profiles.html` uses explicitly labeled sample
metadata to check creation, prompt edits, saved tool choices, and compact/light/dark
layouts without native or model calls. A no-model probe against installed Codex
0.153.4 accepted `thread/start` with separate base and developer instructions.
After `thread/inject_items` supplied one harmless user fixture, its native
`session_meta.base_instructions.text` exactly matched the replacement and
`thread/resume` succeeded. No model turn was started.

All 655 frontend tests, 938 Rust tests (10 ignored), 58 release checks, the
frontend build, and all-feature Clippy with warnings denied passed. The release
checks passed with local server access for their temporary localhost fixtures. Whole-repository formatting
checks still report pre-existing differences. These checks do not qualify
authenticated model turns or packaged releases.

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

- `gui/src/components/WorkspacePage.tsx` composes the project and companion
  assistant. `gui/src/hooks/useWorkspacePageController.ts` owns project/session
  navigation, bounded transcripts, streaming, pending App Server requests,
  and serialized composer saves. `useWorkspaceTranscript.ts` owns on-demand
  transcript pages; `lib/workspaceSessionState.ts` merges metadata acknowledgements
  without replacing the draft or transcript. `WorkspaceDesk.tsx`
  owns the actual project/chat sizing; `WorkspaceProjectNavigation.tsx` and
  `lib/workspaceNavigation.ts` own named destinations and local pins.
  `WorkspaceToolPicker.tsx` provides searchable keyboard navigation and
  `WorkspaceMenu.tsx` is the "more" button over the shared `ui/Menu`.
  `WorkspaceComposerControls.tsx` renders the catalog-backed model and
  thinking picker.
- Floating layers, list boxes, segmented choices, tooltips, sheets, and
  skeletons come from `gui/src/ui/`; `ui/anchoredPosition.ts` is the one
  positioning routine. Colors, radii, elevation, and type sizes are the
  `--ui-*` tokens in `gui/src/App.css`, mapped into Tailwind in
  `tailwind.config.js`. Workspace styles are split by owner:
  `WorkspaceDesk.css`, `WorkspaceTranscript.css`, `WorkspaceComposer.css`,
  `WorkspaceNavigation.css`, and `WorkspaceInspector.css`.
- `WorkspaceComposerMenu.tsx` is the attach menu; it opens the lazy
  `WorkspaceAttachmentsPanel.tsx` to import files or choose a project
  document. `lib/workspaceAttachments.ts` owns the import path and default
  source roles, shared with `hooks/useComposerFileDrop.ts`, which receives
  native file drops through `lib/fileDrop.ts`. `WorkspaceContextTray.tsx`
  renders attached sources as chips.
  `WorkspaceMessageActions.tsx` handles Markdown copy; the paged
  `WorkspaceConversationOutline.tsx` searches prompts/responses and asks the
  page to mount and focus the target transcript window.
- `ChatgptConnection.tsx` owns the common sign-in, account selection, model,
  and quota panel. `WorkspaceConnectionSettings.tsx` composes it with
  conversation-specific preferences.
- `WorkspaceResearchPanel.tsx` supplies the embedded assistant settings and
  the project tools for sources, notes, evidence, results, execution, review
  and exchange; `research-panel/useResearchPanelController.ts` owns scoped
  loading, drafts, and mutations. `WorkspaceAssistantPresetForm.tsx` saves quick
  instruction and module edits as a new preset copy for the current conversation.
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
  serialization, and active-turn identity. `commands/codex.rs` owns submission,
  server requests, and the epoch-scoped event bridge; `commands/` also groups
  desk, project, and research handlers through the same database gates.
- `store.rs` owns SQLite setup, migrations, shared records, and the journal;
  `store/{workspaces,sessions,runtime,views}.rs` implement roots, conversations,
  bindings/turns/event persistence, and snapshots/reconciliation with optimistic
  revisions. `store/roots.rs` revalidates registered folder authority at dispatch;
  `store/history.rs` owns bounded transcript pages and full-history export traversal.
  `store/tests.rs` contains persistence and migration tests.
- `codex/` owns Workspace configuration, App Server supervision, native thread
  bindings, and durable event projection. Shared JSONL transport, compatibility,
  native-session launch/shutdown, typed thread/turn submission,
  process ownership, account/model/quota operations, and simulation live in
  `gui/src-tauri/src/agent_runtime/codex/`. Sharing these primitives with the
  [Workflow backend](../workflow-codex.md) does not share writable execution state. Authentication is supplied by the
  [shared account service](../chatgpt-account.md), with a lease spanning the
  matching terminal event. The development-only `workbench_probe` exercises the no-model
  native boundary.
- `research.rs` owns harness resolution and shared research record types;
  `research/{papers,sources,notes,ledger,tools}.rs` own immutable revisions,
  notes, claims/evidence, and dynamic research tools.
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

`.pwrx` format 2 snapshots SQLite through the online backup API and includes
the validated database, immutable blobs, app-owned conversation working files,
and Markdown/JSON transcripts. Conversation payloads are bounded, staged and
checksummed; export requires idle native turns and local jobs. Format 1 remains
readable but contains no conversation working files. External project folders,
Review/Automation stores, Trash, the isolated Codex home and credentials are
excluded; back up those external folders separately. Import uses a
bounded inspection and exact manifest allowlist, rejects unsafe or inconsistent
archives, requires an empty destination store, and requires an explicit
remap-or-detach choice for every archived root. Imported native bindings are
retired, unfinished turns become interrupted history, and imported tools never run.
Trash journals are stripped on export and import; their machine-local paths
never authorize file operations in the restored store.

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
cargo clippy --locked --all-targets --all-features -- -D warnings
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
