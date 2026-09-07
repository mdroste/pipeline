# Workspace: implementation plan

**Status:** active qualification; implementation slices for WB-00 through WB-12 are present. Deterministic engineering checks pass, while the progress table below identifies the authenticated, real-tool, packaged, and cross-platform acceptance work that remains.  
**Prepared:** September 6, 2026.  
**Product:** Pipeline's standalone interactive research surface, shipped alongside its deterministic Workflow/review surface.  
**Initial runtime:** Codex App Server, authenticated through ChatGPT.  
**Audience:** implementation agents working in this repository.

**Subsequent project improvements:** the user requested PI-00–PI-11 and the Workflow-authoring half of PI-12 from
[`PIPELINE_IMPROVEMENT.md`](PIPELINE_IMPROVEMENT.md). Those implementation slices
now extend this original WB scope with the project surface and
[`Research tools`](docs/workbench/research-studio.md), including an explicit
selected-findings return exchange, the PI-10 Theory panel, and PI-11 selective project exchange, retention, and Workflow drafts. Historical WB exclusions below describe the
original plan; the PI update deliberately extends that narrow bridge without
merging runtime, credentials, cancellation, or writable storage. Qualification
and remaining live gates are in the linked release record.

**Naming:** **Pipeline** is the suite, **Workspace** is its interactive mode, and **Workflows** are its deterministic mode. Existing `workbench` Rust/TypeScript paths, command prefixes, storage directories, and protocol client identifiers are compatibility namespaces and may remain until a deliberate migration. **Review** refers only to the principal paper-review use case and the explicitly named handoff; it is not an authorized code or storage rename.

## 1. Objective and scope

Pipeline is an agent orchestration suite for doing and reviewing academic research. Workspace is its full conversational GUI for ChatGPT with an optional, configurable research harness. It should be useful as an ordinary persistent ChatGPT client before the researcher enables any specialized feature. Research modules add durable workspaces, instructions, context selection, document access, computation, sources, and evidence without forcing every conversation into a deterministic Workflow.

Workspace and Workflows are peer orchestration modes in one desktop shell:

- **Workspace** owns interactive ChatGPT conversations and the customizable research harness described here.
- **Workflows** own deterministic, repeatable dependency graphs for research and review tasks. Paper Review is the principal built-in use case. **Review** remains a possible label for that use case, not an authorized code or storage rename.
- Neither mode replaces the other. Workspace must not alter Workflow providers, authentication, projects, runs, or defaults.
- The optional WB-10 bridge explicitly sends one immutable document revision into the ordinary Workflow launch preview. It is not a first-release dependency and does not merge the two modes.

The central user experience is:

1. Start a normal conversation or open a saved research workspace.
2. Choose the ChatGPT model and, when useful, enable or customize research instructions, context providers, and tools.
3. Work across multiple resumable conversations without requiring a paper, registered folder, Pipeline project, or Workflow run.
4. Optionally attach papers, sources, project files, and computational results and inspect the material behind an answer.
5. Return later with selected research context, decisions, and evidence intact, even in a fresh conversation.

Workspace belongs inside the existing Tauri/Rust/React suite, but behind an independent service and persistence boundary. It does not require a second desktop binary, an Electron prototype, a Python agent bridge, or a replacement model loop. Sharing the shell, renderer components, or carefully extracted low-level libraries does not merge Workspace with the Workflow engine.

### Product and engineering principles

1. **Conversation first.** Sending and resuming a normal ChatGPT conversation is the fast path. Research structure is optional and progressive.
2. **Customizable harness.** Instructions, context providers, tools, inspectors, and recipes are versioned modules that users can enable, disable, edit, clone, and restore. A preset is a convenient configuration, not a hard-coded workflow.
3. **Independent domains.** Workspace data never requires a Pipeline project or Workflow run. Cross-mode data moves only through the explicit handoff/link boundary; neither side scans or imports the other's store automatically.
4. **Modular internals.** Core chat, research memory, documents, computation, and optional bridges depend on narrow interfaces rather than on one another's storage or UI state.
5. **Performance by design.** Lazy-start the runtime and optional modules, keep streaming off the database/UI hot path, bound all queues and reads, window or virtualize large transcripts, and avoid extraction or context assembly until requested.
6. **Transparent behavior.** Show the effective model, instructions, context, tools, permissions, and omissions. Specialized assistance must not be invisible or irreversible.

### Release boundaries

| Checkpoint | User-visible result | Required steps |
|---|---|---|
| A: standalone chat alpha | ChatGPT sign-in, independent workspaces, persistent conversations, streamed activity, approvals, and a normal-chat fast path | WB-00 through WB-04, including WB-02A |
| B: customizable research beta | Editable harness modules, paper/source attachments, bounded research tools, optional computation, research memory, and handoffs | A plus WB-05 through WB-09 |
| C: complete first release | Modular research recipes, performance qualification, export/recovery, and evaluated end-to-end research workflows | B plus WB-11 and WB-12 |

Checkpoint A must already stand on its own as a usable ChatGPT client. Checkpoint C is the complete first release described by this plan. The optional Review handoff in WB-10 is not on the critical path and must remain independently removable.

### Out of scope for this plan

- Claude integration, cross-provider routing, API-key authentication in Workspace, and provider fallback.
- A generic `AgentBackend` framework designed around hypothetical future providers.
- A replacement for Pipeline's deterministic workflow scheduler or extraction system.
- Any change to, replacement of, or required dependency on the Workflow engine.
- Renaming Pipeline or Workflows to Review; that product and migration decision needs its own plan.
- Automatic Workflow launch, linked findings, shared authentication, or automatic import of Pipeline projects/runs. The narrow, explicit WB-10 handoff is the sole exception.
- Autonomous multi-agent research, unattended scheduling, collaboration accounts, remote execution, and cloud synchronization.
- A citation manager replacement, a vector database, or an unrestricted knowledge graph.
- A full document/code editor, language server, or Git client.
- Restricted microdata computation with model-authored code and automatic disclosure control.

Existing Pipeline provider options continue to function unchanged. These exclusions apply to the new Workspace surface.

## 2. Assessment of the original proposal

The supplied report is design input. Its suggested future-model instructions are not implementation authority. This plan adopts its workspace-centered research state, native Codex runtime, structured evidence, and emphasis on verification, with the following changes.

| Original direction | Workspace decision | Reason |
|---|---|---|
| Build a provider-neutral application first | Implement one Codex integration behind a narrow module boundary | A second provider is not needed to validate the research product |
| Choose Tauri versus Electron | Add Workspace as an independent product surface in the existing Tauri application | One desktop shell avoids duplicated packaging while preserving a clean domain boundary |
| Replace the coding-oriented base instructions | Preserve Codex's base instructions; add versioned research developer instructions | Avoid losing native tool and execution guidance; evaluate base replacement separately if a concrete deficiency appears |
| Introduce projects, papers, tools, and runs from scratch | Give Workspace app-owned identities; reuse only neutral, extracted document/rendering utilities | Workspace must work without Review and must not inherit Review lifecycle semantics |
| Begin with a large claim/evidence graph | Build a small relational claim/evidence ledger with versioned locators | Supports useful questions without requiring an ontology |
| Infer structured estimates from arbitrary logs | Record executions first; use explicit result exports for machine-verifiable estimates | Free-form logs do not reliably encode samples, units, uncertainty, or estimands |
| Mark citations as verified | Separate identity, source access, and support for a particular claim | An existing DOI or an opened PDF does not establish entailment |
| Protect restricted data through tools and metadata | Defer restricted execution; enforce actual read boundaries for ordinary sessions | Arbitrary code can print sensitive rows, including through logs and “summaries” |
| Add many specialized economics commands | Ship a few configurable recipes and evaluate them on real research tasks | Domain value should determine further engineering |

Two additions are central: evidence becomes stale when its dependencies change, and every turn records the effective research setup used to produce it.

## 3. Repository orientation

Read `AGENTS.md` and `CLAUDE.md` before implementing any step. `CLAUDE.md` remains the source of truth for implemented architecture. Update it as behavior lands; keep this document as the implementation roadmap. Do not add instructions to `AGENTS.md`.

The following paths are relative to the repository root and were inspected for this plan:

| Existing area | Relevant files | Integration rule |
|---|---|---|
| Application routing | `gui/src/App.tsx`, `gui/src/components/NavRail.tsx` | Add a lazy-loaded Workspace route; keep its state outside `usePipeline` and present Workspace and Review as peers |
| Review project identities and run membership | `gui/src-tauri/src/projects.rs`, `gui/src/components/ProjectsPage.tsx` | Do not use as Workspace identity or lifecycle; any later bridge uses explicit immutable references |
| Review findings | `gui/src-tauri/src/projects/ledger.rs`, `gui/src/components/ProjectIssueLedgerPanel.tsx` | Out of the first-release Workspace path; do not import this state into the research store implicitly |
| Model discovery and resolution | `gui/src-tauri/src/model_catalog.rs`, `model_catalog/discovery.rs`, `model_catalog/resolution.rs` | Reuse neutral selection semantics, with discovery from Workspace's authenticated server |
| Existing Codex calls | `gui/src-tauri/src/pipeline/codex.rs` | Preserve `codex exec` workflow execution and existing context-cache forks |
| Existing App Server probes | `gui/src-tauri/src/model_catalog/discovery.rs` | Reference only: its bounded request/reply subprocess is not a persistent session transport |
| Settings | `gui/src-tauri/src/settings.rs`, `gui/src-tauri/src/commands/config.rs`, `gui/src/components/SettingsPage.tsx` | Keep Workspace settings in an independent namespace; preserve existing provider and Review settings |
| Processes and events | `gui/src-tauri/src/process.rs`, `pipeline/cli_process.rs`, `emit.rs`, `commands/lifecycle.rs` | Reuse suitable low-level helpers; give Workspace independent ownership, queues, and cancellation |
| Document representations | `gui/src-tauri/src/document_bundle.rs`, `document_bundle/`, `pipeline/extract/` | Extract neutral adapters where reuse is worthwhile; importing a document must not invoke or depend on Review |
| Review reports and artifacts | `gui/src-tauri/src/runs.rs`, `runs/`, `findings.rs`, `commands/artifacts.rs` | Remain canonical and untouched in their existing store; not a Workspace first-release dependency |
| Renderers | `ReportViewer.tsx`, `ArtifactExplorer.tsx`, `ReportWorkspace.tsx`, `gui/src/lib/mathMarkdown.ts`, `mathRepair.ts` | Extract reusable presentation pieces where needed; retain validated artifact reads |

