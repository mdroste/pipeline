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
   checks account state through App Server; it never reads or copies OAuth
   tokens. Account changes refresh the Workflow model catalog.
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
providers remain optional. Workspace and legacy CLI sign-ins do not establish
Workflow authentication. A connection held by another Pipeline process remains
unavailable and reports that conflict explicitly.

## Shared implementation and separate ownership

`gui/src-tauri/src/agent_runtime/codex/` contains shared JSONL framing, request
correlation, normalized events, protocol compatibility checks, process-tree
ownership, isolated command construction, account login/logout/status,
paginated model discovery, exact model/effort validation, and quota parsing.
Workspace's supervisor delegates to these primitives while retaining its
SQLite projection and conversation lifecycle.

`gui/src-tauri/src/pipeline/codex_server/` owns the Workflow connection,
invocation lifecycle, host tools, and recovery journal. It uses no Workspace
database, harness state, conversation binding, or credentials. Storage is:

```text
~/.pipeline/providers/workflows/
  runtime.lock         exclusive Pipeline-process ownership
  codex/               native credentials, config, and native thread history
  empty/               working directory with no research artifacts
  attempts/<id>.json   private per-invocation request/outcome receipts
```

Workspace retains `~/.pipeline/workbench/codex/`; legacy CLI retains its own
normal Codex home. Unix directories/files use owner-only permissions. Windows
inherits the user profile's ACLs; Windows packaging and ACL qualification
remain release gates.

The Workflow process is lazy and application-lived, rather than run-lived as
initially proposed. This avoids competing login/discovery and execution
processes sharing one credential namespace. Each invocation still gets a
fresh native thread and exactly one turn submission. App Server receives no
research filesystem roots: all artifact authority is bound to each host tool
handler. Workspace and Workflow processes remain independently owned and
independently cancellable. Application exit shuts down the Workflow connection.

An account write lease lasts through browser login completion/cancellation,
not merely through `account/login/start`. Active calls hold read leases.
Sign-out/account changes cannot race an active call. Login cancellation carries
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
alongside Workflows to verify account and cancellation separation. These live
gates are **not** satisfied by simulator tests or this no-model probe. Claude
SDK migration, native context reuse, instruction replacement modes, automatic
history retention, and automatic recovery into old run artifacts remain outside
this Codex preview.
