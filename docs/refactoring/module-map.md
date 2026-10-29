# Feature module map

Use this map to find an owner before reading an entire feature. Preserve public
APIs, IPC command names, stored JSON, provider supervision, and the separate
product runtimes. This map follows the current checkout, including
splits after R0–R5; it does not establish release qualification.

## App, Workspace, and Tasks presentation

- `App.tsx` composes the desktop shell; `hooks/useAppController.ts` composes
  `hooks/app/` owners for navigation, attention, native close, theme, batch
  activity, setup and Review launch. Each owner retains its original effects;
  navigation commit and legacy persistence remain separate.
  `lib/appRunPreparation.ts` owns pure launch-preparation helpers and types;
  `lib/appClient.ts` is the typed IPC boundary for shell-owned runtime commands.
- `components/WorkspacePage.tsx` composes project and conversation views;
  `hooks/useWorkspacePageController.ts` composes navigation, transcript paging
  and grouped page contracts. `hooks/workspace/` owns Chrome, session mutations,
  session persistence and the conversation runtime; `useTranscriptFollow.ts`
  owns transcript following and paging anchors, and
  `components/WorkspaceTranscriptMessage.tsx` renders memoized messages.
  Persistence has one write lane for drafts and model selections. The runtime owns hydration, submission,
  terminal identity and its private event bridge; native subscription callbacks
  retain their original effect registration phase and 50 ms stream flush.
- `ui/` is the shared interaction kit: `Popover`, `Menu`, `Select`,
  `SegmentedControl`, `Tooltip`, `IconButton`, `Sheet`, `Skeleton`, and the
  icon set, positioned by `ui/anchoredPosition.ts`. Semantic `--ui-*` tokens
  live in `App.css`. Workspace styles are per owner (`WorkspaceDesk.css`,
  `WorkspaceTranscript.css`, `WorkspaceComposer.css`,
  `WorkspaceNavigation.css`, `WorkspaceInspector.css`).
- `components/WorkspaceComposerControls.tsx` (model and thinking picker),
  `WorkspaceComposerMenu.tsx` (attach menu), and `WorkspaceContextTray.tsx`
  (source chips) are the composer's controls. `lib/workspaceAttachments.ts`
  owns file import and default source roles; `hooks/useComposerFileDrop.ts`
  and `lib/fileDrop.ts` own native file drops.
- `components/WorkspaceResearchPanel.tsx` renders the research inspector;
  `components/research-panel/useResearchPanelController.ts` owns its scoped
  loading, drafts, and mutations. The six stateless tab views live beside it.
- `components/ProjectIndexPage.tsx`, `hooks/useProjectIndex.ts`, and
  `lib/projectIndex.ts` own the Projects index. `project-surface/ProjectOverview`,
  `ProjectNotes`, `ProjectSettings`, and `ProjectActionItems` split the project
  home's presentation. `hooks/useWorkspaceEntry.ts` interprets external navigation
  requests; `hooks/useRecentProjects.ts` loads suite navigation metadata. See
  [Projects home](../workbench/projects-home.md).
  `project-surface/ProjectDestination.tsx` dispatches the retained destinations;
  `useProjectSurfaceData`, `useProjectReadingState` and
  `useProjectSelectionActions` retain loading, reading/focus and mutation gates.
- `components/research-desk/Data.tsx` owns the data catalog and samples;
  `DataConnectors.tsx` owns the FRED, DBnomics and World Bank forms,
  `DatasetDictionary.tsx` the label/missing-code dictionary view, and
  `ConnectorNotice.tsx` each source's required notice. Desk IPC uses
  `lib/deskClient.ts`.
- `components/TasksPage.tsx` owns task navigation, selection, refresh, and events.
  `components/tasks/` contains `Builder.tsx`, `Detail.tsx`, `Timing.tsx`,
  `ScheduleCard.tsx`, and shared presentation helpers. All task IPC is routed
  through `lib/taskClient.ts`; task types, templates and pure presentation live
  in `taskTypes.ts`, `taskTemplates.ts`, and `taskPresentation.ts`.
