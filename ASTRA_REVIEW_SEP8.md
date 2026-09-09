# ASTRA codebase review and remediation plan

Reviewed September 9, 2026; filename retained as requested. Pipeline 0.9.5,
branch `main`, base commit `a77a8d73e1a722ba2d3c13f2ffae8f5cb1750467`.

This plan contains **26 items: seven P1, sixteen P2, and three P3**. The first
priority is preserving approval identity, conversation drafts, accepted-turn
ownership, and recoverable Review results. The proposed work stays within the
existing Workspace, Reviews, and Tasks ownership boundaries.

The checkout already contained extensive modified and untracked files, and
another refactoring process changed files during this review. Findings refer
to the working files, including those changes, rather than just the base
commit. Source locations were refreshed after the observed moves; function
names are included where useful because line numbers will continue to move.
The original review produced this plan. Implementation was subsequently requested
for all P1 and P2 items, except item 22, which the user explicitly rejected.

## Implementation status

All P1 and P2 items except **22** are implemented and verified in the working
tree. Item **22 is skipped at the user's request**: Poppler bundling and its
cache policy remain unchanged.
Item **24** was subsequently authorized and implemented as a follow-up. P3
items 25–26 remain outside this implementation request.

| Items | Implemented behavior |
|---|---|
| 01 | Approval responses require the originating epoch and atomically claim the pending request |
| 02 | Pending submissions retain conversation/epoch/thread/turn ownership across navigation; recovery targets their owner |
| 03 | Native acknowledgement records identity before projection; bookkeeping errors retain the permit and are non-retryable |
| 04 | Outer deadlines preserve late success and classify native App Server uncertainty as non-retryable by transport identity |
| 05–06 | Durable atomic wave receipts establish logical completion and supersede provisional units without deleting them; legacy partial steps rerun conservatively |
| 07 | One release coordinator owns the draft; uploads never overwrite assets; final verification requires all four installers and checksums |
| 08–09 | Initial editor hydration preserves drafts; bundle import resets saved baseline/history and notifies the parent |
| 10 | Partial reruns copy retained producer trees; missing trees invalidate producers selected for file context |
| 11 | Primary and named inputs use captured sources for extraction, document adapters and model reads; reruns verify snapshot integrity, and legacy source-dependent reruns fail explicitly |
| 12 | Tool-loop usage receipts retain known tokens and tool counts on failure/cancellation, including warm-up calls, without double counting |
| 13 | CLI batch items share one captured settings/workflow snapshot and retain per-document preflight |
| 14 | Move/delete hold the native turn permit across idle checks and storage mutations |
| 15 | Task outcomes normalize native `inProgress`; cursor checks preserve legacy spellings and same-turn completion transitions |
| 16 | Lag and periodic native reads reconcile matching terminal ownership; recovered completions reach the renderer |
| 17 | Parser verification and extraction hold the engine maintenance lease; detached blocking workers retain it through cancellation |
| 18 | Quarantine serializes with settings writes, checks the original bytes again, and preserves uniquely named backups |
| 19 | Local endpoint probes use standard URL authority/default-port parsing, including IPv6 |
| 20 | The release coordinator fetches full history and verifies signed tag/ref/commit identity through GitHub |
| 21 | Push/PR quality gates include production build, release tests, all-target Rust tests, strict Clippy and native platform coverage; release depends on them |
| 22 | **Skipped by user instruction** |
| 23 | Release documentation describes the actual draft workflow, unsigned Windows installers, outputs, Poppler inputs and manual qualification |
| 24 | Removed the Antigravity CLI adapter, dispatch, discovery, readiness probes and obsolete UI options; retained Google API and saved-format compatibility |

Implementation choices: completion receipts provide a conservative logical-step
resume contract rather than partial unit reuse. Engine use is serialized through
the existing cross-process maintenance lock. Source/producer copies are bounded,
independent files; oversized inputs fail explicitly. Existing draft assets may be
retried only when their bytes match; changed installers require a new version.
No release has been built, uploaded, or published by this implementation task.

## Implementation verification

P1/P2 implementation checks, before the Antigravity CLI follow-up:

| Check | Result |
|---|---|
| Frontend suite | **95 files, 701 tests passed** |
| Production frontend build | Passed |
| Rust all-target suite | **967 passed, 10 ignored**: 953 library tests and 14 CLI tests |
| Strict Clippy, all targets and features | Passed with `-D warnings` |
| Frontend and Rust formatting | Passed |
| Source-size gate | Passed without increasing the baseline for these fixes |
| Markdown links | Passed, 518 documents checked |
| Release contract suite | **60 passed**; release identity validated |
| Workspace no-model probe | Passed against the local native CLI in an isolated home, including filesystem denial, PTY/stdin and process cleanup |

