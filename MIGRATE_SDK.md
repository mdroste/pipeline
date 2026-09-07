# Migrate Workflow provider execution to App Server and Agent SDK

> Codex implementation update (September 6, 2026): the opt-in App Server preview
> is implemented. See [the implementation and qualification record](docs/workflow-codex.md)
> for shipped behavior, deliberate scope adjustments, test evidence, and remaining
> live release gates. The staged proposal below is retained as the design record;
> it is not a claim that every later stage or the Claude SDK migration is complete.


Assessment date: September 6, 2026 (America/Los_Angeles).
Status: implementation proposal; no runtime migration or authenticated qualification
was performed for this assessment.

## 1. Decision

**The migration is technically feasible and worth doing, with an asymmetric
authentication qualification for Claude.** Use the Codex App Server for Workflow
ChatGPT execution and the official Claude Agent SDK for Claude execution. Share
provider integration code with Workspace, while retaining the deterministic
Workflow scheduler and the separate Workspace conversation service.

This is a migration of the non-API provider boundary for all Workflows, including
Paper Review. It is not a conversion of reviews into Workspace recipes, a move
of the DAG into provider-managed subagents, or a replacement of the direct API
implementations. Workspace presently has a Codex integration only; there is no
existing Workspace Claude SDK implementation to extract.

Recommended outcome:

| Area | Decision |
|---|---|
| ChatGPT Workflow execution | Replace `codex exec` with typed App Server thread/turn operations. |
| Claude Workflow execution | Implement an official TypeScript Agent SDK sidecar, with its own packaged runtime. |
| Authentication | Let each native provider own authentication and refresh. Pipeline handles connection selection, status, and lifecycle. |
| Shared infrastructure | Share transport, process ownership utilities, account/model DTOs, capability validation, and instruction construction. |
| Domain state | Keep Workflow runs/artifacts and Workspace conversations/research stores independent. |
| Single sign-in across modes | Defer. Shared code and a consistent settings UI do not require shared credential storage. |
| Reviewer prompts | Add explicit higher-priority reviewer instructions; keep task text and manuscript content separate. |
| Direct API/local providers | Preserve their current transports, credentials, tools, and provider choices. |
| Claude in Workspace | A possible later feature, not a prerequisite or an automatic consequence of this migration. |

Codex exposes the authentication and rich-client lifecycle needed here. Claude
exposes the agent loop and prompt controls, but its SDK is not an App Server
equivalent with the same managed login RPCs. The integration should express this
capability difference rather than fabricate one common OAuth protocol.

### 1.1 Claude authentication: what the evidence establishes