The current Review project schema is version 1 and stores metadata plus run IDs in `~/.pipeline/projects/{id}.json`. It is not the Workspace project model. Workspace uses app-owned workspace IDs and may register zero or one primary folder plus selected external attachments. A later bridge can retain an optional Review project/run reference without changing either object's identity.

The current discovery helper launches App Server for short exchanges, and the existing Codex wrapper also uses App Server for context-cache forks. Neither provides durable interactive sessions, approval routing, or recovery. Do not lengthen a discovery timeout and treat that as a session manager.

## 4. Design decisions

### 4.1 Ownership

```text
Pipeline desktop shell
  /                                           \
Workspace product                         Review product
chat / settings / inspectors              existing workflows and reports
  |                                           |
typed Workspace commands/events           existing Pipeline commands/events
  |
Workspace Rust service
  /                 |                    \
Workspace store     Codex supervisor     Optional research modules
SQLite + blobs      JSONL over stdio     context / documents / tools
                        |
                 Codex App Server
                        |
                ChatGPT authentication
                        |
                  native Codex harness

Optional future bridge: explicit, versioned Workspace export/link → Review input.
```

Codex owns model communication, tool selection, native file/shell behavior, compaction, authentication, and provider thread history. Workspace owns workspace membership, optional document identity, curated research memory, evidence, application-tool results, and UI projections. Review owns its workflows, provider resolution, projects, runs, findings, and reports.

Workspace's transcript is a durable local projection for display and export. It is not a replacement Codex rollout, and Workspace must not edit or reverse-engineer Codex's private session databases.

### 4.2 Runtime and credential namespace

- Use an installed official Codex CLI, resolved through Pipeline's executable/PATH handling. Ship no new Python or Node runtime for the agent integration.
- Launch a Workspace-owned `codex app-server --listen stdio://` process. Do not attach to, control, or depend on the Codex desktop application's internal process.
- Give this child an isolated Codex home under `~/.pipeline/workbench/codex/` through its child-process environment. Do not change the parent application's environment or the user's normal Codex configuration.
- Workspace signs in through the server's managed ChatGPT login flow. Codex owns credential storage and refresh. The host app does not copy tokens from the user's existing Codex installation or browser.
- Validate credential namespace behavior, including the chosen Codex credential-store backend, before promising independent sign-out. If keyring isolation is not established, use the supported file credential store in the private Codex home, with local access protections and export exclusions.
- Strip inherited API keys and unrelated provider overrides from this child environment. Workspace checks for ChatGPT authentication and the OpenAI provider before accepting a turn.
- Use one lazy-started server and one active Workspace turn globally in the first release. Multiple workspaces and conversations can remain open. A visible queue serializes sends; waiting drafts can be edited or removed.
- Disable native agent delegation, ambient cross-workspace memory, hooks, and unselected integrations through the qualified configuration surface. Workspace introduces no invisible background researchers.

The separate sign-in is intentional and should be explained once in Settings. Do not silently change the account used by existing Pipeline workflows or by the user's Codex CLI.

### 4.3 Storage

Use bundled SQLite through a Rust dependency compatible with the repository's toolchain. Run blocking database work on a bounded worker, never on the UI or protocol reader. Use foreign keys, transactions, an application schema version, and migration backups. Keep the database on local application storage rather than in a research folder or network drive.

```text
~/.pipeline/
  projects/{review_project_id}.json     existing Review project metadata; unchanged
  runs/{review_run_id}/                 existing Review workflow artifacts; unchanged
  workbench/
    research.sqlite3                   Workspace records and transcript projection
    blobs/{content_hash}/...            immutable snapshots and adopted artifacts
    jobs/{execution_id}/...             host-owned execution metadata and logs
    context/{snapshot_id}/...           bounded model-readable context exports
    codex/                             isolated Codex runtime state and credentials
    backups/                           research-store migration/backup bundles
```

The host-owned database, credentials, raw journals, and evidence receipts must not be model-writable. Expose only selected context exports and tool results to the model. Model-editable workspace files cannot certify their own provenance.

Workspace IDs are app-owned and independent of Review project IDs. The database holds `workspaces` with their own name, optional registered root, settings, archive state, and ordering. Conversations may also be unfiled. Review references, if introduced later, live in an optional bridge table and never control Workspace lifecycle.

Migration 1 was implemented under the earlier plan with `project_workspaces` keyed by Review project IDs. WB-02A corrects that boundary before any user-facing route or additional research schema lands: migrate existing rows to app-owned workspaces, preserve sessions and provider bindings, and remove runtime dependency on Review project JSON. Do not extend the transitional coupling.

No mandatory folder layout. Register an existing folder without reorganizing it. Offer optional scaffolds for a new empirical paper, theory paper, or literature project. Creating a scaffold shows the files to create and never overwrites existing files.

Workspace deletion never deletes registered source files, attachments outside app storage, or Review data. Archive is the default reversible action. Permanent deletion is separate, names the app-owned records/blobs affected, and cannot cascade into Review.

### 4.4 Configuration versus instructions

The effective harness consists of typed settings, resolved permissions, selected instruction modules, context providers, tools, native instruction sources, and a bounded context packet. Persist a versioned snapshot per turn. The empty/minimal harness is valid and yields an ordinary conversation.

The model may propose changes to research settings. It cannot mutate active permissions, executable paths, enabled integrations, or canonical evidence status by editing a file or emitting prose.

Preserve Codex base instructions. Supply enabled research guidance through the qualified `developerInstructions` field. Do not replace the base prompt, the compaction prompt, or collaboration-mode instructions by default. Compose instruction modules deterministically, show their order, and allow the user to disable every Workspace-supplied research module. The local 0.147.0 generated schema includes `baseInstructions` and `developerInstructions`; this is evidence of field availability, not a reason to replace the base instructions.

Avoid generating or overwriting a workspace's `AGENTS.md`. Show the instruction files Codex reports loading, where supported. Native instruction discovery and organizational requirements still apply; the app must not claim to reconstruct the complete hidden instruction stack.

### 4.5 Harness modules and performance boundary

Core conversation code knows only a small `HarnessSnapshot`: ordered developer-instruction fragments, selected context entries, declared tools, permission requirements, and optional inspector contributions. It does not import paper, evidence, computation, recipe, or Review implementations. Each optional module has a stable ID/version, typed configuration schema, declared dependencies/capabilities, deterministic snapshot contribution, and bounded lifecycle hooks. A module cannot reach another module's tables or invoke Codex RPC directly; it uses Workspace service interfaces and records its contribution in the per-turn snapshot.

User-created harnesses are compositions of module configurations. They can clone shipped modules and edit supported fields, but cannot introduce native executable code merely by importing configuration. Missing or incompatible modules remain visible and disabled. Opening an old conversation uses its immutable snapshot for display and asks for an explicit migration before a new turn if current modules cannot reproduce it.

The core path has explicit performance constraints:

- Lazy-load the Workspace route, start App Server only on a connection/conversation action, and load document/evidence/computation inspectors only when enabled or opened.
- Do not persist or trigger a React render for every token fragment. Coalesce display deltas at a bounded frame cadence while persisting provider items and terminal states durably.
- Page transcript hydration and virtualize long histories. Opening a conversation must not deserialize all historical tool payloads or artifacts.
- Keep database, context assembly, document extraction, hashing, and tool execution off the protocol reader and UI thread, with cancellation and bounded concurrency.
- Measure bundle contribution, cold/warm open, send-to-first-visible-state, streaming smoothness, memory growth, idle resource use, and cancellation latency from WB-04 onward. Record reference hardware and fixture sizes; establish release budgets before WB-11 acceptance rather than retrofitting performance at the end.

## 5. Codex protocol contract and qualification

### 5.1 Evidence and version policy

Official App Server documentation was fetched for this plan. The original locally installed CLI reported **0.147.0**. Its stable and experimental JSON Schemas were generated without running a model or starting a login; they remain the checked-in reference for the implemented protocol generation.

There are material differences between the installed schema and current documentation. For example, `dynamicTools` appears in the local experimental thread-start schema, while the newer documented legacy restricted-read shape is absent from the local `TurnStartParams` definitions. Current examples also use enum spellings that differ from some installed schema values.

Therefore:

1. Workspace requires `codex-cli 0.147.0` or newer. The version is a protocol floor, not an exact allowlist: newer builds proceed through the live handshake and must echo the security-sensitive provider, roots, permission, approval, and model contract after thread start/resume. The no-model suite passed on macOS arm64 with `0.153.4` on September 6, 2026.
2. Generate both stable and experimental schemas from the selected binary. Store relevant schema artifacts, their hashes, the binary version, and compatibility fixtures in the repository.
3. Generate outbound fields and enums from that contract. Do not copy Python SDK snake_case names into JSON-RPC or mix CLI flag values with RPC enum values.
4. Treat experimental opt-in as a versioned product dependency. It does not mean every experimental feature should be enabled.
5. An incompatible capability leaves saved research data accessible and gives a precise compatibility message. Never silently relax permissions or switch to `codex exec` for a Workspace conversation.
6. Re-run schema review and the qualification probe for intentional protocol changes and release validation. Routine newer builds need not match an exact schema hash when the required live contract still holds. Do not invoke the schema generator on every app startup.