Regression coverage includes approval epoch reuse and single claim, accepted-turn
ownership after bookkeeping failure, cross-conversation events before and after
acknowledgement, editor hydration/import, uncertain deadlines, logical wave
recovery, independent producer copies, primary/named source integrity, captured
external LaTeX includes, text-only legacy bundle rebuilding, failed-call usage,
native task states, engine leases, settings quarantine, IPv6 and release assets.

The shared checkout changed throughout implementation. Final integration also
corrected two old migration fixtures to remove newer discovery tables before
simulating older schema versions; production migration history was preserved.
Earlier temporary verification copies and transient failures are superseded by
the final working-tree results above. Existing unrelated changes remain intact.

The 10 ignored tests, authenticated model calls, actual engine installation and
packaged Windows/Linux/macOS release qualification were not run. CI now declares
native platform gates; local macOS results do not establish that those remote
jobs or installers have passed. Item 22's Poppler policy remains unchanged.

## Original review evidence and scope

The review combined parallel source inspection of the frontend, Workflow
engine, Workspace, and Tasks with inspection of settings, managed engines,
release scripts, CI, and current documentation. It traced callers and failure
paths, checked existing coverage, and reproduced three frontend defects in a
temporary copy of the relevant source. Two small Rust probes extracted the
actual settings-quarantine and URL-parsing functions and exercised them on
disposable data. This is a broad repository review, not a claim that every
line or every platform behavior has been exhaustively verified.

| Check | Observed result |
|---|---|
| `npm test` | Latest full run: **88 files, 675 tests passed** |
| `npm run build` | Passed; PDF reader chunk approximately 711 kB minified |
| `npm run format:check` | Passed |
| `cargo test --locked --all-targets` | **943 passed, 10 ignored**, no failures; 929 library tests and 14 CLI tests |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `npm run test:release` | **58 passed**, release identity validated |
| `node scripts/check-markdown-links.mjs` | Passed after adding this plan; 517 files checked |
| `node scripts/source-size-report.mjs --check` | Final run passed after a concurrent baseline reset; **18 oversized source/test files** remain. Earlier run identified eight newly oversized or growing files against the prior baseline |
| Targeted frontend reproductions | Three expected regression failures confirming items 02, 08, and 09 |
| Extracted-function Rust probes | Confirmed items 18 and 19 |

The initial frontend run caught missing names during the concurrent controller
extraction and one report-search timeout. A subsequent full run passed, so
those observations are **not** presented as unresolved defects. The initial
release-test failure was a sandbox denial of a loopback HTTP listener; the
same suite passed outside the sandbox. The large PDF chunk is already loaded
separately and is not, by itself, a correctness finding.

The concurrent work also corrected check-mode reporting and refreshed the
source-size baseline before delivery. Item 26 therefore addresses the remaining
maintainability debt, not a currently failing gate or the corrected reporting.

No paid model calls, authenticated provider qualification, installed-app
interaction, Stata jobs, dependency-advisory audits, or Windows/Linux runtime
qualification were performed. Existing ignored live tests remain outstanding
qualification work. Passing deterministic tests does not establish those
results.

## Priorities and implementation order

- **P1:** Address before relying on the affected approval, recovery, or release
  operation. These can lose work, repeat an uncertain submission, or publish
  mutable/incomplete release assets.
- **P2:** Correctness, provenance, reproducibility, and verification work to
  schedule next. Configuration-policy recommendations are identified as such.
- **P3:** Focused cleanup and documentation maintenance after correctness fixes.

Effort is relative: **S** is a focused local change, **M** spans a few owners,
and **L** changes a persistence or lifecycle contract and needs staged review.

| Order | Work package | Items | Main acceptance condition |
|---|---|---|---|
| 1 | Workspace identity and ownership | 01–03, 14–16 | Old events/approvals cannot affect a successor; accepted work retains its permit |
| 2 | Editor data preservation | 08–09 | Reopening/importing cannot erase drafts or cross profile history boundaries |
| 3 | Review recovery and provenance | 04–06, 10–13 | No uncertain resubmission, missing units, lost checkpoints, or mixed input revisions |
| 4 | Local service reliability | 17–19 | Active extraction survives maintenance; recovery backups survive repeated errors |
| 5 | Release and verification contract | 07, 20–23 | Publication, tag checks, packaged inputs, and documentation agree |
| 6 | Maintenance | 24–26 | Remove unreachable code, update the short map, and make size policy actionable |

Items 02, 08, 09, 18, and 19 have direct reproductions. Other correctness
findings below are confirmed by source-path tracing; their proposed fault
injection and integration tests have not yet been added. There is no confirmed
P0 finding in this review.

## P1 — highest priority

### 01. Bind approval responses to the exact request shown to the user

**Evidence:** `gui/src-tauri/src/workbench/commands.rs:155`
(`ResolveServerRequest`), `workbench/commands/codex.rs:423–455`, and
`agent_runtime/codex/transport.rs:511–516` under `gui/src-tauri/src/`.

