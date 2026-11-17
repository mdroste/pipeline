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
| Conversation | A session (`sessions` table), linked to a native or app-owned thread through `session_bindings` |
| Review | A run of a workflow, also called a profile or pipeline (`pipeline_config/`) |
| Automation | A task chain (`orchestration/`); a research automation is a mission, and research discovery is self-discovery |
| Assistant instructions | The harness: presets, modules, and tools resolved for each turn (`presets` table); **Assistant settings** calls a preset an agent profile |

Code and comments also call the conversation subsystem Workspace and the
review subsystem Workflows. Use the app's nouns in UI text and user guides.

## Three subsystems and what they share

Each subsystem owns its store, processes, and cancellation.

- **Conversations** (`workbench/`): persistent subscription or API
  conversations with optional research tools.
- **Reviews** (`pipeline/`, `commands/`, `runs/`, `pipeline_config/`): saved
  dependency graphs of isolated model calls.
- **Automations** (`orchestration/`): delayed, recurring, or multi-step work
  that calls the other two through adapters and copies their results.

They share the Tauri shell, stateless utilities such as `safety.rs` and public HTTP retrieval in `public_web.rs`, the Codex
protocol code in `agent_runtime/codex/`, and the Claude subscription check in
`agent_runtime/claude.rs`. One ChatGPT account in
`agent_runtime/codex/chatgpt/` gives each runtime in-memory authentication.

Device connection settings supply independently resolved API credentials to
each runtime. Conversations never read Review run snapshots. The runtimes never
share stores, run state, process registries, cancellation, or writable artifact roots. Results cross between subsystems
only as copies.

- Automations go through the owning adapter: `commands/orchestration.rs` for
  reviews, and `workbench/tasks.rs`, `workbench/missions.rs`, or
  `workbench/discovery.rs` for conversations.
- Keep recipes, harness modules, workflows, and task chains separate.
- Avoid `utils.rs` files, service locators, and shared runners with many flags.

## Connections and provider code

`connections/` translates saved choices into a connection and model. Reviews
supply the settings captured at run start. Conversations supply current device
settings at the start of a turn. The shared code does not read run state.

`model_catalog/` discovers models through shared API clients, independent CLI
probes, or the app's ChatGPT account. Catalogs remain tied to their connection.
Model information distinguishes supported, unsupported, and unknown options;
it does not grant access to files or the internet.

`agent_runtime/api/` owns HTTP and provider message formats. Each caller keeps
its own request limits, tools, retry decisions, logs, and cancellation. Reviews
continue through `pipeline/call.rs`; Conversations keep their turn journal.
Automations invoke those same runtime entry points.

