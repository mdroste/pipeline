# Pipeline refactoring audit and implementation plan

Audit date: September 7, 2026. Scope: the current working tree on
`codex/nf07-nf14`, including tracked modifications and untracked feature work.
This is an implementation plan; the audit does not change application code.

## 1. Assessment

The largest opportunity is to separate responsibilities inside a small number
of central modules. Pipeline already has useful service boundaries and shared
components. Extending those boundaries will be more valuable than introducing
a new framework or mechanically dividing every file into equal-sized pieces.

The inventory found **21 source files above 1,200 lines**, containing **42,560
lines**. Of those, 5,335 lines are in top-level inline test modules; 37,225 lines
remain outside those modules. Three files—`supervisor.rs`, `ledger.rs`, and
`structured.rs`—fall below the threshold after their inline tests are separated.
The remaining 18 need an implementation split if the threshold is to be met.

Start with these groups:

1. **Workflow execution:** `pipeline/executor.rs` and `pipeline/api_common.rs`.
   They combine scheduling, artifact authority, prompt construction, provider
   protocols, output validation, and accounting. Changes require reading too
   much unrelated code and can cross important execution boundaries.
2. **Workspace services:** `workbench/research.rs`, `commands.rs`, and `store.rs`.
   Harness configuration, documents, evidence, native turns, SQL persistence,
   and project features have accumulated in broad entry-point files.
3. **Frontend state ownership:** `SettingsPage.tsx`, `PipelinePage.tsx`,
   `ArtifactExplorer.tsx`, and the densely formatted Workspace/Tasks pages.
   Existing component extractions have left substantial state and action
   orchestration in their parents.

Several concrete sharing opportunities exist, particularly subprocess byte
draining, call provenance, Codex protocol validation helpers, Markdown
rendering policy, and account-status presentation. Shared implementation must
continue to use separate Workflow and Workspace owners, credentials, stores,
permissions, cancellation, and recovery rules.

### Method and limits

The audit used repository documentation, a file/byte inventory, symbol maps,
targeted implementation and test inspection, import searches, and a
whitespace-normalized search for repeated ten-line source blocks. The clone
search supplied leads; the recommendations below distinguish literal
duplication from related code with different contracts. It is not a measured
repository-wide duplication percentage or an exhaustive correctness review.

The inventory includes tracked files and nonignored untracked files, plus the
local `CLAUDE.md` and `AGENTS.md`. It covers Rust, TypeScript/JavaScript, Python,
CSS, SQL, shell, HTML, Markdown, configuration files, and Cargo's lockfile.
Build outputs, dependencies, bundled binaries, and ignored generated build
directories are excluded. Counts include comments and blank lines. “Outside
tests” below subtracts top-level inline `mod tests` blocks; it is not an AST
count of executable production code. For example, `workbench/commands.rs`
contains production commands *after* its inline test module.

Line references and counts describe this working-tree snapshot. Recompute
them before implementation because substantial feature work is uncommitted.
No builds, tests, native probes, model calls, or benchmarks were run for this
documentation-only audit. Validation listed below is work for the refactoring
PRs, not a claim that those gates have passed on this snapshot.

## 2. Complete inventory above 1,200 lines

P1 means first refactoring wave because of read burden and mixed
responsibilities. P2 means a subsequent focused cleanup. Risk describes the
behavior that a refactor could accidentally change, not a confirmed defect.
Module names in the final column are proposed destinations.

### Application and development-tool source

| File | Lines | Outside inline tests | Priority / risk | Proposed split |
|---|---:|---:|---|---|
| [pipeline/executor.rs](../gui/src-tauri/src/pipeline/executor.rs) | 5,569 | 4,040 | P1 / high | Scheduler, artifact context, unit expansion, prompt construction, step calls, findings lineage, checkpoints, output accounting; focused tests |
| [pipeline/api_common.rs](../gui/src-tauri/src/pipeline/api_common.rs) | 3,306 | 2,674 | P1 / high | HTTP policy, tool schemas, scoped filesystem tools, tool budgets/dispatch, per-provider wire types and loops; focused tests |
| [workbench/research.rs](../gui/src-tauri/src/workbench/research.rs) | 2,827 | 2,827 | P1 / high | Harness catalog/config/resolution, context, notes, paper capture/read, sources, dynamic tools, evidence |
| [workbench/release/exchange.rs](../gui/src-tauri/src/workbench/release/exchange.rs) | 2,316 | 1,965 | P2 / high | Format, dependency collection, package I/O, import planning, import application, conflicts; tests |
| [auto_review.rs](../gui/src-tauri/src/auto_review.rs) | 2,292 | 1,580 | P2 / medium | Catalog, schema contracts, review-plan validation, profile materialization, specialist rendering; tests |
| [workbench/store.rs](../gui/src-tauri/src/workbench/store.rs) | 2,184 | 2,184 | P1 / high | DTOs/errors, connection/migrations, workspaces/roots, sessions/preferences, native bindings/projections, operation journal |
| [workbench/commands.rs](../gui/src-tauri/src/workbench/commands.rs) | 2,172 | 2,105 | P1 / high | One store worker/gate service; separate turn/event owner; command groups by service |
| [pipeline/claude.rs](../gui/src-tauri/src/pipeline/claude.rs) | 2,068 | 1,631 | P1 / high | Provider dispatch, request types, CLI workspace/prompt preparation, Claude adapter, result parsing; tests |
| [SettingsPage.tsx](../gui/src/components/SettingsPage.tsx) | 1,923 | 1,923 | P1 / medium | Settings draft/autosave, model catalog state, navigation, General/Providers/Workflow sections |
| [bin/cli.rs](../gui/src-tauri/src/bin/cli.rs) | 1,778 | 1,514 | P2 / medium | Argument parsing, run/check/batch commands, workflow/profile/engine commands, output files, interrupt handling; tests |
| [PipelinePage.tsx](../gui/src/components/PipelinePage.tsx) | 1,723 | 1,723 | P1 / medium | Editor state/history, profile operations, import/export, navigator and dialogs; retain existing editor panels |
| [workbench/codex/supervisor.rs](../gui/src-tauri/src/workbench/codex/supervisor.rs) | 1,609 | 1,064 | P2 / high | Separate tests first; then thread contract, side turns, projections, supervisor lifecycle as needed |
| [ArtifactExplorer.tsx](../gui/src/components/ArtifactExplorer.tsx) | 1,596 | 1,596 | P1 / medium | Artifact DTOs/client, bundle inspector, page navigator, tree/grouping, selected-artifact loader, viewer shell |
| [settings.rs](../gui/src-tauri/src/settings.rs) | 1,542 | 1,542 | P2 / high | Persisted schema/defaults, model policies, validation/normalization, persistence, frozen run snapshot; retain existing crypto module |
| [pipeline/extract/pdf.rs](../gui/src-tauri/src/pipeline/extract/pdf.rs) | 1,512 | 1,512 | P2 / high | pdftotext/baselines, Paddle server, Full Parser, LLM ranges/retries, cache fingerprints |
| [workbench/research/execution.rs](../gui/src-tauri/src/workbench/research/execution.rs) | 1,505 | 1,505 | P2 / high | Profiles/authorization, input manifests, process execution, receipt/adoption lifecycle, structured results/comparison |
| [workbench/codex/probe.rs](../gui/src-tauri/src/workbench/codex/probe.rs) | 1,404 | 1,281 | P2 / high | Probe orchestration, bounded probe wire I/O, fixtures/config, sandbox cases, command/process cases; tests |
| [commands/export.rs](../gui/src-tauri/src/commands/export.rs) | 1,379 | 1,379 | P2 / medium | Export package selection/staging/manifests, native destination operations, print math/HTML, print-file lifecycle |
| [projects/ledger.rs](../gui/src-tauri/src/projects/ledger.rs) | 1,377 | 1,100 | P2 / medium | Separate tests first; occurrence/evidence adapters, matching/status derivation, persistence/mutations if subsequently needed |
| [pipeline/structured.rs](../gui/src-tauri/src/pipeline/structured.rs) | 1,274 | 876 | P2 / high | Separate tests first; later isolate provider projections from portable validation if either grows |
| [runs.rs](../gui/src-tauri/src/runs.rs) | 1,204 | 1,204 | P2 / medium | Move `RunWriter` into `runs/writer.rs`; capture helpers into `runs/source_capture.rs`; keep public types/facade |