The required first-release capabilities are managed ChatGPT login, model discovery, thread start/read/resume, turn start/interrupt, item streaming, interactive request handling, enforced read/write scope, and the selected research-tool bridge. Forking, steering, and server-side history pagination are separately gated enhancements.

### 5.2 Transport

Use bidirectional JSON-RPC-style messages over newline-delimited JSON. The documented wire format omits the `jsonrpc` header. The client must distinguish responses, notifications, and server-initiated requests before routing by ID. Client and server request IDs are separate namespaces; preserve string or numeric server IDs exactly.

Connection sequence:

1. Resolve the executable and create its owned process group/job.
2. Start stdout and stderr readers immediately.
3. Send `initialize` with client name `pipeline_workbench` and the Pipeline version.
4. Await success, then send the `initialized` notification.
5. Read account state, applicable configuration requirements, and the model catalog.
6. Create or resume an app-owned thread only after its workspace/conversation settings and optional context are resolved.
7. Keep the reader active while approvals, application tools, and normal requests are pending.

Use one serialized writer and an asynchronous request map. The reader must never await an approval dialog or a research computation. A tool handler may need to make a sandboxed `command/exec` request on the same connection; the reader must remain available to complete it.

Define centralized limits for frame size, outstanding requests, pending events, transcript hydration, stderr, and tool output. Suggested initial limits: 32 MiB per protocol frame, 128 pending client requests, 64 MiB of hydrated transcript data, and 4 MiB of diagnostic stderr. Validate these against fixtures. Oversized required messages cause an explicit protocol error, not partial JSON parsing. Coalesce text deltas for UI delivery; never drop terminal states or approval requests under backpressure.

Diagnostic logging is allowlisted, not a raw dump of RPC traffic. Exclude credentials, OAuth URLs/codes, environment secrets, and raw reasoning content. Keep research text in its intended transcript/artifact store. If qualified history pagination is unavailable, use bounded local transcript pages and a capped native history read; an oversized native thread remains viewable locally and can continue through a reviewed successor session instead of exhausting memory.

### 5.3 Required API mapping

| Workspace action | App Server surface | Required behavior |
|---|---|---|
| Check account | `account/read`, `account/updated` | Distinguish signed out, ChatGPT, unsupported auth mode, and error |
| Sign in | `account/login/start`, `account/login/completed` | Start managed ChatGPT login, open returned URL externally, correlate login ID |
| Cancel login / sign out | `account/login/cancel`, `account/logout` | Affect only the qualified Workspace runtime namespace |
| Discover models | `model/list` | Follow pagination; preserve model IDs and supported efforts |
| Inspect quota | `account/rateLimits/read`, `account/rateLimits/updated` | Prefer available per-bucket data; missing is unknown, not zero |
| New session | `thread/start` | Record provider thread ID and returned effective setup |
| Open history | `thread/read` | Display persisted items without unintentionally starting work |
| Continue session | `thread/resume` then `turn/start` | Reapply and validate intended configuration before sending |
| Stop | `turn/interrupt` | Await terminal state; acknowledgement alone is not completion |
| Text and activity | `item/started`, item deltas, `item/completed` | Final item replaces accumulated preview for the same item ID |
| Turn status | `turn/started`, `turn/completed`, errors/warnings | Use returned status; do not infer success from final-looking prose |
| File changes | `fileChange` items, `turn/diff/updated` | Show actual changes and pending/failed/declined states distinctly |
| Approvals | command/file/permission approval requests | Validate scope and send exactly one supported decision |
| Questions | `item/tool/requestUserInput` | Render questions and map answers by stable question ID |
| Request cleanup | `serverRequest/resolved` | Dismiss stale approval/question controls |
| Research tools | experimental `dynamicTools` and `item/tool/call` | Bind calls to the actual workspace, thread, turn, and saved tool catalog |
| Sandboxed computation | `command/exec` and qualified stream/termination methods | Pass an explicit effective sandbox and track its process independently |
| Compaction and usage | `contextCompaction` items, `thread/tokenUsage/updated` | Display supplied state without implementing a second compactor |

Do not use `thread/shellCommand` or experimental `process/spawn` as convenience execution methods: current documentation describes them as outside the thread sandbox. Do not infer that arbitrary host-executed tools inherit Codex's sandbox. [App Server protocol and API reference](https://learn.chatgpt.com/docs/app-server).

### 5.4 Session and submission state

Keep these concepts separate:

- **Workspace conversation:** stable application ID, optional workspace/document membership, title, saved harness configuration, archived state.
- **Provider binding:** provider thread ID plus runtime namespace and incarnation. A replacement thread creates a new binding while retaining the old transcript.
- **Turn:** one user submission and its actual runtime outcome.
- **Research execution:** a computation or other recorded research operation, possibly invoked during a turn.
- **Review run:** an optional future external reference to an existing immutable Pipeline workflow execution; never required for a Workspace conversation.

Runtime state: `stopped → starting → ready`, with `incompatible`, `auth_required`, `disconnected`, and `failed` branches.

Submission state: `draft → queued → submitting → accepted → terminal`, with `submission_unknown` when delivery cannot be established.

Turn state: `running`, `waiting_for_approval`, `waiting_for_input`, `interrupting`, `completed`, `failed`, `interrupted`, or `outcome_unknown`. Approval/input waits are projections of active requests and do not replace the provider's actual terminal status.

Persist a client submission ID before sending. Use a supported client message ID field for correlation when available, but do not assume it guarantees server-side deduplication. If a socket/process failure occurs after a send, reconcile through the owned thread's history. Never blindly repeat `turn/start`, a compute job, or a database mutation.

On restart, reconnect and reconcile; do not automatically submit queued messages or continue an ambiguous action. Preserve the user's draft and offer a concrete Resume/Send again choice once the known history is displayed.

Forking copies conversation history through the qualified API; it does not copy workspace files or create a Git worktree. Label that distinction in the fork action. A clean independent audit starts a new session with a selected context packet, not a history fork containing the proposed answer.

### 5.5 Cancellation and recovery

- Navigating away does not terminate a running turn. A small global activity indicator keeps Stop and pending questions accessible.
- Stop sends an interrupt and cancels owned research-tool jobs. Give the server a bounded grace period; on failure terminate and reap the owned process tree and mark unconfirmed outcomes accordingly.
- Use per-job ownership. Review's existing global cancellation flag/PID list must not kill Workspace, and Workspace Stop must not kill an unrelated Review workflow.
- On application exit, stop or explicitly resolve active work and shut down owned children. No background daemon is part of this release.
- After a crash, inspect completed provider items and host job receipts before reconciling evidence. Do not mark a result successful merely because its output file exists.
- Subscribe before taking a UI snapshot, then replay events after its sequence watermark. A reload must neither lose terminal events nor append duplicate final messages.

### 5.6 Application command and event boundary

Expose typed application commands rather than a frontend-accessible arbitrary RPC proxy. Initial command families should be:

| Family | Suggested commands |
|---|---|
| Connection | `workbench_runtime_status`, `workbench_login_start`, `workbench_login_cancel`, `workbench_logout`, `workbench_model_catalog` |
| Workspace/conversation | `workbench_create_workspace`, `workbench_register_root`, `workbench_list_sessions`, `workbench_create_session`, `workbench_update_session`, `workbench_session_snapshot` |
| Execution | `workbench_submit`, `workbench_interrupt`, `workbench_answer_request`, `workbench_reconcile_session` |
| Configuration | `workbench_resolve_settings`, `workbench_save_settings`, `workbench_preview_context` |
| Research modules | Typed document/source/note/execution/evidence commands introduced in their WB step; all are optional and artifact reads accept object IDs and bounded selectors |

Every mutation takes a client operation ID and, where editing a saved record, its expected revision. Return a typed conflict rather than overwriting a newer draft. The backend resolves provider thread IDs from owned bindings; the frontend cannot read arbitrary Codex threads or supply a broader filesystem policy.

Use one `workbench:event` envelope with `schema_version`, monotonic `sequence`, optional workspace/session/turn IDs, `kind`, and typed payload. Event kinds include runtime/account state, session/turn state, item updates, pending-request changes, research-record changes, and diagnostics. Commit durable state and its replay event together before publishing. Ephemeral streaming deltas may be coalesced and need not each become database rows; final items and terminal transitions must be durable. Snapshot responses include the replay watermark and any explicitly incomplete history.

Return errors with a stable code, readable message, retryability, and recovery action. Distinguish incompatible runtime, authentication required, unavailable model, invalid input, permission denied, storage conflict, and unknown submission outcome. Do not send raw secrets or stack traces to the user interface.

## 6. Research data contracts

### 6.1 Minimum database entities

Use opaque IDs, UTC timestamps, schema versions, and explicit nullable fields. Never overload a path as an object's identity.