**Problem and trigger:** The response DTO carries a request ID and method but
no originating connection epoch. The handler looks up the current pending
request by ID and compares that retrieved request's epoch with the current
supervisor. Server-assigned IDs are forwarded unchanged. After reconnect, a
new connection can legitimately reuse an ID and method; a delayed response
from the old UI card can therefore approve the new request.

**Proposed fix:** Issue an opaque pending-request token, or require the captured
epoch and thread/turn identity alongside the request ID. Validate the complete
identity and atomically claim the pending request before responding. Never
translate an old card's response into approval of a new request. Update the
app-owned DTO, frontend client, and compatibility coverage together.

**Acceptance:** A simulator reuses an ID/method across two connection epochs
for different commands. The first card's response is rejected; only the
second card's captured identity can resolve the second request. Concurrent
double submission resolves it at most once. **Effort: M.**

### 02. Keep pending messages and failed-turn recovery attached to their conversation

**Evidence:** `gui/src/hooks/useWorkbenchEvents.ts:76–89,169–188` and
`gui/src/hooks/useWorkspacePageController.ts:271–300,500–510`.

**Problem and trigger:** Send in conversation A, navigate to a never-sent
conversation B that contains a draft, then let A fail or the connection close.
Pending text still belongs to A, but restoration uses the currently selected
snapshot's session ID. An unbound B also makes the event matcher accept every
thread. The reproduction replaced `B important draft` with
`A original question` and issued an actual mocked persistence request for B
containing A's text. Streaming content can likewise appear in the wrong view.

**Proposed fix:** Associate pending text, submission status, streaming buffers,
and recovery with the originating session, epoch, thread, and turn. Route
terminal recovery to that session independently of navigation. An unbound
conversation must not wildcard-match another thread. Render only transient
state belonging to the selected conversation.

**Acceptance:** Start A, navigate to B, then emit A's delta, completion, failure,
and connection-close events in separate cases. B's draft and transcript stay
unchanged; A's recovery persists correctly; returning to A shows its own live
or recovered state. **Effort: M.**

### 03. Retain turn ownership when post-acceptance bookkeeping fails

**Evidence:** `gui/src-tauri/src/workbench/commands/codex.rs:270–297,328–340`
and `gui/src-tauri/src/workbench/codex/supervisor.rs:560–567`.

**Problem and trigger:** A native turn can be accepted before its projection or
snapshot attachment fails. `submit_turn` clears all `ActiveTurnState` on any
error, releasing the global permit even though the native turn may still be
running. Subsequent turns or storage maintenance can overlap it, and the
matching ownership identity has been discarded.

**Proposed fix:** Represent pre-dispatch failure, uncertain submission, and
accepted submission separately. Publish the acknowledged native identity
immediately. Hold the permit through its matching terminal event or confirmed
shutdown/reconciliation even if a later database write fails. Surface the
bookkeeping error without making the runtime falsely idle.

**Acceptance:** A slow simulated turn is accepted; separately inject failures
in projection and snapshot attachment. A second submission and relevant
storage maintenance remain blocked until that exact turn terminates.
**Effort: M–L.**

### 04. Preserve App Server uncertainty through the outer timeout

**Evidence:** `gui/src-tauri/src/pipeline/call.rs:290–310`,
`pipeline/codex_server/invocation.rs:159–167`, `pipeline/provider_error.rs:51`,
and `pipeline/executor/step_call.rs:397–404` under the same source root.

**Problem and trigger:** The supervised call starts a timeout before dispatch;
the App Server adapter starts another timeout of the same duration later.
When the outer deadline wins, cleanup's result is discarded and replaced by a
generic timeout. This loses the adapter's `codex-outcome-unknown` classification.
The retry layer can then submit another native turn after an earlier accepted
turn, contrary to the uncertain-submission policy. A successful response that
arrives during the cleanup grace period is also discarded and replaced with
that retryable timeout.

**Proposed fix:** Carry a typed dispatch/outcome classification across both
layers and use one propagated deadline. Preserve the adapter's uncertainty
when the supervisor times out. A potentially accepted App Server submission
must remain non-retryable until reconciled; changing the error's wording is
not a sufficient contract.

**Acceptance:** Exercise the supervisor and logical retry loop together against
a scripted server. Delay dispatch before the adapter deadline starts so the
outer deadline wins after acceptance. Cover both an uncertain result and a
success returned during cleanup; assert exactly one `turn/start` and no
automatic orientation or step resubmission. **Effort: M.**

### 05. Do not equate one checkpointed unit with a completed logical step

**Evidence:** `gui/src-tauri/src/pipeline/executor/parallel.rs:419–434`,
`commands/rerun.rs:87–105,140–180`, and `pipeline/executor/run.rs:217–248`.

**Problem and trigger:** Crash after `technical/claude` completes while
`technical/codex` is pending. Recovery loads the first unit checkpoint and a
synthetic run-level failure. Resume considers base step `technical` complete
because any output exists; reuse validation requires only a nonempty set of
valid outputs. The missing reviewer or fan-out unit is skipped and downstream
dependencies can treat the logical step as successful.

