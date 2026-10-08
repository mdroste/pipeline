# Workspace first-release qualification

This document separates implemented deterministic Workspace checks from the
authenticated, real-tool, packaged-app, and cross-platform work that still
requires a release run.

## Implemented deterministic gates

- Migrations through 15 are transactional and backed by the existing pre-migration SQLite online backup. Migrations 11–15 add the research desk, task exchanges, research programs, base prompts and self-discovery records. Migration 10 adds the app-owned `preferences` table (automatic conversation titles). Migration 9 adds exchange import/conflict journals and the storage trash journal. Migration 7 adds Research Studio records/history and local-job ownership/adoption journals; migration 8 adds theory, check and direction record kinds and re-creates the unchanged history trigger.
- All production Workspace database access, including supervisor projection and reconciliation, crosses the shared bounded worker/gate boundary. Whole-store archive work is exclusive, and restore is rejected during turn setup or an active turn.
- A single process-global turn permit spans setup through the matching terminal event. Connection epoch, native thread, and turn identity prevent stale or very fast events from corrupting the next submission.
- `.pwrx` export uses SQLite's backup API, includes the validated database, immutable blobs, app-owned conversation working files, and Markdown/JSON transcripts, and excludes the isolated Codex home and credentials.
- Archive inspection presents every archived root before import. Import requires an explicit remap-or-detach choice for each root and rejects traversal paths, directory and symlink entries, duplicate or unmanifested entries, expanded-size and entry-count excesses, newer schemas, corrupt databases, broken foreign keys, missing or hash-mismatched blobs, unsafe root remaps, and existing-store ID collisions.
- Restored active native bindings are retired. A later send creates a successor native thread from reviewed Workspace context; the UI does not promise cross-machine native resume.
- Evidence and artifacts are represented in `retained_blobs`; no Workspace blob collector may delete a listed reference.
- Recipe completion accepts only completed executions from the same Workspace and conversation, records every expected check exactly once, and keeps input failures, checks not recorded, unresolved issues, and missing evidence visible.
- Stata validation requires every declared log to exist and be clean; one clean log cannot conceal an error in another.
- The research inspector is lazy and tab-scoped, stream rendering is coalesced to 50 ms updates, and long transcripts render a fixed 200-message window with explicit paging.

## Crash/recovery matrix

| Boundary | Durable state before side effect | Recovery behavior |
|---|---|---|
| Send | draft and client submission ID | no automatic replay after ambiguous acknowledgement |
| Approval | epoch/request/method-scoped pending request | stale requests cannot authorize a new connection |
| Tool execution | running receipt before execution | restart exposes `outcome_unknown`; never reruns automatically |
| Artifact adoption | content-addressed staging then database reference | unreferenced staging is safe; referenced artifacts are immutable |
| Migration | online backup before transactional migration | rollback leaves the prior schema; backup remains recoverable |

## Release-run gates still required

- Execute the 20 fixtures in `research-evaluation-v1.json` for recipe, plain Workspace, and ordinary Codex variants with identical files/model/effort. A researcher must score correctness and traceability.
- Record cold/warm route, first-visible-send, stream cadence, context assembly, long-transcript, cancellation, idle CPU, and idle memory measurements on every supported platform. Millisecond budgets are exposed in the app; CPU/memory baselines are platform qualification data.
- Run authenticated native dynamic-tool qualification, packaging smoke tests, unsupported-platform permission failures, and forced Stata termination/cleanup qualification. The configured macOS LaTeX, Python and cooperative `oldstata` fixtures below have passed.
- Exercise process termination at each boundary in the crash matrix under the packaged build.

These live gates are not replaced by simulator or unit-test results.

## PI-00–PI-03 local observations (September 6, 2026)

The [machine-readable evidence record](project-surface-qualification.json)
contains sanitized outcomes. [Fixture sources](fixtures/project-surface/) and
ignored real-tool tests in `workbench/project/tests.rs` make the narrow checks
repeatable. They are supplementary evidence for this release record, not a
second release checklist.