`gui/connections.json` supplies connection names and saved IDs to both Rust
and TypeScript. Existing settings, workflow fields, and IPC names retain their
meaning.

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
| `commands/` | Review commands for launch, rerun, queue, and export |
| `commands/run_context/`, `commands/review_setup.rs` | Immutable launch snapshots, input checks, and execution previews |
| `commands/settings.rs`, `commands/profiles.rs`, `commands/maintenance.rs` | Renderer settings, workflow profiles, and app maintenance |
| `pipeline/executor/` | Workflow scheduling, step calls, fan-out, checkpoints |
| `pipeline/call.rs` | The supervised model-call boundary |
| `pipeline/providers.rs` | Review provider dispatch |
| `pipeline/api_*.rs`, `pipeline/api_common/`, `pipeline/claude/` | Review API tools, retries, and CLI integration |
| `connections/`, `model_catalog/` | Connection identity, model selection, discovery, and thinking options |
| `agent_runtime/api/` | API request and response formats, streaming, HTTP clients, and tool-result encoding |
| `agent_runtime/events.rs`, `connections/models.rs` | App event and model records |
| `agent_runtime/process.rs`, `agent_runtime/io.rs` | Process construction and limited output reads |
| `gui/connections.json` | Connection names and saved IDs shared by Rust and TypeScript |
| `pipeline/api_common/web.rs` | Review host search tools, per-call budgets, and web execution deadlines |
| `public_web.rs`, `public_web/` | Stateless public HTTP retrieval and search parsing shared by both runtimes |
| `pipeline/codex_server/` | Review connection to the Codex app server |
| `pipeline/extract/`, `document_bundle.rs` | Extraction and the `DocumentBundle` format |
| `auto_review/`, `auto_review/registry/` | Reviewer modules, local candidate retrieval, and role selection |
| `pipeline/orient/` | Document inventory, review planning, and batched discovery |
| `pipeline_config/` | Built-in workflows, validation, migrations |
| `runs/`, `models/` | Review run storage, recovery, retention, and saved records |
| `findings/`, `output/` | Published findings, report artifact contracts, and rendering |
| `app_menu.rs` | The macOS menu bar and its sidebar item |
| `settings/`, `storage.rs` | Device settings and research folder choice |
| `updates.rs`, `updates/` | Update checks, their preference, and in-app install |
| `agent_runtime/codex/` | Shared Codex transport, launch, and version checks |
| `agent_runtime/codex/chatgpt/` | The shared ChatGPT account |
| `agent_runtime/claude.rs` | Claude subscription check |
| `workbench/commands/` | Workspace handlers, the shared store worker gate, and the active turn permit |
| `workbench/store/`, `workbench/migrations/` | Private SQLite store and migrations |
| `workbench/codex/` | Conversation app server supervisor and probe |
| `workbench/api/` | Conversation API turns, tool execution, history, and recovery |
| `workbench/claude/` | Claude Code conversations, WSL2 transport, and the MCP bridge |
| `workbench/capabilities.rs`, `conversation_tools.rs`, `web.rs` | Conversation access, receipts, and web transport error mapping |
| `workbench/shell.rs`, `shell/`, `shell_policy.rs` | Native API commands and proxy; separate stateless sandbox policy for Claude Code |
| `workbench/research/` | Harness, sources, notes, evidence, tools, local jobs |
| `workbench/project/` | Project files, working copies, edit acceptance, file browsing and viewer formats |
| `workbench/project/version_control/` | Researcher-directed Git: changes, commits, branches, sync, and conflicts. Reads and writes stay here; no assistant or automation reaches it |
| `file_stream.rs` | Token-gated `pipeline-file` delivery to viewers: range reads, headers, and the token registry. Each owner validates paths and registers its own source |
| `workbench/release/` | Backups, project packages, retention, recipes |
| `workbench/desk/`, `workbench/data/`, `workbench/programs/` | Research desk, datasets, research programs |
| `orchestration/` | Automations coordinator, missions, discovery |
| `engines/`, `deps/` | Managed local engines and dependency checks |
| `gui/src/lib/` | Feature clients, transport records, and pure helpers |
| `gui/src/lib/project-overview/` | Project source selection, changes since the last visit, and related work |
| `gui/src/hooks/workspace/` | Conversation runtime in the UI |
| `gui/src/components/NavRail.tsx`, `nav-rail/` | Primary sidebar. Global destinations and the current project have separate sections. WorkspacePage supplies project navigation through a slot and owns the save and busy checks |
| `gui/src/components/pipeline-editor/` | **Review designer** |
| `gui/src/ui/` | Shared controls; design tokens are in `App.css` |

Keep modules between about 300 and 800 lines.
`node scripts/source-size-report.mjs --check` fails when a file grows past
the budget. The Paddle parser sidecar has a size exception because the managed
engine embeds, verifies, and refreshes it as a single executable script.

## How a review runs

1. **Ingest.** The launch reads the input as `document`, `latex_project`,
   `source_tree`, `batch`, or `none`. Extraction uses the configured method
   and never switches to another.
2. **Prepare.** Capture the document and declared supporting inputs. Persist
   the `DocumentBundle` before review planning.
3. **Plan.** Automatic reviews prefetch routing cards from the local reviewer
   registry. A provider-neutral structured tool loop can batch catalog and
   evidence requests before returning a validated plan. Custom reviews retain
   their configured document-overview prompt and schema.