Moving tests alone will reduce read overhead immediately, but it is insufficient
for the largest files. Avoid replacing a 5,569-line executor with a 4,040-line
executor and a 1,529-line `tests.rs` and calling the work complete.

### Other files above the threshold

| File | Lines | Disposition |
|---|---:|---|
| [pipeline_config/tests.rs](../gui/src-tauri/src/pipeline_config/tests.rs) | 1,392 | Split by built-ins/migrations, dependency and selector validation, portable workflow parsing, persistence/profile operations |
| [commands/tests.rs](../gui/src-tauri/src/commands/tests.rs) | 1,239 | Split by launch/resume, artifact handling, package export, print rendering, profile import; keep small common fixtures |
| [CLAUDE.md](../CLAUDE.md) | 1,339 | Reduce the mandatory read to an architecture map, invariants and commands; move detailed topic material into linked developer documents |
| [SOL_AUG23_FULLSEND_FEATURES.md](../notes/SOL_AUG23_FULLSEND_FEATURES.md) | 1,637 | Historical proposal; keep as an archive/reference, outside mandatory agent reading. No mechanical split needed |
| [experimental.schemas.json](workbench/protocol/0.147.0/experimental.schemas.json) | 26,014 | Immutable vendor protocol reference; retain exact bytes/checksum. Provide a generated method/type index for targeted reads |
| [stable.schemas.json](workbench/protocol/0.147.0/stable.schemas.json) | 22,658 | Same treatment; do not hand-edit or reformat to meet a line threshold |
| [gui/package-lock.json](../gui/package-lock.json) | 13,388 | Dependency lockfile; exclude from source-size policy |
| [Cargo.lock](../gui/src-tauri/Cargo.lock) | 6,065 | Dependency lockfile; exclude from source-size policy |

`CLAUDE.md` and `AGENTS.md` are intentionally ignored by this checkout's
`.gitignore`. Any later documentation reorganization should put durable topic
guides under tracked `docs/` paths and deliberately update the local navigation
files; it should not force-add personal instruction files.

## 3. Files the line threshold misses

Compressed formatting makes several files appear small while preserving nearly
all of the reading cost. Counts below are raw UTF-8 bytes and the number of
physical lines longer than 200 characters; they are diagnostic, not style
violations in isolation.

| File | Lines | Bytes | Lines >200 chars | Action |
|---|---:|---:|---:|---|
| [WorkspacePage.tsx](../gui/src/components/WorkspacePage.tsx) | 686 | 47,894 | 32 | P1: format in a separate change, then separate session navigation, drafts, turn events and transcript paging |
| [TasksPage.tsx](../gui/src/components/TasksPage.tsx) | 100 | 25,978 | 41 | P1: format, then separate prepared-task form, scheduling, chain library, run details and task-list loading |
| [workbench/release.rs](../gui/src-tauri/src/workbench/release.rs) | 1,124 | 52,095 | 30 | P2: move recipes/completion, evaluations/performance and Review handoffs into named modules |
| [WorkspaceResearchPanel.tsx](../gui/src/components/WorkspaceResearchPanel.tsx) | 179 | 27,496 | 24 | Split tab implementations and tab-scoped data loading after formatting |
| [WorkspaceProjectSurface.tsx](../gui/src/components/WorkspaceProjectSurface.tsx) | 173 | 20,509 | 23 | Retain project shell; move desk navigation and service actions into its existing feature directories |
| [WorkspaceExchangePanel.tsx](../gui/src/components/WorkspaceExchangePanel.tsx) | 291 | 20,171 | 21 | Separate exchange export, import/conflict review and retention surfaces |

`WorkspacePage.tsx` has 30 `useState` calls and a maximum line length of 2,594
characters. `TasksPage.tsx` has 35 `useState` calls despite its 100-line count.
These are stronger signals of parent-component responsibility than physical
length alone. The counts do not imply that every state value needs its own hook.