- `lib/getStarted.ts` owns the device-local first-run record (closed date and
  chosen kind of work) and open requests from Home and Settings.
  `components/get-started/GetStartedHost.tsx` decides when the guide or the
  Example Review is open and lazily loads `GetStartedSheet.tsx`
  (`ConnectionsStep.tsx`, `ToolsStep.tsx`). `guide.ts` holds persona content
  and pure connection, PDF-tool and toolchain summaries over existing readiness,
  account, settings and `workbench_detect_toolchains` results. `ExampleReview.tsx`
  renders the bundled `exampleReviewData.ts` sample through `IssuesTable` with no
  run ID; it writes nothing to the run store. See
  [Get started](../README.md#get-started-in-pipeline).

Paths in this section are relative to `gui/src/`. These controller extractions
do not merge the owning runtime or storage boundaries.

Workspace transport lives under `lib/workbench/`: workspace, conversation,
harness, research, execution, recipe, release and runtime clients. Production
callers import their domain directly; there is no runtime umbrella barrel.
History/export/settings/profile/schema/account views use narrow typed clients,
reusing `artifactClient.ts` for existing run-owned reads. Payloads remain literal.

Pure transcript interpretation lives in `lib/workspaceTranscript.ts`, shared by
conversation rendering and paging/controller hooks. Review event/state records
live in `lib/reviewRuntimeTypes.ts`; `usePipeline` owns subscriptions and
`runViews` owns pure projection. `research-panel/types.ts` owns inspector tab
contracts, and `lib/studioPresentation.ts` owns prose normalization for the
existing Studio preview diff.

The About dialog's easter egg lives in `components/FlappyBirdGame.tsx` (pilot
selection and game loop). `components/flappy/` owns shared game dimensions and
types in `model.ts`, canvas sprites and composition in `render.ts`, and the
scrolling San Francisco artwork in `scenery.ts`.

## Workspace backend

Under `gui/src-tauri/src/workbench/`:

- `commands.rs` retains database workers, archive gates, and active-turn state.
  `commands/codex.rs` owns turn submission, requests, and the event bridge;
  `commands/{desk,project,research}.rs` group the remaining feature handlers.
- `store.rs` retains the Store/error boundary, setup and backup ownership.
  `store/{records,schema,projection,journal}.rs` own records, the migration
  runner, projection and journal mechanics. `store/workspaces.rs`, `sessions.rs`, `runtime.rs`, and `views.rs`
  own workspace roots, conversations, binding/turn/event persistence, and
  snapshots/preferences/root reconciliation respectively. `store/roots.rs` validates
  registered directory identity; `store/history.rs` owns transcript paging and export
  traversal. `project/task_pages.rs` filters open context before its limit and pages
  the complete task history.
- `research.rs` is the service facade with shared persistence helpers.
  `research/{config,context,tool_catalog,harness,snapshots}.rs` own configuration,
  bounded context, tool declarations, ordered resolution and frozen snapshots.
  `research/types.rs` owns the public harness DTOs and `catalog.rs` owns stock
  modules and presets. `research/{papers,sources,notes,ledger,tools}.rs` own the
  corresponding services. `research/execution/` separates types, profiles,
  authorization, process setup, outputs, lifecycle and results.
  `execution/adapters.rs` owns each adapter's argv shape (Stata direct and
  custom launcher, R, Python, Julia, notebook, Quarto) and environment-file
  lists; `execution/toolchains/` owns read-only tool detection, environment
  identity and runtime-version receipts. Complete
  prepare/start/wait/finalize operations retain ownership of their guards;
  `execution_plan.rs`, `jobs.rs` and `finalization.rs` remain separate owners.
- `release.rs` is the explicit service facade for `release/recipes.rs`,
  `recipe_runs.rs`, `evaluations.rs`, and `review_handoff.rs`. Whole-store
  archive, selective exchange, retention and workflow-draft ownership remain
  distinct. `release/exchange/` separates records, dependency selection, package
  decoding, export, import planning/application and conflicts; `fingerprint.rs`
  remains its content-identity owner. `release/archive/` owns whole-store
  format, inspection, export, restore and transcripts; the install guard remains
  in `conversation_files.rs`. Restore remains one ordered operation.
- `project/index.rs` and `project/index.sql` own the bounded, read-only project
  metadata query used by `workbench_project_index`; it does not load project
  artifacts, transcripts, or model runtimes.
- `data/mod.rs` owns dataset DTOs, the data policy and `dictionary_text`.
  `data/catalog.rs` owns capture (`prepare_local` outside database workers,
  `commit` inside), previews, exporter dictionaries and samples;
  `data/{csv,dta,xlsx}.rs` are the bounded format readers (`xlsx/sheet.rs`
  streams one worksheet) and `data/profile.rs` their shared profile and preview
  window. `data/connectors/` owns the `DataConnector` requests/capture split,
  connector terms, and the FRED, DBnomics and World Bank adapters; HTTP
  primitives and the Crossref contact remain in `acquisition.rs`.

`project/{records,home,inventory,blobs}.rs` separate scoped records, project
home/context, bounded file capture and immutable blobs. The project facade
retains mutation dispatch, locking and program input ownership.

See the [Workspace guide](../workbench/README.md) for contracts and invariants.

## Shared ChatGPT invocation