| Entity | Minimum fields and invariant |
|---|---|
| `workspaces` | App-owned ID, name, optional canonical root/root identity, settings revision, ordering, archive state. No Review ID is required |
| `papers` | ID, workspace ID, title, role (`manuscript`, `appendix`, `other`), current revision ID. A workspace may have zero or many papers |
| `paper_revisions` | ID, paper ID, input kind, entrypoint, dependency manifest, hashes, optional compiled PDF, extraction/bundle reference, capture time. Immutable |
| `sessions` | ID, optional workspace ID and paper ID, title, harness/preset ID, overrides, created/updated/archive state. Unfiled conversations are valid |
| `session_bindings` | Session ID, runtime namespace, provider thread ID, incarnation, creation/retirement reason. Unique provider binding |
| `turns` | ID, binding ID, client submission ID, provider turn ID, state, timestamps, instruction/settings/context snapshot IDs, usage, errors |
| `transcript_items` | Binding/turn/provider item ID, item kind, final payload or bounded log reference, finality. Upsert by provider identity |
| `pending_requests` | Connection epoch, server request ID, binding/turn/item IDs, request type, payload, deadline if supplied, decision/resolution. One decision maximum |
| `presets` / `config_snapshots` | Versioned user presets and immutable effective per-turn configuration; no credentials |
| `context_snapshots` | Packet manifest, selected object revisions, body/blob references, source hashes, truncation notes |
| `research_notes` | Workspace/paper scope, kind (`question`, `assumption`, `decision`, `next_step`, `notation`), body, state, provenance, revision |
| `sources` / `source_versions` | Bibliographic identity, identifiers, local/remote version, access date, content hash, text/bundle reference, access state |
| `claims` / `claim_versions` | Scope, claim text, kind, author/proposal origin, revision, paper locators, workflow state |
| `evidence_links` | Claim version, source/run/derivation target, immutable locator, relation (`supports`, `contradicts`, `qualifies`), assessment and freshness |
| `verification_records` | Evidence link, method, checker identity, input hashes, observed result, limitations, timestamp. Append-only |
| `research_executions` | Tool, command specification, cwd, environment/tool identity, declared inputs, outcome, timestamps, exit status, logs, output manifest |
| `artifacts` | Content hash, media kind, size, origin, immutable storage reference and original path if known |
| `review_links` | Deferred optional bridge: workspace/paper/session/turn plus external Review object type/ID and immutable handoff metadata |
| `change_log` | Monotonic app event sequence, entity, mutation origin, concise diff/reference. Supports replay, audit, and export |

Introduce these tables in migrations when their implementation step needs them. Do not build empty panels and schemas for future concepts beyond this list.

### 6.2 Paper and source identity

The workspace's own manuscript is a `paper`. A cited external work is a `source`. The same external work may have working-paper and published versions; do not merge their claims or page locators merely because the title is similar.

A paper revision fingerprints the document dependencies, not just `main.tex`. Record included TeX files, bibliography files, figures/tables actually included, and a compiled PDF when available. Store input-manifest completeness explicitly. Follow Pipeline's bounded include resolution and retain unresolved-dependency quality notes.

A path-only attachment is mutable and must be labeled as such. Evidence locators should target an immutable content snapshot. Where copying a large file is impractical, store a strong identity and show `unavailable` if that version cannot later be read; never substitute the latest bytes under the old hash.

Reuse existing document-bundle locators for nodes, pages, assets, and source spans. A line locator also carries the source hash. If the file changes, the old line number must not silently point at different prose.

### 6.3 Evidence semantics

Keep four independent questions visible:

1. **Identity:** does this bibliographic item or artifact exist?
2. **Access:** was the underlying source available and inspected, or only its metadata/abstract?
3. **Assessment:** who judged that it supports, contradicts, or qualifies this claim, and by what method?
4. **Freshness:** do the linked claim, source version, code, and data identities still match?

Use claim workflow states such as `proposed`, `under_review`, `accepted`, and `retired`. Do not collapse those into a universal “verified” flag.

An evidence assessment may be `not_checked`, `model_assessed`, `human_confirmed`, or `check_passed`/`check_failed` for a named executable check. A passed numeric comparison proves that comparison; it does not establish identification, causal interpretation, or the truth of an economic theory.

Models can propose claims, notes, and evidence assessments. Rust validates object existence, workspace scope, locators, hashes, and the required check receipt. Human confirmation must come from a user action. Models cannot generate a `human_confirmed` record or fabricate a successful executable check through a tool argument.

Invalidate freshness conservatively when a linked dependency changes. Retain the earlier evidence and mark it `stale` with a reason. Unknown dependency coverage is `unknown`, not `current`. A successful recheck creates another verification record; it does not erase history.

Initial useful queries are SQL-backed lists:

- Claims in the introduction without any current supporting evidence.
- Paper statements linked to an outdated table or computation.
- Literature claims based only on metadata or abstracts.
- Decisions and open questions associated with the selected paper.
- Accepted decisions and unresolved questions that should be included in the next conversation.

No graph visualization is required to make these queries useful.

## 7. App configuration and context assembly

### 7.1 Settings hierarchy

For ordinary preferences: built-in default → global Workspace settings → workspace override → conversation override → explicit next-turn override. Show the winning value and its source. “Reset to inherited” removes an override rather than copying today's parent value.

Permissions are resolved separately: organizational requirements and application-enforced boundaries constrain every requested override. A preset or imported manifest cannot broaden them. Settings changes apply between turns; active work retains its immutable snapshot. Show whether an edit applies to the next turn, a resumed thread, or a newly created thread.

Some turn-level settings persist natively in Codex. Workspace therefore sends and checks its intended effective values, and does not assume that omitting a field restores a default. If a changed instruction/tool catalog cannot be applied safely on resume, create a successor session binding with a visible handoff.

### 7.2 Required Settings controls

| Settings group | Controls | Initial behavior |
|---|---|---|
| ChatGPT connection | Codex executable, detected/supported version, sign-in/out, account state, diagnostics | User-installed official CLI; dedicated Workspace login |
| Model | Automatic / role / exact model; supported effort picker; refresh catalog | Reuse `ModelSelection`; Automatic uses current authenticated default |
| Harness modules | Enable/order instruction packs, context providers, tools, inspectors, and recipes; clone/edit/restore modules; preview effective setup | Minimal chat by default; opt-in editable economics research pack |
| Context | Selected paper, pinned decisions/questions, source/result attachments, size budget, preview included/excluded material | Empty context is valid; no recursive upload of the whole workspace |
| Files and execution | Inspect/Edit workspace mode, readable/writable locations, scratch location, execution timeout | Inspect initially; explicit change enables workspace writes |
| Network and integrations | Native web search setting, command network access, selected tools/skills/MCP entries | Web search off until enabled; command network off |
| Research software | Named executable/launcher profiles, test button, working directory, timeout, output declarations | All optional; tools appear unavailable until configured and tested |
| Evidence | Automatic proposal capture, stale-evidence checks, source snapshots, result-export conventions | Proposals allowed; no automatic human confirmation |
| History and storage | Disk usage, transcript export, backup/export, retention, active runtime status | Keep research records; no automatic evidence deletion |

Keep common settings in forms. Advanced instruction text and command argument editors are available without requiring users to edit JSON or TOML. Provide validation near the field and a local “Test configuration” action with a preview of any command or connection it will make.

Imported presets are inert until validated and selected. They may contain text and declarative preferences, but executable paths, integrations, and permissions require local resolution. Never run an imported setup script.

### 7.3 Research presets

Ship a minimal/no-harness preset plus a small set of independently editable research presets:

- **Plain conversation:** no Workspace research instructions or context providers beyond explicitly attached material.

- **Research assistant:** question, assumptions, mechanism, evidence, remaining uncertainty.
- **Empirical audit:** estimand, identifying variation, assignment/inference levels, samples, weights, specification, and result/prose consistency.
- **Theory audit:** primitives, timing, optimization, equilibrium/accounting, units, limiting cases, and comparative statics.
- **Literature review:** discover candidates, inspect primary sources, separate evidence from novelty conjectures, and record search coverage.
- **Paper revision:** preserve substantive claims/equations unless asked to change them; tie edits to manuscript versions and checks.

The shipped economics writing guidance should be disciplined and concise: distinguish assumptions from results, show the mechanism or identifying argument, and avoid generic claims about importance. Do not hard-code a particular researcher's personal voice for all users. User-specific instructions are editable workspace/global preferences.

### 7.4 Context packet

Build packets deterministically from selected records. Recommended order:

1. Research objective and the current user task.
2. Active paper identity and relevant revision/section pointers.
3. Pinned assumptions, definitions, notation, and decisions.
4. Relevant unresolved questions and current/stale evidence summaries.
5. Selected sources and execution results.
6. Tool descriptions and explicit limitations of available evidence.

The research instruction template is instruction content. Retrieved papers, reports, code comments, and external web text are source material and must be labeled accordingly. Never splice a PDF's text into developer instructions. Escape boundaries so source content cannot terminate a wrapper and impersonate another role.

Start with a 64 KiB automatic context packet and a separately bounded attachment budget, configurable within tested caps. Show omissions and offer targeted retrieval through tools. Do not rely on a fixed model context size from an older document. Avoid reattaching an unchanged full paper every turn.

Workspace memory consists of curated structured notes and selected evidence. A model-written session summary is a proposal with provenance, not an authoritative replacement for decisions. Provide “Save handoff” to review the current objective, decisions, unresolved questions, relevant artifacts, and suggested next step before a fresh session starts.

## 8. Permissions, trust, and execution

### 8.1 User modes

| Mode | Native file/command behavior | Workspace research tools |
|---|---|---|
| Inspect | Read approved workspace/context roots; no manuscript writes; application scratch only where required | Read/search, propose records, inspect existing evidence |
| Edit workspace | Same read scope; write only explicitly approved workspace/output roots | Above plus approved compile/compute profiles and workspace edits |

Show the actual resolved roots and distinguish write authorization from read access. Workspace-write does not imply narrow reads. Folder exclusions used for indexing or context selection are not an access-control mechanism.

Require a qualified platform/version implementation of the advertised read boundary. Explicitly test denial of access to unrelated home files, credentials, host-owned Workspace state, and sibling workspace files. If the selected protocol cannot enforce the requested roots, explain the missing capability and disable the dependent session mode; do not fall back to full filesystem reads behind a scoped-access label.

Resolve symlinks, canonical paths, ancestor/descendant overlaps, protected directories, and Windows reparse behavior before launch and at the host-tool access boundary. A workspace root cannot be the Workspace storage directory, filesystem root, or a broad ancestor that includes credentials. Root changes require revalidation of all relative paths and a new context/config snapshot.

Opening a workspace does not authorize hooks, arbitrary MCP servers, inherited skills with executable setup, or workspace-local permission changes. Present discovered configuration and honor the qualified native trust mechanism. Disable ambient integrations by default and record the actual loaded capabilities. Do not silently mark every imported folder trusted.