The current SDK overview says third-party developers need prior approval to
offer Claude login or rate limits in their products. Separately, Anthropic's
June 15 update says its proposed SDK credit change was paused and that SDK,
`claude -p`, and third-party app usage continue to draw from subscription limits.
The latter establishes current billing behavior, not approval for Pipeline to
offer embedded sign-in. Do not describe the obsolete credit table below that
update as current pricing. [SDK overview](https://code.claude.com/docs/en/agent-sdk/overview),
[plan usage update](https://support.claude.com/en/articles/15036540-use-the-claude-agent-sdk-with-your-claude-plan).

Therefore:

1. Build the shared architecture and Codex migration independently of Claude
   login availability.
2. Treat use of a user's officially authenticated Claude runtime as a technical
   compatibility track. Confirm that Pipeline's intended distribution and use
   are supported before promoting this to a newly shipped SDK subscription path.
3. Do not add a Pipeline-owned Claude OAuth implementation, token extraction,
   credential copying, or a subscription-token-to-HTTP adapter.
4. Record Anthropic approval or updated applicable guidance in the release
   qualification record before enabling Pipeline-branded Claude sign-in. A
   successful local query is not evidence of that approval.
5. Keep the existing Claude transport available during development under its
   existing behavior; this plan does not certify its distribution status. If the
   subscription SDK track cannot be qualified, ship Codex and shared-code work
   separately. Claude direct API remains an explicitly selected alternative,
   never an automatic billing switch.

### 1.2 What this buys, and what it does not

The main gains are a single maintained Codex protocol implementation, consistent
account/model selection, native streaming and cancellation, less command-line
assembly, and explicit reviewer instructions. It also removes the current Codex
exec → App Server fork → exec-resume sequence from cached review calls.

The SDKs still run native provider processes. They do not turn Pipeline into a
runtime-free client, guarantee cheaper reviews, make model outputs deterministic,
or supply Pipeline's artifact schemas, dependency rules, evidence semantics, and
recovery policy. Those remain host responsibilities. Prompt replacement may
improve reviewer focus, but that requires an evaluation; it is not an assumed
quality gain.

## 2. Repository findings and entry points

Read `AGENTS.md`, `CLAUDE.md`, `workbench_plan.md`,
`docs/workbench/README.md`, `docs/workbench/protocol/compatibility-policy.md`,
`docs/workbench/protocol/compatibility-0.147.0.md`, and
`docs/workbench/release-qualification.md` before implementation.

| Current code | Relevant behavior and migration action |
|---|---|
| `gui/src-tauri/src/pipeline/call.rs` | Single public model-call boundary, owned Tokio task, timeout, usage attribution, configured exhaustion fallback. Preserve this boundary. |
| `pipeline/claude.rs::call_llm` | Internal dispatcher for every provider; also contains Claude CLI and shared process/path helpers. Separate routing from the Claude implementation. |
| `pipeline/claude.rs` | `claude -p`, appended system instructions, native JSON schema, scoped permissions, native session warm/fork. Replace execution only after SDK parity. |
| `pipeline/codex.rs` | `codex exec`, final-message file, strict schema projection, App Server fork helper, exec resume. Replace with one App Server adapter. |
| `pipeline/context_cache.rs` | Run-local immutable selected context, shared warm-up slots, provider compatibility keys. Preserve the semantic prefix; reimplement native caching separately. |
| `pipeline/executor.rs`, `orient.rs`, `merge.rs`, `reconcile.rs`, `extract/` | Reviewers and auxiliary LLM calls. Every path must continue through `call.rs`; changing only reviewers is incomplete. |
| `pipeline/structured.rs`, `response_journal.rs`, `logging.rs`, `provider_error.rs` | Host schema authority, accepted/rejected response evidence, usage, retry/fallback classification. Extend without bypassing. |
| `pipeline/api_common.rs::ToolAccess` | Existing exact per-call read/write authority and bounded document tools. Reuse host validation for SDK tools. |
| `model_catalog/discovery.rs` | Workflow Codex discovery already launches App Server; Claude discovery uses the CLI's SDK initialization control protocol, not the npm SDK package. Replace duplicate discovery with the selected execution connection. |
| `model_catalog/resolution.rs`, `settings.rs` | Automatic/role/pinned selection; `cli`/`api` override keys; independent access-mode and saved-key choices. Preserve durable semantics. |
| `commands/lifecycle.rs` | Run/pass PID registries and independent child registry. Shared servers need request cancellation, not pass-owned PID termination. |
| `workbench/codex/{wire,transport,process,compatibility,supervisor}.rs` | Reusable bounded JSONL protocol and native process mechanisms, currently mixed with Workspace policy and projection. Extract the mechanisms. |
| `workbench/commands.rs`, `store.rs`, `research.rs`, `release.rs` | Workspace-only ownership, global turn permit, persistence, dynamic tools, immutable Review handoff. Keep in Workspace. |
| `pipeline_config.rs`, `pipeline_config/`, `gui/src/lib/types.ts` | Workflow instruction fields, migration, portable import/export, validation, preview types. |
| `SettingsPage.tsx`, `WorkspaceConnectionSettings.tsx`, `AgentDefaultsControl.tsx`, `gui/src/lib/providers.ts` | Consistent connection/model controls, preserving mode-specific choices. |
| `RunPreview.tsx`, `pipeline-editor/`, `PromptEditor.tsx`, `ReportWorkspace.tsx`, `runProvenance.ts` | Instruction editor, effective launch details, result provenance. |
| `gui/src-tauri/src/bin/cli.rs`, `lib.rs` | Headless CLI and desktop initialization/shutdown must both use the new services. |
| `gui/src-tauri/tauri.conf.json`, `scripts/release/`, `.github/workflows/release.yml` | Sidecar discovery, packaging, signing, notices, and supported-platform qualification. |

Paths abbreviated in this table are relative to `gui/src-tauri/src/` for Rust
and `gui/src/components/` for component names unless otherwise stated.

### 2.1 Existing prompt support is incomplete

`OwnedRequest.system_prompt` already exists, but the normal borrowed step
`Request` conversion sets it to `None`. `StepConfig` has task `prompt`, not a
reviewer system-instruction field. The Claude CLI appends the supplied text to
its defaults. Codex currently emits `-c instructions=...`; current official
configuration documents `instructions` as reserved and recommends other
instruction mechanisms. Do not carry that mapping forward or assume it worked.
[Codex configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference).

### 2.2 Existing isolation must be verified, not inferred from comments

The old Codex wrapper explicitly notes that its sandbox restricts writes while
reads remain unrestricted. Its declared roots are therefore not evidence of
strict read isolation. Claude `allowedTools` is also not a complete available-tool
allowlist. The new implementation must enforce the intended artifact selector
boundary, not merely reproduce those command arguments.

Workspace's checked-in schema generation is `0.147.0`; newer binaries are
capability-qualified rather than exact-version-pinned. The repository records a
no-model macOS arm64 probe using `0.153.4`, not complete authenticated or
cross-platform qualification. Its supervisor currently embeds Workspace binding
creation, store projection, permission choices, and global manager state. Calling
that singleton directly from Workflows would violate existing architecture.

## 3. Target architecture and ownership

Introduce `gui/src-tauri/src/agent_runtime/` as a domain-neutral native-provider
integration module. The names below are proposed implementation names, not
existing APIs.

```text
Workflow call.rs                         Workspace commands.rs
  WorkflowRuntimeService                   Workspace service + projector
  run/pass/attempt identity                session/binding/turn identity
          |                                           |
          +--------- shared agent_runtime code -------+
                       |                |
                  codex/             claude/
                JSONL client       SDK bridge client
                       |                |
               owned App Server    owned Node sidecar
```

Suggested module layout:

```text
agent_runtime/
  mod.rs                 # app-owned public contracts
  types.rs               # connection, model, instruction, event, usage DTOs
  process.rs             # process groups/jobs, independent ownership, reaping
  capability.rs          # operation-level requirements and diagnostics
  codex/
    wire.rs              # provider DTOs stay private
    transport.rs         # extracted bounded correlation/epochs/backpressure
    client.rs            # typed auth/model/thread/turn operations; no stores
    config.rs            # explicit namespace/policy → strict native config
    compatibility.rs
    simulator.rs
  claude/
    bridge.rs            # versioned Rust ↔ sidecar messages
    process.rs
    compatibility.rs
pipeline/
  runtime.rs             # Workflow-only service and active attempt registry
  agent_tools.rs         # per-attempt tools backed by ToolAccess
  dispatch.rs            # provider choice; extracted from claude.rs
  call.rs                # remains the public supervised call boundary
workbench/codex/
  supervisor.rs          # thin Workspace policy/projector over shared client
  probe.rs               # existing public qualification entry remains usable
```

Move reusable code with compatibility re-exports first; avoid a second copy of
the transport. Move `configure_silent_command` and appropriate process utilities
out of the Claude Workflow module so Workspace's native process creation no
longer depends on a provider-specific Workflow wrapper. Keep raw protocol types
inside the shared provider modules; update the canonical documentation's old
`workbench/codex/`-only DTO rule to reflect that relocation.

### 3.1 Share implementation, keep runtime namespaces separate

Initial namespaces:

| Owner | Native state | App-owned durable state |
|---|---|---|
| Workspace Codex | Existing `~/.pipeline/workbench/codex/` | Existing Workspace database/blobs and binding records |
| Workflow Codex | New `~/.pipeline/providers/workflows/codex/` | Existing run manifests plus new private attempt metadata |
| Workflow Claude | Explicitly selected supported Claude auth context; isolated Pipeline session/config location where the runtime supports it | Existing run manifests plus private attempt metadata |

Do not copy `~/.codex/auth.json`, a Claude token, or Workspace's Codex home into
these locations. New managed Workflow Codex access requires a sign-in in its own
namespace. Keep a clearly labelled legacy Workflow CLI selection during rollout;
do not silently switch an existing signed-in user to a signed-out namespace.

For Claude, verify credential lookup, OS keychain behavior, config-directory
controls, and session persistence for the chosen SDK/runtime pair. A changed
config directory does not by itself prove credential isolation. If independent
managed authentication cannot be established, represent the connection honestly
as an external Claude installation, with the qualification restrictions in §1.1.
Do not implement credential import as a workaround.

Use one Workflow Codex process per active run, with multiple independent threads
under `max_workers`. An idle connection service may serve auth/discovery before a
run; transfer exclusive namespace ownership to the run service, or retain the
same process and lease it. Do not leave two independently configured owners
rewriting one home. Use a cross-process lock so the desktop and `pipeline-cli`
cannot concurrently rewrite the managed namespace. Surface a busy state rather
than trying to kill another app instance.

Claude begins with one bridge/native process tree per in-flight call. This fits
existing pass ownership and gives bounded failure isolation; optimize pooling
only after measuring startup costs and qualifying refresh/storage concurrency.

Workspace keeps its own process, cancellation domain, and one-active-turn permit.
Workflow parallelism must neither acquire that permit nor weaken it. Shared
upstream account quotas, when the same account is signed into both namespaces,
remain account-wide even though local processes are isolated.

### 3.2 Single sign-in is a separate optional phase

Do not make credential sharing a prerequisite for unifying adapters. An eventual
account service could own a connection explicitly selected by both modes, but
this changes the documented credentials boundary and requires its own design:
credential storage versus native thread storage, account switching during active
work, quota visibility, logout scope, and per-mode runtime ownership.

Neither symlinking homes nor copying refreshed token files is an acceptable
design. Shared-process sign-in would also make force-kill recovery cross-mode;
host-owned external tokens would move refresh responsibility into Pipeline.
Neither is needed for this migration. Leave the current immutable Review handoff
unchanged and record this single-sign-in phase as deferred.

## 4. Contracts to implement

Separate the durable provider/access selection from its implementation backend.
For the first migration, retain persisted `claude:cli`, `codex:cli`, and `*:api`
model/effort keys as compatibility identifiers for native versus direct access.
Add a backend field rather than globally replacing `cli` strings with `sdk`.

```rust
// Illustrative application contracts, not provider wire declarations.
enum RuntimeBackend { LegacyCli, CodexAppServer, ClaudeAgentSdk }
enum RuntimeOwner { Workflow { run_id: String }, Workspace }
enum InstructionMode { Append, ReplaceBase }
struct InstructionSpec { mode: InstructionMode, text: String }
struct AttemptIdentity {
    run_id: String,
    pass_key: String,
    unit_id: String,
    attempt_id: String,
}
struct AgentInvocation {
    identity: AttemptIdentity,
    connection_id: String,       // opaque, local; never a credential
    model: ResolvedModel,
    instructions: Option<InstructionSpec>,
    task: String,
    access: ToolAccessSnapshot,
    output_contract: OutputContract,
    deadline: Instant,
}
```

Add app-owned capability records, including managed login, external auth,
model/effort discovery, instruction append/replacement, structured output,
read/write scope enforcement, visual/PDF input, hosted search, interruption,
history reconciliation, and concurrent threads. Capabilities may be unavailable
with a reason; do not manufacture parity by dropping requested features.

Normalize events as `Started`, `TextDelta`, `ToolStarted`, `ToolCompleted`,
`UsageUpdated`, `ApprovalRequested`, `Completed`, and `Failed`, always with the
runtime epoch and invocation identity. Keep provider-specific detail bounded and
private. Return an explicit terminal payload, reported model, usage availability,
native thread/session and turn IDs, terminal status, and backend version.

Introduce typed provider errors before formatting for existing callers:

```text
AuthRequired / AuthUnsupported / ModelUnavailable / CapabilityUnavailable
TransientThrottle / AccountExhausted / InvalidRequest / ProviderFailure
PermissionDenied / InvalidOutput / Cancelled / TimedOut / OutcomeUnknown
```

`OutcomeUnknown` and permission/capability failures must bypass ordinary retries.
Account exhaustion alone can invoke the already configured one-time fallback,
within the original deadline, with separate provenance. A fallback must satisfy
the instruction/tool/output capabilities too. It must not silently convert
`ReplaceBase` to `Append` or silently use API billing.

## 5. Codex Workflow adapter

### 5.1 Connection and account lifecycle

Extract the existing `initialize`/`initialized`, managed ChatGPT login, login
cancel, logout, account read, paginated model list, and quota normalization into
the shared client. Codex's managed ChatGPT flow owns tokens and refresh; use that
flow rather than `chatgptAuthTokens`. App Server supports the rich-client
authentication and event interface this design needs.
[App Server reference](https://learn.chatgpt.com/docs/app-server).

Keep the existing bounded HTTPS authorization URL validation and system-browser
opening behavior. Bind a pending login to its connection epoch and namespace;
late login-completed events cannot update a replacement connection. Auth or
account changes invalidate model catalogs and prepared-context native sessions.
Block new calls during logout/account switching; drain or explicitly cancel
affected Workflow calls before completing the operation. Do not terminate
Workspace work as a side effect.

Build the process environment from explicit policy. Remove inherited API keys,
provider endpoint overrides, alternate provider credentials, and environment
options that enable unintended plugins/tools. Set a private home, canonical
native executable, strict config, explicit OpenAI provider, and disabled provider
model fallback. Preserve platform variables needed by the sandbox and bundled
document tools. Keep credentials out of diagnostics, invocation snapshots,
reports, workflow exports, `.pwrx`, and sidecar IPC.

### 5.2 One isolated thread per attempt

For each call:

1. Resolve the connection/model/effort and the complete selected artifact view.
2. Allocate an attempt ID and retain its staged context for the attempt lifetime.
3. Persist a private invocation record before any model submission.
4. Start a fresh persistent thread with exact cwd, policy, read/write roots,
   instructions, and tool declarations. Record the returned thread ID before
   sending a turn. Use persistence for recovery; `ephemeral: true` precludes
   depending on durable history after a crash.
5. Validate the effective response contract before submitting manuscript content:
   provider, exact requested model, cwd, roots, permissions, approval policy, and
   instruction-source expectations. Follow the qualified permission-profile
   mechanism; do not combine it with incompatible legacy sandbox fields.
6. Send `turn/start` with the task, selected context, model/effort where needed,
   and the projected native output schema. The checked-in stable schema contains
   `TurnStartParams.outputSchema`; the current Workspace `StartTurnRequest` does
   not expose it and must not be reused unchanged.
7. Correlate all events by epoch/thread/turn/item. Wait for a matching terminal
   turn, not merely a successful RPC response or an agent-message event.
8. Extract the final answer item using qualified final-message semantics. Never
   concatenate commentary, reasoning, tool text, and multiple assistant messages
   into a report. Preserve phase metadata where provided. If a supported runtime
   lacks an unambiguous final marker, qualify an explicit terminal-history rule
   or reject that output capability; do not guess the last streamed delta.
9. Journal the provider terminal response before host canonicalization and
   original-schema validation. Publish the artifact only after those pass.
10. Release per-attempt resources; unload/archive native threads according to
    bounded retention while retaining host evidence and recovery metadata.

Check expanded manuscript size against the transport's encoded-frame limit
before submission. The existing Workspace transport was designed for chat and
must not silently truncate larger Workflow inputs. Either qualify a bounded
larger limit or deliver selected context through explicitly staged bounded read
tools, retaining the exact task and context snapshot. Do not grant access to an
entire temporary directory to work around an oversized prompt.

Do not use App Server `review/start`: that is Codex's native review feature,
whereas Pipeline Paper Review is its own multi-step research workflow.

### 5.3 Tools and permissions

Do not pass all selected read roots to a generic Workspace edit profile: that
could make source roots writable. Generate and qualify a Workflow policy with
separate exact read and producer-only write scopes, and exclude global temporary
write roots. The model must not read credentials, private attempt records, other
runs, sibling producer output, or unselected artifacts.

Prefer host tools backed by `ToolAccess` for bounded document reads, assets,
batches, and supporting writes. Adapt the qualified dynamic-tool bridge to an
attempt-derived scope; never let model-supplied run IDs select authority. Keep
Workspace research tools and its database entirely out of this bridge.

Dynamic tools add tools; they do not automatically disable Codex's native tools.
The compatibility spike must prove how to disable unrequested native execution,
file changes, search, MCP, skills, plugins, memory, and delegation, or demonstrate
equivalent enforced restrictions on every remaining route. Prompts are not a
sandbox. If the runtime cannot enforce the required surface, leave that capability
unavailable rather than widening the Workflow contract.

Native image/PDF handling and hosted search need explicit parity fixtures. Do not
replace multimodal page reads with a path string. Keep model service networking,
native command networking, and hosted search distinct. Workflow contribution
reviewers currently request live search; Workspace currently withholds native
search pending qualification. Sharing a client must preserve those independent
policies. Qualify per-thread search overrides and concurrent search-on/search-off
calls before enabling them; a process-global toggle is unsafe for mixed calls.

### 5.4 Parallelism, cancellation, and uncertainty

Maintain a Workflow active-call map keyed by run/pass/attempt, holding the native
thread/turn and an interrupt handle. Shared App Server PIDs belong only to the
runtime service and app shutdown registry. Do not register that PID as the child
of each pass, or cancelling one reviewer will kill all reviewers.

Extend `CallTask` drop/panic cleanup and `cancel_pass` to revoke the call's host
tools immediately and signal a supervisor-owned cleanup task. `Drop` cannot await
an RPC; a bounded cleanup owner must interrupt, wait for terminal acknowledgement,
and reap when required after the waiter disappears. Reject tools and submissions
that race with cancellation registration.

On Stop, send `turn/interrupt`, cancel that attempt's host tools, and retain the
staged roots until native use ends. On interrupt timeout, attempt qualified
thread-local cleanup. If only process termination can guarantee cleanup, stop
the Workflow server, mark every affected unfinished attempt `outcome_unknown`
unless history establishes a terminal result, and report the affected passes.
Workspace remains alive. Do not claim sibling cancellation isolation survives an
unrecoverable shared-process failure.

Persist submission state as `prepared → submitting → accepted → terminal`.
Queue acceptance or an RPC timeout is not proof that a turn did not start.
Reconcile through `thread/read` after reconnect/lag. If terminal outcome cannot be
established, stop dependent scheduling and require an explicit rerun. Neither
the Workflow retry loop nor configured provider fallback may replay an ambiguous
send. Generated client IDs are correlation identifiers, not assumed server
idempotency keys. Deduplicate usage and artifacts during reconciliation.

## 6. Claude Agent SDK adapter

### 6.1 Use the actual SDK through a controlled sidecar

Add a separate package such as `gui/agent-sidecar/` with its own lockfile and
TypeScript build. Pin and record a compatible `@anthropic-ai/claude-agent-sdk`
and native Claude runtime pair. Do not import the SDK into Vite/React or expose
its objects to the WebView. The SDK is offered for TypeScript and Python;
neither is a Rust library. [SDK overview](https://code.claude.com/docs/en/agent-sdk/overview).

Use a bundled or privately provisioned, checksum-verified Node runtime. Production
must not rely on a system Node/Python/npm installation or run `npx` to fetch code
at model-call time. The build machine having Node is not a runtime solution.
Prefer a small JS sidecar plus private Node initially over an unqualified
single-executable bundler that may break dynamic SDK assets or native resolution.

Select the installed or packaged Claude executable explicitly using the supported
SDK option for the chosen version. Verify its version and discovery path using
the repository's cross-platform command resolver. Preserve native Windows shim
resolution and opaque arguments; never route manuscript text through `cmd.exe`.
SDK package defaults and native bundling have changed over time, so inspect the
locked package's exports, types, optional platform dependencies, assets, and
licenses in the initial spike rather than assuming the package alone is portable.

### 6.2 Rust ↔ sidecar protocol

Implement a versioned bounded JSONL protocol with host-generated request IDs,
attempt IDs, deadlines, and a startup capability handshake. Proposed operations:
`initialize`, `discover`, `invoke`, `interrupt`, `tool_result`, and `shutdown`.
The Node reader must continue processing interrupt/tool-response frames while
the SDK iterator is active. Reserve stdout for protocol frames and bounded stderr
for redacted diagnostics. Enforce frame, total-output, and pending-request limits
on both ends; use chunked document/asset transfer where needed.

Use SDK initialization/model/account facilities actually present in the locked
version, verifying discovery makes no model call. Replace Workflow's handwritten
Claude initialization exchange after parity. The full online TypeScript reference
could not be fetched during this assessment because the retrieval tool rejected
its size; compile against the installed package declarations and add protocol
fixtures before relying on precise initialization/interrupt signatures.

Run one official `query()` per attempt, initially without session reuse. Map
effective model, effort, instructions, task, tools, and schema explicitly. Handle
all SDK result subtypes, initialization failures, stream errors, missing final
output, and iterator exceptions after an error result. Do not publish twice.

For structured calls, use `outputFormat: { type: "json_schema", schema }` and
extract `structured_output` only from a successful terminal result. A success
subtype with missing structured output is an error for a schema-backed call.
Serialize the value as the terminal response for the existing host journal and
validator. [Structured outputs](https://code.claude.com/docs/en/agent-sdk/structured-outputs).

Interrupt through the supported SDK operation/abort signal, then enforce bounded
process-tree termination and reaping. Killing only Node may leave the native
Claude process alive. Add Unix descendant and Windows Job Object fixtures.

### 6.3 Authentication and tools

Use provider-owned auth status and supported native authentication operations
only within the qualification in §1.1. Claude documents `auth login`, `auth
status`, and `auth logout`; these are native CLI commands, not evidence of an
SDK-managed browser-login RPC. Do not parse an interactive terminal screen as an
OAuth protocol. For an external installation, a Pipeline connection removal
should remove its reference, not log the user out of their terminal globally.
[Claude CLI reference](https://code.claude.com/docs/en/cli-reference).

Construct the SDK child environment explicitly so a saved or inherited API key
does not unexpectedly select paid API access in a native subscription mode.
Report the effective supported auth mode and reject an unexpected mode. Account
information and model availability must come from the same runtime/context used
for execution. Unknown quota information remains unknown; do not fabricate a
ChatGPT-shaped quota meter for Claude.

Set `settingSources: []` explicitly for Workflow calls and explicitly control
plugins, MCP servers, memory, project instructions, and agent definitions. Do not
inherit manuscript-folder `CLAUDE.md` as privileged reviewer instructions.

Use an exact enabled-tool set, explicit deny rules, and a host-enforced
`PreToolUse` gate or host-owned MCP tools for every file access. `allowedTools`
preapproves tools; it is not a list of the only tools available. Autoapproved
tools may never reach `canUseTool`, and `dontAsk` denies unresolved calls without
calling that callback. Design the gate around these semantics rather than using
`acceptEdits` plus a callback and assuming every write is checked.
[SDK permissions](https://code.claude.com/docs/en/agent-sdk/permissions).

Preferred bounded tools are SDK MCP wrappers that ask Rust to execute the
existing `ToolAccess` operations. Use `tools: []` or the qualified exact native
tool selection for the pinned version, and expose only selected host MCP tools.
If native Read is required for PDFs, qualify its mandatory hook and filesystem
scope before enabling it. Account for any SDK-internal structured-output tool in
the capability tests; do not accidentally disable schema completion or permit
arbitrary additional MCP tools to preserve it.

Deny interactive questions and permission escalation for unattended Workflow
calls with a typed, bounded failure. Workspace's interactive approvals remain a
separate domain policy. SDK session persistence can support later recovery, but
until durable terminal reconciliation is qualified, a crash after submission
becomes `OutcomeUnknown`; it must not trigger an automatic new query.

## 7. Reviewer instructions and portable configuration

### 7.1 Fields and precedence

Add an optional `instructions: InstructionSpec` to `StepConfig`. Add corresponding
optional instruction fields to merge and orientation configuration; inventory
extraction, reconcile, calibration, and other helper calls so each has a defined
host default and can carry instructions through `OwnedRequest`. Add the field to
the borrowed `Request` and preserve it in conversion. Avoid a second instruction
path that bypasses `call.rs`.

Start with per-step instructions and explicit helper configuration; global
instruction inheritance is unnecessary for the first version. Missing fields
preserve existing prompt behavior. Use:

```json
{
  "instructions": {
    "mode": "append",
    "text": "You are an academic referee evaluating the identification strategy. Distinguish demonstrated errors from questions requiring more evidence. Cite exact locations for each finding."
  },
  "prompt": "Review the supplied manuscript using the selected artifacts and return the required findings object."
}
```

Keep the example's role instructions separate from the manuscript, artifact
manifest, selected orientation, and prior outputs. Do not interpolate document
or upstream model content into higher-priority instruction fields. Initially
make instructions literal bounded text, without `{step:...}` or file expansion;
document this difference in the editor. Secret variables must never be promoted
to instructions or exported into a prompt snapshot.

Resolve instructions in a deterministic host function. Build its host-owned
artifact/tool/output contract separately, and record exactly how the adapter
combines that contract with user-editable role text. Neither prompt text nor
replacement mode can weaken host permissions, change the provider, or alter the
validated output schema. Preserve current task prompts on migration; adopting
reviewer roles for built-ins is a separate, evaluated change.

### 7.2 Provider mappings

| Intent | Codex App Server | Claude SDK | Existing direct APIs |
|---|---|---|---|
| Append role instructions | `thread/start.developerInstructions`; preserve native base | `systemPrompt` preset `claude_code` with `append` | Existing higher-priority system-instruction parameter; there is no shared native coding preset |
| Replace native base | Qualified `thread/start.baseInstructions`; feature-gated initially | Custom `systemPrompt` string containing the complete Pipeline reviewer contract | Complete application-owned system text |
| Task and selected material | Turn user input | `query` prompt/input | Existing user/context messages |

The checked-in stable Codex schema includes both thread instruction fields;
Workspace deliberately uses only the additive field. This migration must not
change Workspace's base-instruction policy. Current Codex configuration also
documents an instruction-file replacement mechanism; use the typed thread field
for per-reviewer configuration where qualified, not mutable global files or the
reserved `instructions` key.

Claude has a minimal SDK default, a Claude Code preset, and custom system text.
For transport parity, explicitly select the preset: omitting `systemPrompt`
does not reproduce `claude -p` defaults. Custom replacement requires supplying
the tool guidance needed by the review agent. Configuration loading is separately
controlled by `settingSources`.
[System prompt customization](https://code.claude.com/docs/en/agent-sdk/modifying-system-prompts).

Call the ordinary UI field “Reviewer instructions.” Expose “Replace native base
instructions” only under advanced settings after backend qualification, with an
effective-mode preview. Do not promise control over every hidden provider
instruction or identical precedence across vendors. A requested mode that cannot
be honored fails before model submission.

### 7.3 Serialization and provenance

The current profile/export schema is **11**. The strict portable format in
`pipeline_config/workflow.rs` currently uses that same `CURRENT_SCHEMA_VERSION`
constant and publishes its own JSON schema through `workflow_json_schema()`.
Increment the version when these fields are implemented, preserving old defaults
and rejecting newer unsupported versions in both readers. Update parser
allowlists, JSON schema generation, frontend types, profile/step
export, settings bundles, built-in materialization, gallery definitions,
import/export fixtures, and run fingerprint construction. Test actual old
fixtures; `serde(default)` alone does not update strict import validation.

Add backend and instruction metadata to saved step/helper/merge attempts:
connection namespace (non-secret), requested/resolved/reported model, effort,
backend/native/SDK versions, capability-record ID, instruction mode and hash,
host-contract version/hash, selected-context hash, output-schema hash,
instruction-source list, thread/session/turn IDs, and terminal outcome. Preserve
old `transport` and provenance fields for history readers.

Store exact non-secret resolved instructions in host-owned run evidence outside
model write roots. Do not log the entire prompt or raw auth-bearing protocol
frames to the WebView. A hash can identify a configuration, but is not a substitute
for a retained reproducible instruction snapshot.

## 8. Output, usage, caching, and resume compatibility

**Structured output:** Reuse `structured.rs`'s original portable dialect and
Codex strict projection (required properties, nullable original optional fields,
null stripping). Factor the projection into a value-producing helper instead of
creating a schema file just to send JSON over RPC. Preserve the existing advisory
path for valid schemas that cannot be expressed natively, label it in provenance,
and still fail on invalid host output. Neither provider success nor successful
constrained decoding bypasses host validation. Keep the `{content: string}`
envelope for Markdown and the same downstream artifact/report behavior.

**Usage:** Convert provider events into the current `CallUsage` semantics once
per model operation. Preserve cache-read/cache-write distinctions and tool-call
categories; record missing metrics as unavailable rather than invented zeroes.
For cumulative Codex updates, use the proper turn delta or authoritative final
totals, never sum every cumulative event. For Claude, avoid counting the same
usage in assistant messages and the terminal result. Include cancelled/failed
calls, primers, and schema repair. Subscription rows remain API-equivalent list
prices, not actual charges; account quota is a separate value.

**Caching:** In the first adapter release, retain `PreparedContext` and prepend
the exact selected immutable context to each fresh attempt. Record that native
session reuse is disabled for this backend even if the profile requests caching;
do not imply an observed cache hit. This gives a safe parity baseline without
blocking migration on forks.

Restore native warm/fork reuse in a later commit. Codex uses App Server lifecycle
throughout; Claude supports `resume` with `forkSession`, subject to the selected
runtime's persistence scope. [Claude sessions](https://code.claude.com/docs/en/agent-sdk/sessions).
Base sessions must contain only common selected context and a neutral primer,
never another reviewer's critique, privileged tools, or producer writes. Key reuse
by connection/account epoch, backend/runtime generation, exact model/effort,
instruction and host-contract hashes, selected-context hash, and base policy.
Revalidate child scope and role instructions after forking. Do not fork a broad
ancestor and assume changing cwd removes its historical context. On unsupported
cache setup, fall back only before the actual review submission to one fresh
self-contained call. A failed primer with an ambiguous outcome is not a license
to run duplicate primers.

**History/resume:** Old completed runs continue to render without native threads.
Reuse completed checkpoints according to existing rules, and execute missing
units through the launch's explicitly chosen backend with provenance. Native
session IDs from old CLI cache slots are not migrated into Workspace bindings.
Changing backend or instructions changes the run fingerprint. An interrupted
SDK attempt requires reconciliation or explicit rerun; schema repair after a
known terminal invalid answer is a new recorded attempt, not a replay of an
unacknowledged request.

## 9. Connection and model UX

Create shared presentational components and DTOs for account status, exact model
selection, effort options, runtime availability, and actionable errors. Feed them
from owner-specific commands; do not merge Workspace settings into `Settings`.

Workflow Settings should show the access choice (native account versus API),
selected native backend during rollout, connection scope, sign-in status where
supported, and required runtime health. Preserve saved API keys independently
from the access choice. A missing SDK runtime should not change provider or use
a saved API key. Dependency preflight should inspect only the providers/backends
actually selected by orientation, extraction, steps, merge, and fallback.

Use the selected execution connection for model discovery. Keep role policy as
an app-owned overlay on live availability; enumerate all model pages, validate
effort, and never float a pinned ID. For native Automatic, preserve omission of
an explicit model if that is the existing selected policy, then record the
returned effective model. For role/pinned choices, reject runtime substitution.
Recheck at dispatch so a stale catalog cannot authorize a removed model. Do not
persist account-unbound discovery as authenticated availability.

Run Preview should show effective backend/access, reviewer instruction mode,
model policy, tools, and unavailable requested capabilities. It must not start
model work. The headless CLI needs the same preflight and actionable sign-in
instructions, not a dependency on React or an interactive approval dialog.

## 10. Implementation sequence and acceptance gates

Each stage should be a reviewable change. Do not combine transport extraction,
credential migration, prompt redesign, and built-in review changes in one patch.

### SDK-00 — Qualify the two native boundaries

Deliver `docs/agent-runtime/` with a capability matrix, SDK/runtime version record,
checked protocol fixtures, and explicit observed/unobserved results. Generate
Codex schemas from candidate binaries into a temporary directory and compare
relevant shapes with the checked-in reference. Preserve the existing minimum
generation/admission policy unless a newly required field justifies a documented
capability-specific minimum. Inspect the locked Claude SDK types/package assets.

Prove no-model initialization, discovery, namespace isolation, exact permission
contracts, and clean process shutdown first. Separately qualify one authenticated
text turn, structured result, instruction modes, scoped file read/write, a visual
read, and cancellation for each proposed backend. For Codex also test two
concurrent threads with different roots/instructions/search settings.

Record the Claude distribution/authentication decision from §1.1 and Node/native
packaging feasibility. **Gate:** unsupported capabilities have explicit blockers;
do not mark the whole migration impossible just because one Claude login option
is unavailable. Codex implementation can proceed independently.

### SDK-01 — Extract reusable Codex mechanisms

Create `agent_runtime/codex/`, move wire/transport/version validation and generic
process ownership, and parameterize namespace, policy, and event sinks. Remove
Workspace database calls from the shared layer. Retain Workspace service types,
binding projection, event reconciliation, and global turn guard in Workspace.

**Gate:** existing Workspace deterministic tests and no-model probe pass with
unchanged isolation, archive exclusions, drafts, and event semantics. No Workflow
behavior changes yet. Update `CLAUDE.md` and Workspace developer docs to describe
the shared code location while preserving runtime boundaries.

### SDK-02 — Add connection/backend and invocation contracts

Implement the contracts in §4, owner-specific connection services, namespace
locks, capabilities, typed errors, and active-attempt registry. Add backend
selection with legacy defaults. Wire model discovery to the selected connection
without removing old adapters. Preserve `provider:cli` override resolution.

**Gate:** old settings/profiles deserialize identically; API/native choices remain
independent; no-model discovery is proven; selecting SDK with missing runtime or
auth fails clearly; an API key never becomes an implicit fallback.

### SDK-03 — Implement fresh-thread Codex Workflow calls

Add `pipeline/runtime.rs`, a thin App Server adapter, typed output-schema support,
terminal answer extraction, host response journal integration, and usage mapping.
Route every `call.rs` Codex native call to it only when explicitly selected.
Retain full selected-context prefixes and defer native caching.

**Gate:** orientation, reviewer, merge, reconcile, and LLM extraction fixtures
complete with the same artifact contracts; Markdown commentary never enters the
report; strict/advisory schema behavior is preserved; pinned-model mismatch fails.

### SDK-04 — Enforce tool scope and lifecycle under concurrency

Implement per-attempt host tools and Codex permission policies, approval denial,
search isolation, cancellation cleanup ownership, ambiguous-submission records,
reconciliation, and retry classification. Update `CallTask`, run/pass Stop, app
exit, timeout, panic, and batch paths to handle server-owned invocations.

**Gate:** two parallel reviewers cannot read/write sibling output; cancelling one
does not normally stop another or Workspace; force-kill has honest affected-call
outcomes; no crash/timeout replay; every child and tool task is reaped or reported
as unknown with no further scheduling.

### SDK-05 — Package and integrate Claude SDK

Build the versioned sidecar and provision private Node/native assets as described
in §6. Implement discovery, query, structured output, exact tool surface, scoped
host tools/hooks, instruction preset selection, usage, error handling, and
interrupt/reap. Wire through the same `call.rs` dispatch. Use an explicitly chosen
auth context and preserve the Claude qualification gate.

**Gate:** real SDK behavior meets the same artifact/lifecycle contracts as Codex;
no system-runtime prerequisite; an absent sidecar leaves unrelated providers
usable; installer paths with spaces and non-ASCII characters work on all targets.
If subscription qualification is unresolved, this backend is not advertised as
a supported subscription migration merely because API-authenticated tests passed.

### SDK-06 — Add reviewer-instruction configuration

Implement §7 across request plumbing, Workflow/helper schemas, editor, preview,
fingerprints, manifests, and portable imports/exports. Expose append first; enable
replacement only for qualified adapters. Extend direct API instruction plumbing
without changing their transport. Keep legacy backend limitations explicit.

**Gate:** old profiles give the old task text/default mode; distinct parallel
reviewers receive distinct role instructions; manuscript instruction-injection
fixtures remain task data; exports round-trip; changed role text invalidates
reuse; unsupported replacement fails instead of falling back.

### SDK-07 — Restore caching and measure review quality

Implement and test neutral base-session fork reuse under the expanded key. Add
bounded thread/session retention coordinated with run deletion and active leases.
Compare legacy, fresh SDK, and cached SDK behavior with the same model, effort,
source revision, task prompts, and output schemas.

Evaluate append versus replacement separately from transport changes using a
fixed review set with theory, empirical, mixed-method, source-tree, and visual
inputs. Measure valid output rate, substantive false positives, traceable
citations, missed known issues, latency, total usage, and human-rated usefulness.
Include Auto Review Full/Quick routing and final findings/merge behavior. Do not
infer reviewer quality from schema-valid output or self-evaluation alone.

**Gate:** no cross-reviewer history leakage, no hidden unsupported cache fallback,
usage accounted once, and a recorded researcher review of quality changes.

### SDK-08 — Default rollout and retire duplicate wrappers

Enable the new native backend first for explicit opt-in, then for new connections
after qualification. Offer existing users a concrete migration choice describing
the new connection scope and sign-in requirement. Never rewrite stored model IDs
to a recommended current model. Backend changes affect future runs, not active
attempts or completed artifacts.

Remove duplicate Codex RPC discovery/fork and exec paths only after the new
backend covers all selected non-API call paths. Remove the Claude CLI wrapper
only when the SDK deployment/auth track is actually supported. Keep API/local
dispatch and the dormant Google boundary unchanged. Rewrite Help/README claims,
update the developer map and protocol records, and record packaged qualification.

**Gate:** the completion checklist below passes. Rollback selects the retained
legacy backend for a new explicit run; it never resumes an ambiguous SDK attempt
through CLI. If legacy rollback cannot express new instructions/permissions,
preflight rejects it. Persisted artifacts remain readable after rollback.

## 11. Verification matrix

| Layer | Required evidence |
|---|---|
| Configuration | Schema-11 and older fixtures; unknown newer-version rejection; portable format validation; provider/transport overrides; empty/append/replacement instructions; backend default migration; API key/access independence. |
| Shared Codex transport | Reversed/interleaved responses, unknown additive notifications, malformed known frames, oversized output, queue lag, disconnect, stale epochs, and capability-specific rejection. |
| Claude bridge | Initialization before invocation; no-model discovery; version skew; multi-line/large/Unicode input; cancellation while iterator is active; error result followed by exception; missing final payload; bounded stderr. |
| Output | Final versus commentary distinction; strict nullable optional fields; advisory schemas; malformed JSON; missing structured result; host validation failure; rejected response journal; exactly one artifact publication. |
| Authority | Selected file allowed; sibling/source-parent/home/credentials denied; symlink/traversal denied; original source not writable; only producer files writable; empty tool set; native tools cannot bypass host scope; malicious document instructions. |
| Features | PDF extraction; image/page tools return actual content; selected supporting artifacts; hosted search allowed only where selected; search-off turn alongside search-on turn; no ambient agents/hooks/plugins. |
| Lifecycle | Cancel before/after registration, before/after send, while awaiting tool result, during auth expiry, during merge; waiter drop/panic; hung interrupt; server crash; app shutdown; native descendant cleanup. |
| Recovery | Ack lost after send; terminal result lost before checkpoint; event lag; restart with no rollout; outcome unknown bypasses retries/fallback; completed checkpoints preserved; no automatic repeated side effects. |
| Parallel domains | Multiple review passes plus one Workspace turn; independent Stop/logout; sibling roots and instruction separation; account quota not misrepresented as per-mode; namespace lock contention. |
| Accounting | Cumulative versus incremental tokens, cache metrics, primer cost, cancellation, SDK repairs, fallback attempts, duplicate terminal events, missing metrics, correct reported model. |
| Product | New/old settings, Run Preview, no hidden API switch, headless CLI without GUI, batch and rerun, Auto Full/Quick, findings/Projects/report export, Workspace handoff, `.pwrx` credential exclusion. |
| Packaging | macOS arm64/x64, Windows, Linux; clean machine without Node/Python; SDK/native assets; signatures/notarization where applicable; checksums/notices; offline runtime health and unavailable-provider behavior. |

Run focused tests while implementing; before declaring completion run the
repository's normal suites:

```bash
cd gui
npm test -- --run
npm run build
npm run test:release

cd src-tauri
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo run --locked --bin workbench_probe
```

Add sidecar build/unit/contract checks to the documented scripts and manual CI
checks. Preserve the repository's manual-only CI triggers. Authenticated turns,
actual tools, runtime packaging, and cross-platform behavior each need separate
recorded runs; simulators do not establish them. For local GUI inspection on this
computer, use `cd gui && npm run tauri dev`, never the installed application.
Follow the machine's `oldstata` wrapper rule if a real execution fixture invokes
Stata; this migration itself requires no Stata invocation.

## 12. Completion checklist

- [ ] Workflow ChatGPT calls use shared App Server code for execution and discovery.
- [ ] Claude SDK status distinguishes implemented code, authenticated compatibility,
      distribution/login approval, and packaged support.
- [ ] Workflow and Workspace process, storage, cancellation, and credentials remain
      independently owned; only selected low-level implementation is shared.
- [ ] Every model call remains supervised by `call.rs`; the DAG stays in Rust.
- [ ] API/local access and exact provider/model choices retain their semantics.
- [ ] Reviewer instruction fields reach the native higher-priority instruction
      channel and are saved reproducibly, with default-preserving migrations.
- [ ] Output validation, response journals, artifact selectors, findings, merge,
      resume, and quota fallback remain host-controlled.
- [ ] Tools enforce the actual per-attempt authority on every execution route.
- [ ] Ambiguous sends cannot silently repeat, and dropped waiters cannot detach work.
- [ ] Schema, transport, sidecar, product, and platform gates have recorded evidence.
- [ ] Built-in prompt/quality changes are evaluated separately from transport parity.
- [ ] Default rollout, rollback limitations, runtime installation, and unsupported
      Claude login options are accurately documented.

## 13. Evidence and limits of this assessment

Repository evidence comes from the files in §2, the checked-in stable and
experimental schemas, and the existing compatibility/release records. This
assessment did not launch model turns, sign in/out, install SDK packages, change
credentials, test provider permissions, or run packaged applications. Proposed
types, module names, namespace layout, and stages are Pipeline design decisions.

Official sources were fetched during this assessment. Use fetched page content,
not search snippets: notably, snippets still expose the withdrawn Claude SDK
credit announcement. Recheck these sources when selecting the actual runtime
versions for implementation:

- [Codex App Server](https://learn.chatgpt.com/docs/app-server): transport,
  account operations, thread/turn lifecycle, output schemas, and experimental
  capability boundaries. Use local stdio and the repository's qualification
  policy; do not expand this plan into an unqualified remote WebSocket service.
- [Codex configuration](https://learn.chatgpt.com/docs/config-file/config-reference):
  developer instructions, replacement instructions, reserved config keys, search.
- [Claude Agent SDK overview](https://code.claude.com/docs/en/agent-sdk/overview):
  SDK language/runtime distinction and third-party login restriction.
- [Claude plan usage update](https://support.claude.com/en/articles/15036540-use-the-claude-agent-sdk-with-your-claude-plan):
  the June 15 pause, which supersedes the retained older announcement on that page.
- [Claude system prompts](https://code.claude.com/docs/en/agent-sdk/modifying-system-prompts):
  preset, custom prompt, and config-loading distinctions.
- [Claude permissions](https://code.claude.com/docs/en/agent-sdk/permissions):
  available tools versus preapproval and callback/hook ordering.
- [Claude structured output](https://code.claude.com/docs/en/agent-sdk/structured-outputs):
  schema option and terminal structured-result handling.
- [Claude sessions](https://code.claude.com/docs/en/agent-sdk/sessions):
  resume/fork behavior for the later caching phase.
- [Claude CLI reference](https://code.claude.com/docs/en/cli-reference): native
  authentication commands and the distinction between enabled and allowed tools.

The full Claude TypeScript reference fetch failed because of the retrieval size
limit. SDK-00 must verify exact SDK method signatures and runtime dependencies
against the locked package before implementation. The existing no-model Codex
probe and schema fields support feasibility; neither establishes production
support for the new Workflow adapter.