Also watch [App.tsx](../gui/src/App.tsx) (1,165 lines),
[ReportWorkspace.tsx](../gui/src/components/ReportWorkspace.tsx) (1,013),
[models.rs](../gui/src-tauri/src/models.rs) (1,164),
[findings.rs](../gui/src-tauri/src/findings.rs) (1,117),
[pipeline_config.rs](../gui/src-tauri/src/pipeline_config.rs) (1,032), and
[pipeline_config/validation.rs](../gui/src-tauri/src/pipeline_config/validation.rs)
(1,072). Touch these when the owning refactor needs them, rather than expanding
the initial project into an unrelated repository-wide rearrangement.

## 4. Refactoring rules and architecture boundaries

### Reading budget

Use **300–800 normally formatted lines** as a preferred module range, around
1,000 as a warning, and 1,200 as a request for a split plan or a documented
exception. A facade should generally fit in 100–250 lines. Cohesion takes
precedence over making every file exactly the same size.

Track bytes as well: 40–50 KB of hand-maintained source warrants inspection
even below the line limit. Flag unusually long lines and functions over about
200 lines for human review. Long literal fixtures and schemas need different
treatment from multi-responsibility control flow. Byte/4 is not a reliable
token count for code; use an actual tokenizer if token budgets are later added.

Judge each split by the **set of files an agent must read to make a change**.
A routine change should usually require the small facade, its owning module,
and its relevant tests. A forest of tiny helpers, circular imports or a large
shared context object can make reading harder despite reducing file lengths.

### Preserve the existing owners

The implemented boundaries are described in [the Workspace guide](workbench/README.md),
[Workflow Codex](workflow-codex.md), [Tasks](tasks.md),
[the file workspace](file-workspace.md), and [storage selection](storage.md).
The historical product plan must be read alongside these newer implementation
records; for example, cross-mode tasks and the file editor now exist.

| Concern | Shared implementation is appropriate | Ownership that stays separate |
|---|---|---|
| Codex | Wire parsing, bounded transport, version admission, account/model/quota parsing, process mechanics | Runtime singleton, private home/login, activity gates, turn ownership, invocation journals and recovery |
| Documents | Scoped byte operations, deterministic conversion, page rendering, neutral file/PDF UI | Selected roots, extraction policy, Workspace revisions, Workflow runs and retained artifacts |
| Findings | Canonical finding DTOs and immutable exchange conversions | Workflow issue status and history; Workspace researcher decisions, evidence confirmation and staleness |
| Processes | Isolation setup, bounded drains, low-level owned-tree operations | PID registries, cancellation signals, Stata wrapper cleanup, active job/turn/run state |
| Persistence | Small byte/path/atomic-file primitives with explicit policy | SQLite databases, migrations, archive formats, retention journals, writable roots and transactions |
| UI | Markdown/code/PDF rendering, panels, account/usage presentation | Drafts, autosave queues, launch preparation, login subscriptions and product navigation state |

Keep `workbench` command/module/storage names and existing serde JSON fields
stable. Mechanical extraction needs no migration, profile version bump,
changed harness fingerprint or altered archive format. If a proposed cleanup
requires one, make it a separate behavior/compatibility change.

Use explicit imports and narrow `pub(super)`/`pub(crate)` visibility. Retain
small compatibility re-exports at existing public paths while moving internals.
Avoid broad `use super::*` as the permanent interface of new production
modules: it hides which state each module consumes. Co-locate DTOs with their
owning service and expose selected DTOs through existing facades.

## 5. Detailed extraction plans

### A. Workflow executor: highest-value backend refactor

Evidence in `pipeline/executor.rs`: artifact resolution begins at line 320;
`execution_plan` at 692; `execute_steps` at 809; step calls at 2,033; unit
expansion at 2,735; parallel execution at 3,159; sequential execution at 3,869.
The main scheduler alone spans roughly 537 lines. Tests start at 4,041.

Keep `executor.rs` as the public facade for `ExecutionResult`,
`ExecutionPlanStage`, `execution_plan`, `execute_steps`, and `dependents_of`.
Create the following `pipeline/executor/` modules:

| Module | Responsibility and existing symbols | Approximate target |
|---|---|---:|
| `schedule.rs` | Dependency readiness, dependency-policy failure, plan preview, terminal/skipped state; `ready_indices`, `resolve_dependencies`, `dependents_of` | 450–700 lines |
| `run.rs` | `execute_steps` orchestration and completion order; owns mutable run state | 500–750 |
| `artifact_context.rs` | `ArtifactRuntime`, `ResolvedArtifactContext`, staging and read-root selection, selected shared context | 500–700 |
| `prompts.rs` | Parallel/sequential prompt construction, bounded substitutions, evidence guidance, output-format text | 500–750 |
| `units.rs` | File/artifact fan-out and agent expansion, deterministic keys, `build_units`, `build_artifact_units` | 300–500 |
| `findings.rs` | Ordered lineage, validation dispositions, normalized finding hashes, deterministic preserved-findings collapse | 400–650 |
| `step_call.rs` | One unit's retry/call/validation flow, response-journal integration and typed call results | 450–650 |
| `parallel.rs`, `sequential.rs` | Dispatch mechanics for each phase; share unit call primitives, retain phase policies | Each 250–600 |
| `checkpoints.rs` | Atomic completed/failed checkpoints and removal/replacement rules | 200–350 |
| `outputs.rs` | Step output construction and schema-safe merge acceptance; use shared provenance helpers from section 6 | 250–450 |

These ranges are design targets, not a forecast that total lines will shrink.
Small extractions can be combined if their inputs and invariants are the same.

Order: move tests into `executor/tests/{schedule,artifacts,prompts,fanout,findings,outputs}.rs`;
extract pure prompt/graph/findings helpers; extract artifact staging and
checkpoints; finally move async orchestration. Keep one scheduler implementation
behind both launch preview and actual execution. Do not create a separate
preview-only graph algorithm.

Use a small immutable run-input structure and an explicitly owned mutable
execution state where they reduce repeated arguments. Do not make every child
module depend on a “god context” containing settings, storage, provider clients,
and cancellation registries. Keep `pipeline/call.rs` as the supervised model-call
entry point; these modules must not bypass it.

Acceptance: existing scheduling, resume, failure/quorum, zero-match fan-out,
multi-agent namespaces, selector isolation, schema-safe merge fallback,
ordered-finding lineage, response journaling, usage accounting and cancellation
tests remain green. Add characterization coverage only where these boundaries
are currently uncovered. Compare execution-plan IDs and checkpoint products on
the same fixtures before and after the move.

