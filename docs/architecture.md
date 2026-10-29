# Architecture

This page is for contributors. It explains how Pipeline fits together and which
rules hold across modules. Each module's header comment carries its own
details.

## What Pipeline is made of

Pipeline is a Tauri desktop app with three parts:

- **Rust backend** in `gui/src-tauri/src/`. It owns processes, files,
  credentials, and saved data. The crate builds the `pipeline_gui_lib` library
  and two binaries, `pipeline-gui` and `pipeline-cli`.
- **React and TypeScript frontend** in `gui/src/`. It owns presentation and
  interactive state. It calls Tauri commands and listens for events, mostly
  through typed clients in `gui/src/lib/`. Put new calls in the client for their
  feature.
- **Headless CLI** in `gui/src-tauri/src/bin/cli.rs`. It runs the review
  engine without a window. See the [CLI guide](cli.md).

`lib.rs` sets up Tauri and registers every command. Engine code reports
progress through the `EventBus` in `emit.rs`, so it runs the same way under the
window and the CLI. The CLI's modules live in `cli/` and `cli_io.rs`. The Tauri
bundler treats each folder under `src/bin/` as another binary, so `bin/cli.rs`
pulls them in with `#[path]`.

## Names in the app and in the code

The code uses implementation names that differ from the app's. Don't rename
commands, stored fields, or tables to match the app.

| In the app | In the code |
| --- | --- |
| Project | A workspace (`Workspace`, `workspace_id`) in the `workbench` store; files and records in `workbench/project/` |
| Conversation | A session (`sessions` table), whose turns run on a native thread recorded in `session_bindings` |
| Review | A run of a workflow, also called a profile or pipeline (`pipeline_config/`) |
| Automation | A task chain (`orchestration/`); a research automation is a mission, and research discovery is self-discovery |
| Assistant instructions | The harness: presets, modules, and tools resolved for each turn (`presets` table); **Assistant settings** calls a preset an agent profile |

Code and comments also call the conversation subsystem Workspace and the
review subsystem Workflows. Use the app's nouns in UI text and user guides.

## Three subsystems and what they share

Each subsystem owns its store, processes, and cancellation.

- **Conversations** (`workbench/`): persistent ChatGPT or Claude Code
  conversations with optional research tools.
- **Reviews** (`pipeline/`, `commands/`, `runs/`, `pipeline_config/`): saved
  dependency graphs of isolated model calls.
- **Automations** (`orchestration/`): delayed, recurring, or multi-step work
  that calls the other two through adapters and copies their results.

They share the Tauri shell, stateless utilities such as `safety.rs`, the Codex
protocol code in `agent_runtime/codex/`, and the Claude subscription check in
`agent_runtime/claude.rs`. One ChatGPT account in
`agent_runtime/codex/chatgpt/` gives each runtime in-memory authentication.

They never share API keys, stores, run state, process registries,
cancellation, or writable artifact roots. Results cross between subsystems
only as copies.

- Automations go through the owning adapter: `commands/orchestration.rs` for
  reviews, and `workbench/tasks.rs`, `workbench/missions.rs`, or
  `workbench/discovery.rs` for conversations.
- Keep recipes, harness modules, workflows, and task chains separate.
- Avoid `utils.rs` files, service locators, and shared runners with many flags.

## Where code lives

The repository root holds `gui/` (the React app in `gui/src/`, the Rust crate
and CLI in `gui/src-tauri/`), `docs/`, `scripts/` for checks and release
tooling, and `.github/` for CI and community files. Inside the crate,
`prompts/` and `model-policy.json` are compiled into the app, `codex-protocol/`
holds the pinned schema reference, and `tests/fixtures/` holds test inputs.

Paths are relative to `gui/src-tauri/src/` unless they start with `gui/`.
Each module's header comment has the details.