**Proposed fix:** Persist the expanded expected unit inventory and a separate
logical-step completion receipt. Incomplete logical steps must become resume
seeds. Reusing individual successful units is reasonable only with an explicit
unit-level resume contract that also accounts for merges, skips, and quorum.

**Acceptance:** Recover two-agent and multi-file fan-out fixtures after only one
unit finishes. Every missing required unit runs, and downstream success/quorum
checks never accept an incomplete step. Include legacy checkpoint handling.
**Effort: L.**

### 06. Replace checkpoint generations without first deleting durable results

**Evidence:** `gui/src-tauri/src/pipeline/executor/run.rs:395–396` and
`gui/src-tauri/src/pipeline/executor/checkpoints.rs:93–129`.

**Problem and trigger:** A parallel wave deletes its provisional checkpoints
before writing reordered/merged replacements individually. A crash, full disk,
or write failure in between destroys outputs that were already checkpointed.
Atomic writes of individual replacements do not make the whole replacement
operation atomic.

**Proposed fix:** Prefer stable unit checkpoint identities plus separate
ordering/merge metadata. Alternatively write a complete new generation,
atomically publish its active-generation manifest, then retire the old files.
Keep recovery readers backward compatible while introducing the new format.

**Acceptance:** Inject a failure before publication and between replacement
writes. Recovery retains every previously committed output exactly once and
selects a complete generation. **Effort: L.**

### 07. Make published release assets immutable and avoid publication by an individual matrix job

**Evidence:** `.github/workflows/release.yml:146–184`, particularly upload with
`--clobber` and `gh release edit --draft=false --latest`.

**Problem and trigger:** Each platform independently creates/uploads/publishes
the release. In an all-platform run, the first completed installer can make
the release public before the other builds finish or fail. Retrying a platform
against an already public tag overwrites its asset. Initial parallel release
creation also has a check-then-create race. This is current intentional
workflow behavior, reinforced by tests, but it weakens artifact identity and
contradicts the documented publication contract.

**Proposed fix:** Keep platform builds and retries, but upload to a draft owned
by a single release coordinator. Refuse to modify an already published
release. Have a final job verify the explicitly intended platform set and
leave the result ready for the documented publication decision. Publish
replacement binaries under a new version. Make creation idempotent without
letting one platform publish another platform's incomplete work.

**Acceptance:** Mock `gh` for a published release, a partial matrix failure,
simultaneous first uploads, and a platform retry. No published asset is
replaced; incomplete required sets stay draft; publication occurs at most once
after the agreed gate. Update the tests that currently require `--clobber`.
**Effort: M.**

## P2 — correctness and verification

### 08. Preserve the default workflow's recovery draft during initial loading

**Evidence:** `gui/src/components/pipeline-editor/useWorkflowEditor.ts:11–16,50–57`
and `gui/src/components/pipeline-editor/useProfileOperations.ts:58–95`.

**Problem and trigger:** The editor initially has `config=null`, `dirty=false`,
and `activeProfile="auto-review"`. Its persistence effect immediately removes
that profile's local-storage draft. Asynchronous profile loading reads the
draft later, after it has disappeared. This also erases Automatic Review's
recovery entry when another profile is active. The isolated mount reproduction
confirmed the stored value becomes `null`.

**Proposed fix:** Gate persistence and deletion on completed initialization and
a known loaded profile; at minimum do nothing while configuration is absent.
Treat initial recovery inspection, explicit discard, and successful save as
distinct transitions. **Effort: S.**

**Acceptance:** Prepopulate drafts for two profiles, delay startup IPC, and
verify neither draft changes before recovery is resolved. Cover accepting and
declining restoration and startup failure.

### 09. Reset profile history and launch state after settings-bundle import

**Evidence:** `gui/src/components/pipeline-editor/useProfileOperations.ts:533–553`
and `gui/src/components/pipeline-editor/useWorkflowEditor.ts:85–127`.

**Problem and trigger:** Bundle import sets configuration/profile directly,
bypassing `replaceDraft`. It retains the previous profile's undo history and
saved baseline and omits `onProfileChange`. Import A→B, then undo: the editor
still identifies B but now contains A's prompt, which can be saved over B.
The reproduction also confirmed the missing parent notification; launch input
requirements can remain those of the prior profile.

**Proposed fix:** Use the existing profile-mutation guard; apply the result with
`replaceDraft(normalized, normalized, activeProfile, true)`, refresh the list,
and notify the parent. Refresh provider/settings catalogs affected by the
bundle. **Effort: S–M.**

**Acceptance:** After A→B import, undo cannot cross profiles, B is its own saved
baseline, run setup reflects B's input mode, and late requests cannot restore
pre-import state.

### 10. Carry reused producers' supporting artifacts into partial reruns