### B. Direct API infrastructure and the Claude-named router

`api_common.rs` contains tool declarations (line 248 onward), `ToolAccess`
(447), provider wire DTOs (903 onward), three provider loops (1,245, 1,422,
1,614), batch tools (1,930 onward), and HTTP retry policy (2,502 onward).
These have different reasons to change.

Retain an `api_common.rs` facade initially. Extract:

- `api_common/http.rs`: client selection, capped responses, cancellation-aware
  sends, status retry and bounded error formatting.
- `api_common/tool_schema.rs`: Read/Write/batch declarations and shared logical
  capability mapping.
- `api_common/tool_access.rs`: validated read/write roots, file opening,
  text slicing, media reads and write reservations.
- `api_common/tool_dispatch.rs` and `batch.rs`: `ToolBudget`, call/history/media
  accounting, execution and batch aggregation. Keep each under the budget.
- Provider-specific wire DTOs, loops and result conversion beside
  `api_anthropic`, `api_openai`, and `api_google`, in child modules as needed.
  Their current parent files are already 541, 590 and 427 lines; do not simply
  append all moved code to those files.

Preserve explicit provider loops initially. Anthropic content blocks, OpenAI
tool messages, and Gemini parts differ, including hosted search, PDF support,
finish reasons and media limits. A generic provider-loop trait is not justified
by their similar outer control flow. Share bounded transport/tool primitives,
not a lowest-common-denominator protocol.

`execute_native_tool` already lets the Workflow App Server adapter use these
Workflow-owned host tools. Preserve that reuse. Workspace dynamic tools derive
scope from bindings and durable receipts; they should not acquire a Workflow
`ToolAccess` object or its write budget.

`pipeline/claude.rs` also contains neutral workflow dispatch (`call_llm`, line
1,234), all-provider request types (`LlmOverrides`, 353), CLI path planning
(188–352), and command configuration (1,557 onward). Move dispatch/request
types to `pipeline/dispatch.rs` and `pipeline/request.rs`; move workflow CLI
preparation to `pipeline/cli_workspace.rs`. Keep Claude session reuse,
permissions and envelope parsing under `pipeline/claude/`. Preserve temporary
re-exports until callers migrate. This is an internal naming correction, not
a provider migration or a change to the `call.rs` supervision boundary.

Acceptance: cover all three direct APIs plus the local OpenAI-compatible path,
hosted-search capability handling, images/PDFs, per-call budgets and scoped
paths, retry exhaustion, truncated responses, cancellation, fallback provenance,
and native Workflow tool dispatch. Provider-specific schema projections and
error classification must remain unchanged.

### C. Workspace research services

`research.rs` combines harness resolution (`resolve_harness`, line 705), notes
(1,034), papers/capture (1,212), sources (1,834), dynamic tool dispatch (2,109)
and claims/evidence (2,443). Its existing `research/` directory contains
execution, execution plans, jobs and tests, so extend that structure.

Keep a small `research.rs` service facade and create:

- `harness/catalog.rs`, `config.rs`, and `resolve.rs`: built-ins, persistence,
  inheritance, module availability and effective configuration.
- `context.rs`: bounded context assembly and turn preparation, preserving
  exact instruction ordering and the fingerprint input.
- `notes.rs`: note DTOs, validation, CRUD and accepted/pinned-state rules.
- `papers/capture.rs` and `papers/read.rs`: immutable capture/import versus
  revision lookup, read/search and page images. Keep path/hash helpers with
  capture until a verified shared primitive exists.
- `sources.rs`: source identity, acquisition metadata, reads and imports.
- `tools/catalog.rs` and `tools/dispatch.rs`: declared tools versus scoped
  receipt/authorization/dispatch. Service handlers stay in their owning modules.
- `evidence.rs`: claims, evidence, freshness and confirmation rules.

Introduce a Workspace-local persistence/helper module only for the repeated
ID/change-record/JSON-bound logic these services actually share. Avoid a
generic repository/CRUD framework: notes, immutable revisions and evidence
confirmation have materially different transactions and authority.

Split the 1,505-line execution module separately into `profiles.rs`,
`authorization.rs`, `inputs.rs`, `process.rs`, `receipts.rs`, and `results.rs`.
Keep the current prepare → wait → finalize sequence: process waits remain
outside database workers, and output adoption remains recoverable. Preserve
live-file versus captured-plan consistency labels, grant fingerprints,
declared-only dependency coverage and Stata wrapper completion after Stop.

Acceptance: fixed harness snapshots/fingerprints and dynamic-tool declaration
fixtures remain identical; imports retain hashes and exact locators; duplicate
tool calls return the existing receipt; model proposals never become human
confirmation. Exercise result comparison, input changes invalidating grants,
job cancellation and output adoption with existing deterministic fixtures.

### D. Workspace command and store boundaries

`workbench/commands.rs` contains 149 Tauri command annotations, the database
gate/worker pool, turn permits, pending server requests, event bridging and
native submission logic. Its opening comment calls it a thin facade, but its
scope is much broader. Splitting it solely into command wrappers would leave
the most important state ownership obscured.

Create `workbench/commands/` with:

- `store_worker.rs`: the single four-worker semaphore and shared/exclusive
  archive gate, exposing the existing `run_store*` entry points.
- `turn_state.rs`: one process-global turn permit, setup state, epoch/thread/
  turn matching, pending requests and coordinated idle checks.
- `events.rs`: native event routing and the bounded store-projection bridge.
- `turns.rs`: submit/interrupt/reconcile and automated-turn entry points,
  using that same state owner.
- Small command groups: `sessions.rs`, `connection.rs`, `research.rs`,
  `project.rs`, `studio.rs`, `release.rs`, `desk.rs`, and `programs.rs`, adjusted
  to their actual size after formatting.

Keep `workbench::commands` as the public gateway. It may delegate production
SQL work to its private worker module; service callers must still cross this
gateway. Do not create one semaphore or archive gate per command group. Update
Tauri registration paths and test actual command names/argument shapes: moving
`#[tauri::command]` functions also moves macro-generated wrapper paths.

For `workbench/store.rs`, preserve the `Store` type/API but distribute its
`impl Store` blocks and private helpers across `store/`:

- `types.rs` and `error.rs` for app-owned records and errors.
- `connection.rs` and `migrations.rs` for private directories, SQLite setup,
  backups, schema admission and transactional migrations.
- `workspaces.rs` and `roots.rs` for membership-independent workspaces and
  canonical root/identity rules.
- `sessions.rs` for drafts, movement, deletion, preferences and snapshots.
- `bindings.rs` and `projection.rs` for native bindings, turn submissions,
  bounded event projections and replay.
- `changes.rs` for revision checks, operation IDs and monotonic change records.

Keep transactions intact across the split. Child methods should accept an
existing connection/transaction where an operation is already atomic, rather
than opening new connections to call each other. Existing migration SQL stays
versioned and unchanged.

Acceptance: exercise restart/replay, duplicate submissions, terminal events
during setup, stale-epoch approvals, draft edits during submission, session
movement, task/missions callers, archive exclusivity and restore during an
active/setup turn. Preserve migration rollback/backup, foreign keys, revision
conflicts, DTO fixtures and independent credential-home behavior under custom
research storage.

### E. Frontend stateful pages

| Parent | Extraction plan | Behavior that must stay in one owner |
|---|---|---|
| `SettingsPage.tsx` | `settings/useSettingsDraft.ts` for load/serialized autosave; `useProviderCatalogs.ts` for credential/transport-sensitive requests; `SettingsNavigation`, `GeneralSettings`, `ProviderSettings`, `WorkflowDefaults`, `PdfExtractionSettings`, `HistorySettings` | One save queue, latest-snapshot accounting, secret-preserving backend save, stale catalog suppression, dirty navigation protection |
| `PipelinePage.tsx` | Extend `pipeline-editor/` with `useWorkflowEditor`, pure edit/reducer helpers, `useProfileOperations`, import/export operations, navigator and modal host | Undo/redo history, profile mutation request identity, schema draft validity, dirty protection, atomic switching and save-before-export |
| `ArtifactExplorer.tsx` | `artifact-explorer/` with types/client, `useArtifactSelection`, `ArtifactTree`, `PageNavigator`, `DocumentBundleView`, bundle projection helpers and `ArtifactViewer` | Manifest-first loading, selected-content requests, stale-result rejection, page index fallback, back navigation and read-only run adapter |
| `WorkspacePage.tsx` | `workspace/useSessionNavigation`, `useConversationDraft`, `useWorkspaceTurn`, `useTranscriptWindow`; separate sidebar and request cards; retain existing `WorkspaceConversationView` | Draft serialization, active-turn/session identity, event coalescing, pending approvals, 200-message paging and scroll/focus behavior |
| `TasksPage.tsx` | `tasks/TaskList`, `TaskPreparation`, `ScheduleEditor`, `ChainLibrary`, `TaskRunDetail`; hook for bounded list/detail subscriptions | Prepared definition and scope must match Start; task ownership/recovery state and selection-specific async results |
| `App.tsx` | Later: `useRunLaunch` for preflight/variables/start, `useAppNavigation` for dirty guards, and a thin route host | Immutable launch snapshot, explicit Workspace handoff, mode activity separation and close protection |

Extract a component together with the state that belongs exclusively to it.
Do not replace one oversized parent with a 60-property hook result and a
similar-sized prop list. Keep coordinated state together where splitting it
would create synchronization effects. Use reducers for meaningful transitions,
not as a mechanical conversion of every `useState`.

`ArtifactExplorer` defines exported manifest/content DTOs near the top of a UI
file. Move those into an artifact-domain client/types module so report and file
adapters can depend on the contract without depending on a view component.
Avoid moving them into an ever-growing global `lib/types.ts`.

Retain lazy imports for the research desk, Tasks/Missions, file editors and
PDF.js. Keep plain chat free of research-store fetches and heavy editor loads.
Compare emitted chunks and the existing route behavior after hook/component
extraction; a barrel that eagerly imports every tab would undo this design.

Acceptance: existing page, dialog, editor, file-workspace and settings tests;
queued saves and stale responses; deep-linked settings sections; profile
mutation races; manifest-first Sources; chat switching while streaming;
transcript paging; prepared-task scope. Use Tauri development UI smoke checks
for affected interactions, not the installed application.

### F. Remaining focused backend splits

| Module | Concrete extraction and sequencing | Acceptance |
|---|---|---|
| `release/exchange.rs` | Keep format DTOs in facade or `format.rs`; move `index`/`load_object`/`collect` to `collect.rs`, export/extract to `package.rs`, ID mapping and previews to `plan.rs`, transaction application to `import.rs`, conflict resolution to `conflicts.rs`; split tests | Same dependency closure and manifest content, deterministic remaps, keep-local conflicts, idempotence, entry/blob validation and no imported execution authority |
| `auto_review.rs` | `catalog.rs`, `contract.rs`, `routing.rs`, `materialize.rs`, `render.rs`; move tests by concern. Keep build-generated catalog includes and compiled prompt data at the catalog boundary | Full/Quick step identities, configured bounds, catalog revision, schemas, genre injection and ordered findings unchanged |
| `settings.rs` | Separate schema/default functions and `impl Default`, model-selection helpers, normalization/validation, persistence and the frozen-run guard. Preserve flat serde schema and current crypto module | Legacy field/access-mode migration, explicit backend selection, API-key preservation, corrupt-file handling, active-profile locking and run snapshot behavior |
| `extract/pdf.rs` | `pdf/baseline.rs`, `paddle_server.rs`, `full_parser.rs`, `llm.rs`, `cache.rs`, with a small dispatch facade; separate LLM range helpers further only if needed | Same completeness checks, figure-related exception, page ordering, fingerprints, timeout/cancellation, cache verification, no cross-method fallback |
| `commands/export.rs` | `export/package.rs`, `selection.rs`, `print_math.rs`, `print_html.rs`, `print_files.rs`; native command wrappers remain small | Package file manifest/hashes, destination validation, unchanged math sanitization and KaTeX resources, print cleanup |
| `runs.rs` | `writer.rs` and `source_capture.rs` for `RunWriter`, source evidence and existing-file registration; leave types/facade with existing persistence/history/retention/artifacts modules | Exclusive run IDs, interrupted-run manifests, Drop finalization, evidence hashes, compact page index, retention and old report loading |
| `bin/cli.rs` | Child modules under `bin/cli/` for parser/args, run/check/batch, workflow/profile/engine commands, output and signals; retain binary entry path | Same flags, help, unknown/duplicate argument rejection, exit codes, output collision checks, interruption and engine-uninstall semantics |
| `codex/probe.rs` | Split `run_qualification_probe` into named phases with one fixture/process guard; keep independent probe wire checks and external sandbox assertions | Every existing check still runs and contributes to the same report; cleanup on each phase failure, no model turn, real sandbox denial and subprocess reaping |
| `codex/supervisor.rs` | Tests first; `thread_contract.rs`, `side_turn.rs`, `projection.rs` next only where this improves reading. Reuse small shared parsing helpers as below | Same start/resume contract, projection replay, ephemeral side-turn cleanup, no detached processes and stale epoch handling |
| `projects/ledger.rs` | Tests first; later `occurrences.rs`, `matching.rs`, `storage.rs` if growth warrants it | Conservative matching, stable issue IDs, legacy finding/evidence parsing, user status preservation and bounds |
| `pipeline/structured.rs` | Tests first; optionally `projection.rs` for provider dialects and `validation.rs` for the host contract | Strict object-root validation, unsupported-keyword rejection, optional-null handling, CLI size limits and original-schema validation |