4. **Execute.** Ready parallel steps run up to `max_workers` at a time.
   Sequential steps run alone. Every finished step is checkpointed.
5. **Persist.** The report, manifest, responses, and artifacts go to the run's
   folder under `runs/`.

A new review records its expanded step instructions and selected input
identities under `context/execution-contract.json`. Step checkpoints also
record digests of the accepted output and producer files. Resume checks these
records before reusing a step. Changed instructions, inputs, or products
invalidate the step and its graph dependents. Runs that predate the records
can resume from their captured document. The run store verifies the images
it restores and records missing visual evidence in the child bundle.

Every provider submission checks the review's call and elapsed-time limits.
Direct API submissions reserve estimated list-price spend and settle it when
the provider reports usage. Report publication checks explicitly declared
products and attributes a missing result to its producer.

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

**Reviewer registry.** Automatic reviews discover roles from
`gui/src-tauri/prompts/auto_review/catalog/`. `build.rs` validates each manifest
and generates the bundled modules. `auto_review/registry/` returns routing
cards through local prefetching and batched `SearchReviewers` and
`GetReviewers` operations. Full reviewer prompts are loaded only after
selection. The model chooses among supplied IDs; the host supplies executable
steps, tools, and file access. See the
[catalog README](../gui/src-tauri/prompts/auto_review/catalog/README.md).

**Preparation and planning.** `pipeline/orient/preparation.rs` projects
observed structure into the compatibility overview. Stock inventories stay
out of the planning response; edited inventory schemas remain model-owned.
`pipeline/orient/planning.rs` admits at most two catalog/evidence batches and
two repair attempts through the normal supervised call boundary. This
structured protocol works across providers without changing their native
tool surfaces. A final plan must satisfy the saved contract, reference known
document locations, and select only supplied routing cards. An invalid final
response cannot start reviewers.

The run saves `document_preparation.json`, `review_plan.json`, and
`reviewer_registry.json` under `context/`, alongside the compatibility
`orientation.json`. The registry snapshot records its revision, supplied
cards, expansion results, and selected module definitions. Reruns copy these
artifacts when reusing a plan. A changed catalog revision rebuilds the plan
before materializing current modules. New document bundles contain captured structure;
model-authored annotations stay in the overview. Legacy enriched bundles remain
readable. Review preparation and registries do not share mutable state with
Conversations.

**Model calls.** Every provider call goes through
[`pipeline/call.rs`](../gui/src-tauri/src/pipeline/call.rs). Provider modules
must not bypass it. `pipeline/response_journal.rs` saves each response before
schema checks. A rejected response stays as evidence and never becomes context
for a later step. Each step reads only its selected roots and writes only to
its own folder.

Declaude's editorial contract is owned by `pipeline/editorial/`. Marked step
schemas require exact readable-text anchors, source fingerprints, candidate
lineage, and explicit verification dispositions. The supervisor retries
rejected records before accepting them as downstream context. Verification
accepts or rejects consolidated replacements without rewriting them. The host
checks overlaps, required companions, and unchanged deletion witnesses, then
renders accepted text literally. Raw records remain in `StepOutput.raw_text`;
its optional `rendered_markdown` field carries the host-authored display and is
included in checkpoint receipts. Semantic fidelity remains a model judgment.

## How a conversation runs

**Codex app server.** Conversations and reviews each start their own
`codex app-server` process with an isolated `CODEX_HOME`.
`agent_runtime/codex/compatibility.rs` sets the version rules:

- Reject malformed version output and builds older than 0.165.0. Admit newer
  builds.
- Treat the checksum-pinned schemas in `gui/src-tauri/codex-protocol/0.147.0/` as
  the field reference. Never edit them.
- After every thread start or resume, check the returned provider, working
  folder, roots, permission profile, approval policy, and reviewer. Reject a
  substituted model when a specific one was requested.
- On a failed check, stop with an error. Never weaken permissions or fall back
  to `codex exec`.