| Evidence class | Observed | Remaining qualification |
|---|---|---|
| Storage/services | Schema 7 migration; scoped/duplicate requests; accepted-note history; exact/candidate/ambiguous anchors; dirty copies; conflicts; mode changes; partial acceptance; interrupted apply and rollback; undo; Research Studio history and job journals | Packaged abrupt-process-death runs at each journal boundary |
| Native protocol, no model | `workbench_probe` passes with Codex 0.153.4 on macOS arm64; declared roots, denied reads/writes, streaming/control and cleanup | Authenticated model tools, approval/question/interrupt, cross-platform runs |
| Real local research tools | Multi-file TeX build; Python numerical envelope check; `oldstata` regression/assertion/output validation; cooperative timeout and normal-date restoration | Forced wrapper kill/cleanup, broader real-project evaluation |
| Real local Git | Staged index, dirty working content, selected untracked file preserved with `--no-checkout` worktree and rejection | Other platforms, additional repository-feature configurations |
| Native GUI | Tauri debug + Vite, disposable store: create/register, copy/review/accept/undo, source selection, saved annotation, exact mapping, math rendering, all without model calls | Packaged WKWebView; WebView2/WebKitGTK; broader scanned/large PDF and accessibility evaluation |
| Backend measurement | 1,000-file inventory: 99–183 ms over three debug samples; 1.6 MB text / 128 KiB reader segment: 3.5 ms; project home: 36 ms | These exclude IPC/paint; cold/warm UI, long transcripts, idle CPU/memory and platform budgets remain unmeasured |

A live TeX fixture caught executable alias handling: authorizing the canonical
`pdftex` binary must preserve `pdflatex` as argv[0]. Native GUI verification
also caught an unsupported browser prompt in Workspace creation; the inline
form was verified after the fix. The initial release-test localhost restriction
was resolved by rerunning those checks outside the tool sandbox.

Research execution is now visibly **authorized host execution**, not a claimed
conversation sandbox. Changed executables/startup files/declared inputs invalidate
grants; untouched preexisting outputs cannot make a run successful. Stata's
30-second cooperative shutdown allowance is explicit. Forced termination leaves
an actionable cleanup requirement and does not qualify automatic recovery.

File acceptance is disabled on Windows. Linux shares the Unix implementation
but remains unqualified. No authenticated model turn was made for these local
checks, and no installer was produced or tested.

## PI-04–PI-09 local observations (September 6–7, 2026)

The [Research Studio evidence record](research-studio-qualification.json)
contains the narrower feature evidence. These checks cover bounded local paths;
they do not qualify authenticated model calls, arbitrary build scripts,
packaged crash recovery, or another platform.

| Package | Observed | Remaining qualification |
|---|---|---|
| PI-04 | Development Tauri GUI registered a disposable folder, retained an editor draft across tabs, saved through the recoverable path, configured a multi-file TeX build, displayed warning diagnostics and the exact retained PDF, mapped a source location with SyncTeX, and recorded page 1 as inspected. The ignored Rust fixture passed the multi-file LaTeX/SyncTeX path. | Packaged/WebView2/WebKitGTK, scanned/large PDFs, custom build scripts, abrupt death and cross-platform runs |
| PI-05 | Deterministic tests cover versioned finding imports, report paragraph spans, response flags/export, disagreement fields and focused-review coverage. | Authenticated end-to-end review and researcher-scored usefulness |
| PI-06 | All-target Rust tests pass; queue tests cover two slots, workspace conflict rejection, bounded logs, cancellation, changed queued inputs, unknown/adoption recovery. The development GUI completed a detached build and displayed its retained receipt. The cooperative oldstata fixture passed earlier. | Forced wrapper termination, packaged exit, sleep/OS kill and Windows/Linux |
| PI-07 | Deterministic tests cover host-bound imports, v2 IRF validation, uncertainty/units/specification compatibility and baseline preservation. Python exporter is syntactically valid; Stata exporter now formats leading-zero decimals but its live rerun is pending the host usage-limit gate. | Broader qualified toolchains and researcher-scored comparisons |
| PI-08 | Deterministic tests cover exact bindings, rounding/sign/unit drift, changed declared inputs and unrelated-input isolation. | Larger coverage budgets and selected research claims only; unlinked claims remain unknown |
| PI-09 | Development GUI imported duplicate BibTeX keys as distinct working/published records. Deterministic tests cover raw spans, notes, exact abstract/full-text support and version relationships. Official Zotero local API documentation was checked; the local endpoint was unavailable. | Live Zotero collection preview/import and any online acquisition capability |