| Path | Owns |
| --- | --- |
| `commands/` | Review commands: launch, rerun, queue, export, settings |
| `pipeline/executor/` | Workflow scheduling, step calls, fan-out, checkpoints |
| `pipeline/call.rs` | The supervised model-call boundary |
| `pipeline/api_*.rs`, `pipeline/claude/` | Direct API and CLI provider adapters |
| `pipeline/codex_server/` | Review connection to the Codex app server |
| `pipeline/extract/`, `document_bundle.rs` | Extraction and the `DocumentBundle` format |
| `auto_review/` | Specialist catalog and role selection |
| `pipeline_config/` | Built-in workflows, validation, migrations |
| `runs/`, `output/` | Review run store, recovery, retention, report formats |
| `settings/`, `storage.rs` | Device settings and research folder choice |
| `agent_runtime/codex/` | Shared Codex transport, launch, and version checks |
| `agent_runtime/codex/chatgpt/` | The shared ChatGPT account |
| `agent_runtime/claude.rs` | Claude subscription check |
| `workbench/commands.rs` | Conversation commands, database gate, turn permit |
| `workbench/store/`, `workbench/migrations/` | Private SQLite store and migrations |
| `workbench/codex/` | Conversation app server supervisor and probe |
| `workbench/claude/` | Claude Code conversations, WSL2 transport, and the MCP bridge |
| `workbench/capabilities.rs`, `conversation_tools.rs`, `web.rs`, `browser/` | Shared conversation access, receipts, public retrieval, and the optional browser |
| `workbench/research/` | Harness, sources, notes, evidence, tools, local jobs |
| `workbench/project/` | Project files, working copies, edit acceptance |
| `workbench/release/` | Backups, project packages, retention, recipes |
| `workbench/desk/`, `workbench/data/`, `workbench/programs/` | Research desk, datasets, research programs |
| `orchestration/` | Automations coordinator, missions, discovery |
| `engines/`, `deps/` | Managed local engines and dependency checks |
| `gui/src/lib/` | Typed IPC clients and pure helpers |
| `gui/src/hooks/workspace/` | Conversation runtime in the UI |
| `gui/src/components/pipeline-editor/` | **Review designer** |
| `gui/src/ui/` | Shared controls; design tokens are in `App.css` |

Keep modules between about 300 and 800 lines.
`node scripts/source-size-report.mjs --check` fails when a file grows past
the budget.

## How a review runs

1. **Ingest.** The launch reads the input as `document`, `latex_project`,
   `source_tree`, `batch`, or `none`. Extraction uses the configured method
   and never switches to another.
2. **Normalize.** The input becomes a `DocumentBundle`.
3. **Orient.** One required model call returns schema-checked orientation.
   Automatic paper review also picks specialists here.
4. **Execute.** Ready parallel steps run up to `max_workers` at a time.
   Sequential steps run alone. Every finished step is checkpointed.
5. **Persist.** The report, manifest, responses, and artifacts go to the run's
   folder under `runs/`.

PDF extraction methods are `llm`, `paddleocr-vl-full` (the managed PaddleOCR-VL
Full Parser), and `pdftotext`. The `auto` setting picks the Full Parser when it
is installed and `llm` otherwise. LaTeX projects resolve `\input` and
`\include` and use a compiled PDF as a visual companion. DOCX files are read as
Office Open XML.

**DocumentBundle.** `document_bundle.rs` defines the format every extractor
produces: origins, pages, ordered semantic nodes, run-relative assets,
alternate representations such as LaTeX or OMML, provenance, and quality
warnings. `context/document.md` is a readable projection, not the source of
truth. The schema version is `1.0`, and validation accepts only that value.
Add optional fields with Serde defaults and keep the version. A breaking change
needs a new major version and a migrator. New extractors produce the same
bundle, so later steps never branch on the engine.

**Specialist catalog.** Automatic paper review picks roles from
`gui/src-tauri/prompts/auto_review/catalog/`. `build.rs` checks each manifest
against `specialist.schema.json` and generates the Rust catalog, so a malformed
role fails the build. The model returns only IDs. Pipeline supplies each role's
prompt, tools, and file access. See the
[catalog README](../gui/src-tauri/prompts/auto_review/catalog/README.md).

**Model calls.** Every provider call goes through
[`pipeline/call.rs`](../gui/src-tauri/src/pipeline/call.rs). Provider modules
must not bypass it. `pipeline/response_journal.rs` saves each response before
schema checks. A rejected response stays as evidence and never becomes context
for a later step. Each step reads only its selected roots and writes only to
its own folder.

## How a conversation runs

**Codex app server.** Conversations and reviews each start their own
`codex app-server` process with an isolated `CODEX_HOME`.
`agent_runtime/codex/compatibility.rs` sets the version rules:

- Reject malformed version output and builds older than 0.147.0. Admit newer
  builds.
- Treat the checksum-pinned schemas in `gui/src-tauri/codex-protocol/0.147.0/` as
  the field reference. Never edit them.
- After every thread start or resume, check the returned provider, working
  folder, roots, permission profile, and approval policy. Reject a substituted
  model when a specific one was requested.
- On a failed check, stop with an error. Never weaken permissions or fall back
  to `codex exec`.

**Claude Code.** `workbench/claude/` runs each turn as `claude -p` with
stream-json. Before each turn, `agent_runtime/claude.rs` requires a
first-party subscription sign-in. It strips inherited `ANTHROPIC_*` and
`CLAUDE_*` variables and never falls back to an API key. Pipeline supplies up
to 128 KiB of recent history from its own transcript store.

**Conversation tools.** Both providers resolve the same file, web, and browser
capabilities. Ordinary Edit conversations use the project acceptance journal;
shell writes require a task copy. Codex thread-local configuration travels on
start and resume. Claude receives sandbox settings and an allowlisted native
tool catalog. Windows Claude uses a separately authenticated WSL2 environment
when qualified dependencies are present. Workspace owns its browser processes
and per-conversation profiles independently of Review runtimes.