**Claude Code.** `workbench/claude/` runs each turn as `claude -p` with
stream-json. Before each turn, `agent_runtime/claude.rs` requires a
first-party subscription sign-in. It strips inherited `ANTHROPIC_*` and
`CLAUDE_*` variables and never falls back to an API key. Pipeline supplies up
to 128 KiB of recent history from its own transcript store.

When ChatGPT needs a new native thread after changed settings, including the
approval mode, a project move, recovery, or a connection switch, it receives
the same bounded app-owned history. A normal native resume uses the provider's
existing history.

**Direct APIs.** `workbench/api/` owns OpenAI, Anthropic, Gemini, and
OpenAI-compatible conversations. It reads device credentials independently of
Review access modes and run snapshots. The conversation owns its cancellation,
turn records, bounded history, and host tools. Requests and responses are
bounded; SSE text is streamed to the UI while complete native messages retain
tool arguments and thinking signatures. Only terminal replies enter the tool
loop. Redirects and automatic retries are disabled. Accepted submissions
interrupted by a restart are marked interrupted and never resent. API turns
use the same app-wide permit and store gate as subscription turns.

API replies with tool calls are commentary; a terminal reply without calls is
the final answer. Claude's successful result completes its last assistant item
as the final answer. Automations read that answer separately from tool updates.

**Conversation tools.** All providers resolve the same file, web, and command
capabilities. Interactive Edit turns use `project/conversation_copy.rs` to
prepare or reuse a task copy in the existing conversation. Direct edits retain
the project acceptance journal. Automation scopes and manually prepared copies
remain fixed. Shell writes require a task copy. Copy identity participates in
the harness fingerprint so a replacement copy gets a successor native thread
with app-owned history. Codex thread-local configuration travels on
start and resume. Claude receives sandbox settings and an allowlisted native
tool catalog. Windows Claude uses a separately authenticated WSL2 environment
when qualified dependencies are present. API commands use the native Rust runner
in `workbench/shell.rs` and `shell/`. The pinned MXC Rust library applies
Seatbelt on macOS, Bubblewrap on Linux, and native ProcessContainer on Windows.
Compatibility tiers that change host ACLs are refused. Each command receives
its authorized root, system tools, and a private temporary folder. Rust owns
bounded output and cancellation independently of Review processes.

The per-command Rust proxy admits domain names before DNS resolution, rejects
nonpublic addresses, and connects to the checked addresses. HTTP requests are
checked independently. CONNECT and SOCKS5 carry admitted destinations only.
OS restrictions prevent direct outbound connections around the proxy. Dropping
a command stops its owned processes before releasing its proxy. macOS also identifies
descendants by their inherited sandbox policy, so detached children remain
subject to cleanup. Ownership is verified before the command starts. Normal
Python, subprocess, and session creation remain available. Output readers have
a separate shutdown deadline.

Windows networked commands require a temporary,
elevated firewall guard in the same executable. The app and workloads remain
unelevated. The guard authenticates its parent and installs per-container WFP
restrictions before the command starts. Restrictions survive guard failure.
Cleanup stops the matching processes before removing their filters. The guard
runs before desktop initialization and exits with its parent. Web search and
page reading remain separate host tools.

Unix socket access is denied by
the macOS profile and a Linux seccomp guard entered inside Bubblewrap. The
Linux guard runs before desktop initialization in the same executable. It
mounts a filtered project view in a private user and mount namespace before
installing the syscall restrictions. The view protects absent secret paths
without creating mountpoints in the project. Rust serves authorized file
operations through the kernel FUSE interface; no filesystem helper is downloaded.
The command owns the view's lifetime and drops its server after process cleanup.
Linux hosts must permit user namespaces for Pipeline and its sandbox helpers,
including through any AppArmor policy.
Unsupported policies fail before the workload starts.

Windows allows the Win32k initialization required by PowerShell while retaining
native UI resource isolation, clipboard denial, and input-injection denial.
Protected paths with absent parents are denied at their first missing ancestor,
so later-created secrets stay covered without creating host placeholders.

