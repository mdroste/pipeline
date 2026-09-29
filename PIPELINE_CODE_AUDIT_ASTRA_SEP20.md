# Pipeline code audit and release remediation plan

**Date:** September 20, 2026  
**Reviewer:** Codex / Astra  
**Target:** Pipeline 0.9.5, current working tree on `main`  
**Base commit:** `481e17ade60320f632e129fcefc92966b2efac7f`

## Release recommendation

**Initial audit recommendation: hold release pending the P1 fixes and candidate qualification below.** The existing test suites are substantial and pass locally, but they miss reproducible loss of a saved composer draft, hard failures when opening sufficiently long conversations, and silent omission of open research tasks. The ordinary Workspace runtime also does not revalidate the authority of a registered folder before passing it to the native runtime. Two mandatory repository checks currently fail.

This report contains nine prioritized findings, six focused refactoring packages, and an ordered implementation and release-validation plan. Three disposable regression probes reproduced the draft, history-limit, and task-context defects. Other findings distinguish direct code traces from failure-mode risks. Passing a probe that asserts existing faulty behavior is evidence of reproduction, not evidence that the defect is fixed.

The initial audit included no implementation fixes; its temporary probes were removed after execution. The subsequent user-requested fixes are recorded in the [implementation follow-up](#implementation-follow-up--september-20-2026). Existing working-tree changes were preserved.

## Scope and method

The audited tree includes 56 tracked modified files and additional untracked feature files predating this audit. It includes the shared ChatGPT account service, Projects home/index changes, and discovery changes. Findings and line references describe that working tree, not the base commit alone. A release candidate must be rebuilt and checked from its final reviewed commit.

The review followed [AGENTS.md](AGENTS.md), [CLAUDE.md](CLAUDE.md), the [module map](docs/refactoring/module-map.md), and the owning feature documents. It used repository-wide searches, targeted source and test review, local quality gates, and disposable synthetic fixtures. This was a repository review; it did not dispatch a paid Pipeline model workflow. It was not a line-by-line certification of every file.

| Area | Review emphasis |
|---|---|
| Workspace persistence and commands | Root authority, session hydration, turn ownership, worker gates, revisions, migrations and transcript access |
| React application | Conversation lifecycle, draft saves, asynchronous refreshes, navigation, project metadata and notifications |
| Reviews / Workflow engine | Scheduling and provider boundaries, output validation, response capture, run persistence and recovery |
| Native integration and shared account | Separation of execution owners, account leases, credential handling, refresh routing and compatibility qualification |
| Research and project services | Automatic context, task records, local execution ownership, file operations, archive/exchange and retention boundaries |
| Tasks, missions and discovery | State transitions, immutable action results, restart recovery, attention signals and lifecycle ownership |
| Release engineering | Actual CI gates, platform packaging configuration, release documentation, source budgets and evidence requirements |

The review did not read real credentials, run authenticated model turns, invoke Stata, run destructive crash tests, install a package, or publish anything. Cross-platform and clean-machine results were not generated. Dependency advisories were not scanned; no conclusion about current dependency vulnerabilities is implied. Existing local dependencies were used rather than a new clean installation.

### Severity and evidence

- **P1 — fix before release:** data loss, incorrect authority, loss of a core workflow, or a mandatory release gate failure.
- **P2 — resolve before broad release:** correctness, recovery, notification or support-contract problems with a narrower trigger. A deliberate deferral needs a documented product boundary and an owner.
- **Refactoring:** structural improvements that should be separated from behavior fixes unless needed to make the fix safe.
- **Reproduced:** exercised locally using the current implementation and disposable data.
- **Code trace:** a specific input or event path follows from the implementation; the complete native scenario was not exercised.
- **Durability risk:** a missing persistence guarantee, without a claim that a power-loss failure was reproduced.

## Validation performed

Pinned toolchains were used for the principal results: Node **24.18.0** and Rust **1.97.1**. The initial frontend run used the shell's Node 22.17.1; frontend tests and the production build were repeated with the repository's pinned Node version.

| Check | Result | Interpretation |
|---|---|---|
| Frontend tests, `npm test` | **718 passed in 98 files** | Pinned-Node repeat passed |
| Frontend production build, `npm run build` | **Passed** | TypeScript and Vite build; large PDF chunk warning remains |
| Rust, `cargo test --locked --all-targets` | **967 passed; 11 ignored** | 953 library tests plus 14 CLI tests; opt-in qualification was not run |
| Rust, `cargo clippy --locked --all-targets --all-features -- -D warnings` | **Passed** | Strict all-feature lint, not all-feature runtime testing |
| Rust, `cargo fmt --all -- --check` | **Passed** | Rechecked after temporary probes were removed |
| Release-contract tests, `npm run test:release` | **60 passed** | Release identity validated as 0.9.5; not installer qualification |
| Source budget, `npm run check:source-size` | **Passed against baseline** | 17 oversized files remain grandfathered |
| Frontend, `npm run format:check` | **Failed** | `WorkspaceConnectionSettings.test.tsx` |
| Documentation, `npm run check:links` | **Failed** | 26 missing temporary-file targets in `ASTRA_SEP7_BUGREPORT.md` |
| Audit React probe | **1 reproduction passed** | A delayed hydration erased a newly persisted draft |
| Audit Rust probes | **2 reproductions passed** | Conversation limits and silent open-task omission |

The build reports `PdfReader` at roughly **711 kB minified / 218 kB gzip**. It is a separate chunk; the warning alone does not establish a first-route performance defect. Measure its route-load cost before deciding whether additional splitting is useful.

The Rust ignored tests include live native integration, public acquisition, real TeX/Python/Stata, Git worktrees, and performance measurements. The ordinary green suite does not establish those capabilities on this release candidate.

## Findings at a glance

| ID | Priority | Finding | Evidence | Main owner |
|---|---|---|---|---|
| F01 | P1 | A replaced registered folder can become the ordinary conversation runtime root | Code trace | Workspace store / Codex command boundary |
| F02 | P1 | Delayed hydration can erase a draft that was successfully saved | Reproduced | Workspace controller |
| F03 | P1 | History limits make mature conversations fail to hydrate | Reproduced | Workspace views / transcript API |
| F04 | P2 | Newer completed tasks silently hide older open tasks from project context | Reproduced | Project records / context assembly |
| F05 | P2 | Discovery attention and completion have no shell notification path | Code trace | Discovery coordinator / shell |
| F06 | P2 | Review steps continue when response-journal persistence fails | Code trace | Workflow step calls / response journal |
| F07 | P2 | Task result publication lacks a directory-durability barrier | Durability risk | Task adapters |
| F08 | P1 | Required formatting and documentation-link gates fail | Reproduced | Frontend tests / repository documentation |
| F09 | P2 | Support claims disagree with the actual installer and CI behavior | Code/configuration comparison | Release documentation / packaging |

## F01 — Revalidate registered roots before granting runtime authority

**Priority: P1. Evidence: code trace; no native access to a substituted directory was attempted.**

**Trigger and consequence.** Register a project folder, move or remove that folder, and replace its old path with a symlink to another directory. A later ordinary conversation send accepts the saved path if it is a directory. The newly resolved directory can therefore become the native runtime's working/read/write scope despite never passing the original registration policy. The same problem includes replacement by a different directory at the same pathname.

**Evidence.** [Root registration](gui/src-tauri/src/workbench/store/workspaces.rs:49) canonicalizes the path and saves a root identity. [The registration policy](gui/src-tauri/src/workbench/store.rs:750) excludes filesystem/home roots and overlap with private research, settings and credential storage. However, [ordinary runtime-root resolution](gui/src-tauri/src/workbench/store/views.rs:149) only checks `path.is_dir()` for a saved workspace root. It neither reruns that policy nor compares the saved identity. [Turn submission](gui/src-tauri/src/workbench/commands/codex.rs:189) forwards the result to native thread start/resume. [Supervisor root validation](gui/src-tauri/src/workbench/codex/supervisor.rs:798) checks that paths are absolute existing directories; that does not establish that they are still the approved project.

This is an application authority gap, not a claim that the native sandbox was escaped. A sandbox can correctly enforce the wrong root supplied by its host. The [project-file root helper](gui/src-tauri/src/workbench/project.rs:82) does rerun canonical exclusions, demonstrating inconsistent policy across entry points.

**Recommended fix.**

1. Introduce a Workspace-owned resolver for registered-folder authority. Canonicalize, apply private-root exclusions, and compare a stable filesystem identity before dispatch.
2. Route ordinary send, resume and other native entry points through it. Keep explicitly owned checkpoint, discovery and rootless-conversation directories as distinct, validated cases.
3. On an unavailable or changed root, block new work and require explicit root registration. Do not silently rewrite the stored identity during reconciliation.
4. Inspect platform identity behavior: [the non-Unix fallback](gui/src-tauri/src/workbench/store.rs:805) hashes a pathname, which cannot detect replacement at the same path. Use an appropriate native file identity or state a narrower guarantee.
5. Preserve existing validated-handle defenses for host file operations. A pre-dispatch path check alone is not a complete defense against an adversarial swap after validation.

**Acceptance tests.** Unchanged directory succeeds. Missing root, renamed-and-replaced directory, symlink substitution, ancestor substitution and substitution into private storage fail before a native request is sent. Legitimate explicit re-registration succeeds. Include supported Windows reparse-point behavior and verify checkpoint/discovery roots remain usable.

## F02 — A late snapshot can overwrite and then persist an obsolete draft

**Priority: P1. Evidence: reproduced with the real React controller and deferred mock IPC.**

**Trigger.** A hydration request begins with an empty saved draft. While it is pending, the user types and successfully saves a new draft. The save removes its recovery-cache entry. The older hydration then returns and replaces the composer with the old empty value. A subsequent save or navigation flush writes that empty value back to storage.

**Evidence.** [Hydration](gui/src/hooks/useWorkspacePageController.ts:266) rejects responses for another conversation, but has no request generation, record-revision ordering or local-edit revision guard for the same conversation. It reads the recovery cache and calls `setDraft` at line 291. [Draft persistence](gui/src/hooks/useWorkspacePageController.ts:170) correctly removes the matching cache entry after saving, so the cache does not protect the newer draft in this ordering. [Successful turn completion](gui/src/hooks/useWorkbenchEvents.ts:138) initiates reconciliation/hydration while releasing the composer for further input.

**Observed probe.** Start delayed hydration; edit to `important fresh draft`; call `saveCurrentDraft`; verify the backing mock store contains that text and the cache is empty; resolve the old snapshot. The composer becomes empty. Calling `saveCurrentDraft` again makes the backing store empty too. The probe passed all these assertions. This is a demonstrated overwrite of saved user input, not merely a transient stale label.

**Recommended fix.**

- Separate initial conversation loading from transcript/metadata refresh. Refreshes should not assign composer text.
- Track session selection, hydration generation and a monotone local draft revision. Apply a draft loaded from storage only when the request still owns initialization and no later edit/save has occurred.
- Prevent an older same-session snapshot from replacing a newer revision or newer active-turn/stream state. Draft saves should merge returned session metadata rather than replace unrelated conversation state with the snapshot obtained before the save.
- Preserve ambiguous-submission recovery and the current cross-conversation ownership guards.

**Acceptance tests.** Keep the reproduced ordering as a regression with the desired assertion that the new draft survives both refresh and subsequent persistence. Cover debounce-save overlap, navigation flush, overlapping refreshes resolving in reverse order, turn completion followed by typing, failed submissions, and conversation switches. Use deferred promises and explicit state assertions rather than timing-dependent sleeps.

## F03 — History safety limits turn into permanent read failures

**Priority: P1. Evidence: reproduced against a migrated disposable SQLite store.**

**Trigger and consequence.** A conversation accumulates **501 distinct turns** or **2,001 transcript items**. [The conversation snapshot API](gui/src-tauri/src/workbench/store/views.rs:54) reads one more than its cap and returns an error instead of a bounded page. Tool-rich conversations can reach the item cap well before the turn cap. Reopening the conversation through this API fails; data remains on disk, but the normal read path cannot access it.

**Evidence.** Errors are raised at [the turn cap](gui/src-tauri/src/workbench/store/views.rs:94) and [the item cap](gui/src-tauri/src/workbench/store/views.rs:126). [Frontend hydration](gui/src/hooks/useWorkspacePageController.ts:271) requires the full snapshot. Its 200-message rendering window is applied after the entire snapshot has arrived, so it does not solve backend hydration. Draft persistence also calls the full conversation snapshot. Conversation export uses it in [the research command](gui/src-tauri/src/workbench/commands/research.rs:500).

**Observed probes.** A migrated store with one valid session/binding and 501 completed turns returned `Conversation turn history exceeds its hydration limit`. After replacing those with 2,001 transcript items, it returned `Conversation item history exceeds its hydration limit`.

**Recommended fix.**

1. Add a cursor-based transcript page API with a stable composite order and explicit `hasMore`/cursor metadata.
2. Fetch conversation metadata and active-turn identity independently of historical transcript pages. Use the existing lightweight session snapshot where full history is unnecessary, including draft revision reads.
3. Load the newest page first and retrieve older pages on demand. Make export stream or iterate all pages under a separate bounded export policy.
4. Keep per-request item and byte bounds. Raising or removing the total caps is not a sufficient fix.
5. Review similar collection boundaries, including the new [500-project index cap](gui/src-tauri/src/workbench/project/index.rs:31), so accumulated valid records do not make entire navigation surfaces unavailable.

**Acceptance tests.** Exercise 500/501 turns and 2,000/2,001 items, plus a 10,000-item history. Reopen, send, save drafts and export successfully. Verify stable paging under equal timestamps, new arrivals and successor bindings, without duplicates or omissions. Measure response bytes and rendered node counts, not only SQL row limits.

## F04 — Context truncation can silently drop all open tasks

**Priority: P2. Evidence: reproduced against a disposable store.**

**Trigger and consequence.** An old open task is followed by 500 more recently updated completed tasks. [The generic record query](gui/src-tauri/src/workbench/project.rs:148) selects only the newest 500 records. [Project context](gui/src-tauri/src/workbench/project.rs:277) filters completed/rejected tasks only after that query. It checks for `tasks.len() == 1000` before adding its omission warning, which is unreachable through a query capped at 500. The model receives no open task and no warning that tasks were omitted.

**Observed probe.** Insert one older open task containing `OPEN_SENTINEL`, followed by 500 newer completed tasks. `project_context` contains neither `OPEN_SENTINEL` nor `Task context is limited`. The generic record list returns 500 rows.

**Recommended fix.** Query the relevant open statuses before applying the context limit. Return explicit truncation/count metadata from bounded record queries. Use deterministic ordering and a consistent shared limit. Give the full task-management surface pagination so omitted older records remain discoverable. Retain the context byte budget and clearly identify additional byte-budget truncation.

**Acceptance tests.** One old open task plus 500 newer completed tasks remains visible in automatic context. More open tasks than the context budget produce a truthful omission count. Test boundary counts, equal timestamps, rejected/completed exclusions and access to older records in the task UI. Do not fix only the warning constant; that would leave the open-task selection defect intact.

## F05 — Discovery loses attention signals at the shell boundary

**Priority: P2. Evidence: event-path trace.**

**Trigger and consequence.** A discovery portfolio reaches supervised selection, fails/blocks, exhausts its budget, or completes while the user is elsewhere in the app or the main window is hidden. The discovery page can refresh when mounted, but there is no corresponding completion/attention notice through the app notification path. A supervised run can wait for input without drawing attention.

**Evidence.** [Discovery notification](gui/src-tauri/src/orchestration/discovery/mod.rs:28) emits `discovery:changed` and the coordinator's ordinary changed event. [Task save](gui/src-tauri/src/orchestration/mod.rs:443) suppresses child `tasks:notice` events when a mission or discovery portfolio owns the child. [App notifications](gui/src/hooks/useAppNotifications.ts:62) subscribe to task and mission notices only, as does [the shell attention listener](gui/src/hooks/useAppController.ts:141). Missions already have a transition-aware notice implementation in [their owner module](gui/src-tauri/src/orchestration/missions/mod.rs:25).

**Recommended fix.** Add discovery-owned transition notices and a typed frontend decoder. Notify on actionable selection/blocked states and terminal outcomes, honoring existing user preferences. Provide a route to the affected portfolio and an appropriate attention indicator. Deduplicate repeated saves of the same transition; child progress and ordinary polling must remain quiet.

**Acceptance tests.** Verify selection-required, failure and completion notices while the discovery route is unmounted and while the window is hidden. Repeated identical state saves produce one notice. Disabled notifications remain disabled. Child completion must not create duplicate task and discovery notices.

## F06 — Failure to retain a provider response does not stop its use

**Priority: P2. Evidence: code trace through capture and successful step return.**

**Trigger and consequence.** Response capture fails because its directory is unwritable, storage is exhausted, or the per-run response-journal budget is reached. [The step-call wrapper](gui/src-tauri/src/pipeline/executor/step_call.rs:98) logs a warning and returns `None`. Validation and acceptance continue, and [the step can return success](gui/src-tauri/src/pipeline/executor/step_call.rs:466). Rejected responses can likewise lose the inspectable evidence the journal was intended to retain. This conflicts with the repository contract that provider responses are journaled before validation.

The budget is an explicit **128 MiB per run**, with individual captures bounded to **8 MiB**, in [the journal implementation](gui/src-tauri/src/pipeline/response_journal.rs:13). This is not limited to hypothetical device failure. A sufficiently response-heavy run can encounter the normal configured limit. Runs intentionally configured without persistent storage are a separate case; `capture(None, ...)` is explicitly a no-op and should remain distinguishable.

**Recommended fix.** Return a typed capture outcome that distinguishes intentionally nonpersistent execution from failed required persistence. For persistent runs, require a durable capture before accepting or retrying a response under the normal evidence contract. If degraded continuation is a supported product decision, persist that loss explicitly in the run manifest and expose it in the report; a transient warning line is insufficient. Classification failure should retain the original captured file and surface an incomplete classification status.

**Acceptance tests.** Inject a capture-write failure and a journal-budget rejection around otherwise valid output. Confirm no ordinary successful step is recorded without the required evidence. Exercise rejected output, retry, cancellation and classification-write failure. Verify the already captured raw response survives classification failure and no invalid response enters downstream context.

## F07 — Task result files are atomic but not fully durable on Unix

**Priority: P2. Evidence: durability risk from source inspection; no power-loss experiment was run.**

**Trigger and consequence.** The machine loses power after a task result is renamed into place but before the containing directory entry is durable. [Task `atomic_json`](gui/src-tauri/src/orchestration/adapters.rs:184) flushes and syncs the temporary file, then persists it by rename, without syncing the parent directory. [Task snapshot publication](gui/src-tauri/src/orchestration/adapters.rs:225) has the same omission. A completed native action can therefore lack the recovery file expected after restart despite successful return from the publisher.

These files are part of recovery, not disposable presentation caches: [the adapter](gui/src-tauri/src/orchestration/adapters.rs:290) recognizes `actions/<operation>/result.json`, and [coordinator recovery](gui/src-tauri/src/orchestration/mod.rs:546) reads it for unknown receipts. By comparison, [the response-journal publisher](gui/src-tauri/src/pipeline/response_journal.rs:103) already syncs the containing directory on Unix.

**Recommended fix.** Adopt a small policy-neutral durable-file publisher with a tested write/flush/file-sync/rename/directory-sync contract. Account for newly created ancestor directories and error propagation before recording completion. Define the supported Windows equivalent separately. Keep each mode's state machine, storage layout and adoption policy in its owner.

**Acceptance tests.** Fault injection at write, file sync, rename and directory sync must yield explicit incomplete/unknown outcomes without automatic replay of ambiguous side effects. Add packaged abrupt-termination recovery tests. Process-kill tests are useful but do not by themselves prove power-loss durability; document the filesystem guarantee being relied on.

## F08 — Two required quality gates currently block the candidate

**Priority: P1 as a release blocker. Evidence: commands failed locally.**

1. `npm run format:check` identifies [WorkspaceConnectionSettings.test.tsx](gui/src/components/WorkspaceConnectionSettings.test.tsx). The failure was reconfirmed after audit probes were removed.
2. `npm run check:links` reports **26** missing local targets in [ASTRA_SEP7_BUGREPORT.md](ASTRA_SEP7_BUGREPORT.md), including absolute temporary logs and probe files. A clean CI machine will not have those historical `/tmp` files.

Both are mandatory steps in [the quality workflow](.github/workflows/build.yml:61), which [the release workflow](.github/workflows/release.yml:33) requires before packaging. Green unit tests do not bypass these gates. The broken historical links are also acknowledged in the existing shared-account validation document; this audit did not introduce them.

**Recommended fix.** Format the affected test file. Replace historical temporary-file links with durable checked-in evidence, stable CI artifacts with an explicit retention policy, or plain-text historical log names. If the September 7 report is historical material, move it deliberately into the historical documentation structure and update references; do not broadly weaken link checking to hide the failures. Do not commit logs containing user content or credentials.

**Acceptance tests.** Formatting and the complete Markdown-link check pass on a clean checkout with no prior machine-local audit files. Preserve the current formatting/link steps in CI.

## F09 — Correct public support claims to match shipped behavior

**Priority: P2. Evidence: source/configuration comparison.**

[SUPPORT.md](SUPPORT.md:16) says the Windows installer includes the WebView2 offline installer. [Tauri configuration](gui/src-tauri/tauri.conf.json:55) uses `embedBootstrapper`. [RELEASING.md](RELEASING.md:15) correctly states that installation needs a network connection when WebView2 is missing. These describe different installation guarantees.

The support document also says CI compiles and opens the application on the platform matrix. [The quality workflow](.github/workflows/build.yml) runs tests and a frontend build; [the release workflow](.github/workflows/release.yml) builds packages but does not launch a packaged application. The release guide explicitly disclaims packaged smoke results. Similarly, setting `MACOSX_DEPLOYMENT_TARGET=15.0` does not establish the minimum OS version of every copied Poppler library; the active workflow does not invoke the dedicated Mach-O validation utility.

**Recommended fix.** Make `SUPPORT.md` reflect the current bootstrapper and actual CI coverage immediately. Keep clean-machine installation/launch qualification as a publication gate. If offline installation is a product requirement, deliberately change the installer mode and test it with networking disabled. If macOS dependency-floor enforcement is claimed, invoke the closure validator on the bundled binaries and retain its result. Update [Workspace qualification's migration summary](docs/workbench/release-qualification.md:8), which still describes migrations through 10 although the source now contains migrations through 15.

**Acceptance tests.** A reviewer can map each public claim to an active workflow step or dated candidate qualification record. Test Windows on a clean machine without WebView2, macOS at the declared minimum, and Linux with the advertised AppImage prerequisites. Documentation must clearly distinguish automated checks from manual evidence.

## Refactoring plan

These are proposed bounded changes, not a recommendation for a large pre-release rewrite. Fix demonstrated defects first, preserve behavior during extraction, and use the existing module ownership boundaries.

### R1 — Split conversation state by ownership

**Targets:** [useWorkspacePageController](gui/src/hooks/useWorkspacePageController.ts), [useWorkbenchEvents](gui/src/hooks/useWorkbenchEvents.ts).

The controller is 1,192 lines and couples session selection, composer persistence, submissions, event reconciliation, transcript paging, research destinations and menus. Its size is below the current line threshold but its asynchronous ownership is already implicated in F02/F03.

Extract a composer/draft owner, a transcript-page reader, and a submission lifecycle owner with explicit session/request identities. Keep a small UI-facing coordinator. Prefer reducer/state-machine transitions for submission ownership over exposing another large bag of setters. Make initial load, background refresh and save acknowledgement different operations.

**Sequence:** add F02 tests; fix draft ownership; introduce the F03 API; extract only the boundaries exercised by those tests. Preserve the existing failure-before/after-ack and cross-conversation tests.

### R2 — Centralize mechanics for root validation and durable publication

**Targets:** [Workspace store](gui/src-tauri/src/workbench/store.rs), [store views](gui/src-tauri/src/workbench/store/views.rs), [project helpers](gui/src-tauri/src/workbench/project.rs), [Task adapters](gui/src-tauri/src/orchestration/adapters.rs), [response journal](gui/src-tauri/src/pipeline/response_journal.rs).

Address F01/F07 with narrow APIs whose return types represent validated roots and completed publication. Separate path policy from byte-writing mechanics. Do not merge Workspace, Review and Task stores, supervisors or cancellation owners. Audit call sites when adopting the helper rather than performing a global mechanical replacement.

### R3 — Decompose archive and exchange while retaining policy separation

**Targets:** [exchange.rs](gui/src-tauri/src/workbench/release/exchange.rs), [archive.rs](gui/src-tauri/src/workbench/release/archive.rs), [retention](gui/src-tauri/src/workbench/release/retention.rs).

`exchange.rs` is 1,973 lines / 85,326 bytes; `archive.rs` is 1,247 lines / 53,096 bytes. Extract bounded package reading, manifest/graph validation, remapping, conflict decisions and transactional adoption into owner-specific modules. Share safe archive/byte primitives only. Whole-store `.pwrx` restore and selective `.pwex` import must remain separate policies.

As part of this work, make [retention-reference refresh](gui/src-tauri/src/workbench/release/archive.rs:1040) transactional and explicit about reference ownership. It currently deletes every retention row before rebuilding artifact/evidence references, whereas [exchange imports](gui/src-tauri/src/workbench/release/exchange.rs:1845) add `exchange_import` reasons. Whole-store export calls this refresh. Do not claim proven blob deletion from this alone: imported blobs also receive artifact records, and the collector reads those directly. The concrete concerns are loss of retention provenance and partial registry state if rebuilding fails.

**Acceptance:** existing malformed archive, remap, conflict, rollback and portability tests remain green; add a failed-refresh transaction test and a documented rule for preserving or rebuilding non-derived retention reasons.

### R4 — Separate execution policy from process lifecycle

**Targets:** [research execution](gui/src-tauri/src/workbench/research/execution.rs), [settings](gui/src-tauri/src/settings.rs), [automatic review](gui/src-tauri/src/auto_review.rs).

These files contain 1,560, 1,527 and 1,582 lines respectively. Extract executable/profile validation, authorization fingerprints, process/log supervision and result adoption into separate execution modules. Split settings persistence/migration from provider catalog and credential operations. Split automatic-review catalog selection, orientation validation and workflow materialization.

Preserve execution receipts before side effects, cancellation ownership, account isolation, the `oldstata` invocation rule, and the distinction between model proposals and accepted research evidence. Use existing characterization tests; add tests only for extracted contracts or uncovered failure paths.

### R5 — Give bounded queries and lifecycle events explicit contracts

**Targets:** [project records/context](gui/src-tauri/src/workbench/project.rs), [project index](gui/src-tauri/src/workbench/project/index.rs), [orchestration](gui/src-tauri/src/orchestration/mod.rs), [discovery](gui/src-tauri/src/orchestration/discovery/mod.rs).

Replace silent capped `Vec` results where necessary with page/count/truncation metadata. Query status relevance before pagination. Introduce typed internal lifecycle and notice states to reduce stringly typed comparisons. Preserve serialized names for stored/public states unless a separately reviewed compatibility migration is necessary.

Expand compact multi-operation SQL/state-transition blocks into readable owner functions. A file can satisfy a line-count budget while still being difficult to review. Couple these extractions to F04/F05, not an unrelated mass formatting change.

### R6 — Reduce the remaining large entry points and use measured performance

**Targets:** [CLI](gui/src-tauri/src/bin/cli.rs), [export](gui/src-tauri/src/commands/export.rs), [run persistence](gui/src-tauri/src/runs.rs), [PDF extraction](gui/src-tauri/src/pipeline/extract/pdf.rs).

The CLI is 1,510 lines, export 1,379, runs 1,204 and PDF extraction 1,302. Split CLI argument/command handling from machine-readable output; export document generation from platform printing; run persistence from recovery and retention; PDF adapter policy from bounded extraction mechanics.

Keep the source-size baseline as a debt inventory, not evidence that these modules are already small. Reduce the baseline as modules are actually extracted. Split oversized test files by behavior/fixture ownership without weakening assertions. For the large PDF frontend chunk, collect cold/warm route and memory measurements before changing dependencies or chunk configuration.

## Ordered implementation plan

The proposed packages are sized for separate reviewable changes. They are an order of work, not calendar estimates or assignments to particular people.

| Package | Work | Dependencies | Completion evidence |
|---|---|---|---|
| A — Restore release gates | F08; correct the factual support claims in F09 | None | Clean formatting/link checks; support text matches configuration |
| B — Restore root authority | F01; narrow portion of R2 | None | Replacement/symlink/private-root tests fail before dispatch; unchanged roots work |
| C — Preserve composer state | F02; narrow portion of R1 | None | Deferred-response regression preserves saved text and active-turn state |
| D — Page accumulated data | F03, F04; bounded query contracts from R1/R5 | Coordinate frontend changes with C | 10,000-item history remains usable; older open tasks remain visible |
| E — Preserve evidence and recovery | F06, F07; durable publisher from R2 | None; preserve mode boundaries | Injected storage failures do not produce misleading ordinary success |
| F — Connect discovery attention | F05; typed notice portion of R5 | None | Hidden/unmounted route notices, deduplication and preferences tested |
| G — Freeze and qualify candidate | Qualification matrix below; remaining F09 evidence | A–F, or explicit narrow P2 disposition | Exact commit, installer hashes, platform and live evidence recorded |
| H — Continue structural cleanup | Remaining R3–R6 | Stable behavior and focused regression coverage | Smaller owner modules, unchanged contracts, reduced size baseline |

**Suggested first implementation order:** A, B, C, then D. These remove immediately visible release blockers and protect user data and project authority. E and F should follow before broad distribution. Structural cleanup beyond what those fixes need should not delay a safety fix or be mixed into its review.

For each package, record the owning modules, compatibility impact, new regression case, targeted checks and final integrated validation. Add new numbered SQL migrations if needed; never edit an already applied migration. Preserve command names, persisted fields and archive formats during mechanical extraction.

## Candidate qualification still required

This is an evidence backlog, distinct from the nine findings. The [Workspace qualification record](docs/workbench/release-qualification.md), [shared-account record](docs/chatgpt-account.md:106) and [discovery record](docs/self-discovery.md:128) explicitly limit prior results. Synthetic credentials and no-model native probes establish protocol behavior, not real provider authentication or research quality.

| Gate | Required candidate exercise | Evidence to retain |
|---|---|---|
| Shared ChatGPT account | Browser sign-in/cancel/sign-out; migration from one and conflicting saved accounts; genuine token renewal; simultaneous independently owned Workspace and Review work; cancellation and later account switch | Sanitized transitions and results, exact native version; no credentials |
| Authenticated tools | Real turn in each supported mode; streaming; approval/question handling; interrupt; denied reads/writes; changed-root rejection; model/protocol incompatibility | Minimal reproducible fixtures and outcomes |
| Discovery / missions | Supervised selection, unsupervised continuation within approved scope, failure, budget/deadline exhaustion, cancellation, restart and attention notifications | Durable state transitions, immutable captured outputs and navigation behavior |
| Research execution | Real TeX/Python and separately authorized `oldstata`; cooperative and forced cleanup, changed inputs, invalid outputs and adoption | Receipts, retained validated artifacts and documented cleanup limits |
| Crash recovery | Abrupt packaged-process termination around submission, approval, job receipt, result publication, file acceptance/undo, archive adoption and task delivery | Before/after database and artifact assertions; unknown outcomes never automatically replayed |
| Packaged platforms | Both macOS architectures, Windows installer/WebView2 setup, Linux AppImage/FUSE; clean install, upgrade/data preservation, launch, native resources and export | Exact installer hashes, OS versions, signing/notarization checks and dated results |
| Root and filesystem behavior | Symlinks/reparse points, missing/replaced roots, locks, case sensitivity and path handling on each supported platform | Platform-specific fixtures and declared limits |
| Performance / accessibility | Cold/warm project and chat navigation, first visible send, long-history paging, PDF route, streaming, cancellation, idle CPU/memory, keyboard and screen-reader flows | Repeated measurements and pass/fail budgets rather than a single debug timing |
| Dependency and distribution review | Candidate lockfiles, Rust/JS advisories, actual bundled Poppler/library closure, notices and macOS minimum-version checks | Dated scan/closure results and disposition of relevant findings |
| Research quality | Applicable checked-in evaluation fixtures with researcher scoring of correctness, support and traceability | Scored results; deterministic tests alone are insufficient |

The release workflow already leaves a draft and requires separate human publication. Keep it unpublished until applicable qualification is complete. Do not describe missing SBOMs, signatures on intentionally unsigned Windows installers, or unrun helper scripts as completed assurances. If a platform or feature cannot be qualified, narrow the advertised release scope explicitly.

## Regression reproduction recipes

These recipes summarize disposable probes executed during this audit. The probes exercised public store methods or the production React hook, with synthetic data and mocked IPC; they made no model calls. Temporary test sources and logs were not added to the repository. Convert the recipes into permanent desired-behavior tests when implementing the fixes.

### Probe A: saved-draft overwrite

1. Use `renderHook(() => useWorkspacePageController({}))` with the existing controller test's mocked Workbench client. Load session A with empty draft and revision 1.
2. Start `hydrate()` and hold that snapshot response in a deferred promise.
3. Call `editDraft("important fresh draft")`, then await `saveCurrentDraft()`. Make the mock update persist the draft and return revision 2.
4. Assert the stored draft is the new text and `pipeline.pendingDraft.A` has been removed.
5. Resolve the deferred hydration with the revision-1 snapshot.
6. Observed: the displayed draft is empty. Await another `saveCurrentDraft()`; the stored draft becomes empty too.
7. Corrected behavior: both values remain `important fresh draft` throughout.

Audit command from `gui/`: `npx vitest run src/hooks/useWorkspacePageController.astra-audit.test.ts`, using pinned Node. Result: **1 test passed**, asserting the existing defect.

### Probe B: history access limits

1. Open `Store::open_at` under a temporary directory; create a session through `create_session`.
2. Open a separate SQLite connection to `store.database_path()` and insert a valid binding with 501 completed turns, each with distinct submission/provider IDs.
3. Observed: `conversation_snapshot` returns the turn-history-limit error.
4. Delete those synthetic turns and insert 2,001 distinct final transcript items under the binding.
5. Observed: the same method returns the item-history-limit error.
6. Corrected behavior: a bounded first page and usable metadata are available in both cases, with older pages retrievable.

### Probe C: silently omitted open task

1. Create a workspace in a temporary migrated store.
2. Insert one valid project task record with objective `OPEN_SENTINEL`, open status and an older timestamp.
3. Insert 500 newer valid completed task records.
4. Observed: `project::records(..., "task")` returns 500 rows; `project_context` contains neither the sentinel nor its task-limit warning.
5. Corrected behavior: relevant open work is selected before the context limit, and any further omission is explicit.

Audit command from `gui/src-tauri/`: `cargo test --locked --test astra_sep20_audit`. Result: **2 tests passed**, asserting the existing defects in probes B and C.

## Final release acceptance criteria

- [ ] F01–F03 and F08 are fixed, with reproductions converted to regression tests for correct behavior.
- [ ] F04–F07 and F09 are fixed or have an explicit, reviewed narrow release disposition; silent data loss or incorrect authority is not an acceptable deferral.
- [ ] Frontend tests/build, Rust tests/format/strict Clippy, release tests, source-size and complete documentation-link checks pass on the final clean candidate checkout.
- [ ] Candidate validation uses pinned toolchains and the reviewed commit; local audit results are not reused as evidence for later source changes.
- [ ] Applicable authenticated, real-tool, crash, installer and platform qualification is recorded against that candidate.
- [ ] Public support and release claims match the produced artifacts and retained evidence.
- [ ] Any refactoring preserves runtime ownership, storage isolation, cancellation semantics, immutable handoffs and migration compatibility.


## Implementation follow-up — September 20, 2026

The nine P1/P2 findings were addressed in a subsequent, narrowly scoped change.
The broader R1–R6 refactoring backlog and candidate qualification remain separate.
No applied migration, dependency version, model provider configuration, or release
publishing policy was changed.

| Finding | Implemented correction | Regression evidence |
|---|---|---|
| F01 | Registration policy and stable directory identity are checked again before ordinary native dispatch and project file access. Windows identity uses volume/file index; legacy pathname-only registrations require explicit re-registration. | [Root replacement, symlink/private-root and rootless tests](gui/src-tauri/src/workbench/store/roots.rs) |
| F02 | Hydration has request/revision guards and initializes the composer only when loading a conversation. Draft saves merge newer metadata and preserve transcript/turn state. | [Saved-draft and out-of-order refresh tests](gui/src/hooks/useWorkspacePageController.test.ts) |
| F03 | Latest bounded snapshot, on-demand older transcript pages, and full-history export traversal replace total-history rejection. Metadata saves use the lightweight session API. | [10,000-item, equal-timestamp, arrival, successor and byte-bound tests](gui/src-tauri/src/workbench/store/history/tests.rs); [UI paging/race tests](gui/src/hooks/useWorkspaceTranscript.test.ts) |
| F04 | Open tasks are filtered before the context cap, with explicit omission metadata. Action-item history has cursor paging and carries selected older records into the edit view. | [Context/history tests](gui/src-tauri/src/workbench/project/task_pages.rs); [older action-item tests](gui/src/components/project-surface/ProjectActionItems.test.tsx) |
| F05 | Discovery emits actionable transition notices; the shell delivers them using existing preferences and includes an Open portfolio action. | [Shell notice tests](gui/src/hooks/useAppNotifications.test.ts), [preference/action tests](gui/src/lib/appNotifications.test.ts), [portfolio navigation test](gui/src/components/self-discovery/Discovery.test.tsx) |
| F06 | Required response capture/classification errors propagate and stop the step. Explicitly nonpersistent calls remain distinct; failed classification retains the raw capture. | [Capture failure/budget tests](gui/src-tauri/src/pipeline/executor/tests/response_capture.rs); [raw retention test](gui/src-tauri/src/pipeline/response_journal.rs) |
| F07 | Coordinator result and snapshot publication syncs file data, created parents and the published directory entry on Unix; Windows uses write-through moves. Errors propagate before successful adoption. | [Publication failure tests](gui/src-tauri/src/orchestration/durable.rs) and existing coordinator recovery tests |
| F08 | Formatted the affected test and converted historical temporary-file links to plain-text references without disabling link validation. | Complete formatting and Markdown-link gates |
| F09 | Support documentation now describes the network bootstrapper, actual CI coverage, and manual packaged qualification; migration documentation includes schema 15. | Configuration/documentation comparison and release-contract suite |

Validation on the final implementation:

- **729 frontend tests passed** in 100 files; production TypeScript/Vite build passed.
- **978 Rust tests passed** (964 library + 14 CLI), with the existing 11 opt-in tests ignored.
- **60 release-contract tests passed**; release identity remained Pipeline 0.9.5.
- All-target/all-feature Clippy with warnings denied, Rust formatting, frontend formatting, source-size, Markdown links (522 files) and Git whitespace checks passed.
- The isolated **no-model Workspace probe passed against Codex 0.153.4 on macOS arm64**: declared roots, denied private/sibling reads and writes, permission profiles, native lifecycle, command control and process cleanup. The native version meets the compatibility floor; it is newer than the checksum-pinned reference schema.

The first Rust link attempt encountered the machine's Xcode license-state error.
Validation used the already installed Command Line Tools through process-local
`DEVELOPER_DIR`/`SDKROOT`; no license was accepted and no machine setting changed.
The existing large PDF-chunk build warning remains outside these fixes.

Real-account, packaged, Windows/Linux execution and power-loss qualification remain
outstanding and are not inferred from local deterministic checks or the no-model
probe. The F01 Windows identity upgrade and F07 write-through publication need the
normal Windows release qualification. No implementation findings were deferred.
