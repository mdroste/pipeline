# Workflow Codex App Server migration

Implementation status: default subscription connection, September 7, 2026;
live and packaged release qualification remains incomplete. This is the Codex
implementation of [MIGRATE_SDK.md](../MIGRATE_SDK.md). Claude continues to use
its existing CLI/API adapters. Existing installations migrate the former
legacy Codex default to App Server. Legacy CLI remains an explicit advanced
compatibility option.

## Using it

1. Install a compatible Codex executable. The checked protocol generation is
   0.147.0; the Workflow configuration and native sandbox probe have been
   exercised with 0.153.4 on macOS arm64. Older/newer installations must pass
   strict configuration and runtime contract checks; a minimum version check
   alone is not a compatibility guarantee.
2. In **Settings → Providers → OpenAI → Review & workflows**, choose
   **Subscription**. Codex App Server is the default connection.
3. Use **Sign in to ChatGPT**. Complete the native browser flow. Pipeline
   uses the [shared ChatGPT account](chatgpt-account.md) for both Conversations
   and Reviews. Account changes refresh the model catalogs.
4. Configure Workflow model/effort policies as before. Pinned models and
   efforts are checked against the signed-in account's live catalog. API mode
   continues to select the existing OpenAI API adapter.
5. The workflow editor's execution options include **Reviewer instructions**.
   This optional `system_prompt` field is literal trusted text, passed
   separately from the task. On App Server it becomes `developerInstructions`.
   Empty instructions preserve native defaults. This preview does not expose
   replacement of Codex's base instructions. Legacy Codex also forwards this
   field through its supported `developer_instructions` configuration key;
   its old reserved `instructions` key is no longer used.

The backend setting defaults to `codex_backend: "app_server"`. The persisted
`codex_backend_preference_version: 1` marker distinguishes a new deliberate
choice from the former default: old settings with a missing marker migrate
`legacy_cli` to `app_server` once. Later explicit legacy choices survive saves
and reloads. Existing `codex:cli` model/effort keys remain valid. API mode and
stored keys are preserved; changing the native backend never copies tokens.
Users who only signed in through the legacy CLI must sign in through Pipeline.

To use the retained CLI adapter, open **Advanced connection settings** in the
OpenAI subscription section and choose **Legacy Codex CLI**. That mode uses
the terminal's `codex login` account. Pipeline never falls back to it
automatically. Switching to API mode preserves the selected native backend
for a later return to Subscription.

Dependency checks show this managed connection as **Workflow ChatGPT** and
use its live account, pending sign-in, and unresolved-attempt state. A live
connection supersedes the standalone CLI version probe. The dependency badge
refreshes after connection state changes and whenever its dialog is reopened;
every provider required by the selected workflow must be ready, while unused
providers remain optional. The shared sign-in establishes authentication for both Conversations and
Reviews; the explicitly selected legacy CLI continues to use its terminal account. A connection held by another Pipeline process remains
unavailable and reports that conflict explicitly.

## Shared implementation and separate ownership

`gui/src-tauri/src/agent_runtime/codex/` contains shared JSONL framing, request
correlation, normalized events, protocol compatibility checks, process-tree
ownership, isolated command construction, account login/logout/status,
paginated model discovery, exact model/effort validation, and quota parsing.
Both owners now use `session.rs` for executable/version admission, startup,
handshake, isolated-home verification, diagnostics, and bounded shutdown.
`invocation.rs` defines their common typed thread-start and text-turn requests,
including provider/fallback defaults, model/effort selection, literal base and
supplemental instructions, dynamic tools, and optional output schemas. Each
owner supplies its own roots, approval policy, tools, and configuration, then
validates the returned contract. Workspace's supervisor retains its SQLite
projection and conversation lifecycle; Reviews retains its attempt journal and
result collection. The shared methods never retry a submission.

`gui/src-tauri/src/pipeline/codex_server/` owns the Workflow connection,
invocation lifecycle, host tools, and recovery journal. It uses no Workspace
database, harness state, or conversation binding. The shared account service
supplies temporary in-memory authentication to the execution runtime. Storage is:

```text
~/.pipeline/providers/workflows/
  runtime.lock         exclusive Pipeline-process ownership
  codex/               config and native thread history; in-memory authentication
  empty/               working directory with no research artifacts
  attempts/<id>.json   private per-invocation request/outcome receipts
```

Workspace retains `~/.pipeline/workbench/codex/`. The managed account is stored
under `~/.pipeline/providers/chatgpt/`; legacy CLI retains its own normal home. Unix directories/files use owner-only permissions. Windows
inherits the user profile's ACLs; Windows packaging and ACL qualification
remain release gates.

