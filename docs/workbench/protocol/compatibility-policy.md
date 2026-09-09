# Workspace Codex App Server compatibility policy

Workspace does not pin Codex CLI to one exact build. Routine Codex updates are
admitted when they are at least the oldest implemented protocol generation and
continue to satisfy the live contract used by Workspace.

## Admission and enforcement

1. Reject malformed version output and builds older than `0.147.0`.
2. Start App Server with Workspace's isolated `CODEX_HOME`, strict config, and
   the canonical native executable path. The canonical path matters because
   current Codex builds may re-exec themselves as a macOS sandbox helper.
3. Use bounded parsing and fail closed when a required response or field is
   missing or malformed.
4. After every thread start or resume, verify the returned provider, working
   directory, runtime roots, active permission profile, approval policy, and
   reviewer. When an exact model was requested, also reject substitution.
5. Keep app-owned workspaces, drafts, and transcripts usable when the live
   runtime cannot connect. Never weaken permissions or fall back to
   `codex exec` to conceal an incompatibility.

The checked-in `0.147.0` schemas remain the reference for the protocol
generation implemented by Pipeline. They are not an allowlist. New schema
snapshots are useful for reviewing additions, but ordinary additive changes do
not require a Pipeline release when the live invariants still hold.

The development-only `workbench_probe` is the broader qualification check. It
uses no model turn and exercises the handshake, isolated state, permission
profile activation, thread start/read/resume, dynamic-tool declaration,
sandbox allow/deny behavior, command streaming and control, and process
cleanup. It passed on macOS arm64 with `codex-cli 0.153.4` on September 6,
2026. Authenticated model/tool behavior and other operating systems remain
separate release gates.

See the [official App Server documentation](https://learn.chatgpt.com/docs/app-server)
and [permissions documentation](https://learn.chatgpt.com/docs/permissions) for
the upstream transport and profile contracts.

## Project surface catalog 2

PI-00–PI-03 add `workbench_project_context` and `workbench_anchor_read` as bounded,
read-only dynamic tools under the existing `paper_tools` module. They use the
same scoped services as the UI. The catalog version and effective harness
fingerprint change so an existing native binding requires the normal successor
binding path. There are no new provider DTO fields, wire methods or permission
modes. Host execution authorization remains a UI-only capability. Isolated task
sessions receive the task directory as their native runtime root and disable
legacy host-profile tools. Deterministic catalog/scope tests and the no-model
probe pass; authenticated use of this catalog remains a release gate.

## Research studio catalog 3

PI-04–PI-09 add `workbench_research_records`, a Workspace-scoped read-only tool
under `paper_tools`, capped at 20 records / 64 KiB. Human decisions, immutable
source material and model assessments retain their provenance. No new provider
wire method, permission mode, or authorization tool is introduced. The harness
fingerprint requires the normal successor-binding path. `workbench_research_run`
still returns its terminal receipt, but its process wait now runs outside the
database worker/gate. Turn-owned jobs stop with their turn; explicitly detached
UI jobs use the separate local-job Stop control. Authenticated use remains a
release qualification gate.


## Task chain catalog 6

The optional `task_tools` module adds `workbench_task_catalog` and
`workbench_task_propose`. They read the portable chain schema/profile identities
and save a bounded, conversation-scoped proposal. Neither starts a task, changes
native permission modes, nor authorizes host execution. Task preparation and
Start remain native desktop commands. The changed catalog fingerprint follows
the existing successor-binding policy; no provider wire DTO or method changed.
Workspace store migration 12 adds immutable `task_exchanges`; execution state
remains in the independent coordinator database. See [Tasks](../../tasks.md).
Authenticated task-driven native turns remain a release qualification gate.

## App-owned approval identity

Workspace approval responses require the connection `epoch` captured by the
request card as well as its unchanged native request ID and method. Missing or
stale epochs fail closed. The host claims a pending request before sending its
response, so duplicate clicks and uncertain writes cannot replay an approval.
`SendTurnResult` also includes its originating epoch; transient conversation
state uses that identity when matching native terminal events. These are
app-owned IPC fields, not changes to the pinned native protocol schemas.