The three test-first files do not need all optional splits just to satisfy the
line budget. Record a cohesive sub-1,200-line implementation as complete unless
another concrete change needs a clearer internal seam.

## 6. Confirmed duplication and sharing opportunities

### 6.1 Complete existing Codex sharing, without sharing a supervisor

[agent_runtime/codex](../gui/src-tauri/src/agent_runtime/codex/mod.rs) already
owns wire types, transport, compatibility, account/model/quota parsing, process
primitives and a store-free `NativeSession`. Workspace re-exports shared account
types and delegates account calls. Workflow uses those primitives through its
own `codex_server/connection.rs`. Do not plan another wholesale transport merge.

There are smaller remaining copies: `validate_identifier` in
`agent_runtime/codex/account.rs:282` and `workbench/codex/supervisor.rs:796` is
identical. TOML string escaping is repeated in `workbench/codex/process.rs:6`
and `probe.rs:495`; launcher permission preparation is also repeated.
Move these stateless helpers to a narrowly named shared module and test the
edge cases once, while retaining product integration assertions.

Keep Workspace's inspect/edit/network profiles distinct from the Workflow
host-tools-only profile. Keep the probe's allow/deny assertions independent of
the production contract validator so qualification cannot simply confirm the
same implementation against itself. Consider reusing `NativeSession` launch
mechanics in the Workspace supervisor only after comparing shutdown, event
projection and side-turn lifetimes; it is a later opportunity, not a necessary
part of the size reduction.

### 6.2 Share process mechanics with explicit cancellation ownership

Similar capped drain loops occur in [process.rs](../gui/src-tauri/src/process.rs):32,
[extract/core.rs](../gui/src-tauri/src/pipeline/extract/core.rs):100, and
[research/execution.rs](../gui/src-tauri/src/workbench/research/execution.rs):387.
The last additionally streams job logs. Extraction and probe runners also
repeat output structures, stream threads and timeout/cleanup mechanics.

First extract a bounded byte collector/drainer, optionally with an observer
for job logs, and reuse the existing `process::configure_isolation` where its
contract matches. Preserve continued draining after the retained-byte cap,
truncation reporting, stream-error behavior and descendant-held-pipe handling.
Only then consider a common synchronous runner with explicit owner and
cancellation/watch hooks. Keep async provider streaming separate initially.

**Important current coupling:** `process::run_bounded` registers through
`crate::commands::register_child_pid`; it is not an ownership-neutral runner
despite its shared location. Workspace execution uses separate independent-PID
functions in `commands/lifecycle.rs`. Replacing Workspace execution with
`run_bounded` as-is would change cancellation ownership. A future neutral
runner must receive ownership explicitly; it must not silently register under
the Workflow's global cancellation path.

Acceptance: cap overflow drains to EOF, slow/held pipes finish within bounds,
child trees are reaped, Workflow Stop does not stop Workspace work, Workspace
Stop does not kill unrelated children, and Stata's wrapper can finish cleanup.

### 6.3 Separate reusable document mechanics from Workflow extraction policy

Workspace already calls `document_bundle::extract_docx_text`, and calls
`pipeline::extract::extract_pdftotext` and `render_pdf_page_preview` from
`research.rs` (lines 1,402 and 582). This is existing reuse with awkward
dependency direction, not duplicated extraction implementations.

After process ownership is made explicit, place deterministic text conversion
and page rendering behind a neutral document-service interface. Both owners
pass staged files, limits and process context. Keep Workflow's configured
Paddle/LLM method, cache, fallback prohibition and run assets on its side;
keep Workspace immutable revision capture, deterministic quality labels and
exact paper/source locators on its side. Retain compatibility re-exports from
the extraction facade during migration.

Use fixture-level parity for DOCX, pdftotext text and rendered page metadata,
plus a cross-mode cancellation isolation test. Do not route Workspace imports
through the full Workflow run preparer.

### 6.4 Consolidate call provenance and carefully narrow run preparation

`executor.rs:2503` constructs `StepOutput` and primary/fallback
`StepCallRecord`s. `pipeline/merge.rs:343` repeats the model, usage, effort,
duration and fallback-record mapping, with merge-specific roles. The clone
scan found multiple matching windows here.

Create `pipeline/provenance.rs` with record builders taking an explicit role,
provider resolution and measured usage. Keep role names (`step`, `merge`,
`usage_limit_primary`, `usage_limit_fallback_merge`, `failed_merge`) and the
policy for assigning them in the caller. Share usage subtraction/aggregation
only after testing attempt accounting and saturating arithmetic. Test zero
usage, fallback success/failure, failed merge, retries and retained first-unit
output so spend and effective-output provenance cannot diverge.

`commands/run.rs` and `commands/rerun.rs` also repeat scoped-source staging
around lines 458 and 292. Move verified common preparation into the existing
`run_context.rs`/`run_storage.rs` seams. Fresh runs, resumes and reruns retain
different input discovery, checkpoint and reuse policy; do not combine their
entire orchestration into a flag-heavy function.

### 6.5 Establish one Markdown core with product-specific presentation