File and web tools journal admission before acting, compare the captured
configuration with current access, and never repeat an ambiguous call. Network
waits release database workers. Public retrieval validates and pins DNS at each
redirect. Browser page requests use the same public-address policy. Account
services retain identity-scoped quota observations with individual reset times;
observations are not fresh account balances.

**One active turn.** One turn runs at a time across the app, for either
provider. The permit in `workbench/commands.rs` spans setup, submission, and
the matching terminal event. Only an event with the same connection epoch,
thread, and turn releases it. Drafts are saved before sending. Pipeline never
retries an ambiguous send.

**Database gate.** Every production store operation goes through the helpers
in `workbench/commands.rs`. Ordinary work shares a read gate and four blocking
workers. Whole-store export and restore take the gate exclusively. Process
waits hold neither a worker nor the gate.

**Journal and projection.** Turn and item events are projected into the store
by `workbench/store/projection.rs`. Each gets a deterministic operation ID in
`change_log`, so a replayed event changes nothing.

**Recovery.** The event stream isn't the source of truth. On lag or resume,
the supervisor reconciles from the app server's history with
`thread/turns/list`, one turn per page. A heartbeat finds a terminal event the
stream lost.

**Approvals.** A pending request is keyed by connection epoch, request ID, and
method. The backend claims it before answering, so a duplicate click can't
replay it, and an old connection can't approve on a new one.

**Harness.** Each turn saves a snapshot of the resolved harness. A changed
fingerprint starts a new native binding. Bump `TOOL_CATALOG_VERSION` in
`workbench/research.rs` when you change a dynamic tool. Tools record a running
receipt before any side effect. Model-created notes and evidence stay
proposals until the researcher accepts them.

Plain chat must not load research stores or heavy editors. Streams reach the
UI in 50 ms batches, and transcripts render 200 messages at a time.

## Crash and recovery

Each boundary saves state before acting. After a crash, Pipeline shows an
uncertain outcome instead of repeating work.

| Boundary | Saved first | After a crash |
| --- | --- | --- |
| Conversation send | Draft and submission ID | No automatic resend |
| Approval | Pending request with its epoch | An old request can't approve on a new connection |
| Tool call | Running receipt | `outcome_unknown`; never reruns |
| Local job | Record with the process instance | `outcome_unknown`; reconciling re-hashes outputs first |
| Output adoption | Content-addressed staging | Unreferenced staging is disposable |
| Store migration | Online backup in the store's `backups/` folder | Rollback keeps the old schema |
| Review model call | Response journal; attempt receipt for Codex | No repeat; open receipts block new Codex calls until reviewed |
| Review step | Checkpoint | Startup turns checkpoints into a partial report that a rerun reuses |
| Automation action | Action journal | Marked uncertain; the researcher reconciles or retries |

## Automations coordinator

`orchestration/` owns task definitions, a pure interpreter, the scheduler, and
a SQLite store in the research folder. `owner.lock` lets one process own it.
The store uses WAL, optimistic revisions, unique schedule occurrences, and
append-only events.

- At most four leaf actions run at once, within the app-wide limits of one
  conversation turn and one review run.
- `orchestration/adapters/execute.rs` is the one cancellation and publication
  envelope.
- A response is written to its action journal before the coordinator advances.
- Settings are pinned at preparation. Review credentials are resolved only
  inside the review adapter.
- A task keeps its review pinned until it adopts the result
  (`commands/orchestration/pins.rs`).
- Before an automated turn, the conversation adapter checks the project,
  harness, root, and unsent draft. Any change needs attention.

Research automations (`orchestration/missions/`) add planning and assessment
on the same coordinator. Their completion is an assessment that never accepts
edits or widens permissions. Research discovery (`orchestration/discovery/`)
uses the same adapters. Its conversation roles in `workbench/discovery.rs` run
without network access and never ask for approval.

## Data on disk

```text
~/.pipeline/               always on this device
  settings.json, keyfile   device settings; encrypted API keys
  storage.json             research folder choice
  updates.json             update-check state
  providers/chatgpt/       shared ChatGPT account
  providers/workflows/     review Codex home and receipts
  workbench/codex/         conversation Codex home
  native/, cache/          managed engines and caches

<research folder>/         ~/.pipeline/ unless storage.json names another
  workbench/               conversation store and blobs
  runs/                    review runs
  profiles/, prompts/      saved workflows and prompt overrides
  projects/                review run groups
  orchestration/           automations store
```

`storage.json` has one field, `directory`: an absolute path, or `null` for the
default. `storage.rs` fixes the location for the life of the process, so a new
choice applies after restart. A missing folder is an error, never a fallback to
the default. The CLI uses the same root. Credentials and engines stay on the
device when the folder moves.