Under `gui/src-tauri/src/agent_runtime/codex/`, `session.rs` owns the common
native launcher, handshake, home verification, diagnostics, and shutdown;
`invocation.rs` owns typed thread-start and text-turn submissions. Workspace's
`codex/supervisor.rs` and Reviews' `pipeline/codex_server/` each own independent
instances and retain their policy checks, persistence, and cancellation.
`agent_runtime/codex/chatgpt/` owns the shared managed account, migration,
refresh handling, and active-work leases; `ChatgptConnection.tsx` is the common
account panel. See [ChatGPT account](../chatgpt-account.md).
See [Workflow Codex](../workflow-codex.md) for the consolidation record.

## Workflow provider calls

Review exports retain their command boundary in `commands/export.rs`.
`commands/export/{types,files,manifest,package,print,desktop}.rs` own command
records, confined copying/publication, checksums, disclosure selection,
self-contained HTML rendering, and session-scoped reveal/print cleanup.
`commands/export/formats.rs` loads a finished run, verifies the reviewed
manuscript against the run's content hash, and saves shareable report formats
without writing to run storage. The pure renderers live in `output/formats/`:
`model.rs` (format-neutral findings and evidence labels), `quote_match.rs`
(quotation normalization and search), `html.rs` (offline HTML with a
hash-pinned CSP), `docx_package.rs` and `docx_markdown.rs` (fresh Word report),
`docx_scan.rs` and `docx_comments.rs` (comments anchored in a copy of the
reviewed `.docx`), and `pdf_words.rs` and `pdf_annotate.rs` (Poppler word
boxes and `lopdf` incremental-update highlights). Workspace tracked-change
export is not part of this owner.
The headless entry point is `bin/cli.rs`; its implementation lives in `cli/`
(outside `bin/` to preserve bundler binary discovery), with pipe-safe output
still in `cli_io.rs`.

Under `gui/src-tauri/src/pipeline/`, `api_common.rs` retains shared constants,
HTTP clients, and the public facade. `api_common/loops.rs` owns tool loops,
`execution.rs` owns host-tool execution, `retry.rs` owns HTTP retry/error policy,
and `tool_schema.rs`, `tool_access.rs`, `tool_files.rs`, `wire.rs`, and
`usage.rs` separate exact tool declarations, per-call authority, bounded file
operations/run quotas, wire records and accounting. Provider loops remain
independent and retain their Drop-based usage receipts.

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

Report presentation helpers live in `lib/reportPresentation.ts` and navigation/
AST helpers in `lib/reportNavigation.ts`. `components/report-workspace/` owns
independent resource reads and stateless report sections; `report-viewer/` owns
contents and error boundaries. Console grouping/formatting is in
`lib/consolePresentation.ts`; request/token views live in `components/console/`.
The existing parents retain interactive selection, scroll, focus and timing.

`gui/src-tauri/src/auto_review/` separates compiled assets, generated catalog
identity, selection checks, prompt/schema construction, readable findings and
host-only materialization. The facade retains its public API and contract keys.

## Settings and workflow editor

`gui/src-tauri/src/settings/` separates flat serialized records (`types.rs`),
model policy, extraction resolution, ordered validation, persistence and run
snapshots. Crypto, quarantine and recovery retain distinct contracts and share
the existing persistence lock. The settings facade preserves public imports.


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

Workflow configuration records live in `pipeline_config/{step,inputs,profile,legacy}.rs`.
`validation/` separates steps, graph, artifacts, conditions and profile checks
without changing their ordered entry points. `persistence.rs` owns the existing
profile mutation lock and bounded reads. `migrations.rs` retains the ordered
runner; `migrations/{catalog,prompts,retirement,suites}.rs` contain its unchanged
migration operations and marker rules.

PDF extraction has a small `pipeline/extract/pdf.rs` facade and focused
`pdf/{native,paddle_server,full_structure,cache,page_quality,llm}.rs` owners.
The full-parser lease/invocation remains in `extract/paddle_full.rs`.

`gui/src-tauri/src/runs/` separates immutable records, identity/OS locking,
file inspection, writer/Drop, history, recovery and trash. Existing artifact,
atomic persistence, retention and unlisted-output owners remain distinct.
Run tests are grouped by records, writer, recovery and retention behavior.

`commands/run.rs` and `rerun.rs` retain separate run coordinators, frozen
settings, cancellation scopes, writers and temporary-file lifetimes.
`run/{inputs,visuals,orientation,document,report}.rs` own fresh-run stages;
`rerun/{files,reuse,inputs,document}.rs` own captured-input and reuse policy.
Helpers borrow the coordinator's resources or return temporary handles that
are immediately bound there. `reuse_artifacts.rs` retains confined snapshots
and producer copying; no live-source fallback is introduced for reruns.

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
| `budget.rs` | Per-Review cost, call, and time limits checked before each wave; the preflight usage estimate shown in the run preview and `check` |
| `analyzer_step.rs` | Runs analyzer steps on the host and records their outputs and failures like model steps |

