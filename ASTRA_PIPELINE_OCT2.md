# Pipeline pre-release audit — October 2, 2026

**Audited revision:** `0705a6c489ae9ce09445c6bc49e0d1dd9e82a90e` (`main`, version `0.9.5`). The working tree was clean at the start of the audit. The initial audit made no implementation changes. The user subsequently authorized P1/P2 fixes and then the remaining P3 fix; their disposition and validation are recorded below.

**Release recommendation: hold for qualification.** Implementation/build/documentation findings F01–F07 and F09–F16 have fixes in the working tree. F08 remains open: authenticated, packaged, and cross-platform evidence is incomplete, and upstream development-tool advisories remain unresolved. Nothing has been committed, packaged, or published by this remediation. F16 (P3) was fixed in the subsequent authorized follow-up.

## Remediation status — October 2

“Implemented” below means code and focused regression coverage exist. It does not certify real-account or packaged behavior. The later sections preserve the original audit evidence and proposed acceptance criteria; they describe the audited revision, not the fixed working tree.

| Finding | Status | Implementation and verification |
|---|---|---|
| F01 | Implemented | Export/import strip `storage_trash`; retention validates identifier, allowed storage category, exact destination and symlink-free ancestors before filesystem actions. Synthetic foreign-path and symlink tests preserve sentinels. See [path validation](gui/src-tauri/src/workbench/release/retention/paths.rs) and [archive regressions](gui/src-tauri/src/workbench/release/archive/regression_tests.rs). |
| F02 | Implemented | Production side calls require a managed account and launch an independently owned native process in a temporary home. Cleanup retains its account lease and cannot terminate the foreground process. [Ownership tests](gui/src-tauri/src/workbench/codex/supervisor/side_ownership_tests.rs) and the isolated native startup/shutdown probe pass. Authenticated overlap remains part of F08. |
| F03 | Implemented | Independent ceilings bound control decisions, interpreter visits and progress bytes. Every store save checks the context bound; the coordinator preserves prior bounded state, requires attention and stops before child dispatch when adoption exceeds it. Full action evidence remains in its durable adapter journal. [Coordinator regressions](gui/src-tauri/src/orchestration/tests/coordinator.rs) cover overflow at save and immediately before dispatch. |
| F04 | Implemented | Recovery-cache reads/writes/removals are caught; unread recovery data is not deleted; the mounted workflow and dirty state survive. An alert directs the user to Save or Export. Both failure paths have [regressions](gui/src/components/pipeline-editor/useWorkflowEditor.test.ts). |
| F05 | Implemented; audit corrected | Workspace receives typed route identity, flushes drafts on same-page navigation, and invalidates superseded asynchronous loads. Same-project destination changes preserve the conversation. Optional navigation-cache failures do not block the entry. The original audit overlooked an existing route subscription; the confirmed defects were missing same-page guarding and stale-response protection, not a complete absence of mounted routing. See [entry owner](gui/src/hooks/useWorkspaceRouteEntry.ts) and [controller](gui/src/hooks/useWorkspacePageController.ts). |
| F06 | Implemented | Direct-API Write now uses the same bounded atomic write ownership rule as native Write: mutation settles in the current poll before cancellation/finalization can proceed. [Regression](gui/src-tauri/src/pipeline/api_common/execution/write_ownership_tests.rs) covers completion before a subsequent attempt and dropping an unpolled invocation. Slow-filesystem responsiveness still needs F08 performance qualification. |
| F07 | Implemented | Rust formatting corrected; adaptive workflow UI tests and archive/coordinator tests split into coherent modules. No source-size baseline increase. |
| F08 | **Open — release blocker** | Added a [candidate evidence manifest](docs/releases/1.0.0-qualification.json) and [validator](scripts/release/qualify-release.mjs). Pending rows, commit mismatch and modified evidence/installer files fail the gate. These controls do not manufacture missing live/platform observations. The manifest intentionally remains pending. |
| F09 | Implemented | Import retires bindings and records unfinished turns as interrupted with an explicit unknown-outcome message, without replay. Hydration only adopts activity from the matching active binding. Archive and successor-binding regressions pass. |
| F10 | Implemented | Matching task pins on authoritatively failed/cancelled Reviews are released after execution settles and reconciled at startup. Explicit stop/retry abandonment is recoverable from durable records and releases matching settled pins when the Review runtime is idle. Active owners and un-abandoned uncertain/successful outcomes remain pinned. Mismatched journals and symlinks fail closed. [Pin tests](gui/src-tauri/src/commands/orchestration/pins.rs) cover ownership and terminal-state distinctions. |
| F11 | Implemented | Automation entries, including repeated links, reset selection through an explicit request. Stale detail responses are discarded. Builder edits participate in navigation/close guarding. [Page regressions](gui/src/components/TasksPage.test.tsx) cover task replacement and repeated entries. |
| F12 | Implemented metadata/build changes | Removed unenforced Homebrew/APT pins, exhaustive DLL-matching claims and promised SBOM production. The active bundler records tool version and hashes of actual files; exact-key cache hits are verified and inventories regenerate after signing. [Release tests](scripts/release/release-fixes.test.mjs) detect changed, missing and extra cached files. Platform package inspection and distribution review remain F08 obligations. |
| F13 | Implemented | Unsigned local macOS builds skip release signing; official builds require identity and select the exact CLI output. Default, explicit-triple and custom target paths have release-tool tests. Full installer builds remain unqualified. |
| F14 | Implemented | README now describes the shared account, implemented bridges, Connections settings and actual inventory contract. Storage recovery directs users to Data & Storage. |
| F15 | Implemented | `.pwrx` format 2 includes bounded, staged, checksummed app-owned conversation working files; format 1 remains readable. Export requires idle turns/jobs. Restore validates ownership and payloads and refuses to overwrite existing conversation files. UI/docs disclose exclusions. Round-trip, corruption, symlink and collision regressions pass. |
| F16 | Implemented | Malformed percent escapes and invalid UTF-8 in route segments return `null`, preserving startup and hash-navigation fallbacks. Unreadable saved navigation is optional. [Router regressions](gui/src/lib/router.test.ts) cover corrupt persisted/initial/changed routes, subsequent valid navigation, and valid Unicode/encoded delimiters. |

