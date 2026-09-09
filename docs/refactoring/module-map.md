# Feature module map

Use this map to find an owner before reading an entire feature. Preserve public
APIs, IPC command names, stored JSON, provider supervision, and the separate
product runtimes. This map follows the current checkout, including
splits after R0–R5; it does not establish release qualification. The
[R0–R5 implementation record](r0-r5-progress.md) contains evidence for that
stage; the [full plan](../refactoring-plan.md) tracks the broader work.

## App, Workspace, and Tasks presentation

- `App.tsx` composes the desktop shell; `hooks/useAppController.ts`
  owns navigation, application state, events, and Review launch coordination.
  `lib/appRunPreparation.ts` owns pure launch-preparation helpers and types;
  `lib/appClient.ts` is the typed IPC boundary for shell-owned runtime commands.
- `components/WorkspacePage.tsx` composes project and conversation views;
  `hooks/useWorkspacePageController.ts` owns navigation, transcript paging,
  pending requests, and serialized composer saves. `hooks/useWorkbenchEvents.ts`
  isolates the live event subscription, coalesced streams, and terminal-turn
  cleanup.
- `components/WorkspaceResearchPanel.tsx` renders the research inspector;
  `components/research-panel/useResearchPanelController.ts` owns its scoped
  loading, drafts, and mutations.
- `components/TasksPage.tsx` owns task navigation, selection, refresh, and events.
  `components/tasks/` contains `Builder.tsx`, `Detail.tsx`, `Timing.tsx`,
  `ScheduleCard.tsx`, and shared presentation helpers. All task IPC is routed
  through `lib/taskClient.ts`.

Paths in this section are relative to `gui/src/`. These controller extractions
do not merge the owning runtime or storage boundaries.

The About dialog's easter egg lives in `components/FlappyBirdGame.tsx` (pilot
selection and game loop). `components/flappy/` owns shared game dimensions and
types in `model.ts`, canvas sprites and composition in `render.ts`, and the
scrolling San Francisco artwork in `scenery.ts`.

## Workspace backend

Under `gui/src-tauri/src/workbench/`:

- `commands.rs` retains database workers, archive gates, and active-turn state.
  `commands/codex.rs` owns turn submission, requests, and the event bridge;
  `commands/{desk,project,research}.rs` group the remaining feature handlers.
- `store.rs` retains shared records, store setup, migrations, and journal
  mechanics. `store/workspaces.rs`, `sessions.rs`, `runtime.rs`, and `views.rs`
  own workspace roots, conversations, binding/turn/event persistence, and
  snapshots/preferences/root reconciliation respectively.
- `research.rs` retains harness resolution and shared persistence helpers.
  `research/types.rs` owns the public harness DTOs and `catalog.rs` owns stock
  modules and presets. `research/{papers,sources,notes,ledger,tools}.rs` own the
  corresponding services; `execution.rs`, `execution_plan.rs`, and `jobs.rs`
  own local work.

See the [Workspace guide](../workbench/README.md) for contracts and invariants.

## Workflow provider calls

Under `gui/src-tauri/src/pipeline/`, `api_common.rs` retains shared constants,
HTTP clients, and the public facade. `api_common/loops.rs` owns tool loops,
`execution.rs` owns host-tool execution, `retry.rs` owns HTTP retry/error policy,
and `models.rs` owns provider types, usage, and bounded tool file access.

`claude.rs` remains the compatibility facade. `claude/dispatch.rs` routes model
calls; `claude_cli.rs`, `cli_workspace.rs`, `artifacts.rs`, and `process.rs`
own Claude CLI calls, staging, artifact handling, and process helpers. Preserve
the supervised `pipeline/call.rs` boundary when changing these adapters.

## Artifact Explorer

`gui/src/components/ArtifactExplorer.tsx` composes the Sources shell. Read:

- `gui/src/lib/artifactTypes.ts` for manifest/content/selection DTOs;
  `artifactClient.ts` for typed run-owned reads, also used by ReportWorkspace
  and the read-only file-workspace adapter.
- `components/artifact-explorer/useArtifactSelection.ts` for manifest-first
  loading, preloads, cancellation of stale results, selection and back history.
- `ArtifactTree.tsx`, `PageNavigator.tsx`, `navigation.ts`, `grouping.ts` for
  artifact grouping and compact/legacy page navigation.
- `ArtifactViewer.tsx`, `PdfArtifactPreview.tsx`, `ArtifactPreviews.tsx` for
  dispatch and preview fallbacks. The existing PDF.js/file workspace stays lazy.
- `DocumentBundleView.tsx` and `documentBundle.ts` for bundle presentation and
  equation-number coalescing. Format helpers are in `format.ts`.

Relative component filenames above are under `gui/src/components/artifact-explorer/`.
Run Sources still uses its owning manifest and read-only adapter; Workspace
file saves remain scoped to its own backend with expected-hash validation.

## Settings and workflow editor