`pipeline/analyzers.rs` is the facade for deterministic host analyzers.
`analyzers/quotes.rs` verifies every finding's quoted evidence after a run and
writes the `evidence_verification` product; `analyzers/fold.rs` owns the
documented text folding with offset maps. `analyzers/registry.rs` is the closed,
versioned list of analyzer steps, their options, inputs, and network use.
`analyzers/references.rs` (with `references/{bibtex,bibliography,crossref}.rs`)
implements `reference_audit`; `analyzers/replication.rs` implements
`replication_static`. Workflow validation of analyzer steps is in
`pipeline_config/validation/steps.rs`.

`pipeline/provenance.rs` shares call-record construction with `merge.rs`.
Callers choose explicit roles and retain their existing accounting policy;
logical step attempts and provider attempts are distinct. Provider calls still
enter through `pipeline/call.rs`, with no changes to cancellation registries,
credentials, stores, or Workspace lifecycle.

Executor tests live in `executor/tests/{schedule,artifacts,findings,units,outputs,
prompts,placeholders,analyzers}.rs`, with shared fixtures in `executor/tests.rs`. Other
large inline test modules moved beside their original source under
`<source-stem>/tests.rs`. Command and profile tests are grouped by behavior.

## Durable task coordination

Under `gui/src-tauri/src/orchestration/`, the root retains the coordinator,
singleton, database-worker limit, ownership lock and shared gate.
`coordinator/{scheduler,completion,recovery}.rs` retain complete scheduling,
completion and recovery methods. Scheduling keeps save-before-dispatch adjacent
to worker creation; completion and recovery retain their distinct sequences.

`store/{records,runs,receipts,schedules,signals,chains}.rs` group methods along
existing transaction boundaries. `adapters/{prepare,snapshots,execute,review,preview}.rs`
separate immutable preparation and product adapters while `execute.rs` retains
one complete cancellation/journal envelope, including its early returns.
`missions/{accounting,chains,planning,assessment}.rs` own pure policy; admission
and adoption stay in their existing transaction owners. Coordinator tests are
grouped into interpreter, availability, schedules, receipts and snapshots;
the existing real-store coordinator scenarios remain separate.

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

## October release fixes

- `workbench/release/retention/paths.rs` validates local trash journal paths before filesystem actions.
- `workbench/release/archive/conversation_files.rs` stages and restores bounded version-2 conversation payloads; archive migration and regression tests have separate modules.
- `commands/orchestration/pins.rs` reconciles settled failed Review pins against operation ownership.
- `hooks/useWorkspaceRouteEntry.ts` invalidates superseded navigation loads while the controller owns draft persistence and selection.
- `scripts/release/poppler-inventory.mjs` records actual bundled file hashes; `qualify-release.mjs` validates candidate-specific release evidence.
- `runs/unlisted.rs` registers files that landed in a run directory without `add_text`: a destructive policy for model output and a non-destructive one for captured source snapshots. `runs/history.rs` owns the single run lock path shared by the pipeline guard, recovery, and pin release.
- `workbench/release/archive.rs` owns the bounded zip-entry copy shared with `.pwex` import; `workbench/research/papers.rs` hashes host-owned files through their resolved targets for launch manifests.
- `agent_runtime/codex/transport.rs` offers a lossless per-thread server-request sink that Review App Server calls register for the life of a call.
- `workbench/release/exchange/fingerprint.rs` owns the content fingerprint that ignores import-rewritten bookkeeping; `workbench/research/notes.rs` and `ledger.rs` load single notes, claims (including retired) and evidence links by id for export.
- `settings/recovery.rs` owns load diagnostics for the Settings page, discarding a stored key that no longer decrypts, and import normalization of exported settings.
- `cli_io.rs` (included by `bin/cli.rs`) owns pipe-safe stdout output, report file writing, and absolute resolution of CLI input paths.
- `workbench/missions.rs` owns the read-only source-scope refresh and the retry re-adoption used by missions and self-discovery; `orchestration/discovery/mod.rs` classifies failures as attention or blocked.

## Claude subscription boundary

- `agent_runtime/claude.rs`: stateless CLI authentication and billing guard shared
  by Reviews and Workspace, with independently owned process supervision.
- `workbench/claude/`: Workspace stream-json transport, lifecycle, and MCP bridge;
  `shell.rs` sandbox policy, `title.rs` title process, `usage.rs` usage reports.
- `workbench/history.rs`: bounded app-owned text history for independent turns
  and provider switches.
- `workbench/project/assistant_files.rs`: scoped Claude file tools over the
  existing safe-root and Studio edit journal.
- `useWorkspaceModelCatalog.ts`: provider-specific connection and model loading.

See [Claude Code](../claude-code.md) for the contract and qualification evidence.