### Validation of the fixed working tree

- Frontend: **790 tests pass in 107 files**, including initial deep-link draft preservation, rapid route transitions, and the F16 malformed-route/storage regressions. Production build and frontend formatting pass. Vite retains its existing large-chunk advisory.
- Rust: **983 library and 14 CLI tests pass**, with 11 opt-in library tests ignored. Strict Clippy and Rust formatting pass. The opt-in **isolated no-model Codex startup/shutdown probe passes**; it made no authenticated model request.
- Release tooling: **63 tests pass**, release identity `0.9.5` validates. Signing tests use synthetic identities and paths; they do not sign installers.
- Source-size and Markdown-link gates pass. The baseline was not relaxed. Applied SQL migrations, saved workflow formats, and live user stores were not changed. Dependency patches and their remaining advisory limits are recorded below.
- The user subsequently approved the online npm audit. The production-only audit reports **zero vulnerabilities**; the full audit retains **16 high-severity development-dependency entries** from three root dependencies. A refreshed RustSec database identified a rustls issue; the candidate patches it to `0.23.45`. See the dependency disposition below. Existing Rust exception review and yanked-package checks remain release work.


### Dependency disposition after the approved online audit

The initial online npm audit reported 22 affected package entries (19 high, 3
moderate), all in development tooling. Compatible targeted fixes update Vitest
and its mocker to `4.1.11`, brace-expansion to `5.0.12`, ip-address to `10.7.3`,
js-yaml 4.x to `4.3.2`, and Undici 6/7/8 to `6.28.1` / `7.29.1` / `8.10.2`.
The final `npm audit --package-lock-only --json` reports 16 high entries, all
propagated through WebdriverIO/browser-download tooling from these root issues:

| Root dependency | Remaining issue | Required disposition before qualifying the development/test environment |
|---|---|---|
| `extract-zip` 2.0.1 | [Archive symlink path traversal](https://github.com/advisories/GHSA-jmr9-qjv8-65gv) and [arbitrary file writes](https://github.com/advisories/GHSA-7pqw-9j4j-h8q3); no patched release is published in its current line | Adopt an upstream browser-manager release that removes this extractor, or replace the browser-download owner with a reviewed implementation. Validate browser installation and Tauri E2E on every supported platform. Do not override the major browser-manager API without compatibility testing. |
| `basic-ftp` 5.3.1 | [Unbounded directory-listing parser CPU](https://github.com/advisories/GHSA-c475-qrg2-pj4r); patch starts in 6.2.1, outside the caller's 5.x range | Upgrade the proxy/URI dependency chain to a compatible patched release and exercise proxy/browser provisioning. |
| `deepmerge-ts` 7.x | [Recursive graph stack exhaustion](https://github.com/advisories/GHSA-ggr8-5vv4-36mx); patch requires 8.x | Coordinate WebdriverIO and the Tauri service's pinned WebdriverIO dependency so all copies use patched merge semantics; rerun embedded-driver E2E. |

These are **not resolved or waived** by their development-only classification.
`npm audit --omit=dev --package-lock-only --json` reports zero affected production
packages; this is a dependency-set result, not a claim of exhaustive application
security. Forced downgrades/major overrides that merely silence npm's tree report
were not retained. F08 remains open for these upstream/test-environment decisions.

The fresh RustSec database (commit
`117edb3bed98e9be112f277b7615eea3252e7c43`, 1,280 advisories) identified
[RUSTSEC-2026-0285](https://github.com/rustls/rustls/security/advisories/GHSA-2mjx-qc3c-rqvc)
in locked rustls `0.23.42`. The lockfile now uses patched `0.23.45` and compatible rustls-webpki `0.103.15`. The fresh-db
recheck reports zero unsuppressed vulnerabilities or warnings using `cargo audit --no-fetch --no-yanked --json`: yanked-package checking
is deliberately not claimed, and the 17 existing advisory exceptions have not
been broadened. Final test/lint results above refer to this patched candidate.

The archive format change is deliberate: older Pipeline builds cannot read new format-2 backups. Files outside the app-owned conversation store are not swept into backups. The direct-API write fix guarantees ownership by completing a bounded write synchronously; network-volume latency must be measured before making performance claims.


## Scope and evidence

The review combined repository-wide inventory and release checks with parallel, targeted source review of Workspace/native lifecycle, archives and retention, Reviews/providers, Tasks, the React frontend, packaging, and current documentation. It followed `AGENTS.md`, `CLAUDE.md`, the owning feature documents, and Workspace compatibility/qualification policy. Historical audits were used as context, not treated as proof that an old finding remains open.

Evidence labels below distinguish:

- **Reproduced:** observed command failure or a small isolated regression probe using the current implementation.
- **Source-confirmed:** a concrete failure path traced through the current implementation; its end-to-end native scenario was not executed.
- **Evidence gap:** a required release claim is not established by the checked-in evidence or this audit.
- **Documentation mismatch:** current prose contradicts implementation or another authoritative current document.

This is a broad pre-release audit, not proof that every path is correct. It does not claim authenticated provider qualification, installer qualification, a destructive security demonstration, Windows/Linux execution, or exhaustive dependency-advisory coverage. UI probes used mocked native boundaries; the control-flow probe exercised the validator/interpreter without running the desktop coordinator or model calls. All temporary probes were outside the repository and used synthetic data.

### Validation performed

| Check | Result | Interpretation |
|---|---|---|
| `npm test` with Node `24.18.0` | **Pass:** 105 files, 773 tests | Existing frontend suite |
| `npm run build` with Node `24.18.0` | **Pass** | Type checking and production bundle; includes a large PDF-reader chunk warning and a Vite configuration-loader warning |
| `npm run format:check` | **Pass** | Frontend formatting |
| `npm run test:release` | **Pass:** 60 tests, identity `0.9.5` | Rerun outside sandbox for a local fixture HTTP server; the initial bind denial was environmental |
| `cargo test --locked --all-targets`, Rust `1.97.1` | **Pass:** 969 library and 14 CLI tests; 11 library tests ignored | Opt-in live/tool qualification was not run |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | **Pass** | Strict native lint |
| `cargo fmt --all -- --check` | **Fail** | Module ordering in `workbench/project.rs`; F07 |
| `node scripts/source-size-report.mjs --check` | **Fail** | `PipelinePage.test.tsx` grew beyond its accepted baseline; F07 |
| `node scripts/check-markdown-links.mjs` | **Pass:** 523 files before this report | Resolves file targets, not semantic correctness of documentation |
| Isolated frontend regression probes | **Fail as expected:** quota handling, same-page task entry, same-page conversation entry | Newly uncovered behaviors in F04, F05, F11 |
| Isolated task validator/interpreter probe | **Budget bypass reproduced** | F03; no model or host-tool execution |
| macOS pre-bundle hook without signing identity | **Fail reproduced** | F13; no signing or packaging attempted |
| `cargo audit --no-fetch --no-yanked --stale --json` | No unsuppressed vulnerabilities/warnings reported for 606 locked dependencies | Cached database of 1,242 advisories; freshness unavailable; 17 configured exceptions; yanked checks disabled |
| Online npm advisory audit | **Not run: automatic approval review rejected the request** | It would transmit dependency names/versions to npm's advisory service; permission is needed for that exact disclosure |

The read-only source inventory contained 1,760 relevant files and 17 source/test files above the size threshold. Existing accepted size debt is not automatically a release defect; the newly growing file is a real CI failure. No model spending, Stata invocation, account mutation, installer installation, release publication, or application-data migration was performed.

## Prioritized findings

| ID | Priority | Area | Finding | Evidence |
|---|---|---|---|---|
| F01 | P1 | Archive / retention | Restored trash records retain authority over foreign filesystem paths | Source-confirmed |
| F02 | P1 | Workspace lifecycle | Failed automatic title generation can terminate a concurrent conversation | Source-confirmed |
| F03 | P1 | Tasks | Control-only loops bypass action/context budgets and monopolize the coordinator | Reproduced |
| F04 | P1 | Workflow editor | A full recovery cache crashes the editor and discards its in-memory draft | Reproduced |
| F05 | P1 | Project navigation | Same-page guards and asynchronous identity transitions are incomplete | Corrected on implementation review; see remediation status |
| F06 | P1 | Review host tools | Direct-API writes can outlive cancellation or timeout | Source-confirmed |
| F07 | P1 | Release gates | Two mandatory CI gates fail on the audited commit | Reproduced |
| F08 | P1 | Release qualification | Required authenticated, packaged, and platform evidence remains open | Evidence gap |
| F09 | P2 | Archive / conversation recovery | Unfinished restored turns can be paired with the wrong native binding | Source-confirmed |
| F10 | P2 | Task / Review retention | Failed or cancelled task-owned Reviews can remain permanently pinned | Source-confirmed |
| F11 | P2 | Automation navigation | A changed task deep link is ignored while Automations remains mounted | Reproduced |
| F12 | P2 | Distribution documentation | Shipped notices and README overstate actual Poppler provenance/SBOM production | Documentation mismatch |
| F13 | P2 | Source build | Documented macOS source build is blocked by the unconditional signing hook | Reproduced hook failure; build path traced |
| F14 | P2 | User documentation | README describes obsolete account ownership, bridges, and settings navigation | Documentation mismatch |
| F15 | P2 | Backup completeness | “Export all research data” omits rootless conversation working files | Source-confirmed / UI contract mismatch |
| F16 | P3 | Route parsing | Malformed encoded route segments throw instead of falling back | Reproduced |

### F01 — Restored trash records retain foreign filesystem authority

**Impact and trigger.** A `.pwrx` archive restores the entire SQLite database, including `storage_trash`, but does not package trash payloads or rebase/retire their absolute paths. A normal archive imported into another store therefore exposes stale trash entries. If the source store still exists, Restore can move material inside the source store; if it is gone, the destination offers recovery actions for nonexistent payloads. In addition, imported IDs and paths are not sufficiently validated before destructive filesystem operations. A syntactically valid imported database can carry values that escape the intended trash directory.

**Evidence.** [Archive export](gui/src-tauri/src/workbench/release/archive.rs#L333) snapshots the database and collects `blobs` at lines 367–405. [Import reset](gui/src-tauri/src/workbench/release/archive.rs#L976) clears several authority-bearing tables but not `storage_trash`, then copies the database at lines 1010–1015. [Database validation](gui/src-tauri/src/workbench/release/archive.rs#L574) checks SQLite integrity and foreign keys; those checks do not establish path authority. [Trash schema](gui/src-tauri/src/workbench/migrations/009_project_exchange.sql#L26) imposes no path/identifier restrictions. [Restore](gui/src-tauri/src/workbench/release/retention.rs#L463) trusts stored source/destination paths and joins the stored ID during cleanup. [Empty Trash](gui/src-tauri/src/workbench/release/retention.rs#L499) uses a lexical `starts_with` check after joining the ID; lexical containment does not reject parent traversal. `list_trash` reconciles existence, not ownership.

**Fix plan.** Define trash as device-local operational state and strip its rows from portable snapshots/restores, or implement a fully specified portable-trash format with packaged bytes and destination-owned paths. The first option is the smaller release fix. Independently validate trash IDs as single approved components and derive paths from validated relative records under the current store. Reject traversal, symlinked ancestors, and foreign roots before any rename or deletion. Audit other imported operational journals under the same rule; safe ZIP entry names do not establish safe database values. Preserve existing migrations; add a migration only if the stored representation changes.

**Acceptance.** Round-trip a store with trashed material into a second disposable store, with the source both present and absent. Operations in the destination must never touch the source. Add negative fixtures for malformed IDs, foreign absolute paths, and symlinked ancestors; assert rejection and unchanged sentinel files. Confirm normal prune/restore/empty and interrupted-move recovery still work. No destructive reproduction against real data was attempted in this audit.

### F02 — Background title failure can kill an active conversation

**Impact and trigger.** After a normal turn finishes, title generation starts asynchronously on the same native process. The next user turn can begin while that title call is running. If the title returns a failed/interrupted terminal status, produces no text, times out, or otherwise fails after submission, its drop guard terminates the shared native connection. The foreground conversation is collateral damage.

**Evidence.** [Completion bridge](gui/src-tauri/src/workbench/commands/codex.rs#L640) releases the main turn permit before spawning title generation. [Title spawning](gui/src-tauri/src/workbench/commands.rs#L492) runs independently. [Side-turn setup](gui/src-tauri/src/workbench/codex/supervisor.rs#L327) retains `self.native`; only `Ok(Ok(text))` marks the call completed at lines 377–382. [Collection](gui/src-tauri/src/workbench/codex/supervisor.rs#L441) returns errors even for authoritative terminal failure or empty completed output. [Drop cleanup](gui/src-tauri/src/workbench/codex/supervisor.rs#L1088) calls `native.terminate()` for a submitted, non-completed call with an account lease.

**Fix plan.** Give background model calls an independently owned native process, or design cleanup around their exact thread/turn identity with a proved terminal state. Distinguish “native turn settled” from “title text accepted”; an authoritative failed/empty terminal result is settled even when the feature returns an error. Preserve account ownership through uncertain cleanup, but do not terminate unrelated foreground work. Avoid fixing this merely by dropping the account lease early.

**Acceptance.** Simulate a foreground turn concurrently with a title call that fails, completes empty, requests a tool, times out, or is dropped. The title must fail visibly without killing the foreground connection. Test account-switch rejection until actual cleanup and a successful later title. Follow with a narrow authenticated title/foreground overlap check during qualification.

### F03 — Control-only loops bypass task budgets

**Impact and trigger.** Valid nested loops containing only conditionals/control steps expand inside one synchronous interpreter call. They produce no leaf receipts or budgeted actions, so `maxActions` and the existing instance check do not constrain them. A small definition can consume substantial CPU/memory while holding the coordinator gate, blocking task control and completion processing. Larger permitted definitions can make the application effectively unresponsive.

**Reproduction.** A 911-byte valid chain with four nested `Repeat(maxIterations=20)` nodes, each stop condition false, and an innermost false `If` with empty branches was accepted with `maxActions=1`, `deadlineHours=1`, and `actionTimeoutSecs=30`. One `advance` returned `Finished` with **0 actions, 0 receipts, 496,841 decisions, and 23,040,954 serialized progress bytes**, taking approximately 1.23 seconds in the isolated local probe. This already exceeds the coordinator's nominal 8 MiB retained-context budget. The probe used the current interpreter and validator, with only dependency wiring redirected outside the repository; no expensive full-depth stress run was needed.

**Evidence.** [Validation](gui/src-tauri/src/orchestration/definition.rs#L254) allows eight nesting levels and loops of 1–32 iterations. [Interpreter entry](gui/src-tauri/src/orchestration/state.rs#L54) limits receipts, not transitions/decisions. [Repeat expansion](gui/src-tauri/src/orchestration/state.rs#L109) has no transition budget. [Coordinator tick](gui/src-tauri/src/orchestration/mod.rs#L162) holds its gate and calls `advance` synchronously at line 230. The action/context checks at lines 314–316 are in the leaf-ready branch, so `Finished` bypasses them.

**Fix plan.** Add a per-tick transition budget and a durable cursor/yield result. Count every control transition and decision, independently of side-effect action budgets. Enforce retained-state bytes/count limits during expansion and before every save, including `Finished`. Check cancellation/deadline between bounded slices. Add conservative structural expansion validation as a second defense, using checked arithmetic. Bound accumulated history or compact completed control state without changing output/provenance semantics.

**Acceptance.** Keep the bounded fixture above as a regression: it must yield or stop with an explicit budget result before oversized progress is allocated. Cover `Repeat`, `While`, `ForEach`, nested chains and parallel controls. Verify another task's Stop and completion can acquire the gate promptly, restart resumes at a stable cursor, and ordinary loops retain correct semantics.

### F04 — Recovery-cache failure crashes the workflow editor

**Impact and trigger.** Editing a workflow when `localStorage` is full throws from the draft-persistence effect. The top-level error boundary replaces the application, unmounting the editor and losing its unsaved in-memory state. Retrying can repeat the failure. This is credible in normal use: the file editor uses the same origin storage for both original and edited source text.

**Evidence and reproduction.** [Workflow persistence](gui/src/components/pipeline-editor/useWorkflowEditor.ts#L50), lines 55–57, calls `setItem`/`removeItem` without handling storage failures. [The application boundary](gui/src/main.tsx#L14) wraps the whole app; [retry](gui/src/components/ErrorBoundary.tsx#L54) remounts its children. An isolated regression probe injected `QuotaExceededError`, edited a workflow, and observed “Something went wrong” in place of the editor. [File draft retention](gui/src/components/file-workspace/FileWorkspace.tsx#L213) stores original plus draft text and already catches quota errors, illustrating the intended behavior elsewhere.

**Fix plan.** Make optional recovery-cache writes/removals nonfatal. Preserve the live draft, undo history, dirty state, and explicit Save/Export actions; show a persistent warning when crash recovery could not be saved. Centralize small safe preference/cache operations where useful, but do not silently swallow failures of actual durable document saves. Audit adjacent unguarded preference writes, including `useWorkspacePageController.ts` lines 364–373 and route-side storage updates.

**Acceptance.** Inject quota and security failures for read/write/removal. The editor must remain usable with the exact draft and undo history, explicit save must still work, and the warning must disappear after recovery succeeds. Test interaction with large file drafts sharing storage.

### F05 — Same-page project/conversation transitions lack complete guarding

**Correction to the initial audit.** The audited controller already subscribed to router commits. The initial isolated probe changed storage and entry props without traversing that subscription, so it did not establish that ordinary mounted navigation was always ignored. The earlier broader claim is withdrawn.

**Confirmed impact and evidence.** The same-page early return in [the shell guard](gui/src/hooks/useAppController.ts) bypassed Workspace's registered draft/editor guard. The old router subscription in [the Workspace controller](gui/src/hooks/useWorkspacePageController.ts) started asynchronous snapshot/list/selection work without a request-generation check, allowing a superseded load to commit after a later route. Entry identity was split between local-storage initialization, pane props and this subscription. These gaps justified a single explicit target and guarded, cancellable transition.

**Fix plan.** Pass a typed entry request containing project/session/destination identity into the controller and process it as a guarded selection transition. Save or retain the outgoing draft, honor active-turn/navigation rules, invalidate superseded hydration, load the requested target, and update the route/selection consistently. The same-page early return in `useAppController.ts`'s `confirmLeaveCurrentPage` also needs identity-aware guarding. Do not add an unconditional remount key that discards drafts or active-turn state. Treat destination navigation as an explicit request rather than an out-of-band local-storage write.

**Acceptance.** At the App boundary, open A, edit its composer, click recent project B, and verify B is selected while A's draft survives. Cover same-page conversation links, back/forward, same-project destination changes, rapid A→B→C navigation, and active-turn constraints. Confirm destructive/edit actions receive the displayed project's identity.

### F06 — Direct-API writes can outlive their invocation

**Impact and trigger.** Direct-API `Write` runs in a blocking task. Cancellation or the 30-second file-tool timeout returns control while that task can continue writing. Slow storage, a delayed flush, or a queued blocking task can therefore publish files after its owning invocation has returned, interfering with artifact inspection, retry, or run finalization. The issue is ownership across cancellation; the existing bounded atomic write does not resolve it.

**Evidence.** [Blocking tool wrapper](gui/src-tauri/src/pipeline/api_common/execution.rs#L106) spawns the closure and awaits it through cancellable/timeout futures. [Direct-API Write](gui/src-tauri/src/pipeline/api_common/execution.rs#L653) uses that wrapper. [The write implementation](gui/src-tauri/src/pipeline/api_common/models.rs#L382) stages, syncs, and then persists the destination. In contrast, [native tool Write](gui/src-tauri/src/pipeline/api_common/execution.rs#L707) explicitly completes in the current poll to prevent a detached mutation racing artifact ingestion. The direct-API route lacks equivalent ownership.

**Fix plan.** Use one cancellation-safe mutation owner for host writes across provider transports. Do not let the call/run become terminal or release write-root ownership until an already-started mutation has settled. Add cancellation checkpoints before commit and isolate attempts where retries can reuse a producer root. A bounded inline implementation consistent with the native path may be an interim fix, but assess its effect on the async executor and slow filesystems; dropping a `spawn_blocking` handle is not a fix.

**Acceptance.** Gate a synthetic write before commit, request cancellation/timeout, begin cleanup or retry, and release the gate. Prove no old write can mutate a later attempt or an artifact tree already treated as final. Cover provider error, schema repair, run-wide cancellation, and quota accounting. No slow-filesystem race was forced against user data in this audit.

### F07 — Mandatory source-size and Rust formatting gates fail

**Impact.** The audited commit cannot pass the repository's own quality workflow, which is a dependency of release preparation. These are release blockers even though the functional suites pass.

**Evidence.** `cargo fmt --all -- --check` requires the test-module order to change at [project.rs](gui/src-tauri/src/workbench/project.rs#L35). `source-size-report.mjs --check` reports [PipelinePage.test.tsx](gui/src/components/PipelinePage.test.tsx) at **1,406 lines / 49,380 bytes**, versus its [accepted baseline](docs/refactoring/source-size-baseline.json#L18) of **1,395 / 48,875**. [CI](.github/workflows/build.yml#L64) runs both checks before later quality steps. The remaining oversized files are accepted baseline debt, not new failures.

**Fix plan.** Apply rustfmt to the affected module order. Split the growing test file along coherent behavior groups with small shared fixtures, preserving coverage. Do not refresh the size baseline just to hide growth. Keep any broader source decomposition separate from release bug fixes.

**Acceptance.** Both exact failing commands pass with the pinned toolchain, the extracted test groups still run, and the full quality workflow succeeds for the final candidate commit.

### F08 — Required live and packaged qualification is not established

**Impact.** The current evidence does not support a general release claim across all advertised platforms and enabled research capabilities. The repository correctly distinguishes deterministic/no-model tests from authenticated and packaged behavior; this audit cannot close those gates.

**Evidence.** [Workspace qualification](docs/workbench/release-qualification.md#L30) leaves authenticated dynamic tools, crash boundaries, platform performance, and several real-tool checks open. [Shared-account evidence](docs/chatgpt-account.md#L106) uses synthetic credentials and explicitly excludes genuine browser completion/token renewal/model turns. [Release instructions](RELEASING.md#L78) require clean-machine and packaged checks. [Release automation](.github/workflows/release.yml) builds/uploads/verifies installers but does not run those checks. Historical local GUI observations are useful evidence, but do not certify this commit or its final installer bytes. The latest online dependency audit is also unverified in this run.

**Fix/qualification plan.** Maintain one candidate-specific evidence manifest with commit, native/provider versions, OS/architecture, installer hash, scenario, expected outcome, observed outcome, and sanitized durable evidence. Execute the matrix below after the fixes. Either qualify the intended feature/platform or explicitly restrict the release scope and disable unsupported capabilities. Do not relabel unit tests as authenticated or packaged qualification.

| Gate | Minimum candidate evidence |
|---|---|
| Shared ChatGPT account | Real sign-in/cancel/sign-out; migration choices; real renewal; simultaneous independently owned Conversation/Review work; account change after cancellation |
| Workspace native tools | Real model request/approval/question/interrupt; denied authority expansion; exact turn ownership and reconnect; title/foreground overlap |
| Review providers | One supported path per advertised transport; bounded input/tool/result flow; cancellation; rejected output; resume without duplicate effects |
| Tasks / missions / discovery | Delay/schedule, sleep/wake, budget/deadline, stop/restart/reconciliation, immutable handoff, attention navigation; include F03/F05/F10/F11 scenarios |
| Research tools | Qualified LaTeX/Python paths; current machine-approved `stata` alias through interactive login zsh, cooperative cancellation, forced-termination cleanup, and pending exporter checks where enabled |
| Packaging | Clean install/upgrade/data preservation on macOS ARM/Intel, Windows client + missing WebView2, Linux/FUSE; relocated Poppler; actual macOS minimum OS and signature/notarization |
| Crash/storage | Process death around submission, tool receipt, artifact adoption, migration and edit/undo; archive into a separate store; no foreign-path mutation |
| Performance/accessibility | Bounded long transcripts and large PDFs, responsive Stop, cold/warm navigation, idle resource use, keyboard/screen-reader coverage |
| Dependencies/notices | Fresh approved advisory checks, reviewed exceptions, actual bundled component inventory, notices/source-distribution review and documented results |

**Acceptance.** All applicable rows have pass evidence bound to the final candidate or an explicit, implemented scope restriction. No unresolved P1 can be waived merely because a draft installer exists.

### F09 — Restored unfinished turns can create false active state

**Impact and trigger.** Export can snapshot an unfinished native turn. Import retires its binding but leaves the turn nonterminal. After a successor binding is created, hydration can select the old unfinished turn and combine its turn ID with the new binding's thread ID. The UI reports work in progress when no such turn exists, can disable sending, and directs Stop to a mismatched identity.

**Evidence.** [Export command](gui/src-tauri/src/workbench/commands/research.rs#L349) excludes active local jobs but not native conversation turns. [Import](gui/src-tauri/src/workbench/release/archive.rs#L987) retires bindings without updating unfinished turns. [Snapshot query](gui/src-tauri/src/workbench/store/views.rs#L64) includes turns across bindings. [Hydration](gui/src/hooks/useWorkspacePageController.ts#L283) chooses any unfinished provider turn and at lines 300–304 pairs it with `activeBinding`, without comparing binding IDs.

**Fix plan.** Reconcile imported unfinished work into an explicit outcome-unknown/interrupted record with preserved provenance; never replay it. Hydration must select activity only from the matching active binding and connection ownership. Apply the same rule to successor bindings after incompatible-runtime or scope changes, not just archive import.

**Acceptance.** Restore a synthetic in-progress archive, create/complete a successor turn, and verify idle composer state and correct Stop identity. Add a snapshot fixture containing old unfinished and new finished turns. Verify imported uncertainty remains visible in history while normal active-turn reconciliation still works.

### F10 — Failed/cancelled task-owned Reviews leak retention pins

**Impact and trigger.** A task-owned Review is pinned before execution. Failure/cancellation can leave `.task-pin` indefinitely, even after the automation is stopped or retried. The run is then undeletable through normal History controls and excluded from cleanup, so repeated failures accumulate retained data.

**Evidence.** [Review dispatch](gui/src-tauri/src/commands/orchestration.rs#L67) creates the pin. [Coordinator completion](gui/src-tauri/src/orchestration/mod.rs#L481) releases it only after a successful result; the error branch does not. [Reconciliation](gui/src-tauri/src/orchestration/mod.rs#L612) also releases only a recovered result. [Recovered Review results](gui/src-tauri/src/commands/orchestration.rs#L86) exclude failed/cancelled manifests. [Deletion](gui/src-tauri/src/runs/history.rs#L664) rejects every run with a pin, regardless of whether an active owner still needs it.

**Fix plan.** Make pin lifecycle explicit and recoverable: record operation/run ownership durably, retain it during genuine outcome uncertainty, and release it once the owning action has reconciled, been explicitly abandoned, or settled without a result to adopt. Retry must settle or retire the old operation's pin before assigning a new operation. Add startup reconciliation for existing orphan pins; do not simply remove all pins on errors that might hide live work.

**Acceptance.** Cover success, provider failure, cancellation, crash between run creation and result adoption, explicit stop/abandon, and retry. A live/uncertain owner remains protected; a settled abandoned run becomes deletable without manual filesystem intervention.

### F11 — Same-page automation deep links are ignored

**Impact and trigger.** If Automations is already mounted, navigating from task A to task B leaves A selected. Entry requests for a new builder/session/file can similarly retain old local state. This undermines attention notifications and browser-style navigation.

**Evidence and reproduction.** [TasksPage](gui/src/components/TasksPage.tsx#L53) initializes `selectedRef` and `creating` from entry props once. [App](gui/src/App.tsx#L368) keys the page only by discovery identity/request. An isolated probe rendered A, changed `initialTaskId` to B, and observed that `task_get(B)` was never called.

**Fix plan.** Consume a complete entry identity with a guarded effect that clears stale selection, resets the appropriate view/paging state, and loads the requested task or builder inputs. Reuse the navigation principles of F05 without mixing the Tasks and Workspace runtime stores. Protect unsaved builder edits before a replacement entry.

**Acceptance.** Test task A→B while mounted, repeated links, session/file builder entry, attention notifications, discovery links, and back/forward. A stale response from A must not overwrite B.

### F12 — Packaged notices overstate provenance and SBOM production

**Impact.** Users and release reviewers cannot infer the actual distributed components from the shipped notice. Metadata that claims exact pins or binary matching without the active build enforcing them is unreliable evidence for dependency, minimum-OS, and distribution review.

**Evidence.** [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md#L18) describes an exact Homebrew commit, Ubuntu snapshot/package version, copied-file hashes/provenance, and per-PE conda matching. [The active bundler](scripts/release/bundle-poppler.sh) uses available Homebrew/APT binaries and verifies the Windows archive hash; it does not perform all those provenance steps. [Notice preparation](scripts/release/prepare-package-notices.mjs) copies the static document into every package. [README](README.md#L121) says software bills of materials are included with each release, while [RELEASING](RELEASING.md#L70) accurately says the active workflow does not produce them. Separate unused provenance utilities and their passing tests do not establish active enforcement.

**Fix plan.** Choose and document the actual distribution contract. Either wire the existing pin/provenance/inventory tools into packaging and validate their output, or rewrite shipped claims to the guarantees actually implemented and generate an inventory from the bundled tree. Bind component versions, hashes, notices, and source references to actual artifacts, including cache hits. Keep legal/distribution review as a separate required release decision; this finding is a demonstrated metadata mismatch, not a legal conclusion.

**Acceptance.** Build each supported platform, inspect the installed notices and actual native files, and compare them with a generated manifest. No unsupported claim of SBOMs, package pinning, or exhaustive binary matching remains. A changed/cached Poppler closure must be detected before upload.

### F13 — macOS source build requires release signing unexpectedly

**Impact and trigger.** The documented `npm run tauri build` path on a normal Mac reaches an unconditional pre-bundle script that requires `APPLE_SIGNING_IDENTITY`. Thus source users cannot follow the documented build instructions without signing credentials. Even with an identity, the script searches only explicit `*-apple-darwin/release` directories, not the default `target/release` output or a custom target directory.

**Evidence and reproduction.** [README build instructions](README.md#L268) provide the command without signing setup. [Tauri configuration](gui/src-tauri/tauri.conf.json#L10) runs the signing hook before bundling. [The hook](scripts/release/sign-macos-cli.mjs#L14) throws when the identity is absent and at lines 17–26 restricts discovery to target-triple directories. Executing the hook with the identity unset reproduced `APPLE_SIGNING_IDENTITY is required to sign pipeline-cli`; it failed before signing or mutation. The official matrix supplies the identity/target, so this finding concerns the documented source-build path, not a demonstrated failure of signed CI packaging.

**Fix plan.** Separate official signed-release enforcement from local source packaging. Require signing in the official workflow, but support the documented unsigned/ad-hoc local build deliberately. Resolve the binary from the current build's target/profile/output metadata; do not scan and sign unrelated leftover builds. Account for default target and custom target directories. Update the source-build instructions to match the chosen support policy.

**Acceptance.** Build on a clean Mac without signing secrets using the documented command; separately prove the official release fails closed when required credentials are absent. Cover explicit target, default target, and custom output paths with harmless fixture tests for script resolution.

### F14 — README retains obsolete account and integration descriptions

**Impact.** Setup and account-change instructions can send users to obsolete settings locations and imply ownership boundaries that no longer match behavior. The overview also understates implemented integrations.

**Evidence.** [README](README.md#L45) says credentials remain separate and the only implemented bridge is a paper handoff. [The current shared-account contract](docs/chatgpt-account.md#L1) uses one managed ChatGPT account across Conversations and Reviews; [Tasks](docs/tasks.md) and [project exchange](docs/workbench/project-exchange.md) describe additional implemented coordination/exchange paths. README's **Settings → Providers → OpenAI → Reviews** instructions at lines 70–74 conflict with the current **Settings → Connections → OpenAI → ChatGPT account** guide. Storage recovery text in [storage.rs](gui/src-tauri/src/storage.rs#L78) still directs users to **Settings → General**, whereas [storage documentation](docs/storage.md#L1) places it under **Data & Storage**.

**Fix plan.** Reconcile the README and user-facing recovery copy with the canonical account, Tasks, storage, and navigation documents. Explain shared authentication while preserving separate execution state, cancellation, storage, and adapter ownership. Link to one authoritative setup guide rather than duplicating stale paths. Check privacy/security/account wording for the same distinction; retain dated historical evidence as historical, not current guarantees.

**Acceptance.** Walk the documented sign-in, storage-recovery, and project/automation flows in the development GUI using disposable data, then verify the candidate's installed UI. All labels and described ownership effects must match observed behavior. Run link checks, but also perform this semantic review.

### F15 — Backup omits conversation working files despite its broad promise

**Impact and trigger.** A conversation without a registered project folder can produce working files in Pipeline's private conversation directory. The backup UI promises projects, research files, and conversations, with an “Export all research data” action. Its archive does not include those working files. Restoring on another machine creates a fresh empty conversation directory, so a user who relied on the advertised backup can lose generated research material when the original store is removed.

**Evidence.** [Backup UI](gui/src/components/WorkspaceReleasePanel.tsx#L130), lines 131–142, makes the broad promise. [Runtime-root resolution](gui/src-tauri/src/workbench/store/views.rs#L132) creates `jobs/conversations/<session>`. [Archive contents](gui/src-tauri/src/workbench/release/archive.rs#L367) inventory only blobs, with the writer at lines 443–540 adding the manifest, database, transcripts, and those blobs. No conversation working-directory payload is included. The retention code separately recognizes and protects conversation working files, so they are not merely disposable execution scratch. This is distinct from F01's nonportable trash authority and F09's turn bookkeeping.

**Fix plan.** Decide and implement the backup contract explicitly. Prefer a bounded, versioned archive section for app-owned conversation working files, with safe paths, per-file hashes, no symlink traversal, and destination-owned restoration. External registered project folders and task copies need their own clearly disclosed policy; do not silently expand authority to copy arbitrary roots. If file backup is deferred, narrow the button/copy and show omissions before export, with a separate supported way to export the working files.

**Acceptance.** Create a synthetic rootless conversation with a generated source file and output, export, restore into a different store, and verify exactly the promised files survive. Test missing/oversized/unsafe entries and older archive versions. If files remain excluded, the UI must clearly identify that omission before the user treats the archive as a full backup.

### F16 — Malformed route encoding throws instead of falling back

**Impact and trigger.** A malformed development deep link or corrupted saved route can throw during route parsing, breaking navigation or startup rather than restoring a safe page. Ordinary routes generated by the UI are encoded correctly, so this is lower priority than the identity bugs.

**Evidence and reproduction.** [Route parsing](gui/src/lib/router.ts#L145) maps `decodeURIComponent` over path segments without handling decoding errors. `parseRoute("#/project/%")` and `parseRoute("#/settings/%E0%A4%A")` both throw `URIError: URI malformed` in the isolated Node probe. [Saved-route restoration](gui/src/lib/router.ts#L310) and initial/hash navigation invoke the same parser.

**Fix plan.** Make `parseRoute` total for arbitrary strings: reject malformed encoding by returning `null`, preserving the router's existing fallback. Keep valid encoded Unicode and delimiters working. Handle unreadable persisted navigation separately as optional state rather than a startup requirement.

**Acceptance.** Cover lone percent signs, incomplete escapes, invalid UTF-8, valid Unicode, encoded delimiters, startup with a corrupt persisted route, and malformed hash changes. None should cause the application error boundary or prevent subsequent valid navigation.

## Implementation sequence and release exit criteria

The following was the original remediation plan. See the status table above for implemented changes and remaining qualification. Owners are responsibility areas rather than assigned people. Keep fixes small and independently reviewable; do not combine broad architecture refactoring with the release corrections.

| Stage | Work | Owner | Dependencies / completion evidence |
|---|---|---|---|
| 1 — Restore a green baseline | F07 formatting and focused test split | Build/frontend | Both failing gates pass; existing tests unchanged in meaning |
| 2A — Restore/import authority and completeness | F01, F09, F15 | Workspace persistence + frontend recovery | Disposable two-store, backup-content and cross-binding tests; old archive compatibility; no applied migration edits |
| 2B — Mutation/lifecycle ownership | F02 and F06 | Native runtime + Review tool owner | Failure/cancel/drop tests prove unrelated work survives and no writes outlive finalization |
| 2C — Bounded automation execution | F03, then F10 | Tasks coordinator + Review adapter | Bounded transition/yield tests; responsive control; restart and pin reconciliation |
| 2D — Durable UI state and routing | F04, F05, F11, F16 | Frontend owners | Quota probes, malformed-route checks and route-entry regressions pass at component and App boundaries |
| 3 — Distribution and documentation | F12, F13, F14 | Release tooling + documentation | Local source build works; signed release remains strict; notices match actual component inventory; walkthrough complete |
| 4 — Candidate qualification | F08 and fresh approved advisory review | Release owner + researcher | Evidence matrix complete against exact commit/installers; unresolved exceptions explicitly scoped |
| 5 — Publication decision | Review all findings and final evidence | Release owner | Every P1 closed; P2 fixed or explicitly scoped out; immutable tag/assets verified; publish remains a separate human action |

Recommended test design is failure-oriented: deterministic synchronization for cancellation races, synthetic two-store archive fixtures, bounded control-flow inputs, and route changes while pages remain mounted. Tests that merely assert the current implementation shape will not catch these defects. Preserve the architectural boundaries in `CLAUDE.md`: share small mechanics where appropriate, but keep Workspace, Reviews, and Tasks execution/store/cancellation owners distinct.

After each focused fix, run its owning tests. Once the fixes converge, run the complete frontend build/tests/format/release suite, Rust format/strict Clippy/all-target tests, source-size and Markdown checks, then the candidate-specific native and packaged matrix. Do not repeatedly rerun every suite between unrelated documentation edits, and do not use a passing unit-test count as a substitute for F08.

The cached Rust audit is limited evidence only. A fresh Rust advisory/yanked check and the online npm advisory check remain release work; npm's query requires approval to transmit the locked dependency names and versions to its advisory service. Review the 17 existing Rust exceptions rather than treating a zero-unsuppressed-result count as an unqualified clean bill of health.

## Audit disposition

The initial audit performed no fixes, dependency updates, commits, or release actions. The subsequently authorized implementation changes are recorded above; no commit or release action has been performed. Existing functional checks passing is useful evidence; the identified regressions explain why additional failure-path tests and candidate qualification are needed. Temporary reproduction harnesses and logs were kept outside the repository; this report records their inputs, observed outcomes, and implementation locations so remediation does not depend on ephemeral file links.