### 8.2 Approval UI

Render approvals inline with their session, tool/command, working directory, requested access, reason, and available decisions. Edit workspace mode permits ordinary edits inside its approved write scope without requiring a separate confirmation for every file. Show resulting diffs and failures.

Support approve once and deny first. Offer session-scoped decisions only where the server provides them and the scope is visible. Do not implement an unrestricted “approve everything” shortcut.

Expire requests when the turn ends, connection epoch changes, or the server resolves them. A button from an old turn cannot authorize a new operation. Unknown requests receive a supported denial or method error; they must not hang the stream indefinitely.

Keep questions distinct from permissions. If the server provides an automatic question-resolution timeout, Workspace does not use it to auto-approve execution. No response is not permission.

### 8.3 Three separate network surfaces

Configuration must distinguish model-service traffic, hosted web search, and network access by local commands/MCP tools. Turning off command network access does not make a cloud model session offline. Selected file contents and tool outputs are sent to the model as part of normal inference. Explain this when a workspace is connected, without showing a repeated warning for ordinary turns.

Do not expose a “restricted data is safe” toggle. For the first release, sensitive work uses a separately prepared workspace containing only material the user may send to ChatGPT. Workspace must not mount or read the restricted source directory. Future restricted computation needs an independently enforced execution and disclosure boundary, including logs, errors, filenames, temporary files, and exports; it is a separate design project.

### 8.4 Computational tools and local conventions

Use typed execution profiles: an executable or approved launcher, an argument array with bounded placeholders, a working directory, input/output declarations, timeout, and environment allowlist. Do not concatenate model-supplied arguments into shell text.

Where a shell wrapper is necessary, the wrapper body is trusted configuration and user/model paths are passed as positional arguments with tested escaping. A process launched directly by Rust is outside Codex's sandbox unless separately contained. Prefer explicit `command/exec` sandbox policies for ordinary compilation and computation. Tool profiles requiring host execution must declare that fact and use a scoped user-approved capability; they must not silently become unsandboxed after a failure.

Bind host-execution authorization to the resolved launcher, arguments, roots, and relevant script/configuration hashes. Revalidate before launch; an edited script cannot inherit approval for different executable content. Serialize Workspace edits and managed computations over the same workspace. External edits cannot always be prevented: detect changed inputs and mark the receipt's snapshot consistency as uncertain rather than claiming an exact reproduction.

On this user's current Mac:

- All Stata calls, including version/help/tests, must go through `/bin/zsh -lic` and the `oldstata` function. Never invoke a Stata executable directly.
- Stata scripts must exit cleanly with `exit, clear`; allow the wrapper to complete its `Legacy Time Off` cleanup. Cancellation should try graceful termination and preserve the wrapper long enough for cleanup. A forced termination must leave a visible cleanup-required state if completion cannot be confirmed.
- If Shortcuts cannot communicate inside the sandbox, request the exact wrapper operation through a scoped host-execution approval. Do not bypass the wrapper.
- All Git commands must run through zsh outside the sandbox with scoped authorization. This includes read-only Git metadata checks. Git provenance is optional if this path is unavailable; content hashes remain usable.

Treat those as this machine's configured launch policy, not cross-platform defaults. Other installations select and test their own launcher. Do not discover Stata by running a forbidden binary first. No automatic Git commits, resets, or branch changes belong in this release.

## 9. Research tools and provenance

### 9.1 Bridge choice

Use App Server's experimental dynamic-tool surface for the first research-tool bridge, pinned and tested in WB-00/WB-07. It provides direct Rust handling and thread/turn attribution without shipping another tool server. Do not simultaneously implement an MCP transport. Keep research services independent of wire types so an MCP bridge can be added later if needed.

Use valid flat tool names such as `workbench_paper_read`; the descriptive names below are conceptual. The host determines workspace/thread/turn identity from the authenticated request binding, not from model-supplied fields. Reject cross-workspace IDs even when they exist.

Each tool has a versioned input/output schema, access class, size/time limit, cancellation behavior, and mutation policy. Model-written proposals can be saved automatically as proposals. Launch permissions and promotion to user-confirmed evidence remain host decisions.

Persist a tool-call receipt keyed by provider binding, turn ID, and call ID before performing a mutation. A duplicate delivery returns the recorded result or an explicit in-progress/unknown outcome; it never reruns a computation. Database-only mutations and their receipts commit in one transaction. External side effects use the execution state machine and require reconciliation after a crash; do not claim exactly-once execution across a process boundary.

On resume, check that persisted tool declarations match the available handlers and contract version. Do not assume the API can replace every tool field on resume. A removed or incompatible tool produces a clear disabled state or successor session, never an unvalidated dispatch by name.

### 9.2 First-release tool set

| Tool | Behavior | Guardrail |
|---|---|---|
| `workspace_context` | Retrieve selected workspace notes and object pointers | Bounded, workspace-scoped reads; absent in plain conversations |
| `paper_read` | Read a revision's section/node/page/text span | Return locator and revision hash with text |
| `paper_search` | Search indexed paper/source text | Deterministic text search first; preserve page/line mapping |
| `paper_asset` | Return a selected page/figure image | Reuse validated asset readers and multimodal output supported by the pinned bridge |
| `source_register` | Propose a bibliographic record and source version | Identity is not support for a claim; validate URLs/paths |
| `claim_propose` | Create or revise a proposed research claim | Record model/session origin; no human-confirmed status |
| `evidence_propose` | Link a claim version to a source, run, or derivation | Validate target, locator, and freshness; store assessment provenance |
| `note_propose` | Propose a decision, assumption, question, or handoff note | User accepts durable decisions; keep rejected proposals out of default context |
| `run_lookup` | Inspect an existing Workspace research execution | Missing/deleted artifacts are explicit |
| `latex_compile` | Execute a configured paper build, capture diagnostics/output | Disable shell escape by default; explicit bounded launcher and output scope |
| `research_run` | Execute a selected, configured script/profile | Structured run receipt; no arbitrary executable chosen by the model |
| `stata_run` | Invoke the configured Stata profile and inspect its log | Enforce machine launcher policy and Stata error semantics |
| `result_compare` | Compare declared structured outputs with units/tolerances | Rust computes comparisons; do not delegate arithmetic equality to prose |

Literature discovery initially uses enabled native Codex web search and explicit local PDF/BibTeX import. Source registration can store a user-selected or agent-discovered URL. A separate acquisition service validates destinations, redirects, content type, and size before download; it must not fetch arbitrary local/private-network addresses from paper text. Do not add a paywall bypass or silently treat an abstract as full text.

Generic citation metadata providers and managed external MCP configuration are follow-ups after local sources and native search work well. Settings may initially expose only tested integrations; do not build an arbitrary marketplace.

### 9.3 Execution receipts

Record an execution before starting it. Capture:

- Originating workspace/session/turn/tool call and user authorization scope.
- Exact executable/launcher and argument vector, cwd, selected environment identity, tool version when safely available.
- Declared input files and hashes; dependency-capture completeness and any external/unhashed inputs.
- Random seeds and package/environment manifests when supplied. Missing values remain missing.
- Start/end times, interruption/timeout state, exit status, bounded stdout/stderr and durable log references.
- Declared outputs, produced outputs, hashes, media types, and validation results.
- Optional authorized Git commit/dirty-tree metadata, supplementary to actual file identities.

Host-adopt output artifacts after checking path scope, regular-file type, size, and hash. Do not treat an arbitrary model-written `manifest.json` as a trusted receipt. Never describe a partial input inventory as full reproducibility.

Native Codex command activity is visible in the transcript. It becomes a recorded research execution only when captured through the research service or adopted with explicit limitations. A command returning zero is insufficient to mark estimates, citations, or identification valid.

For Stata, inspect the log and expected exports as well as process exit status; batch failures can be represented in the log. For LaTeX, record build diagnostics and the new PDF's identity. Check unresolved references and missing artifacts separately from whether the process returned zero.

### 9.4 Structured numeric results

Define a small `research-results-v1` JSON/CSV interchange contract for explicitly exported estimates:

```text
result_id, estimand, specification_id, sample_id,
estimate, standard_error?, confidence_interval?, n?,
units, transformation?, uncertainty_method?,
source_execution_id, artifact_locator
```

Specification/sample IDs reference recorded descriptions; they cannot be free-floating labels that falsely imply matching samples. Store finite numeric values and precision deliberately. Comparisons require compatible units, transformation, estimand, and sample/specification identity, or an explicit comparison rationale. Tolerances are configured and recorded per check.

Offer lightweight language-specific export examples, starting with the toolchains actually evaluated. Do not promise reliable universal parsing of Stata, R, Python, MATLAB, and Julia output in the first release.

## 10. Frontend behavior

Add **Workspace** to the primary navigation as a peer of the existing Review/Pipeline surface. Inside it:

```text
Workspaces / chats       Active conversation           Optional inspector
------------------      ------------------------      ------------------
Workspace selector      Title, model, harness          Documents / artifacts
New conversation        Transcript and activity       Sources
Conversation list       Approval / question cards     Evidence
Harness settings        Composer + attachments        Results / notes
```

The inspector is lazy, resizable, and collapsible. Do not render it for a plain conversation with no active research module. Avoid a dense grid of empty panels. On smaller windows show one inspector tab at a time. Reuse neutral theme, Markdown/math rendering, keyboard, and panel-width components without importing Review page state.

Required flows:

1. **Start or organize:** begin an unfiled conversation immediately, or create/open a Workspace and optionally register a folder. No Review project is created or required.
2. **Connect:** detect Codex, explain unsupported/missing versions, and sign in through the browser. Browsing saved workspaces and transcripts works while signed out.
3. **Start conversation:** choose a model and harness. Plain conversation requires no paper, preset, tool, folder, or extra launch confirmation.
4. **Work:** display commentary/final messages, command progress, file diffs, tool cards, compaction, errors, and pending questions. Render provider-readable summaries where available; do not synthesize hidden reasoning.
5. **Inspect evidence:** selecting a claim opens the exact source passage/result and assessment history. “Stale” links say which dependency changed.
6. **Resume:** restore the session draft and display history first; reconcile before accepting another submission.
7. **Save handoff:** review proposed durable notes and start a fresh session with selected context.