**Evidence:** `gui/src-tauri/src/commands/rerun.rs:423–451,509–535` and
`gui/src-tauri/src/pipeline/executor/artifact_context.rs:328–345`.

**Problem and trigger:** A partial rerun preloads reused `StepOutput` records
into a fresh run but does not copy their producer-owned supporting files.
Consumers selecting upstream files look in the new run's `by-step` tree and
silently continue if it is absent. A reused producer's CSV, code, or figure can
therefore disappear from the consumer's context.

**Proposed fix:** Before dispatch, safely copy and register the bounded artifact
trees for reused producer units, preserving identity and ownership. Missing
required retained artifacts should invalidate reuse or produce an explicit
error. **Effort: M; coordinate with 05.**

**Acceptance:** A producer writes a CSV and a consumer selects it. Rerunning
only the consumer supplies identical bytes, and the new run remains usable
after parent retention removes the old run.

### 11. Prevent reruns from mixing captured text with changed live source files

**Evidence:** `gui/src-tauri/src/commands/rerun.rs:201–210,262–277,431,723–732`
and `pipeline/executor/artifact_context.rs:198–220,262–284`.

**Problem and trigger:** Edit the original PDF, LaTeX project, or named source
after a run. Rerun reuses the old extracted text and orientation but stages or
exposes current source files. Its report still uses the parent's `paper_hash`.
The model can read different revisions in one context while provenance
identifies the older input.

**Proposed fix:** Retain immutable source representations needed by selectors
and reconstruct reruns from them. For older runs without retained sources,
verify source identity before reuse; invalidate extraction/downstream reuse
or explicitly omit changed source context with a recorded limitation. Do not
silently label new bytes with the old hash. **Effort: M–L.**

**Acceptance:** Change a PDF, a LaTeX include, and a named source between runs.
Every rerun uses one declared revision or stops with an actionable identity
conflict. An unchanged source continues to work.

### 12. Retain reported usage when direct API calls fail

**Evidence:** `gui/src-tauri/src/pipeline/api_common/loops.rs:267–280` and
`gui/src-tauri/src/pipeline/api_openai.rs:355–380`; analogous loop/adapter
structure exists for Anthropic and Google.

**Problem and trigger:** A tool loop accumulates usage locally, then returns an
error after a later HTTP failure, truncation, tool-budget exhaustion, or
timeout. The adapter emits usage only after a successful return, dropping
already reported tokens/tool usage. Successful cache warm-up usage can also
disappear if the main call fails. Retrying understates the total further.

**Proposed fix:** Record each response's usage incrementally or carry it in a
structured failure result. Preserve accumulated usage through cancellation,
timeouts, and logical retries without double counting success. **Effort: M.**

**Acceptance:** Mock a billed tool response followed by failure, truncation,
budget exhaustion, and successful warm-up followed by failure. Persisted run
totals retain all usage actually reported by the provider.

### 13. Freeze CLI batch configuration once for all documents

**Evidence:** `gui/src-tauri/src/bin/cli.rs:948–978,1004–1016` and
`gui/src-tauri/src/commands/run_entry.rs:202–208,304–310`. The desktop batch's
snapshot ownership is in `gui/src-tauri/src/commands/batch.rs`.

**Problem and trigger:** CLI batch validates an initial workflow, but each
document constructs new headless options and reloads persisted settings and
profile state. Changing the active desktop profile during a batch without
`--profile` can switch workflows halfway through. An explicit profile or
portable workflow still inherits newly changed provider/default settings.

**Proposed fix:** Capture a complete batch `RunSnapshot` once and pass it through
the headless execution path for every document. Share snapshot mechanics with
the desktop adapter without sharing mutable runtime ownership. **Effort: M.**

**Acceptance:** Change the active profile, a prompt, provider, and default model
after the first fixture finishes. Every batch item retains the initial
workflow/settings fingerprint.

### 14. Serialize conversation move/delete with turn setup

**Evidence:** `gui/src-tauri/src/workbench/commands.rs:292–324` and
`gui/src-tauri/src/workbench/store/sessions.rs:275–290`.

**Problem and trigger:** `ensure_session_idle` checks state and drops its lock;
move/delete then runs in another awaited store operation. A send can start
between the check and mutation. Delete can remove bindings/transcript for a
new active turn; move can change its project while the native turn keeps the
old root.

**Proposed fix:** Hold a lifecycle guard shared with submission setup across
validation and mutation. Keep unrelated idle conversations usable where the
ownership model permits it. **Effort: M; coordinate with 03.**

**Acceptance:** Use barriers to pause move/delete after the idle check, start a
send, and resume the mutation. The operations serialize or one is rejected;
no live turn loses its owning session.

### 15. Normalize native turn states before follow-up lifecycle decisions

**Evidence:** `gui/src-tauri/src/workbench/store/runtime.rs:105`,
`workbench/programs/followups.rs:262–270`, `workbench/programs/commands.rs:428–437`,
and `workbench/tasks.rs:49–61,75–89`.