Migrations are append-only. Never edit an applied file in
`workbench/migrations/`. Add the next numbered one. The store refuses a schema
newer than the build.

## File formats

**Workflow JSON** has `"type": "profile"` and `schema_version` 11. Readers
accept versions 1 to 11 and reject newer ones. `pipeline-cli workflow schema`
prints the schema. See the [CLI guide](cli.md).

**`.pwrx` research backup** is a zip, format version 2. It holds an online
SQLite backup of the conversation store, blobs, conversation working files, and
transcripts. It excludes external project folders, review and automation
stores, Trash, the Codex home, and credentials. Restore requires an empty store
and a remap-or-detach choice for every root. Older archive databases migrate on
an extracted copy first. Native bindings are retired, and imported tools never
run. Limits are 2 GiB, 20,000 entries, and 512 MiB per entry.

**`.pwex` project package** is a zip, format version 1, with `manifest.json`,
`README.md`, `objects/<kind>/<id>.json`, and `blobs/`. It imports into a
nonempty store:

- An unknown ID is inserted as is.
- An ID in the target project is compared by content fingerprint. A different
  object becomes a conflict, and the local version stays.
- An ID in another project is remapped deterministically from the source
  namespace, so reimporting is idempotent.
- Child IDs follow their owner. Blobs resolve by content hash.

Packages are limited to 5,000 objects, 2 GiB expanded, 256 MiB per entry,
64 MiB per blob, and 20,000 entries. They never include conversations,
credentials, execution settings, or declared inputs. The two formats share zip
mechanics in `workbench/release/archive.rs`, never import policy.

**`.pwrc` replication capsule** is a zip with a `research-capsule-v1`
manifest and up to 16 command plans. Import stays inert until the recipient
binds each plan to a local toolchain. Limits are 64 MiB in total and 8 MiB per
entry.

**Results files** use `research-results-v1` or `research-results-v2`. See the
[exporters README](examples/research-exporters/README.md).

## Testing

[Contributing](../.github/CONTRIBUTING.md) lists the standard checks. Only the
tests marked "uses quota" below make a signed-in model turn.
[Releasing](releasing.md) covers packaged builds.

**Fixtures.** `gui/src-tauri/tests/fixtures/` holds TeX, Stata, and Python
sources for real-tool tests, and `research-evaluation-v1.json` there defines
scored evaluation tasks. Harness snapshots are in
`workbench/research/fixtures/`.

**Browser fixtures.** Run `npm run dev` in `gui/` and open
`/e2e/app-full.html`. It renders the app over synthetic data. Add
`?project=rich`, `?project=thin` (conversations only), or `?project=bare`
(folder only).

**Disposable store.** In debug builds, set `PIPELINE_WORKBENCH_DEV_ROOT` to an
absolute path before `npm run tauri dev`. Release builds ignore it.

**No-model probe.** From `gui/src-tauri/`, run
`cargo run --locked --example workbench_probe`. It needs a compatible Codex
CLI but no sign-in. It checks the handshake, permission profiles, thread
lifecycle, sandbox rules, and process cleanup without a model turn. The
`workflow_codex_probe` example checks the review connection. Run both after
changing Codex transport, sandbox, or process code. Set
`PIPELINE_PROBE_NETWORK=1` for the optional public-domain, denial, and proxy-bypass
checks. Browser transport tests run with
`node --test gui/src-tauri/src/workbench/browser/public-request.test.mjs`.

**Opt-in tests** are marked `#[ignore]`. Run one with
`cargo test --locked --lib <name> -- --ignored`.

| Test | Needs |
| --- | --- |
| `live_supervisor_starts_in_an_isolated_temporary_home_and_shuts_down_cleanly` | Codex CLI |
| `claude_subscription_handshake_probe` | Claude Code sign-in |
| `claude_subscription_model_probe` | Claude Code sign-in; uses quota |
| `live_studio_build_and_synctex` | TeX |
| `live_oldstata_and_cooperative_cancellation` | The `oldstata` wrapper |
| `qualified_scientific_figures_and_tex_table` | TeX, `PIPELINE_PROGRAM_PYTHON`, `PIPELINE_PROGRAM_QA_DIR` |

Other opt-in tests need Git, Python, or network access. Search for
`#[ignore =` to find them. Each reason names what the test needs.

**Codex schemas.** To review a new Codex release, generate its schemas and
compare them with the pinned bundles:

```bash
codex app-server generate-json-schema --out <stable-dir>
codex app-server generate-json-schema --experimental --out <experimental-dir>
```

Moving the reference to a newer generation is a separate change. Update the
bundles, `SCHEMA_REFERENCE_CODEX_VERSION`, and the checksums in
`agent_runtime/codex/compatibility.rs` together.