Use safe Markdown/HTML rendering for all model/source text. Artifacts are opened through validated Tauri commands by object ID, not arbitrary `file://` URLs. Local links resolve against registered roots and immutable snapshots. Approval controls must be keyboard accessible, remain reachable during streaming, and resist double submission.

## 11. Implementation steps

Each step should be a reviewable change with its own acceptance evidence. Implement in dependency order. A step is complete only when its behavior works, its tests pass, and `CLAUDE.md` describes the implemented portion. Do not mark a placeholder button or a mock-only feature as complete.

Resulting implementation layout:

```text
gui/src-tauri/src/workbench.rs
gui/src-tauri/src/workbench/
  commands.rs             typed, bounded Tauri facade and store concurrency gate
  store.rs                app-owned records, migrations, queries, backup, event projection
  migrations/             five versioned SQLite schema migrations
  codex/                  process, transport, wire DTOs, events, compatibility
  research.rs             harness, context, documents, tools, and evidence facade
  research/execution.rs   execution profiles, adapters, receipts, and result validation
  research/tests.rs       cross-cutting research tests
  release.rs              recipes, evaluations, performance budgets, and Review handoffs
  release/archive.rs      validated portable archive export/restore
  release/tests.rs        cross-cutting release tests
  store/tests.rs          storage and migration tests

gui/src/components/WorkspacePage.tsx
gui/src/components/WorkspaceConnectionSettings.tsx
gui/src/components/WorkspaceResearchPanel.tsx
gui/src/components/WorkspaceRecipesPanel.tsx
gui/src/components/WorkspaceReleasePanel.tsx
gui/src/lib/workbenchTypes.ts
gui/src/lib/workbenchClient.ts
docs/workbench/protocol/...
docs/workbench/research-evaluation-v1.json
docs/workbench/release-qualification.md
tests/fixtures/workbench/...
```

The larger research facade may be split further along harness, documents, and evidence boundaries as those APIs stabilize. Keep credentials and raw provider wire payloads out of frontend DTOs.

### WB-00 — Qualify the integration contract

**Depends on:** nothing.  
**Deliverable:** protocol fixtures, compatibility manifest, and a bounded integration probe.

1. Read the existing discovery, Codex wrapper, process, safety, and lifecycle code. Identify reusable executable resolution and process cleanup helpers.
2. Select an official CLI build for qualification; generate stable and experimental JSON Schema bundles and record exact version/hashes.
3. Write a capability matrix for authentication, model listing, instructions, permissions, dynamic tools, command execution, history, and shutdown on each supported platform.
4. Implement a temporary/focused probe that initializes correctly, reads capabilities, and exercises a dummy tool and harmless fixture-folder session in an isolated runtime namespace. Keep real login/model checks opt-in.
5. Verify read denial outside approved roots, write denial outside the selected scope, and separate credential/configuration storage. Check initialization before instructions/hooks can execute.
6. Record wire field/enum mappings and unsupported operations. Choose the tested version range before implementation agents depend on exact protocol names.

**Acceptance:** a checked-in compatibility record distinguishes documentation, generated schemas, and observed behavior. The selected build passes the required scope tests. There is a concrete resolution for the local version mismatch; no invented API is left as an implicit assumption.

### WB-01 — Add the research store and Workspace records

**Depends on:** WB-00.  
**Deliverable:** versioned database, workspace-folder registration, basic conversations.

1. Add the SQLite dependency and initial migrations for workspaces, sessions/bindings, turns/items, settings/context snapshots, and change log.
2. Implement transactions, private directory creation, bounded reads, migration backup, and unsupported-newer-schema handling.
3. Use app-owned Workspace IDs. Add optional root registration/relocation and missing-root reconciliation without consulting Review project metadata.
4. Add typed workspace/conversation CRUD commands, including rename/archive and draft persistence. Permit unfiled conversations. No provider call is needed for these operations.
5. Introduce app-owned object IDs and event sequence numbers. Establish DTO tests between Rust and TypeScript.

**Acceptance:** independent workspaces and unfiled conversations survive restart and can be archived/restored without changing source files or any Review project/run. Failed migration restores the prior usable store.

### WB-02 — Implement the persistent Codex supervisor

**Depends on:** WB-00 and WB-01.  
**Deliverable:** long-lived transport and typed event normalization.

1. Implement owned process lifecycle, isolated child environment, handshake, separate stdout/stderr handling, serialized writes, and request correlation.
2. Add connection epochs, request deadlines, frame/output caps, backpressure handling, and explicit unknown-notification behavior.
3. Normalize thread/turn/item events into Workspace projections; keep versioned wire parsing inside `codex/`.
4. Implement start/read/resume/interrupt and graceful shutdown. Reject unsupported configuration rather than omitting it silently.
5. Add a protocol simulator supporting delayed/interleaved replies, requests, malformed frames, process exit, and reconnect.

**Acceptance:** fixture transcripts stream without blocking during a pending server request; completed items reconcile correctly; cancellation reaps descendants; a normal Review/Pipeline cancellation does not terminate the Workspace server.

### WB-02A — Correct the standalone Workspace boundary

**Depends on:** WB-01 and WB-02.  
**Deliverable:** app-owned Workspace identity and explicit separation from Review before UI work continues.

1. Add a migration from the transitional `project_workspaces` shape to app-owned `workspaces`; preserve sessions, bindings, turns, drafts, sequences, and registered roots.
2. Remove service-time lookups of Review project JSON from Workspace CRUD and reconciliation. A missing folder is a Workspace root state, not an orphaned Review extension.
3. Support unfiled conversations and workspaces with no registered folder.
4. Define narrow neutral interfaces for any reused renderer, document, process, and filesystem services. Workspace modules must not import Review orchestration or `usePipeline` state.
5. Add independence tests: deleting/renaming/archiving on either side does not affect the other; Review cancellation and provider settings do not alter Workspace; Workspace works with no Review projects present.

**Acceptance:** a fresh install can use Workspace without creating Review state, and an existing transitional database migrates without transcript loss. No Workspace foreign or logical key points at Review project storage.

### WB-03 — Add ChatGPT connection and model settings

**Depends on:** WB-02A.  
**Deliverable:** usable Settings connection panel and scoped model discovery.

1. Implement account state and managed browser login, cancellation, failure, and logout through the server. Add device-code login only if qualified and useful.
2. Open the auth URL with the system browser; do not put credentials in webview storage or logs.
3. Discover all model pages from this server and adapt them to existing `ModelSelection` semantics. Refresh on account changes.
4. Validate exact models and supported efforts; show unavailable pinned choices without substituting another model.
5. Show quota data when provided, with correct used/remaining labeling. Do not estimate a subscription bill from API list prices.
6. Detect auth expiration and usage exhaustion, preserve drafts, and provide a resumable error state. Do not fall back to API keys or another provider.

**Acceptance:** login/cancel/logout/account-change flows work against the simulator and one opt-in live account. Workspace auth does not alter existing CLI or Review/Pipeline auth. Model selection is bound to the Workspace account and runtime namespace.

### WB-04 — Deliver persistent sessions and interactive requests

**Depends on:** WB-01 through WB-03.  
**Deliverable:** Workspace route with a functioning standalone conversation client.

1. Add navigation, optional workspace selector, conversation list, transcript, composer, global activity indicator, and safe Markdown/math rendering.
2. Implement the global Workspace queue and immutable send snapshot. Persist before transport submission.
3. Render command/file/permission approvals and model questions. Correlate and invalidate them by connection/thread/turn/request identity.
4. Add Stop, navigation persistence, history hydration, crash reconciliation, and ambiguous-submission UI.
5. Add archived session browsing and read-only transcript export. Gate fork/steer controls on verified support.

**Acceptance:** send → streamed work → approval/question → completion → restart → resume works end to end. Repeated clicks cannot duplicate a turn or approval. A lost send acknowledgement does not trigger an automatic duplicate. Active work remains controllable from another page.

**Checkpoint A:** usable standalone ChatGPT client. It must pass without a paper, folder, research preset, research tool, or Review object.

### WB-05 — Make the research setup configurable

**Depends on:** WB-04.  
**Deliverable:** modular harness configuration, context preview, effective settings, and working access modes.

1. Add defaulted global settings and workspace/conversation overrides with reset-to-inherited behavior.
2. Implement a versioned harness registry for instruction packs, context providers, tools, inspectors, and recipes. Modules declare dependencies and capabilities; disabled modules do no work.
3. Implement plain conversation plus the shipped research instruction presets as editable, clonable data, preserving Codex's base instructions.
4. Resolve and snapshot module order, settings, instructions, selected tools, actual permission scope, and bounded context before every turn.
5. Implement Inspect/Edit workspace modes, web-search/network controls, native instruction-source display, and trust/configuration diagnostics.
6. Test settings persistence through native resume. Changes requiring a new binding generate a visible successor conversation with a bounded handoff.
7. Add initial manually curated research notes and context selection; model proposal workflows arrive in WB-09.

**Acceptance:** a user can customize a research harness entirely in the app, inspect what a turn will use, and return to it after restart. An unavailable permission capability disables the affected action. Unrelated native Codex settings remain untouched.

### WB-06 — Add papers, revisions, and source inspection

**Depends on:** WB-05.  
**Deliverable:** paper registration/versioning and a reusable evidence inspector.