**Problem and trigger:** Native turns persist as `inProgress`, while follow-up
reconciliation recognizes only `running`. A successful slow follow-up is
immediately marked `attention`. Binding validation similarly permits
`:running`→`:completed`, but the actual predecessor cursor uses `:inProgress`,
so normal completion can be rejected as changed context.

**Proposed fix:** Define one app-owned state mapping at the adapter boundary and
use it consistently for outcomes and cursor compatibility. Preserve raw
provider status separately if needed for diagnostics. **Effort: S–M.**

**Acceptance:** Queue against a real projected `inProgress` predecessor, complete
that same turn, and validate the binding. A dispatched slow follow-up moves
from running to completed without false attention.

### 16. Recover active-turn ownership after event-bridge lag

**Evidence:** `gui/src-tauri/src/agent_runtime/codex/transport.rs:18` and
`gui/src-tauri/src/workbench/commands/codex.rs:416–419,600–637`.

**Problem and trigger:** The broadcast buffer holds 256 events. If a terminal
event is evicted before the bridge reads it, the lag handler only emits a UI
notice. Store reconciliation does not reconcile `ActiveTurnState`. The
transcript can show completion while the permit stays held, blocking later
sends and leaving Tasks busy indefinitely.

**Proposed fix:** On lag and explicit reconciliation, inspect authoritative
runtime state for the active epoch/thread/turn and reconcile its ownership.
Connection closure must also have a reliable ownership path independent of a
lossy presentation stream. **Effort: M; coordinate with 03.**

**Acceptance:** Pause the bridge, emit completion and more than 256 subsequent
events, then resume. Recover and release only the matching completed turn;
never release a successor's permit.

### 17. Hold an engine-use lease throughout managed PDF extraction

**Evidence:** `gui/src-tauri/src/pipeline/extract/pdf.rs:647–652,731–769`,
`engines/installer_io.rs:89–127`, and `engines/operations.rs:71–88`.
The Uninstall button in `gui/src/components/EnginesPanel.tsx:311–318` does not
reflect extraction use.

**Problem and trigger:** Extraction resolves verified runtime paths and later
executes from them without a lease. Uninstall takes the installer lock and
deletes both runtime roots; extraction does not hold that lock. Uninstall,
repair, or cleanup can race a reader, remove scripts/model files, and fail
active extraction. A UI-only disable would not protect another process.

**Proposed fix:** Add an explicit runtime-use lease held across verification,
server/sidecar use, and cleanup. Maintenance needs exclusive ownership while
readers may share it where safe. Surface busy state in Settings. Define how
sidecar refresh upgrades its ownership without deadlock. **Effort: M.**

**Acceptance:** A paused synthetic extraction holds the lease while uninstall
or repair is attempted from another command/process. Maintenance waits or
returns a clear busy result; runtime files remain available until use ends.

### 18. Preserve distinct corrupt-settings backups and serialize quarantine

**Evidence:** `gui/src-tauri/src/settings.rs:1124–1139,1342–1355` and the write
lock at `settings.rs:1360–1376`.

**Problem and trigger:** Every malformed settings file is renamed to the fixed
name `settings.json.corrupt`. On this Mac, a second quarantine overwrites the
first recovery copy; the extracted-function probe confirmed only the second
payload survives. Quarantine is also outside the settings-write lock, so a
reader can diagnose old bytes and subsequently rename a newer successful
write at the same path.

**Proposed fix:** Use unique, no-clobber recovery names and perform quarantine
under the existing process/cross-process settings lock. Re-read or verify the
file identity after locking before moving it. Preserve diagnostics linking
the failed load to its exact backup. **Effort: S–M.**

**Acceptance:** Repeated corrupt loads preserve both byte sequences. A barrier
test replaces the bad file with a valid settings write before quarantine;
the valid replacement must remain active.

### 19. Use the validated URL parser for local-provider readiness

**Evidence:** `gui/src-tauri/src/deps/checks.rs:163–203` and
`gui/src-tauri/src/pipeline/api_openai.rs:410–416`.

**Problem and trigger:** Readiness reparses the URL by splitting its authority
on the last colon. A valid IPv6 URL with the default port, such as
`http://[::1]/v1`, produces `None` and is rejected before any HTTP probe. The
extracted-function probe confirmed this; the explicit-port form succeeds.
The request adapter already uses the standard URL validator.

**Proposed fix:** Reuse the `reqwest::Url` returned by validation, including
`host_str()` and `port_or_known_default()`, and build the model endpoint from
the normalized URL. Remove the hand parser and stale TCP-only comments.
**Effort: S.**

**Acceptance:** Cover IPv4, hostnames, bracketed IPv6 with and without ports,
trailing slashes, and accepted whitespace normalization. Readiness and request
dispatch must agree on the endpoint.

### 20. Give release identity checks complete tag evidence

