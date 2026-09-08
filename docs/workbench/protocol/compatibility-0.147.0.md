# Workspace Codex App Server compatibility: 0.147.0

**WB-00 state:** historical schema and qualification record. The exact-build
policy in the original record has been superseded by
[`compatibility-policy.md`](compatibility-policy.md). This file still records
the protocol generation Pipeline implements and the observations made against
0.147.0.

## Evidence

The checked-in schema bundles were generated from the installed official
`codex-cli 0.147.0` binary on September 6, 2026:

```text
codex app-server generate-json-schema --out <stable-dir>
codex app-server generate-json-schema --experimental --out <experimental-dir>
```

| Bundle | SHA-256 |
|---|---|
| `0.147.0/stable.schemas.json` | `f72b2caa3cbfa4298de9e85c62dda6dfbaf2266ffeb916fed30615ca69ff8c74` |
| `0.147.0/experimental.schemas.json` | `babfd5c98cd978dd858b4762cdfbc9fba941e1a0e4053de0050e4082ae1f075a` |

The generated schemas are authoritative for outbound field names and enum
spellings in this candidate. The current [official App Server
documentation](https://learn.chatgpt.com/docs/app-server) is the behavioral
reference, but examples from a newer build are not copied into the 0.147.0
wire contract without an observed test.

## Version policy

`0.147.0` is the minimum implemented protocol generation and the source of the
checked-in schema reference, not an exact runtime allowlist. Newer builds pass
through the live handshake and fail-closed thread-contract validation described
in [`compatibility-policy.md`](compatibility-policy.md). An incompatible
runtime never falls back to `codex exec` for Workspace.

## Wire mapping from the generated schemas

| Concern | 0.147.0 mapping | Stability |
|---|---|---|
| Handshake | `initialize` request, then `initialized` notification | Stable |
| Account | `account/read`; `account/login/start`; `account/login/cancel`; `account/logout` | Stable schema; login behavior not yet observed in the isolated namespace |
| Models | `model/list` with `cursor`, `limit`, and `includeHidden` | Stable schema; no-login response observed |
| Thread lifecycle | `thread/start`, `thread/inject_items`, `thread/read`, `thread/resume` | Stable schema; persistent fixture history observed without a model turn |
| Turn lifecycle | `turn/start`, `turn/interrupt`; terminal status from `turn/completed` | Stable schema; not yet observed |
| Instructions | `thread/start.developerInstructions` for supplemental layers; `baseInstructions` for an explicit profile replacement (null inherits the default) | Stable schema |
| Permission profiles | `permissionProfile/list`; `thread/start.permissions`, `thread/resume.permissions`, and `command/exec.permissionProfile` select a profile by ID | Experimental schema in this build; custom inspect profile listed, activated, and exercised on macOS |
| Sandbox | A custom profile uses `filesystem` entries plus `workspace_roots`; permission profiles cannot be combined with legacy `sandbox`/`sandboxPolicy` fields | Generated schema and [official permissions documentation](https://learn.chatgpt.com/docs/permissions); read-only narrow-root command enforcement observed on macOS |
| Approvals | `approvalPolicy` uses `untrusted`, `on-request`, `never`, or the granular object; server requests include command, file-change, and permission approval methods | Stable schema; interactions not yet observed |
| Dynamic tools | `initialize.capabilities.experimentalApi = true`; declarations in `thread/start.dynamicTools`; calls through server request `item/tool/call` | Experimental declaration accepted; an authenticated tool call is not yet observed |
| Questions | Server request `item/tool/requestUserInput` | Stable schema; not yet observed |
| Standalone command | `command/exec` plus qualified write/resize/terminate methods | Buffered/streaming execution, stdin write/close, PTY resize, and explicit terminate observed under the custom profile on macOS |
| History | `thread/read` is stable; `thread/turns/list` and `thread/items/list` are experimental in this build | Mixed |
| Shutdown | Close the Workspace-owned stdio connection; active connection-scoped commands must terminate; retain process-tree termination as bounded fallback | Clean exit and active-command PID cleanup after stdin close observed on macOS; host-forced descendant cleanup not yet exercised here |

Requests and responses omit the `jsonrpc` member on the wire. Request IDs may
be numbers or strings and must be echoed without coercion. The first probe
fixture exercises both forms.

## Observed no-login probe

`cargo run --locked --bin workbench_probe` passed on macOS arm64 on September
6, 2026. The probe:

1. resolved the installed executable through Pipeline's command resolver;
2. verified the candidate version met the implemented protocol generation;
3. launched `codex app-server --listen stdio://` with a temporary, mode-0700
   `CODEX_HOME` and the file credential store;
4. removed inherited provider API keys and endpoint overrides;
5. completed `initialize`/`initialized`, `account/read`, and `model/list`;
6. listed permission profiles, then started a persistent fixture thread with an
   experimental dynamic-tool declaration, a custom read-only inspect profile,
   and one runtime workspace root;
7. injected one harmless user-history item, then verified `thread/read` and
   `thread/resume` preserved the thread identity and effective scope;
8. used `command/exec` under that profile to read an approved fixture while
   denying reads of the isolated Workspace config and a sibling fixture
   project, and denying a write even inside the approved fixture root;
9. verified that the server reported the temporary Codex home, no ambient
   account, and no native instruction sources; and
10. started a long-running streaming PTY command with a connection-scoped
    process ID, successfully resized it, sent `command/exec/terminate` while
    the original request remained pending, correlated both responses, and
    verified that the command PID was no longer running;
11. started a buffered stdin command, sent a base64 fixture through
    `command/exec/write` while closing stdin, and verified the exact echoed
    output plus both request responses;
12. started a second streaming command, closed the owned stdio connection, and
    verified both that App Server exited successfully and that the second
    command PID was no longer running, without starting a model turn.

Observed result: isolated home verified, ambient account absent, a five-entry
unauthenticated model catalog returned, and the account-independent thread
lifecycle passed. The server listed and activated the
`pipeline-workbench-inspect` profile. Its standalone command boundary allowed
the declared fixture read, denied Workspace-state and sibling-project reads,
and denied fixture-root writes. A successful `thread/start` also accepted the
exact experimental dynamic-tool declaration. The command-session checks
round-tripped stdin, resized a PTY, and explicitly stopped and reaped a running
fixture command while its original request was pending. The server then exited
cleanly when the probe closed its stdin connection and reaped a second active
command rather than leaving it orphaned.

These command checks establish the candidate's host-side profile behavior on
this macOS machine. They do not establish that a model can call the dynamic
tool or that native tools in an authenticated turn remain within the same
roots; those require the opt-in authenticated turn below.

In this build, `thread/start` alone does not create a readable rollout, even
with `ephemeral: false`; `thread/read` reports `no rollout found` until history
exists. The probe therefore uses `thread/inject_items` to materialize harmless
fixture history without making a model request. An `ephemeral: true` thread is
not suitable for this read/resume qualification.

## WB-02 supervisor observation

The persistent supervisor smoke check passed on macOS arm64 on September 6,
2026 against the exact `codex-cli 0.147.0` candidate. It launched App Server in
a temporary mode-0700 Workspace home, completed the production transport's
handshake, verified the server-reported home, closed stdin, observed a clean
exit, and started no model turn. A separate host-forced-cleanup test exercised
the same independent process owner against a leader with a live descendant:
the entire process group was killed and reaped, while Pipeline's ordinary
global cancellation path left that Workspace-owned tree running. This closes
the host-supervisor forced-cleanup gate on macOS; Windows and Linux remain
separate platform qualification gates.

The scripted simulator additionally covers reversed concurrent responses,
delayed replies, interleaved server requests and notifications, request
deadlines, malformed and oversized frames, bounded-event lag, process exit,
and reconnect with a new connection epoch. Completed turn/item payloads are
projected into the app-owned SQLite store and reconciled idempotently from
authoritative completion events.

## Reusable and non-reusable existing code

- Reuse `deps::resolve_command` for native executable and Windows npm-shim
  resolution, `env::full_path`/`configure_silent_command` for GUI-safe PATH
  and process-group setup, and `pipeline::logging::next_bounded_line` for
  allocation-bounded JSONL framing.
- Do not reuse `model_catalog::rpc_exchange` as the session transport. It is a
  short request/reply probe, suppresses server requests, and exits after a
  bounded exchange.
- Do not put Workspace children in Pipeline's global run PID registry. WB-02
  must own its server and computation process trees independently.
- Do not use `thread/shellCommand` or experimental `process/spawn`; neither is
  the intended sandboxed execution boundary.

## Remaining WB-00 gates

- Run managed ChatGPT login/cancel/logout against an opt-in test account and
  confirm that the file credential store is independent of the user's normal
  Codex account.
- Exercise an authenticated dynamic tool call, approval, question, and
  interrupt. Thread start/read/resume and stdio shutdown are already observed
  without authentication.
- Exercise native file and shell tools in an authenticated turn and confirm
  they cannot exceed the roots enforced by the command-level profile checks.
- Repeat the required process and permission checks on Windows and Linux, or
  explicitly narrow the first supported platform set.
- Keep the required root restrictions fail-closed as newer builds evolve. If a
  build cannot express them, disable the affected capability and report the
  incompatibility without hiding app-owned research data.