The Workflow process is lazy and application-lived, rather than run-lived as
initially proposed. This avoids competing login/discovery and execution
processes sharing one credential namespace. Each invocation still gets a
fresh native thread and exactly one turn submission. App Server receives no
research filesystem roots: all artifact authority is bound to each host tool
handler. Workspace and Workflow processes remain independently owned and
independently cancellable. Application exit shuts down the Workflow connection.

A shared account write lease lasts through browser login completion/cancellation,
not merely through `account/login/start`. Active calls hold read leases.
Sign-out/account changes cannot race an active Conversation or Review call. Login cancellation carries
both login ID and connection epoch. Another Pipeline process cannot open the
same managed Workflow connection concurrently.

## Invocation behavior

- The existing dispatcher selects App Server only for Codex subscription calls
  when the setting is enabled. Extraction, orientation, reviewers, sequential
  steps, and merge use this common route. Their scheduler, output envelopes,
  artifact validation, and provider fallback rules remain in Pipeline.
- Prepared shared context is expanded into a fresh thread's input. Native
  resume/fork caching is deliberately disabled and recorded as such.
- The generated permission profile grants native tools minimal system reads
  and launcher access, with no research roots, writes, or command networking.
  Native shell, image-viewing, image generation, subagents, plugin discovery,
  hooks, and ambient host-skill discovery are disabled by configuration.
  Unsupported approval/question requests receive errors. Web search is set
  per thread from the existing `WebSearch` tool declaration.
- Dynamic tools implement Pipeline's existing `Read`, batched text/image
  reads, document assets, and `Write` contract. `Edit` is represented by full
  replacement `Write`, consistent with the host artifact-writing contract.
  `ReadPdfPage` renders a bounded numbered PDF page through Poppler. `Read`
  on a PDF returns its first page plus pagination guidance. All paths go
  through `ToolAccess`; sibling producers and unselected artifacts are denied.
- Host writes complete atomically within one poll, so cancelling a call cannot
  leave a detached artifact write racing report ingestion. Reads/rendering use
  bounded workers. Duplicate dynamic call IDs reuse their result; reusing an
  ID with different arguments is rejected. There is no cross-process tool replay.
- Thread setup checks the reported provider, pinned model, canonical working
  directory, empty runtime roots/instruction sources, approval policy, and
  active read-only permission profile. Schema constraints use the same strict
  projection as the legacy Codex adapter. Pipeline still validates the result.
- Only an explicitly marked `final_answer` agent item becomes the response.
  Commentary is never used as the report. Identity checks cover thread and
  turn; event subscriptions are connection-scoped. Cumulative token usage
  replaces prior totals rather than adding repeated snapshots. The receipt
  records unavailable usage as null; the older aggregate UI retains its
  existing numeric representation.

The transport caps frames at 8 MiB, submission prompts at 2 MiB, serialized
dynamic results at 7 MiB, aggregate completed output at 50 MiB, and distinct
host calls at 64. Existing host-tool read, media, write, and artifact quotas
also apply. An oversized call fails explicitly; no automatic legacy-backend
fallback weakens its access policy.

## Recovery and retained data

An attempt receipt is written before `turn/start`, updated with native
thread/turn IDs, and finalized with output and usage. It includes the exact
expanded prompt, reviewer instructions and hashes, model/effort selection,
artifact root paths, runtime version, and schema metadata. These receipts and
native thread history can contain unpublished research; they are local private
data and are not automatically included in ordinary report exports.

A timeout, disconnect, or lost submission acknowledgement never automatically
repeats the turn. A cleanup owner outlives the dropped call: it interrupts the
exact turn and checks terminal notifications/history. If cleanup cannot confirm
termination it stops the Workflow connection. Sibling calls then report
uncertain outcomes; Workspace continues independently.

On a new connection, unfinished receipts block new Workflow model calls until
the user reviews them under the connection settings. **Check saved result**
uses `thread/read` for the recorded identity and stores the reconciliation.
It does not create a new turn. Missing acknowledgements are not guessed from
other turns. **Acknowledge and allow a new run** records explicit abandonment;
the user may then launch another run. It does not erase past usage or imply
that the earlier provider request never ran.

Receipts are retained until manually archived. Startup rejects archives above
4,096 records or 1 GiB, and bounded readers reject records above 64 MiB. There
is no automatic deletion of unresolved records. Native history has its own
runtime-managed storage; automated retention is deferred. Completed-result
reconciliation is inspectable but does not yet splice recovered output into a
previously failed Pipeline run.