The failed first Stata-exporter probe was a fixture defect (a leading-dot
decimal), corrected in the checked-in exporter. It is not counted as a passed
Stata result-export qualification until the required wrapper can be run again.

## PI-10 local observations (September 7, 2026)

| Package | Observed | Remaining qualification |
|---|---|---|
| PI-10 | Deterministic tests cover: a proof sketch with unresolved steps cannot be supported; an abandoned approach requires a reason and appears with its assumptions in the automatic project context while its derivation prose does not; numerical checks require a tested domain and cannot claim generality, heuristics and model assessments cannot establish a statement, and the host scope/label distinguish general, instance-only and refuting-instance evidence; promotion stages derivation prose into an isolated task copy with an assumptions header, leaves the working file untouched and records the promotion on the note; directions reject score fields and convert into a task only after a discriminating test is stated. Migration 8 keeps the history trigger and rejects unknown kinds. Frontend tests cover the disabled generality claim for numerical methods, the abandonment reason, side-by-side proposition/proof opening and direction conversion. | Native GUI walk-through of the Theory panel, authenticated model reads of theory records through catalog version 4, researcher-scored usefulness of the recipes, and the same packaged/cross-platform gates as PI-04–PI-09 |

## PI-11 and PI-12 (authoring half) local observations (September 7, 2026)

| Package | Observed | Remaining qualification |
|---|---|---|
| PI-11 | Deterministic tests cover: export closes anchors, papers, and notes from a task selection, writes a readable README and per-object JSON, excludes sessions and Codex state, and refuses a destination inside private storage; a tampered blob and a foreign file are rejected; import into a nonempty store remaps IDs into a new Workspace, resolves anchor→revision references, copies paper text, imports no conversations, is idempotent on reimport, records a conflict when a note diverges while keeping the local body, resolves it either way, and registers retained blobs; prune never moves referenced evidence, previews first, journals moves, restores, and empties on request. Frontend tests cover selection without conversations, default new-Workspace import target, conflict resolution, prune preview before apply, and draft saving. | Round trip between two machines, large-package performance, native GUI walk-through, and the packaged/cross-platform gates shared with earlier packages |
| PI-12 (authoring) | Draft validates through the portable workflow parser, adds a consolidation step, excludes incomplete steps, and lists execution/build steps as unsupported prerequisites. | Import of a saved draft through the Workflows page in the native GUI and a fixture run reproducing a selected supported process |
| PI-12 (concurrency) | Not implemented; the one-active-turn invariant is unchanged. | Requires a separate protocol/lifecycle design, compatibility-record update, and live qualification before any change |


## September 20 audit remediation

The [September 20 audit](../../PIPELINE_CODE_AUDIT_ASTRA_SEP20.md#implementation-follow-up--september-20-2026)
records the scoped fixes and their validation. Registered project roots are checked
again before native dispatch. Existing Windows registrations using pathname-only
identities require explicit folder re-registration; the new identity uses the
volume and file index. Transcript reads now use 200-item / 8 MiB payload pages;
rendering retains its 200-message window, and export traverses all pages under
its separate 64 MiB budget. Draft acknowledgements merge session metadata without
replacing conversation history, and background hydration preserves the composer.
These deterministic changes do not qualify power-loss recovery, authenticated
model calls, installers, or another platform.

## October 2 release-fix candidate

The candidate evidence manifest is [1.0.1-qualification.json](../releases/1.0.1-qualification.json).
Its live and packaged rows remain pending. Run the validator described in
[RELEASING.md](../../RELEASING.md) against the final candidate commit and actual
installer bytes. Passing source tests does not fill these rows.