Report rendering already reaches Workspace readers, and both modes use
`file-workspace/markdownComponents.tsx`, safe links and shared file navigation.
There is no need to create a second shared file viewer.

There is policy duplication: `ReportViewer.tsx:399` applies math repair,
validation, configured KaTeX options and an error fallback, while
`WorkspaceConversationView.tsx:68` directly builds a GFM/math/KaTeX pipeline.
Extract a memoized `MarkdownBody` that owns common plugin setup, safe links,
code/image components and math failure behavior. Keep report TOC, issue styling,
find bar and reading preferences in `ReportViewer`; keep message actions,
streaming and transcript layout in Workspace. Navigation/components can be
explicit inputs rather than a product-global context.

Sharing the report's repair behavior with chat can change rendered output.
First extract with the current policies represented explicitly; then harmonize
them in a separate, reviewed behavior change backed by malformed-LaTeX,
fenced-code, safe-link and streaming examples. Retain memoization so typing in
Find or receiving unrelated state does not reparse a long report.

### 6.6 Share account display, not account state

[WorkflowCodexConnection.tsx](../gui/src/components/WorkflowCodexConnection.tsx)
and [WorkspaceConnectionSettings.tsx](../gui/src/components/WorkspaceConnectionSettings.tsx)
repeat account/plan text, sign-in controls, busy/error presentation and quota
windows. Workflow polls status and exposes unresolved-attempt reconciliation;
Workspace listens for events and also owns model and title preferences.

Extract presentational `AccountSummary`, `LoginControls`, and `UsageWindows`
with mode labels and callbacks. Keep separate controller hooks and typed
clients. Do not implement a shared singleton account hook or merge caches.
Prefer app-owned neutral account/limit display types or adapter outputs over
having Workflow import Workspace UI DTOs.

Test both cards mounted together: refreshing, signing out or cancelling one
must call only its own client. Retain unavailable-versus-zero quota display,
login epoch binding, model-refresh behavior and Workflow recovery controls.

### 6.7 Reuse filesystem/hash primitives only after specifying their contracts

Hash/copy/path validation appears in research capture, exchange packages, run
artifacts and exports. The implementations differ in maximum size, whether
symlinks are rejected, short versus full SHA-256, atomic publication, existing
destination behavior, and whether size means metadata size or bytes observed.
They are related code, not interchangeable copies.

Build an explicit caller matrix before extraction. Prefer small helpers near
the existing `safety.rs` boundary: bounded streaming hashing over an already
validated handle; bounded copy-and-hash into caller-owned staging; relative
path syntax validation separate from canonical-root checks. Leave transaction,
replace/no-clobber decisions, permissions, hashes stored on disk, and error
classification with the owning service. Avoid a catch-all `utils.rs`.

First use two matching callers. Add grow-during-read, unexpected hash,
symlink/special-file, Unicode path and partial-publication fixtures. Do not
expand shared use until the behavior table matches every proposed caller.

### 6.8 Keep compatible DTOs and duplicated renderers in sync

Rust's specialist renderer in `auto_review.rs:904` has a TypeScript counterpart
in [issues.ts](../gui/src/lib/issues.ts):344. Findings/evidence representations
also appear in `models.rs`, the Workflow project ledger and the Workspace
finding bridge. Rust and TypeScript cannot literally share the implementation
without changing where rendering occurs.

First centralize canonical app-owned DTO ownership and test both renderers
against the same checked-in fixtures. Keep legacy readers and Workflow/Workspace
status records as adapters. Later, evaluate generating TypeScript DTOs from
app-owned Rust schemas only if recurring drift warrants the tooling cost.
Do not generate frontend bindings for the entire vendor Codex protocol bundle,
and do not introduce code generation as a prerequisite for ordinary file splits.

### Similar code that should stay distinct

- [diff.ts](../gui/src/lib/diff.ts) provides a bounded, potentially truncated
  report preview. [fileDiff.ts](../gui/src/lib/fileDiff.ts) preserves exact
  offsets/newlines and supports acceptance. Share a low-level diff algorithm
  only behind these separate contracts; never use preview operations to edit files.
- `.pwrx` restore is whole-store and empty-destination; `.pwex` selectively
  imports with conflicts. Share ZIP/path/hash mechanics only after specification,
  not import policy or transactions.
- Workspace recipes/harnesses, Workflow dependency graphs, and durable task
  chains have different control-flow and persistence semantics. Keep the
  existing coordinator and owning adapters; do not replace them with one engine.
- Workflow finding status, Workspace research evidence, and human acceptance
  are different provenance classes. A common finding DTO does not imply a
  shared mutable ledger.
- Legacy Codex support remains a compatibility path. Its removal requires a
  product/support decision; file size alone does not justify deletion. The
  deprecated Antigravity CLI was removed in the September 9 review follow-up;
  Google remains API-only, with legacy saved provider fields preserved.

## 7. Reviewable implementation sequence

The user requested one PR at the end for all current and future work in this
section. Implement and validate these stages in order within that PR. Keep
mechanical moves, formatting and new abstractions distinguishable in the
implementation record; “mechanical” means symbols move without changing behavior.