`gui/src/components/SettingsPage.tsx` composes the selected settings section.
Under `components/settings/`, `useSettingsDraft.ts` owns settings loading,
immutable snapshots and one serialized autosave queue. `useProviderCatalogs.ts`
owns credential/transport-sensitive discovery, request identity and stale-data
invalidation; successful saves explicitly notify this owner. Navigation,
controls and each settings section are separate modules. Workspace account
state remains in its existing isolated connection component. `CredentialField.tsx`
keeps edits private until Save; `SaveState.tsx` aggregates pending operations for
navigation without merging stores. `search.ts` owns searchable preference metadata.
`lib/appPreferences.ts` owns device presentation preferences; `useAppAppearance.ts`
and the reader/editor/composer consume them. `useAppNotifications.ts` adapts shell
outcome events into `lib/appNotifications.ts`; it never shares runtime ownership.
See [Settings behavior and validation](../settings.md).

`gui/src/components/PipelinePage.tsx` composes the selected workflow editor.
Under `components/pipeline-editor/`:

- `useWorkflowEditor.ts` owns the profile-scoped draft, selection, schema
  validity, undo/redo and local recovery. Its replacement transition resets
  history when switching profiles.
- `useProfileOperations.ts` owns loading, mutation identity, saving, profile
  operations and save-before-export. It uses the draft owner rather than
  maintaining a second copy of the configuration.
- `useStepActions.ts` owns edit confirmations and step operations;
  `editorState.ts` projects extraction changes and shares dependency/artifact/
  condition cleanup between disabling and removing a step.
- `WorkflowNavigator.tsx` renders profile, step and schema navigation.
  `useWorkflowView.ts` holds shared visual selection, and
  `useWorkflowShortcuts.ts` keeps keyboard handlers current.
- `useWorkflowCatalogs.ts` loads the editor's persisted provider catalogs.
  Existing editor panels, schema editors and dialogs keep their prior roles.

## Workflow executor

`gui/src-tauri/src/pipeline/executor.rs` is the public facade. Its modules are
private to the executor feature; sibling imports name their dependencies.

| Module | Owner |
|---|---|
| `schedule.rs` | One dependency readiness algorithm used by preview and runtime; skip and dependency policies |
| `run.rs` | Mutable run state, completion ordering, resume, checkpoints and phase dispatch |
| `parallel.rs`, `sequential.rs` | Phase-specific dispatch; both use the supervised unit-call primitive |
| `step_call.rs` | Retry/validation flow, cancellation checks, bounded report ingestion and response journaling |
| `artifact_context.rs` | Private selector-filtered artifact views, staging and shared context preparation |
| `prompts.rs` | Bounded substitutions, evidence guidance and output-format instructions |
| `units.rs` | Deterministic file/artifact fan-out and agent expansion |
| `findings.rs` | Ordered finding lineage, validation dispositions and preserved-findings collapse |
| `checkpoints.rs` | Atomic completed/failed products, replacement/removal and run output budget |
| `outputs.rs` | Step output construction and schema-safe merge acceptance |
| `paths.rs` | Deterministic step identity and output directories |

`pipeline/provenance.rs` shares call-record construction with `merge.rs`.
Callers choose explicit roles and retain their existing accounting policy;
logical step attempts and provider attempts are distinct. Provider calls still
enter through `pipeline/call.rs`, with no changes to cancellation registries,
credentials, stores, or Workspace lifecycle.

Executor tests live in `executor/tests/{schedule,artifacts,findings,units,outputs,
prompts,placeholders}.rs`, with shared fixtures in `executor/tests.rs`. Other
large inline test modules moved beside their original source under
`<source-stem>/tests.rs`. Command and profile tests are grouped by behavior.

## Size reporting

Run `node scripts/source-size-report.mjs` from the repository root. It reports
source/tests above 1,200 lines or 50,000 bytes, long physical lines, and growth
relative to `source-size-baseline.json`. `--json` exposes the full inventory.
The default is report-only. Add `--check` (or run `npm run check:source-size`
from `gui/`) to fail on newly oversized files or growth beyond the baseline;
unchanged existing debt does not fail the check. Generated-file exceptions,
ownership and review triggers are in `scripts/source-size-policy.json`.
Do not refresh the baseline simply to hide new growth. Use the current report
to locate remaining size debt rather than inferring it from a milestone label.

## September review fixes

- `commands/reuse_artifacts.rs` owns bounded immutable source snapshots and
  producer-file copies for reruns. Snapshot hashes detect altered retained sources.
- `pipeline/executor/checkpoints.rs` publishes atomic wave completion receipts;
  `runs/history.rs` overlays them on provisional unit checkpoints during recovery.
- `pipeline/extract/paddle_full.rs` owns full-parser invocation under its engine
  lease; `settings/quarantine.rs` owns serialized, non-overwriting recovery backups.
- `scripts/release/draft-release.mjs` verifies signed tag identity and coordinates
  immutable draft assets. The active workflow is documented in `RELEASING.md`.

- `pipeline/extract/staged.rs` records and resolves captured LaTeX references, including nearby external includes, without reopening the live project.

Google Review calls and model discovery use the Gemini API under the existing
`antigravity` provider ID. The retired CLI adapter and probe mechanics have been
removed; only saved-settings compatibility fields remain. `deps/checks.rs` owns
the API-key readiness record labeled Google API.