## Validation and remaining release gates

Implementation checks on September 6: `cargo test --locked --all-targets`
passed 746 library tests and 14 CLI tests (four live-tool tests ignored);
`cargo clippy --locked --all-targets -- -D warnings` passed; `npm test` passed
428 tests; `npm run build` passed. The global cancellation regression test now
runs in its own test process so parallel dependency probes cannot be killed by
its intentional call to `kill_all_children`. A repository-wide formatting check
also surfaced an unrelated in-progress edit in
`workbench/release/archive.rs`; migration-owned Rust files were formatted.

Deterministic tests cover invocation routing primitives, separate tool
authority, literal instructions and workflow fingerprints, final-answer
selection, cumulative usage, duplicate tool responses, disconnects,
cancellation cleanup, restart acknowledgement, settings compatibility, and
the Workflow sign-in UI. Shared transport/Workspace regression tests must run
whenever shared primitives change.

The development-only probe is:

```bash
cd gui/src-tauri
cargo run --locked --bin workflow_codex_probe
```

It creates a temporary isolated home, verifies signed-out account state,
starts a thread with dynamic tools and the real permission profile, injects a
harmless history item, checks `thread/read`, and exercises a successful
sandbox baseline followed by denied private reads and native writes. It never
logs in or submits a model turn. On macOS this probe must be allowed to create
its own sandbox; a parent sandbox can cause `sandbox_apply: Operation not
permitted` before the baseline command runs.

[The recorded macOS result](workflow-codex-probe-0.153.4-macos-arm64.json)
passed on September 6, 2026. During qualification the installed runtime rejected
`tools.view_image`; the versioned [upstream configuration schema](https://raw.githubusercontent.com/openai/codex/rust-v0.153.4/codex-rs/core/config.schema.json)
places that toggle under `features.view_image`. Strict startup exposed this
documentation/runtime mismatch rather than silently ignoring it.

The September 7 default change does not qualify the remaining live release
gates: browser OAuth completion and
cancellation, account switching, real model/effort choices, actual dynamic-tool
invocation and images/PDFs, reviewer/schema outputs, enabled/disabled web search,
token/quota limits, concurrent reviewers, cancellation during native work,
restart recovery, and packaged macOS/Windows/Linux behavior. Run Workspace
alongside Reviews to verify the shared account and separate cancellation ownership. These live
gates are **not** satisfied by simulator tests or this no-model probe. Claude
SDK migration, native context reuse, instruction replacement modes, automatic
history retention, and automatic recovery into old run artifacts remain outside
this Codex preview.

## Shared invocation consolidation — September 15, 2026

Feasible without merging runtime ownership: both default subscription routes
already used Codex App Server, while Workspace duplicated native startup and
both owners assembled thread/turn payloads independently. They now call the
same native-session launcher and typed submission methods. Workspace side turns
also use this path. Review thread creation now supplies its selected reasoning
effort at setup as well as turn submission. Review schemas and web-search
configuration remain scoped to each call. Public commands, saved backend
preferences, provider selection, credentials, and storage paths are unchanged.
Explicit legacy CLI and direct API choices retain their existing dispatch.

Both launchers use the exact resolved executable used to construct their native
permission configuration. A shared existing-path comparison rejects unresolved
paths, including the former Review edge case where two failed canonicalizations
could compare equal. This check also protects the reported credential namespace.

Validation on macOS arm64 with Codex 0.153.4:

- The focused Codex suite passed 54 tests; the full Rust suite passed 947 library
  tests and 14 CLI tests, with 10 opt-in tests ignored.
- Both `workflow_codex_probe` and `workbench_probe` passed with temporary
  signed-out homes and no model turns. These exercised the reported thread
  contract, dynamic-tool declarations, native read/write denial, command
  control, and process cleanup. The opt-in production Workspace supervisor
  launch/shutdown test also passed through the shared `NativeSession`.
- All-target/all-feature Clippy with warnings denied, Rust formatting, the
  frontend build, and the source-size gate passed.
- The first frontend suite run passed 715 tests and timed out in the existing
  large-report search test. That entire 25-test file passed in isolation; a
  subsequent full run passed all 716 frontend tests.
- The Markdown link checker reports missing historical `/tmp` audit artifacts
  in `ASTRA_SEP7_BUGREPORT.md`; none concern the new documentation links.

These checks do not qualify authenticated model turns, browser sign-in, or
packaged/cross-platform operation. The subsequent [shared-account change](chatgpt-account.md) supersedes this
stage’s separate sign-ins while preserving execution ownership.