**Evidence:** `.github/workflows/release.yml:53–57`,
`scripts/release/validate-release-identity.mjs:51–68,149–174,189–195`, and
`scripts/release/validate-signed-tag-binding.sh`.

**Problem and trigger:** The workflow uses checkout defaults and then compares
the proposed version only with locally available tags. It does not request
the complete tag history or query published versions. A shallow checkout can
miss a newer release, allowing an old tag to pass the monotonicity check. The
existing signed-tag/SHA binding script is not invoked either, despite the
release guide promising that gate.

**Proposed fix:** Fetch the complete relevant tag set or query an authoritative
paginated release/tag inventory. Run the existing signed-tag binding check
against the immutable workflow SHA before packaging/publication. Make older
version and existing-publication handling explicit. **Effort: S–M.**

**Acceptance:** Simulate a shallow checkout of an older release while a newer
stable release exists remotely. Reject it, plus lightweight, unverified, or
retargeted tags under the documented signing policy.

### 21. Replace tests that require missing quality gates with a deliberate verification policy

**Evidence:** `.github/workflows/build.yml:3–4,19–61`,
`.github/workflows/release.yml`, and
`scripts/release/workflow-hardening.test.mjs:42–91`.

**Problem:** Ordinary tests run only by manual dispatch, only on Linux, and
omit frontend build, formatting, Clippy, and CLI-target coverage. Packaging
does not depend on those tests. The hardening tests explicitly assert that
automatic triggers, platform test coverage, and release verification commands
are absent, so adding a quality gate makes those tests fail. This is a
deliberate current configuration, not an unexplained test failure.

**Proposed fix:** Agree on a cost-conscious policy: automatically run the
deterministic frontend/build/Rust gates for relevant changes and require
verification of the packaged SHA before publication. Keep expensive live
qualification explicit. Add targeted platform path/process coverage where
behavior differs. Test required outcomes and ownership, rather than asserting
that useful checks must never exist. **Effort: M; policy recommendation.**

**Acceptance:** A change with a TypeScript compile error or failing CLI test
cannot reach publication unnoticed. Reusing evidence requires the exact same
SHA and declared platform/feature coverage.

### 22. Make bundled Poppler identity match its lock and notices

**Skipped by explicit user instruction. The proposal below is retained only as review history.**

**Evidence:** `scripts/release/bundle-poppler.sh:16–38,41–59,62–82`,
`.github/workflows/release.yml:80–105`, and `THIRD_PARTY_LICENSES.md:18–36`.

**Problem:** macOS copies the available Homebrew installation; Linux copies
packages installed from ordinary current APT sources. These paths do not
enforce the Homebrew commit, Ubuntu snapshot, or exact versions described in
the lock/notices. Windows checks the provider ZIP hash but does not execute
the documented per-PE conda ownership verification. The packaging path does
not generate the platform provenance described by those documents. A cache
key containing the lock's hash does not verify the cached payload's identity.

**Proposed fix:** Either restore enforcement of the declared pinned inputs or
deliberately revise the input policy and generate truthful notices from the
actual resolved closure. Verify cached inputs before reuse; retain a
machine-readable inventory of the exact shipped files and dependencies.
Reconnect existing provenance helpers where appropriate. **Effort: M–L.**

**Acceptance:** Build from a clean runner and from a cache. Both produce a
verified inventory matching the packaged files and notices; mismatched
versions/hashes fail before upload. This finding concerns source traceability;
no particular installed version or vulnerability is asserted here.

### 23. Rewrite the release guide around the implemented and agreed release contract

**Evidence:** `RELEASING.md:3–5,16–28` and its “Tag, build, and review” section;
compare `.github/workflows/release.yml:3–16,139–184` and
`scripts/release/prepare-package-notices.mjs`.

**Problem:** The guide says tags start builds, Windows signing is mandatory,
the workflow verifies a 20-asset draft, scans SBOMs, produces attestations, and
leaves publication to a human. The current workflow is manually dispatched,
builds intentionally unsigned Windows packages, uploads installers, and
publishes automatically. Several documented jobs and artifacts are absent.
Operators cannot follow the guide to understand the current release.

**Proposed fix:** After resolving 07 and 20–22, update the guide in the same
change as the workflow. Clearly distinguish implemented checks, required
manual checks, and future work. Remove nonexistent job names and secret
requirements, or restore their implementation where that is the chosen
policy. Reconcile README/notices claims about bundled evidence as part of
that pass. **Effort: S–M.**

**Acceptance:** A maintainer can follow the guide from a clean checkout to the
intended draft/publication state; every named job, secret, and generated
artifact has a current producer or an explicit manual procedure.

## P3 — cleanup and maintainability

### 24. Remove unreachable Antigravity CLI readiness code