1. Add paper, revision, source-version, and artifact tables. Support PDF, TeX project, DOCX, and an explicit source-tree attachment using existing contracts.
2. Extract a neutral document-preparation library from current code if reuse is worthwhile. A Workspace import must not enter Review commands, run storage, orientation, or referee workflows. Any model-based extraction uses Workspace's explicit ChatGPT/Codex runtime context; it cannot inherit a Review provider or fallback from Review settings. Deterministic and managed local extraction remain available.
3. Snapshot selected inputs, reuse compatible extraction caches, preserve extraction settings and quality notes, and store dependency/hash manifests.
4. Implement bounded text search and exact node/page/asset reads. Reuse page navigation and math rendering.
5. Detect changed source identities on selection/resume and via bounded refresh. Create new revisions explicitly; preserve old versions and locators.
6. Add local PDF/BibTeX source import and duplicate-candidate handling without automatically merging different editions.

**Acceptance:** selecting a claim-ready paper passage opens its exact revision and page/source span. Editing an included TeX file changes the revision identity. An extraction failure remains visible and does not silently switch extractor or pretend that missing pages were read.

### WB-07 — Expose research tools to Codex

**Depends on:** WB-05 and WB-06.  
**Deliverable:** versioned dynamic-tool registry and document/research read tools.

1. Add the experimental opt-in and exact dynamic-tool declaration/response shapes qualified in WB-00.
2. Implement optional workspace context, paper read/search/assets, source registration proposals, and Workspace execution lookup through Rust services.
3. Validate tool schemas and bind IDs/paths to the actual caller's workspace. Apply per-tool time, byte, and call budgets.
4. Support cancellation, unknown/removed tools, handler errors, and persisted catalog compatibility on resume.
5. Record tool receipts outside model write roots, and return structured, bounded content with stable locators.

**Acceptance:** a real or simulated Codex turn can retrieve the selected paper section and its image, while cross-workspace access, traversal, oversized results, and unsupported tool names fail predictably. A pending tool does not block the protocol reader or Stop.

### WB-08 — Record computational research runs

**Depends on:** WB-07.  
**Deliverable:** execution profiles, LaTeX/Stata adapters, receipts, and numeric comparison.

1. Add execution-profile settings with argv/environment validation and a previewed test action.
2. Implement the sandboxed `command/exec` path with explicit policies, output capture, timeout, cancellation, and independent process ownership.
3. Add the machine-specific `oldstata` launcher policy and scoped host-execution route where required. Test wrapping and cleanup without invoking a direct Stata binary.
4. Capture input/output manifests and host-owned execution receipts. Implement LaTeX build diagnostics and Stata log/expected-output validation.
5. Define and validate `research-results-v1`; add deterministic comparisons and a small export example for the evaluated toolchain.
6. Add a Results inspector showing commands, input completeness, outputs, failures, and comparison limitations.

**Acceptance:** a tiny paper compiles into a linked immutable PDF; a fixture computation yields a recorded estimate; a changed input produces a different execution dependency identity; timeout/error/cancel are never labeled success. Stata testing follows this machine's wrapper, including graceful cleanup.

### WB-09 — Add claims, evidence, freshness, and handoffs

**Depends on:** WB-06 through WB-08.  
**Deliverable:** an actionable research ledger.

1. Add versioned claims, evidence links, verification records, and model-proposed notes.
2. Implement propose/accept/reject/edit flows with origin and assessment identity; distinguish user confirmation from model assessment and executable checks.
3. Validate source/result locators and propagate stale/unknown status through recorded dependencies.
4. Add the useful evidence and memory queries from section 6.3 and an inspector that opens underlying evidence.
5. Implement reviewed handoff notes and deterministic fresh-session context assembly.
6. Add source identity/access/support states and web/local acquisition provenance. Preserve partial access and unresolved citations.

**Acceptance:** changing an estimate artifact makes its linked manuscript claim stale; a recheck creates a new receipt. A fabricated tool request cannot create a human-confirmed claim or successful numeric check. A new session can explain the current question and unresolved issues using accepted Workspace records.

**Checkpoint B:** academic research beta, including actual research objects and computations.

### WB-10 — Optional explicit bridge to Review (deferred)

**Depends on:** WB-09 and an explicit product decision to build the bridge. It is not required by WB-11, WB-12, or the first Workspace release.  
**Deliverable:** a narrow, optional handoff between otherwise independent products.

1. Define an explicit versioned handoff object containing an immutable document/artifact reference and user-selected metadata. Do not expose either product's internal database to the other.
2. If a “Review this revision” action is approved, open the existing Review launch preview with copied/staged input. Review resolves its own workflow, models, providers, authentication, scheduling, and cancellation exactly as it does today.
3. Do not carry Workspace credentials, Codex home, model choice, tool catalog, or permissions into Review. Do not pause Workspace globally merely because Review is running; enforce only actual shared-resource constraints.
4. Return only an explicit external reference or user-selected exported artifact to Workspace. Do not automatically ingest findings, reports, project membership, or provider state.
5. Make handoff and linking idempotent. A failed Workspace write must not rerun a completed Review workflow.

**Acceptance:** either product remains fully usable when the bridge is disabled. When enabled, an explicit handoff opens the normal Review preview without changing Review configuration or Workspace runtime state, and cancellation remains scoped.

### WB-11 — Ship research recipes and measure value

**Depends on:** WB-09.  
**Deliverable:** a small modular recipe picker, performance budgets, and research evaluation fixtures.

1. Add recipes as optional versioned harness modules: instructions, required inputs/tools, suggested permission mode, expected records/checks. They are not another scheduler and never block plain conversation.
2. Ship four complete recipes: manuscript consistency, empirical-result audit, theory audit, and literature/precedent review. Add paper revision as a fifth only if its compile/inspection flow passes.
3. Implement required-input checks and allow the user to edit recipes through existing Workspace configuration forms.
4. For each recipe, show a structured completion card: artifacts produced, checks actually run, unresolved issues, and missing evidence. Do not force all chat replies into a JSON artifact envelope.
5. Assemble a small initial evaluation set, then expand to roughly 20 representative tasks. Record fixture versions, expected checks, model/settings, outcomes, latency, and token usage where available.
6. Compare with the Workspace plain-conversation preset and an ordinary Codex session given the same files and model settings. Evaluate research correctness and evidence traceability, not interface polish or verbosity.
7. Set and test performance budgets for cold route load, warm conversation open, send-to-first-visible-state, streaming update frequency, large-transcript scrolling, context assembly, and idle resource use. Lazy-load document/evidence/computation modules and render long transcripts through a bounded window or virtualization.

**Acceptance:** each shipped recipe completes at least one substantive end-to-end fixture. Plain conversation remains fast with every research module disabled. Failing/unavailable checks remain visible, and measured performance stays within recorded budgets.

### WB-12 — Qualify recovery, portability, and release

**Depends on:** WB-11.  
**Deliverable:** a recoverable first release with compatibility evidence.

1. Implement research-store backup/export/import with a consistent SQLite snapshot and referenced immutable blobs. Never copy a live WAL database as an ordinary file backup.
2. Export workspaces, conversation metadata, paper metadata, accepted/proposed notes, sources, claims/evidence, execution receipts, harness definitions, and transcript Markdown/JSON. Exclude credentials and private runtime internals.
3. Validate imported archives against traversal, symlinks, size limits, schema versions, missing blobs, and ID collisions. Ask the user to remap roots; do not run imported tools.
4. Explain that portable research data and readable transcripts do not guarantee resumable native Codex threads on another machine/account. Offer a new session from a reviewed handoff when the binding is unavailable.
5. Add retention/reference tracking so evidence-linked blobs are not garbage-collected. Optional external references may become unavailable; do not hide or silently recreate them.
6. Exercise crashes during send, approval, tool execution, artifact adoption, and database migration.
7. Run the repository's required checks and platform qualification, including safe permission failures on unsupported platforms. Update Help and `CLAUDE.md` to match actual behavior.

**Acceptance:** checkpoint C and the final acceptance scenario below pass. Remaining deferred items are listed as follow-up work rather than described as implemented.

## 12. Testing and completion gates

### Engineering tests

| Area | Tests that matter |
|---|---|
| Protocol | Interleaved requests/events, numeric/string IDs, duplicate notifications, final-item replacement, malformed/oversized frames, unknown item types, cleanup after EOF |
| Submissions | Lost acknowledgements, late events, restart while queued, double-click send, ambiguous completed turn; no automatic replay of mutations |
| Permissions | Read/write escape, symlink/reparse behavior, overlapping roots, unsupported policy, stale approvals, required organizational restrictions |
| Tools | Cross-workspace IDs, invalid paths, output limits, cancellation, simultaneous nested `command/exec`, catalog changes on resume, forged success status |
| Storage | Transitional-schema migration, independent Workspace lifecycle, schema rollback, atomic mutations, missing-root recovery, retained evidence blobs, corrupt/incomplete export |
| Research | Source-version mismatch, stale manuscript locator, unsupported abstract-only claim, incompatible units, Stata log failure despite exit zero, missing compiled PDF |
| Frontend | Stream responsiveness, keyboard-accessible approval/question UI, navigation during work, unresolved-state rendering, context/setting precedence |
| Integration | Isolated auth, workspace-local instruction trust, model discovery, no provider fallback, unchanged Review behavior/settings/storage, scoped cancellation, no-Review-installed-state startup |
| Performance | Lazy route/runtime/module startup, bounded streaming/coalescing, large-transcript virtualization, context assembly budgets, idle CPU/memory, cancellation responsiveness |

Keep deterministic tests independent of a paid account, network, Stata license, or installed TeX. Use a scripted App Server simulator and process fixtures. Maintain opt-in live tests for the qualified Codex build and local research software. A simulator does not replace those live qualification checks.

Use the toolchain pinned by `gui/.nvmrc` and `rust-toolchain.toml`. Relevant existing commands:

```bash
# From gui/
npm test
npm run build
npm run test:release

# From gui/src-tauri/
cargo test --locked --all-targets
```