Review PDF export in `commands/export/pdf.rs` uses a short-lived mode of the
Pipeline executable with an isolated native webview. WKWebView on macOS,
WebView2 on Windows, and WebKitGTK on Linux print the self-contained report to
private staging. A shared PDF pass adds page counters before the atomic save.
The helper starts before ordinary app setup and exposes no app commands. Its
parent owns the deadline and process cleanup independently of conversation tools.
The stateless
policy in `workbench/shell_policy.rs` is shared with Claude Code. Public web
search uses the same bounded retrieval policy as page reads. Discovery writer
file tools journal changes in their role folder and retain immutable versions.
They do not redirect writes into the main project.

File and web tools journal admission before acting, compare the captured
configuration with current access, and never repeat an ambiguous call. Network
waits release database workers. Public retrieval validates and pins DNS at each
redirect. Account
services retain identity-scoped quota observations with individual reset times;
observations are not fresh account balances.

**One active turn.** One turn runs at a time across the app, for every
provider. The permit in `workbench/commands.rs` spans setup, submission, and
the matching terminal event. Only an event with the same connection epoch,
thread, and turn releases it. A composer send that finds the permit taken is
refused with `turn_busy` before anything is written. It never waits, because
the turn ahead may be waiting on an approval. The renderer reads the running
turn from `workbench_active_turn`, on mount and after `activeTurnChanged`, so it
survives page changes. Drafts are saved before sending. Pipeline never retries
an ambiguous send.

**Database gate.** Every production store operation goes through the helpers
in `workbench/commands.rs`. Ordinary work shares a read gate and four blocking
workers. Whole-store export and restore take the gate exclusively. Process
waits hold neither a worker nor the gate. Blocking workers own their admission
guards until completion, including after a caller cancels its wait.

**Journal and projection.** Turn and item events are projected into the store
by `workbench/store/projection.rs`. Each gets a deterministic operation ID in
`change_log`, so a replayed event changes nothing.

**Recovery.** The event stream isn't the source of truth. On lag or resume,
the supervisor reconciles from the app server's history with
`thread/turns/list`, one turn per page. A heartbeat finds a terminal event the
stream lost.

**Approvals.** A pending request is keyed by connection epoch, request ID, and
method. The backend claims it before answering, so a duplicate click can't
replay it, and an old connection can't approve on a new one. Requests reach the
event bridge through a lossless queue, so lag can't drop one, and a method no
screen can answer is refused at once rather than left waiting. The request card
reads the app-owned record built in `workbench/codex/requests.rs`, not App Server
params.

**Harness.** Each turn saves a snapshot of the resolved harness. A changed
fingerprint starts a new native binding. Bump `TOOL_CATALOG_VERSION` in
`workbench/research.rs` when you change a dynamic tool. Tools record a running
receipt before any side effect. Model-created notes and evidence stay
proposals until the researcher accepts them.

Plain chat must not load research stores or heavy editors. Streams reach the
UI in batches of 50 to 300 ms, longer as an answer grows, and transcripts render
50 messages at a time.

## Crash and recovery

Each boundary saves state before acting. After a crash, Pipeline shows an
uncertain outcome instead of repeating work.

| Boundary | Saved first | After a crash |
| --- | --- | --- |
| Conversation send | Draft and submission ID | No automatic resend |
| Approval | Pending request with its epoch | An old request can't approve on a new connection |
| Tool call | Running receipt | `outcome_unknown`; never reruns |
| Local job | Record with the process instance, whose lock file shows it is alive | `outcome_unknown` once that instance has ended; reconciling re-hashes outputs first |
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

The Automations page combines regular runs, investigations, and discovery in
one read-only index. Detail reads and controls continue through each kind's
own commands.

- At most four leaf actions run at once, with one conversation turn and one
  review action at a time. A review action takes the run lock exclusively, so
  it waits until no other review is running. **Reviews at once** governs the
  review queue, not these actions.