**Status: implemented in the authorized follow-up.** Removed
`pipeline/antigravity.rs`, CLI model discovery/parsers, version/authentication
probes, installation hints, CLI dispatch and effort controls, and the unused
bundled CLI policy entry. Google uses the API exclusively; readiness is labeled
Google API. Legacy provider IDs and serialized settings remain compatible.
Regression coverage checks API-only readiness for legacy subscription settings,
rejection of CLI discovery before subprocess launch, saved-field roundtripping,
and frontend API readiness.

**Follow-up verification:** 951 Rust tests passed (937 library and 14 CLI;
10 existing tests ignored), 49 focused frontend tests passed, production build
and strict all-target/all-feature Clippy passed. After the final policy cleanup,
the 14 model-catalog tests and legacy-settings roundtrip test passed again.
Formatting, source-size, patch-whitespace and documentation-link checks passed.
The original finding follows for review history.

**Evidence:** `gui/src-tauri/src/deps/checks.rs:34–55,118–151` and its
test-only callers in `gui/src-tauri/src/deps/tests.rs`.

**Problem:** Google subscription dispatch is disabled and startup deliberately
does not probe `agy`, but the version gate, authentication subprocess helper,
timeouts, and associated tests remain. `allow(dead_code)` keeps this unused
production path around. It is retained implementation code rather than a
necessary serialized compatibility field.

**Proposed fix:** Delete unreachable probe mechanics and their now-irrelevant
tests. Preserve legacy provider IDs, migrations, and persisted formats that
remain in use. Keep one focused regression asserting Google API-only
readiness never launches `agy`. **Effort: S.**

**Acceptance:** Repository search finds no production callers of removed
helpers, API-only readiness coverage passes, and old profiles still migrate.

### 25. Correct the README's cross-mode bridge description

**Evidence:** `README.md:45–49` says the paper handoff is the only implemented
bridge. `docs/workbench/README.md:34–40` describes finding exchanges;
`docs/tasks.md` and `gui/src-tauri/src/orchestration/` describe the coordinator.

**Problem:** The introductory architecture description excludes implemented
immutable finding exchanges and coordinated cross-mode tasks. This can send
contributors toward the wrong integration assumptions even though the
detailed documents are more current.

**Proposed fix:** Summarize the three explicit integration paths and link to
their owning documentation. Retain the separation of stores, credentials,
cancellation, and mutable runtime state. **Effort: S.**

**Acceptance:** README, CLAUDE.md, Workspace documentation, and Tasks
documentation describe the same ownership boundaries and bridge inventory.

### 26. Give the remaining oversized modules explicit cleanup ownership

**Evidence:** `scripts/source-size-report.mjs --check` reports 18 oversized
source/test files. Eight were newly oversized or growing against the baseline
at the start of this review; concurrent work has since accepted them into the
baseline. Examples include
`workbench/research/execution.rs` (1,560 lines / 67,499 bytes),
`workbench/release/archive.rs` (1,247 / 53,101), and the large Settings,
Pipeline, program, orchestration, and project tests. The existing exchange
owner remains 1,973 lines / 85,326 bytes. Paths are under `gui/src-tauri/src/`
or `gui/src/` as appropriate.

**Problem:** The check now passes, but accepted size measurements do not remove
the responsibility and review burden in these owners. Several files still
combine large execution/persistence paths or many independent test behaviors.
This is maintenance debt, not evidence of a runtime failure by itself.

**Proposed fix:** Give the remaining exceptions owners and concrete split
boundaries in the existing refactoring plan. Prioritize execution, archive,
and exchange responsibilities, and divide oversized tests by behavior. Do not
restart completed refactors or move a large component wholesale into an
equally large hook. Preserve the new growth gate and review subsequent
baseline changes explicitly. **Effort: M–L.**

**Acceptance:** Selected owners become cohesive modules within budget or have
documented exceptions. Test discovery and coverage remain intact, and new
growth is surfaced without repeatedly resetting the baseline.

## Validation and delivery discipline for the fixes

Land small coherent changes with the relevant regression alongside each fix.
For 01, 03–06, 10–11, and 14–17, preserve existing command names and persisted
formats where possible; introduce explicit migrations or compatibility readers
when a new durable identity/receipt is necessary. Do not edit applied SQL
migrations or checksum-pinned protocol reference files.

After each owning subsystem is repaired, run its focused tests. At integration,
run the frontend tests/build/format checks, Rust all-targets tests/format/Clippy,
release-script tests, documentation links, and the agreed size check. Add
scripted transport and fault-injection coverage for the identified lifecycle
and recovery composition gaps. Provider protocol/process changes also require
the repository's applicable no-model probes and, separately, the documented
authenticated/packaged/platform qualification before making release claims.

The temporary reproduction evidence is available locally at
`/tmp/astra-frontend-review/reproduction.log` and
`/tmp/astra-sep8-source-probes/`. Full check logs use
`/tmp/astra-sep8-*.log`. These are disposable audit aids; the triggers and
acceptance conditions above are the durable plan and do not depend on keeping
those temporary files.