Run targeted tests during a step; run the required full checks before the completed feature is handed off. Git commands on this machine must use the authorized zsh path outside the sandbox. For GUI validation use `cd gui && npm run tauri dev`; never open the installed Pipeline application under `/Applications/`.

### Research evaluation cases

Include at least these cases across the evaluation set:

- A conclusion cites an obsolete coefficient after a table is regenerated.
- A paper's empirical prose uses a different sample or outcome unit from its result export.
- A citation exists, but the accessible source does not support the claimed mechanism.
- A literature novelty statement exceeds the documented search coverage.
- An appendix already resolves an apparent objection in the main text.
- A theory derivation omits a budget/resource term or fails a limiting-case check.
- A numerical example passes, but does not establish a general proposition.
- A Stata run fails in its log or lacks its expected output despite a superficially successful process outcome.
- A LaTeX edit compiles but introduces unresolved references or a missing figure.
- A fresh session must recover accepted decisions while distinguishing rejected suggestions.

Do not present a percentage improvement without a recorded baseline, comparable configuration, and a stated scoring rule. Have a researcher assess judgment-heavy outputs; deterministic checks alone cannot evaluate economic interpretation.

### Final acceptance scenario

Starting with no Review project and, optionally, an ordinary research folder, a user can:

1. Open Workspace, sign in with ChatGPT, and begin a plain conversation without configuring a workspace or research feature.
2. Create an independent Workspace, optionally register the folder, choose a model/harness, edit modules and access settings in the app, and inspect the effective setup.
3. Register a paper revision and start a conversation that reads an exact passage and page image.
4. Resume that conversation after restarting the app, with history, drafts, and correct settings.
5. Compile a small paper and run a configured computation, obtaining host-recorded receipts and artifact identities.
6. Link a claim to a result and a primary source, seeing the assessment method and any unresolved evidence.
7. Change a relevant input and see the old evidence become stale without losing its earlier version.
8. Disable the research harness and start another plain conversation without loading document, evidence, computation, or Review services.
9. Start a fresh conversation from accepted Workspace notes and selected evidence.
10. Export and restore Workspace records without leaking credentials, modifying Review data, or pretending an unavailable Codex thread can be resumed.

All ten are required for the first release described by this plan. The optional WB-10 Review bridge is explicitly excluded from this acceptance scenario.

## 13. Implementation-agent handoff

At the start of an implementation task:

1. Read current `AGENTS.md`, `CLAUDE.md`, this plan, and the applicable protocol compatibility record.
2. Identify the next incomplete WB step and its dependencies. Inspect the actual working tree before assuming prior steps are present.
3. State the bounded step being implemented. Preserve unrelated user changes.
4. Implement the smallest complete vertical slice for that step, including real persistence and failure behavior.
5. Run its focused checks, update `CLAUDE.md`, and record the outcome below or in a linked implementation log.

At handoff, report: completed WB step(s), files changed, checks run with outcomes, exact protocol/runtime version tested, known limitations, and the next step. Distinguish mocked validation from live validation. Do not silently expand into Claude support, import Review concepts, or rewrite the existing workflow runner.

If a verified API differs from this plan, update the compatibility record and the affected plan section with evidence before proceeding. Preserve the user-visible behavior and trust boundary; never make a weaker implementation appear equivalent by changing a label.

Current progress:

| Steps | State |
|---|---|
| WB-00 | In progress: 0.147.0 protocol floor/reference schemas, capability-based compatibility policy, fixtures, isolated no-login probe, account-independent thread start/read/resume, complete standalone command control checks, clean stdio shutdown with active-command cleanup, and macOS command-level least-privilege scope checks are implemented. The full no-model probe passes on 0.153.4; authenticated native-tool/login and cross-platform gates remain |
| WB-01 | Implemented initial storage slice: SQLite migration 1, private layout and migration backup/rollback, session persistence, optimistic revisions and operation IDs, monotonic sequences, bounded blocking Tauri commands, and shared Rust/TypeScript DTO fixtures. Its original Review-project-keyed identity has now been corrected by WB-02A |
| WB-02 | Implemented backend slice: minimum-version plus live-contract-gated persistent supervisor, canonical native launcher, isolated process/environment ownership, bounded bidirectional JSONL transport, concurrent request correlation and deadlines, connection epochs/reconnect manager, typed thread start/read/resume/turn/interrupt operations, fail-closed provider/root/permission/approval/model validation, normalized events and server requests, bounded lag reconciliation, durable turn/item projection, graceful and forced process-tree cleanup, and hostile scripted simulator coverage. WB-04 now exposes it through the Workspace route; authenticated live-turn qualification remains pending |
| WB-02A | Implemented: transactional schema migration 2 preserves workspaces/sessions/bindings/turns/items/change history while introducing app-owned workspace IDs/names, optional registered roots, rootless workspaces, unfiled conversations, root availability reconciliation, typed Rust/TypeScript v2 DTOs, and no service-time Review project lookup |
| WB-03 | Implemented against the pinned protocol and simulator: isolated managed ChatGPT account state, hardened browser login start/cancel/logout, account-triggered refresh, paginated visible-model discovery, exact model/effort validation without fallback, per-session durable selections, quota normalization and updates, typed Tauri/TypeScript DTOs, and a dedicated Settings panel. Opt-in authenticated live acceptance remains pending and is not simulated evidence |
| WB-04 | Implemented standalone client: Workspace navigation, rootless/unfiled and organized conversations, durable conversation hydration and idle-draft autosave, guarded global turn serialization and activity/attention indicators, pre-transport immutable draft persistence, ambiguous-send handling without retry, streamed messages, epoch/request/method-scoped approvals and questions, Stop, native resume plus `thread/read` crash reconciliation, cross-incarnation deduplication, cross-process-safe store initialization, safe Markdown/math, archived browsing, and message-only Markdown transcript export. Deterministic store/protocol tests pass; opt-in live end-to-end conversation acceptance remains pending |
| WB-05 | Implemented integrated slice: versioned modular presets, editable clones, global/Workspace/conversation inheritance, immutable per-turn config/context/tool/permission snapshots, inspect/edit and command-network profiles, effective-source diagnostics, native instruction-source capture, successor bindings, and reviewed notes/handoffs. Native web search is visibly unavailable pending a qualified per-thread protocol control |
| WB-06 | Implemented integrated slice: immutable PDF/DOCX/TeX/text and bounded source-tree revisions, deterministic extraction with visible failures, stable hash/byte/page locators, bounded search/read, PDF page-image rendering, local source provenance, and non-merging duplicate candidates |
| WB-07 | Implemented against the pinned 0.147.0 experimental schema and simulator: exact dynamic-tool shapes, binding-derived Workspace scope, bounded paper/read/page and proposal tools, tested execution lookup/run, 64-call turn budgets, exactly-once host-owned receipts, asynchronous protocol handling, and Stop-linked execution cancellation. Opt-in authenticated live dynamic-tool acceptance remains pending |
| WB-08 | Implemented integrated slice: validated profiles, independent bounded processes, timeout/cancel/output capture, LaTeX diagnostics, mandatory macOS `oldstata` wrapping, dependency/output manifests, immutable artifacts, `research-results-v1`, comparisons, and Results inspector. Real local LaTeX/Stata acceptance fixtures remain to be run on configured toolchains |
| WB-09 | Implemented integrated slice: claims/evidence/verification records, model proposal versus human confirmation boundaries, paper/execution staleness propagation, source access/provenance states, reviewed handoffs, fresh-session context, and Evidence/Memory inspectors |
| WB-10 | Implemented optional bridge slice: versioned/idempotent immutable revision staging, ordinary Review launch preview handoff, isolated configuration/runtime state, and optional idempotent external linking. Live Review-run qualification remains |
| WB-11 | Implemented product slice: four editable versioned recipes, required-input checks, per-turn recipe instructions, structured completion cards, comparable evaluation records, validated 20-case v1 fixture, recorded performance budgets, separately lazy research modules with tab-scoped data loading, 50 ms coalesced streaming, and a fixed 200-message transcript window with older/newer paging. Substantive three-variant live evaluation remains |
| WB-12 | Implemented recovery slice: exclusively gated consistent SQLite `.pwrx` snapshot export/restore, immutable blobs, structured JSON plus readable Markdown transcripts, safe preflight inspection, exact archive-entry allowlisting, hostile archive validation, empty-store collision policy, an explicit remap-or-detach decision for every archived root, nonportable-binding retirement, retained-blob references, Help/docs, and deterministic round-trip coverage. Packaged crash/platform/live-tool qualification remains |

## 14. Sources and verification notes

- [Codex App Server](https://learn.chatgpt.com/docs/app-server): fetched September 6, 2026; reference for transport, handshake, threads/turns/items, auth, approvals, dynamic tools, command execution, and experimental boundaries. Read the exact relevant sections again when qualifying a different binary.
- [Codex configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference): fetched September 6, 2026; reference for additive developer instructions, replacement instruction files, credentials, MCP configuration, memory/delegation settings, and search controls.
- [Codex sandbox and approvals](https://learn.chatgpt.com/docs/agent-approvals-security#sandbox-and-approvals): fetched September 6, 2026; reference for the distinction between technical execution boundaries and approval policy.
- Local generated protocol schemas from `codex-cli 0.147.0`, with and without `generate-json-schema --experimental`: inspected for this plan. They confirmed instruction fields and experimental dynamic-tool declarations; they are not a live compatibility certification.
- Repository sources listed in section 3, plus `gui/package.json` and `gui/src-tauri/Cargo.toml`: inspected for actual integration points and build commands.
- User-supplied `economics_ai_research_harness_report.md`: conceptual input, reviewed and narrowed in section 2. Its provider-policy discussion and future-model instructions are not carried forward as implementation requirements.

This document's schemas, storage choices, defaults, milestones, and release gates are proposed Workspace design decisions. They are not claims that OpenAI supplies those research features automatically.