- `orchestration/adapters/execute.rs` is the one cancellation and publication
  envelope.
- A receipt identifies one action kind and operation. A durable dispatch marker
  precedes the owner call; a new receipt without that marker proves no submission.
  Recovery asks only the action’s owner or reads its result journal. A failed
  coordinator write retries publication of the same attempt without repeating
  the action. Earlier receipts use their saved definition to find the owner.
- Group clocks are absolute deadlines keyed by iteration and branch. Admission
  applies all enclosing action limits and the shortest remaining timeout.
- Each runtime admits its own model requests through a narrow caller-supplied
  meter. The Automations store reserves estimated API spend transactionally
  across sibling Reviews and retries; missing usage keeps its reservation.
  Subscription and local work have separate counters.
- Delivered files are copied by verified digest into Workspace. Each file
  belongs to its conversation exchange, with the bytes and receipt references
  covered by Workspace retention, relocation, and research backups.
- Automation file and folder snapshots resolve against the selected library.
  A committed literature lookup can be recovered by its project and operation
  without issuing another network request. An unreadable active record is
  isolated from dispatch and keeps its original body for repair.
- Settings are pinned at preparation. Review credentials are resolved only
  inside the review adapter.
- A task keeps its review pinned until it adopts the result
  (`commands/orchestration/pins.rs`).
- Finished runs leave the store 30 days on, once opened or if they saved no
  result (`orchestration/coordinator/retention.rs`). Actions of a research
  automation stay. A blob goes only when no surviving record or journal names it.
- Before an automated turn, the conversation adapter checks the project,
  connection, harness, root, and unsent draft. Any change needs attention.
  Older tasks without a saved connection require refreshed context; older
  schedules require preparation again. The submission
  boundary inherits the saved model and effort when an automation leaves them
  unset. An explicit model override uses its own effort or the model default.
- Task cancellation reaches conversation setup before prompt dispatch. Once a
  native request is in flight, the adapter retains ownership until it can
  interrupt the acknowledged turn. API and Claude workers also observe the
  task's stop signal before sending a prompt.

Research automations (`orchestration/missions/`) add planning and assessment
on the same coordinator. Their completion is an assessment that never accepts
edits or widens permissions. `orchestration/claims.rs` grades inspections against
host-retained evidence for an exact round or manuscript version. A model reply
cannot grant researcher acceptance. The explicit researcher command checks the
run revision and subject version before saving that decision. Older assessments
without a claim record retain their original status. Research discovery (`orchestration/discovery/`)
uses the same adapters. Its conversation roles in `workbench/discovery.rs` run
without network access and never ask for approval. Optional idea teams run
inside a single discovery step and hold the Workspace turn permit. Workspace
starts an isolated Codex app server or Claude Code process, verifies the
native child events, and returns the lead's sketches through the task adapter.
The coordinator journals that result before developing the proposals.

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
and a remap-or-detach choice for every root. Every schema object must match what
that schema version creates, and older archive databases then migrate on an
extracted copy. Native bindings are retired, and imported tools never run. The
manifest can list references the researcher chose to leave out because their
files were lost; restore clears those and reports them. Limits are 2 GiB, 20,000
entries, and 512 MiB per entry.

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

**Results files** use `research-results-v1` or `research-results-v2`. No view
imports them since Experiments was retired; the parser stays until the back
end behind that view is removed.

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

**No-model probe.** After `npm ci` and `npm run prepare:release-resources` in
`gui/`, run `cargo run --locked --example workbench_probe` from `gui/src-tauri/`.
It needs a compatible Codex CLI but no sign-in. It checks the handshake, permission profiles, thread
lifecycle, sandbox rules, and process cleanup without a model turn. The
`workflow_codex_probe` example checks the review connection. Run both after
changing Codex transport, sandbox, or process code. Set
`PIPELINE_PROBE_NETWORK=1` for the optional public-domain, denial, and proxy-bypass
checks. Shared web transport tests run with
`cargo test --locked --lib public_web` from `gui/src-tauri/`.

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