| Stage | Scope | Dependencies | Size / risk | Exit condition |
|---|---|---|---|---|
| R0 | Refresh inventory, preserve/coordinate current feature work, record relevant test baseline, add reporting-only size check | None | Small / low | Stable implementation baseline and explicit exceptions; no mass reformat |
| R1 | Format the dense target UI files in focused changes; split inline and oversized standalone tests by behavior | R0 | Medium / low | No test lost, filtered test commands still find intended cases, visible formatted size |
| R2 | ArtifactExplorer types/client, bundle/page/tree modules using existing file workspace | R1 | Medium / medium | Manifest-first and scoped read tests; focused UI smoke |
| R3 | Settings draft/catalog and section extraction; Pipeline editor state/profile extraction as separate validated steps | R1 | Large / medium | Save and mutation races, dirty guards and profile operations preserved |
| R4 | Executor pure helpers, prompts, fan-out/lineage, checkpoints and provenance | R1 | Large / high | Existing execution products and focused tests unchanged |
| R5 | Executor async run/parallel/sequential boundaries | R4 | Large / high | Preview/runtime parity, resume/failure/cancellation and accounting gates |
| R6 | Direct API module separation; Claude-named dispatcher/request/CLI separation | R1; consume R4 provenance when available | Large / high | Provider and tool capability matrix unchanged; no bypass of `call.rs` |
| R7 | Workspace command worker/turn/event owners, then grouped wrappers | R1 | Large / high | One gate/permit owner, IPC shape and task/mission regressions pass |
| R8 | Workspace store and research service split in successive steps; execution module afterward | R7 | Large / high | Unchanged SQL/DTO/fingerprint behavior; recovery and receipts pass |
| R9 | WorkspacePage and TasksPage controller/component extraction, then App launch/navigation | R1; align interfaces with R7–R8 | Large / medium | Bounded lazy chat/task behavior and development UI flows preserved |
| R10 | Small Codex, bounded-I/O, account-display and Markdown-core sharing changes, each separately | Owning structural splits | Medium each / medium–high | Both product suites pass; any rendering change reviewed separately |
| R11 | Neutral deterministic document services and broader process reuse, only after explicit ownership | R6, R8, process work in R10 | Medium–large / high | Cross-mode cancellation, extraction parity and scope checks pass |
| R12 | Remaining P2 modules: exchange, auto review, settings, PDF, export, writer, CLI | Relevant owning stage | Medium each / medium–high | Table in section 5F satisfied; no new oversized destination |
| R13 | Update developer maps, remove temporary re-exports where safe, enforce growth policy | All selected stages | Small / low | Agents have a short map; completed modules stay within budget |

The labels describe relative engineering effort, not elapsed-time commitments.
R2–R3 and R4–R8 are separate frontend/backend tracks, but changes touching the
same broad file should be serialized to avoid extraction conflicts. The first
milestone is R0–R4: an honest formatted inventory, smaller tests, three major
UI surfaces, and the executor's pure boundaries. Full runtime sharing is a
later milestone because it has higher behavioral risk.

For every PR, specify the owning module, unchanged externally visible
contracts, tests run, and any intentionally deferred behavior changes. Keep a
mechanical move independently revertible. Avoid persisted schema changes so
rollback requires no data conversion.

## 8. Validation plan

Before a refactoring branch starts, establish the relevant baseline from the
current worktree. Existing documentation reports older successful suites, but
those records do not establish this branch's present state. Preserve unrelated
changes; use an isolated checkout/worktree from an agreed snapshot if necessary.
On this machine all Git commands must run through an escalated login zsh as
specified by `AGENTS.md`.

### Per-change checks

- Mechanical Rust moves: compile all targets, run the affected module tests,
  and check Tauri command registration if wrappers moved. Private-module test
  paths may change; update exact-name child-process invocations and scripts.
- Stateful Rust changes: run the owning integration/recovery tests, including
  task and mission adapters when Workspace or Workflow lifecycle changes.
- Frontend extraction: targeted Vitest component/client suites and
  `npm run build`; inspect relevant route chunks for lazy-loading changes.
- Shared process/Codex/file/renderer changes: both Workspace and Workflow
  regression suites, not just the caller used to motivate the extraction.
- Native UI checks: launch `cd gui && npm run tauri dev`, use isolated test
  data, and exercise affected user flows. Never open `/Applications/Pipeline`
  for this repository's computer-use checks.

Do not add tests that merely assert a file moved or mirror a new helper's
implementation. Preserve current regression coverage and add meaningful tests
for behavior at the newly exposed seam. Capture before/after serialized
products where compatibility is the contract; avoid brittle snapshots of
irrelevant DOM or formatting.

### Integration gate for a completed refactoring milestone

From `gui/`, use the pinned Node environment in `.nvmrc` and run:

```sh
npm test
npm run build
```

From `gui/src-tauri/`, use the checked-in Rust toolchain and run:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

If baseline formatting or tests fail on unrelated in-progress work, report the
exact baseline failure and validate the refactoring scope separately; do not
reformat unrelated files or call the full gate green. If shared protocol,
launch, permission or process behavior changes, also run the two development
no-model probes (`workbench_probe`, `workflow_codex_probe`) with the required
native sandbox access. Keep the independent-process cancellation test isolated
as its existing test harness requires.

Use [release qualification](workbench/release-qualification.md) and
[compatibility policy](workbench/protocol/compatibility-policy.md) for affected
live gates. A deterministic split needs no new release claim. If a refactor
changes runtime mechanics, no-model probes still do not qualify authenticated
tools, OAuth, packaged applications or other platforms. Preserve that distinction.

## 9. Preventing the next large-file cycle

Add a small reporting tool in `scripts/` that enumerates owned source files
with `rg`, records lines and UTF-8 bytes, and separately labels tests,
generated/vendor data and documents. Pass in an explicit Git-derived manifest
when needed rather than hiding machine-specific Git invocation in the tool.
Keep a reasoned exception list with an owner/topic and review trigger.

Initially report growth against the R0 baseline. Flag newly over-budget files
and further growth of existing oversized files; do not block every unrelated
change because legacy files remain. Adopt a hard new-file rule only after the
policy has useful exceptions. Do not add push-triggered CI as part of this
work: repository Actions are intentionally manual-only. Make the report a local
command and, if desired, part of the existing manual checks.

For each refactored feature, add a short module map explaining entry points,
state ownership, dependencies and focused tests. Keep `CLAUDE.md` as the
repository-wide navigation/invariant source, with detailed workflow execution,
provider, export and extraction mechanics in linked topic documents. Existing
Workspace topic guides should be updated instead of duplicated. The audit's
module map should describe the final architecture, not retain temporary moves.

Completion criteria:

1. Every inventoried source file is within the agreed reading budget or has a
   specific, reviewed exception. Formatted dense files are assessed by bytes
   and responsibility as well as line count.
2. No extraction merely relocates the oversized file into a new `mod.rs`,
   giant hook, global types file or large undifferentiated test module.
3. The main executor, Workspace commands/research/store, and large UI parents
   expose recognizable ownership boundaries with narrow interfaces.
4. Implemented sharing opportunities have at least two real consumers and
   preserve their differences in authority, persistence and lifecycle.
5. Existing serialized products, commands, paths, fingerprints and migrations
   remain compatible; any deliberate behavior change has separate evidence.
6. Relevant tests/builds pass, native checks are recorded where applicable,
   and an agent can locate a feature, its implementation and its tests from
   one short map.
