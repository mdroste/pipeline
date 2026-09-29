# Pipeline audit — September 7, 2026

This audit found **14 actionable issues: three P1, ten P2, and one P3**. The most urgent are loss of an unsaved editor draft, pruning of files used by an active conversation, and an incomplete trash journal after a failed database commit. The proposed fixes are local changes to the existing editor, storage, queue, and task code; none requires replacing an orchestration subsystem.

**Scope and version.** Audited the working checkout of Pipeline 0.9.5 on macOS, branch `codex/nf07-nf14`, based on commit `31b8959b14ecde0e550a4d820bb086725b245013`. The checkout already contained substantial modified and untracked implementation files. Findings describe that working checkout, including the new Tasks, missions, research-program, and file-workspace code. They should not all be attributed to the committed release. Source line numbers below refer to the checkout inspected on September 7.

**Method.** Read the project architecture and relevant topic documentation; traced runtime ownership, state transitions, storage boundaries, and frontend event handling; ran the existing test suites and build checks; then used temporary component tests and disposable SQLite stores to reproduce suspected failures. No implementation fixes were made during the original audit. Temporary audit tests were removed from the repository after recording their results.

**Priority definitions.** P1: fix before relying on the affected path for ongoing work or recoverable storage. P2: functional, compatibility, provenance, or resource-lifecycle defect that should be fixed in the normal development cycle. P3: an existing verification failure without a demonstrated runtime effect.

| ID | Priority | Issue | Evidence |
|---|---|---|---|
| 01 | P1 | Reload overwrites edits made while the read is pending | Component regression reproduced |
| 02 | P1 | Storage pruning moves an active conversation's working files | Disposable-store reproduction and runtime trace |
| 03 | P1 | Trash move can succeed without a recoverable database record | Commit-failure injection reproduced |
| 04 | P2 | Pruning applies categories that were not in the displayed preview | Component regression reproduced; backend traced |
| 05 | P2 | CRLF source selections point to the wrong bytes | Actual CodeMirror state reproduced |
| 06 | P2 | Older research archives inspect successfully but cannot restore | Schema-12 archive reproduction |
| 07 | P2 | Follow-ups after the first 100 records disappear | Disposable-store reproduction |
| 08 | P2 | The second queued follow-up becomes unusable after the first turn | Disposable-store reproduction |
| 09 | P2 | Monitors suppress repeated transitions into previously seen states | Disposable-store reproduction |
| 10 | P2 | Thirty-two blocked schedules starve later eligible schedules | Exact selection-query reproduction and scheduler trace |
| 11 | P2 | Task validation accepts dependencies that prevent parallel work from starting | Validator and state-machine reproduction |
| 12 | P2 | Retry drops the structured history of the previous action attempt | Source trace through persistence and output retrieval |
| 13 | P2 | Replacing a PDF viewer leaves old listeners and observers attached | Application and installed PDF.js implementation trace |
| 14 | P3 | The checkout fails the Rust formatting check | Check reproduced across 14 existing files |

## Implementation follow-up — issues 01–08

Issues **01–08 have been fixed in order** in the working checkout. The original
findings below remain as the audit record; their line references describe the
pre-fix snapshot. This follow-up does not implement issues 09–14.

| Issue | Implemented change | Added regression coverage |
|---|---|---|
| 01 | Reload preserves a buffer changed after the read began, including its original base hash and retained draft | Delayed reload plus typing; clean reload followed by a save using the new hash |
| 02 | Conversation working directories are retained; storage mutations hold the native turn permit and exclusive store gate and reject active jobs | Running unfiled conversation; idle scratch cleanup; mutual exclusion with native setup |
| 03 | The trash recovery record commits before rename; listing reconciles interrupted moves/restores under the exclusive gate | Deferred commit failure; reopen before/after move and after interrupted restore |
| 04 | Category changes invalidate the preview; apply requires a matching token over categories, paths and nested file identities | Changed categories, added files, replaced same-size files, stale async previews and token transmission |
| 05 | CodeMirror preserves the detected LF/CRLF separator and reports serialized-source selection offsets | Unicode byte-span round trips, one-character edits, and changed newline conventions on reload |
| 06 | Isolated archive databases pass integrity/reference checks, use the existing store migrations, then pass validation again before restore | Schema-12 records and evidence restore; migration failure leaves the destination empty; newer/mismatched declarations reject |
| 07 | All active follow-ups are returned before a 100-record page of terminal history; UI exposes older/newer pages | 105 historical requests followed by a visible, cancellable/runnable request; active controls on older pages |
| 08 | Explicit context review and confirmation refresh an unchanged queued request; sending remains separate | New conversation turn, changed context/settings/root, stale revision, preserved text/model/effort, and prevention of duplicate dispatch |

The source editor and queue changes remain in their existing components. Storage
uses its existing gate, native permit, journal table, and migration runner; no
new runtime, database schema, or dependency was introduced.

**Fix validation.** The frontend suite passed **611 tests across 81 files** with
`cd gui && npm test -- --no-file-parallelism`; `cd gui && npm run build` also
passed. A separate copy of the saved starting Rust source with these fixes
applied passed **904 library tests and 14 CLI tests**, with 10 library tests
ignored, using `cargo test --locked --all-targets -- --test-threads=1`. Strict
Clippy (`cargo clippy --locked --all-targets -- -D warnings`) passed on that same
copy. Regression tests are retained alongside the implementation.

The isolated Rust run was necessary because other work was concurrently
refactoring this checkout. Live checks encountered transient unrelated compile
and lint errors; a parallel Rust run also failed an existing job-queue test
whose fixture shares a global launcher with other tests. The isolated serial
suite passed that test. These results validate the saved starting source plus
the fixes, rather than certify the changing combined Rust checkout. The
repository-wide formatting check still fails on existing files (issue 14).
No authenticated model run, native GUI session, or packaged release was
qualified by these checks.

Validation logs: frontend (historical temporary path: `/tmp/pipeline-astra-fixes-frontend.log`),
web build (historical temporary path: `/tmp/pipeline-astra-fixes-build.log`),
isolated Rust suite (historical temporary path: `/tmp/pipeline-astra-fixes-isolated-rust.log`),
isolated Clippy (historical temporary path: `/tmp/pipeline-astra-fixes-isolated-clippy.log`),
live Rust attempt (historical temporary path: `/tmp/pipeline-astra-fixes-rust.log`), and
formatting (historical temporary path: `/tmp/pipeline-astra-fixes-fmt.log`).

## Implementation follow-up — issues 09–13

Issues **09–13 have been fixed in order**. These changes use the existing
scheduled-check revision, schedule query, chain validator, task event journal,
and PDF.js lifecycle API. No dependency, database migration, background runner,
or implicit dependency scheduler was added. Issue 14 remains outside this work.

| Issue | Implemented change | Added regression coverage |
|---|---|---|
| 09 | Each result carries the revision captured before the check; attention deduplicates that occurrence rather than every visit to the same state | A → B → A → B, acknowledged alerts, unchanged observations, repeated failure episodes after recovery, duplicate persistence, and pause/resume during a check |
| 10 | Due selection excludes schedules with nonterminal occurrences before applying its 32-row limit; the transaction still rechecks overlap | Thirty-two blocked schedules do not starve the 33rd; blocked cursors remain coalesced; completing one occurrence permits exactly one new run |
| 11 | Validation follows execution order for bindings and explicit Workspace prompt references, isolating parallel branches and checking loop entry/body/exit availability | Sibling, self and forward dependencies reject; parallel joins and embedded chains remain valid; a shared frontend/Rust fixture preserves the guarded built-in review loop |
| 12 | Retry appends complete prior receipts to the existing event journal in the same transaction that removes them from current progress; Activity log and output/artifact lookup select the exact operation | Failed and uncertain attempts survive retry/reload, late results remain inspectable, stale retries do not duplicate history, cross-task/address lookups reject, and an injected journal failure rolls back removal |
| 13 | Each viewer receives an abort signal; cleanup aborts, clears viewer/link-service documents, and handles loading-task destruction rejection | Repeated document replacement, DOM scroll-listener removal, retry, fallback, unmount and rejected teardown |

Output availability is an ordering check, not a guarantee that a conditional
path ran or supplied a correctly typed value. Existing runtime checks still
handle those cases. Earlier loop outputs are optional until guarded or given a
usable fallback. Previously discarded retry receipts cannot be reconstructed
from older textual events; those events remain readable.

**Fix validation.** The current checkout passed **917 Rust library tests and
14 CLI tests**, with 10 opt-in library tests ignored, using
`cargo test --locked --all-targets -- --test-threads=1`. Strict all-targets
Clippy and the production web build passed. The PDF lifecycle tests exercise
component ownership and a real DOM listener with mocked PDF.js objects; the
installed PDF.js source confirms that the signal disconnects its observer.
No authenticated provider run or native/packaged memory-stress qualification
is claimed.

The final live frontend run passed **621 tests** and failed **nine tests** in
`App.test.tsx` and `WorkspaceDocumentReader.test.tsx`. Those failures concern
navigation roles and control names changed by concurrent UI work. The earlier
live attempt encountered 28 such failures before that work's tests were updated.
The new regressions passed in the live runs.

For independent verification, the saved starting frontend source plus these
fixes passed **616 tests across 81 files**: 613 passed in the full isolated run;
the remaining three DTO tests passed separately after copying their required
fixture into the temporary checkout. This isolates the fixes from the concurrent
UI changes; it does not certify the combined frontend checkout as passing.

Validation logs: Rust suite (historical temporary path: `/tmp/pipeline-astra-09-13-rust.log`),
Clippy (historical temporary path: `/tmp/pipeline-astra-09-13-clippy.log`),
web build (historical temporary path: `/tmp/pipeline-astra-09-13-build.log`),
final live frontend run (historical temporary path: `/tmp/pipeline-astra-09-13-frontend-final.log`),
isolated frontend suite (historical temporary path: `/tmp/pipeline-astra-09-13-frontend-isolated.log`), and
isolated DTO rerun (historical temporary path: `/tmp/pipeline-astra-09-13-frontend-isolated-dto.log`).

## 01 — Reload overwrites edits made while the read is pending

**Priority: P1.** A routine editor action can erase unsaved work.

**Locations:** [FileWorkspace.tsx:105](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/file-workspace/FileWorkspace.tsx:105), especially the unconditional replacement at line 140; Reload at line 431; draft retention at line 207.

**Trigger and observed behavior.** Open a clean editable file, click Reload, and type while `adapter.read()` is still pending. When the read completes, `load()` replaces the buffer with the returned disk contents. A component test delayed the second read, entered `NEW UNSAVED WORK`, then returned `Original`. The editor changed back to `Original`; the assertion that the new draft survived failed.

The Reload button is disabled when an editable buffer is already dirty, but this only checks the state before the read starts. The editor remains writable during that read. The replacement also makes the buffer clean, so the normal retention effect can remove the previously retained local draft. This is a lost-update bug, even when the file on disk has not changed.

**Proposed fix.** Capture a per-buffer edit generation when starting a reload. Apply the returned contents only if that generation is unchanged; otherwise preserve the current draft and explain that the reload did not replace newer edits. A simpler acceptable alternative is to make that buffer temporarily read-only during the explicit reload. Reuse the existing pending-read state rather than adding a general operation framework.

**Regression check.** Delay Reload, edit, resolve the read, and verify both the displayed draft and its retained copy survive. Also verify that a reload with no intervening edits still updates the base file and hash.

## 02 — Storage pruning can move an active conversation's working files

**Priority: P1.** Cleanup can disrupt active work and remove files from their expected paths.

**Locations:** [retention.rs:152](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/release/retention.rs:152), active-job check at line 182 and pruning guard at line 296; [store.rs:1261](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/store.rs:1261); [commands.rs:881](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:881).

**Trigger and observed behavior.** Unfiled conversations, and conversations without a registered project root, use `jobs/conversations/<session-id>` as their native runtime directory. Storage scanning classifies every immediate child of `jobs` as disposable `job_scratch`, including the entire `conversations` directory. The pruning guard counts only queued/running `research_executions`; it does not check native Workspace turns.

In a disposable store, I created an unfiled conversation with a persisted running turn and a file at its runtime path, then applied `job_scratch` pruning. Result:

```text
ACTIVE_CHAT_PRUNE: moved=1, active_file_exists=false
```

The reproduction used a stored running-turn fixture, not an authenticated model process. The command path confirms the missing runtime guard: pruning uses ordinary `run_store`, whereas archive restoration explicitly checks native setup/turn ownership and active research jobs under an exclusive store operation. Moving the directory does not immediately destroy its bytes, but paths used by subsequent tool calls disappear, and the retained conversation can lose access to its own work until manually restored.

**Proposed fix.** Exclude active conversation roots from disposable scratch and protect the scan/apply interval against new turn or job startup using the existing ownership/gating mechanisms. At minimum, reject scratch pruning while native setup, a native turn, or a local research job is active. A database-only check without synchronization leaves a startup race. Do not introduce another runtime registry.

**Regression check.** Attempt scratch pruning with an active unfiled conversation and with native setup in progress; neither may move its runtime directory. Verify idle disposable job scratch remains prunable and referenced artifacts remain protected.

## 03 — A successful trash rename can lose its recovery record

**Priority: P1.** The advertised recoverable cleanup path is not recoverable through the application after a commit failure or a crash at the wrong point.

**Location:** [retention.rs:347](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/release/retention.rs:347).

**Trigger and observed behavior.** `prune_storage()` starts a SQLite transaction, inserts a `storage_trash` row, renames the file into trash, and only then commits. If the commit fails, SQLite rolls back the row but cannot roll back the filesystem rename. A process interruption between the rename and durable commit has the same consistency problem.

I injected a deferred foreign-key violation through a trigger in a disposable database. This deliberately forces failure at commit, after the real filesystem move, without touching user data:

```text
TRASH_COMMIT_FAILURE:
  error=Storage operation failed: FOREIGN KEY constraint failed
  original_exists=false
  listed_trash=0
```

The file remained under the private trash directory, but `list_trash()` returned no recovery entry. This demonstrates orphaned recoverable data, not immediate permanent deletion. The injected constraint is a test mechanism for the commit boundary, not a claim that production schemas contain that constraint.

**Proposed fix.** Durably record a small move intent before renaming, then finalize its state after the rename. Reconcile unfinished intents on store startup or before listing trash. Handle ordinary failures with a compensating move where possible. A compensation alone does not cover process interruption. Use the project's existing journal approach; a new filesystem transaction framework is unnecessary. Apply the same ordering discipline to restore operations.

**Regression check.** Inject failure immediately after rename and simulate reopening the store. The file must either remain at its original path or appear as a recoverable trash entry. Repeat for an interrupted restore, and preserve enough detail to explain a partially completed multi-file prune.

## 04 — Pruning is not bound to the displayed preview

**Priority: P2.** The user can approve one preview and move a different set of files.

**Locations:** [WorkspaceExchangePanel.tsx:226](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspaceExchangePanel.tsx:226), apply handler at line 233; [retention.rs:288](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/release/retention.rs:288).

**Trigger and observed behavior.** Select Orphan blobs and preview deletion. Then also select Job scratch. Changing the checkbox updates `pruneKeys` without clearing `prunePlan`; Move to trash remains enabled while the text still describes the old preview. The apply handler sends the current keys, including the newly selected category.

A focused component test reproduced this: after the second category was selected, the assertion that Move to trash should be disabled failed. Independently, the backend accepts only categories and an `apply` flag, rescanning on apply. It cannot establish that the applied candidates match any prior preview, even if the frontend categories stay unchanged.

**Proposed fix.** Clear the preview whenever the categories change. Bind backend application to a hash/token of the reviewed candidate paths and relevant identities, and require a new preview if that set changes. Workflow retention already has a preview-token pattern in [runs/retention.rs:69](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/runs/retention.rs:69) and checks it at line 286; follow that pattern locally.

**Regression check.** Changing categories invalidates the preview. Adding or replacing a candidate between preview and apply must produce a refresh-required result rather than silently broadening the move.

## 05 — CRLF files produce incorrect selection offsets and implicit newline conversion

**Priority: P2.** Source evidence can point to the wrong bytes, and a small edit can rewrite every line ending.

**Locations:** [SourceEditor.tsx:129](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/file-workspace/SourceEditor.tsx:129), update/selection callbacks at lines 151–160; [FileWorkspace.tsx:356](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/file-workspace/FileWorkspace.tsx:356).

**Trigger and observed behavior.** Load a Windows-style CRLF text file and select text after the first line. CodeMirror's default document representation uses one position per line separator. `FileWorkspace` interprets those positions as indices into the original `buffer.draft`, which still contains two-character `\r\n` separators, before converting the prefix lengths to UTF-8 byte offsets.

Using the actual installed `EditorState` with `alpha\r\nbeta\r\n`, the second line is `beta`, with positions 6–10. Slicing the original string at those positions returns `\nbet`. The regression assertion failed with exactly that mismatch. The selection therefore combines the visible quote `beta` with a byte range for different source text. Separately, `doc.toString()` in the change callback emits LF text, so an ordinary edit normalizes the whole draft.

**Proposed fix.** Define the newline representation explicitly. Preserve the detected separator when serializing edits, and map editor positions back to source positions before computing UTF-8 offsets. With a configured line separator, serializing a document prefix can provide the original-source prefix length; raw CodeMirror positions still must not be applied directly to CRLF strings. Keep the existing content-hash identity check.

**Regression check.** For CRLF and LF fixtures containing non-ASCII text, decode the reported byte range and verify it equals the selected quote. Save a one-character CRLF edit and verify unrelated line endings remain unchanged.

## 06 — Older research archives inspect successfully but cannot be restored

**Priority: P2.** A schema upgrade makes previously created research backups unusable through the current restore path.

**Locations:** [archive.rs:111](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/release/archive.rs:111), schema equality check at [archive.rs:663](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/release/archive.rs:663); existing migration runner at [store.rs:1686](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/store.rs:1686).

**Trigger and observed behavior.** Inspect and restore an archive from a prior supported store schema. Manifest validation accepts versions no newer than the application. Import subsequently requires the embedded database version to equal the current version exactly; it never migrates an older database.

I built a genuine schema-12 database from checked-in migrations 001–012, checked its SQLite integrity and foreign keys, and packaged it as a format-1 research archive. The current schema is 13. Inspection succeeded and reported version 12. Import into a new empty destination returned:

```text
Archive store schema 12 is not directly restorable by schema 13
```

This fixture was generated from the migrations rather than exported by an older installed binary. It establishes the schema compatibility failure; it does not qualify every historical archive format.

**Proposed fix.** Validate the extracted database, run the existing applicable migrations on the isolated extracted copy, validate again, and then perform the normal restore. Continue rejecting newer schemas and invalid databases. Inspection should identify an unsupported archive before offering an apparently usable restore. Avoid adding a second migration implementation.

**Regression check.** Restore a previous-schema archive with representative retained objects and blob references into an empty current store. Verify migrated records and remapped paths, and verify migration failure leaves the destination untouched.

## 07 — Follow-ups become invisible after 100 historical records

**Priority: P2.** The queue eventually stops exposing newly saved messages.

**Location:** [followups.rs:57](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/programs/followups.rs:57), insertion at line 63 and list query at line 69; frontend list retrieval at [Followups.tsx:40](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/research-programs/Followups.tsx:40).

**Trigger and observed behavior.** Accumulate 100 completed, failed, or cancelled follow-ups in a conversation, then enqueue another. Listing orders all records by ascending position and returns the first 100, including terminal history. New records receive ever-higher positions. The separate 20-item limit applies only to active entries and therefore does not prevent this condition.

In a disposable store, I enqueued and cancelled 100 entries, then enqueued `NEW FOLLOW-UP`:

```text
FOLLOWUP_PAGE: returned=100, new_visible=false
```

The frontend has no cursor or history page for retrieving later entries. New requests persist but are absent from the controls needed to run or cancel them; continuing to enqueue can eventually fill the 20-pending limit with hidden requests.

**Proposed fix.** Always return the bounded active queue separately from terminal history. Page history, or initially show a bounded recent history with an explicit way to retrieve older entries. Do not delete retained requests merely to make the current query work.

**Regression check.** With more than 100 historical entries, newly queued messages remain visible and runnable/cancellable. Queue ordering and the pending cap still apply only to the intended active set.

## 08 — Multiple queued follow-ups cannot advance through successive turns

**Priority: P2.** Saving several follow-ups works, but completing the first invalidates the remaining saved requests with no supported refresh action.

**Locations:** [followups.rs:24](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/programs/followups.rs:24), dispatch validation at line 133; [tasks.rs:49](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/tasks.rs:49); [Followups.tsx:108](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/research-programs/Followups.tsx:108).

**Trigger and observed behavior.** Enqueue two messages at the same conversation cursor. Dispatch the first and let its new turn complete. The second still carries the cursor captured before that turn. `validate_binding()` accepts a running-to-completed transition of the same turn, but rejects a new turn ID.

The disposable-store reproduction prepared the first dispatch and then inserted its completed-turn state before preparing the second. The second returned:

```text
The conversation, project, or research settings changed.
Review the new context before continuing the task.
```

This check protects context scope and should remain. The defect is that the queue supports multiple pending requests but provides no way to review and refresh a saved request's binding: controls expose run, reorder, and cancel. The user must cancel and reconstruct the message. This finding does not call for automatic submission or bypassing context checks.

**Proposed fix.** Add an explicit review-and-refresh action for a queued request. Show its saved text and current selected context, capture a fresh binding and fingerprint after review, and then allow the existing one-turn Run action. Preserve the user's text/model/effort while making any changed context visible.

**Regression check.** Queue two messages, complete the first, refresh the second through the supported action, and dispatch it once. Project, root-identity, harness, and selected-context changes must still require review.

## 09 — Monitors suppress later transitions into previously seen states

**Priority: P2.** A repeated meaningful change can produce neither a new attention item nor a notification.

**Location:** [monitors.rs:152](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/programs/monitors.rs:152), deduplication and insertion at lines 158–166; acknowledgement at line 194.

**Trigger and observed behavior.** Let a monitored object move through A → B → A → B. Change detection correctly compares each result with the last fingerprint. However, attention IDs and the stored deduplication key depend only on the destination fingerprint for that check. The second transition to B reuses the first B attention identity, so `INSERT OR IGNORE` suppresses it. Acknowledging the original item does not remove that identity.

Calling the real `record_outcome()` against a disposable store produced notification decisions:

```text
MONITOR_A_B_A_B: [false, true, true, false]
```

The initial false is the intended quiet baseline. The final false is the missed transition. The same construction also suppresses a later episode of three identical failures after recovery, because the failure body hashes to the prior attention identity.

**Proposed fix.** Deduplicate processing of the same check occurrence, not all future visits to the same state. Include a stable occurrence or transition sequence in the attention identity while retaining last-fingerprint comparison to keep unchanged checks quiet. Merely hashing the old and new states is insufficient for repeated A → B transitions.

**Regression check.** A → B → A → B yields `[false, true, true, true]`; repeated unchanged B checks remain quiet. A recovered monitor can notify about a new failure episode, while retrying persistence of the same occurrence does not duplicate its attention item.

## 10 — Blocked schedules consume the entire due-schedule page

**Priority: P2.** Later schedules can remain unfired indefinitely while unrelated schedules wait for input or attention.

**Location:** [orchestration/store.rs:498](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/orchestration/store.rs:498), selection at line 501 and active-occurrence check at line 548.

**Trigger and observed behavior.** Create 33 overdue schedules. Give the earliest 32 an existing nonterminal occurrence, such as a task waiting for user input; leave the 33rd eligible to run. `fire_schedules()` first selects the earliest 32 overdue schedules, then skips each with an active occurrence. Their overdue cursors intentionally remain unchanged. Every later poll selects those same 32, and the eligible 33rd is never examined.

I extracted the actual selection SQL and ran it against a minimal SQLite fixture, then applied the actual active-occurrence predicate:

```text
STARVATION: selected=32 eligible_selected=0 33rd_selected=false
```

This was a query-level reproduction, not a live desktop scheduler test. The surrounding loop establishes that skipped schedules retain their place. There is no documented 32-schedule total limit that would make the later schedule invalid.

**Proposed fix.** Exclude schedules with nonterminal occurrences in the initial SQL, before `LIMIT 32`, using the same active-state predicate as the existing check. Keep the in-transaction check for races. This needs a query correction, not a fair-scheduling subsystem.

**Regression check.** With 32 earlier blocked schedules, an eligible 33rd creates its occurrence on the next pass. The blocked schedules retain their single coalesced pending occurrence and do not overlap their active runs.

## 11 — Validation accepts dependencies that prevent parallel execution from starting

**Priority: P2.** A task can pass validation but immediately enter an attention state it cannot resolve through ordinary execution.

**Locations:** [definition.rs:440](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/orchestration/definition.rs:440), parallel-reference validation at line 468; [state.rs:85](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/orchestration/state.rs:85), parallel traversal at line 144.

**Trigger and observed behavior.** Define two parallel branches: the first contains a producer step; the second starts with a condition that reads that producer's output. Reference validation checks that the producer ID exists somewhere in the chain, without checking whether its output is available at this point.

On the first state-machine advance, the first branch contributes a ready producer. The second branch evaluates its condition before the producer has run, returns an unknown-input error, and discards the accumulated ready actions. The producer therefore never starts. A minimal two-branch definition produced:

```text
CROSS_BRANCH_VALIDATE: Ok(())
CROSS_BRANCH_ADVANCE:
  Attention("Consumer: condition has missing or unknown inputs")
```

The reproduction used only a zero-second delay as the producer, so it required no provider or external tool. The problem occurs before the eventual condition result matters.

**Proposed fix.** Reject output references to sibling parallel branches at definition validation and explain that dependent steps belong in sequence. Check other clearly unavailable references, including self/forward references, with the same traversal. Account for constructs such as a repeat-until condition that legitimately reads its own completed body. Apply availability checks to all binding-bearing fields, including Workspace prompt references. Do not add an implicit dependency scheduler to support an invalid parallel definition.

**Regression check.** The reproduced definition fails validation with the offending step ID. Independent parallel branches remain valid, and a condition after the parallel block can read completed outputs.

## 12 — Retry drops the structured history of the previous action attempt

**Priority: P2.** After retry, the task's recorded history no longer identifies the earlier attempt well enough to inspect its output or reconcile its operation through the normal API.

**Locations:** [orchestration/commands.rs:173](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/orchestration/commands.rs:173), output lookup at line 350; [orchestration/store.rs:231](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/orchestration/store.rs:231); start-event persistence at [orchestration/mod.rs:375](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/orchestration/mod.rs:375).

**Trigger and source evidence.** Put a task in attention with a failed or uncertain receipt, then choose Retry. The handler removes those receipts from `run.progress.receipts` and saves the new run. Its comment says the old attempt journal is retained in events and on disk, but the retry event contains only `Task updated`; start events contain the label, not a structured receipt or operation ID. Store saving replaces the run's JSON instead of retaining the previous revision as history.

Output retrieval looks up the current receipt by address and uses its operation ID to find `actions/<operation>/result.json`. After removal, the earlier operation is no longer available through that lookup; after a new attempt starts, the address points to the replacement. Existing event text can retain an error message, and some artifacts can remain on disk, but neither preserves the complete address-to-attempt mapping. This is a source-confirmed provenance/recovery gap; I did not run a provider action to reproduce it end to end.

**Proposed fix.** Before removing a receipt, append its operation ID, address, sequence, state, timestamps, error, and output/child references to durable attempt history in the same transaction. A structured payload in the existing event journal is sufficient; a separate task engine or database is unnecessary. Let inspection address the old operation explicitly while keeping current progress small.

**Regression check.** Fail or interrupt an action, retry it, complete the replacement, and reload the task. Both attempts remain distinguishable and inspectable, including the earlier operation's result if one arrived before retry. Existing textual error events alone should not satisfy this test.

## 13 — Replacing a PDF viewer leaves old listeners and observers attached

**Priority: P2.** Repeated PDF changes or retries can retain old viewer objects and duplicate work on the current scroll container.

**Location:** [PdfReader.tsx:154](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/file-workspace/PdfReader.tsx:154), effect cleanup at line 266.

**Trigger and source evidence.** Change `documentKey`, retry loading, or switch the viewer into fallback mode. The effect constructs a new `PDFViewer`, but cleanup only calls `viewer.cleanup()` and destroys the loading task. The installed PDF.js implementation attaches a scroll listener bound to that viewer and starts a `ResizeObserver` in its constructor. An optional abort signal controls listener removal and observer disconnection; Pipeline does not supply it.

In the installed `pdfjs-dist/legacy/web/pdf_viewer.mjs`, the constructor's abort cleanup is at lines 13264–13273. Its `cleanup()` at line 14459 only resets unfinished page views; it is not viewer destruction. Destroying the PDF loading task does not detach the viewer's DOM listeners. Old listeners therefore retain viewer instances on a reused container, and observers are not explicitly disconnected. This was established by tracing the shipped dependency implementation; no numerical memory-growth claim or long-running GUI stress result is asserted.

**Proposed fix.** Give each effect instance an `AbortController`, pass its signal to the viewer supported by the installed implementation, and abort during cleanup. Clear the viewer/link-service document references and destroy the loading task with rejection handling. If the bundled declaration lacks the signal option, add a narrow local type adaptation rather than broadly disabling type checks.

**Regression check.** Replace a document repeatedly and verify each prior viewer's signal is aborted and its document references are cleared. In a real browser lifecycle check, listener/observer counts should return to the baseline after unmount, with only the current viewer responding to scroll.

## 14 — The existing checkout fails the Rust formatting check

**Priority: P3.** This is a reproducible repository check failure, not a demonstrated runtime bug.

**Evidence.** `cargo fmt --all -- --check`, run from `gui/src-tauri` before adding audit probe files, exited unsuccessfully and reported differences in 14 existing source files. Examples include [commands.rs:12](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/commands.rs:12), [runs/retention.rs:180](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/runs/retention.rs:180), [workbench/commands.rs:449](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:449), and [workbench/store.rs:1768](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/store.rs:1768).

The other affected areas are run history, project relations, project/studio results and review, project state, exchange and its tests, research execution and execution plans, and research state. The full list and diff are in the formatting log below.

**Proposed fix.** Format the affected Rust changes with the pinned toolchain and inspect the diff for accidental unrelated edits. No new lint rule is needed. Formatting was deliberately not applied as part of this report-only audit.

**Regression check.** The same `cargo fmt --all -- --check` command exits successfully.

## Original audit validation and coverage

Commands below use paths relative to the repository for reproducibility. Frontend checks used the repository's available Node 24.18.0 environment. Rust used the pinned 1.97.1 toolchain. All probes used temporary stores or mocked components, not the user's research database.

| Check | Result |
|---|---|
| `cd gui && npm test` | 593 tests passed across 80 files |
| `cd gui && npm run build` | TypeScript and Vite production build passed |
| `cd gui/src-tauri && cargo test --locked --all-targets` | 893 library tests and 14 CLI tests passed; 10 library tests ignored |
| `cd gui/src-tauri && cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cd gui/src-tauri && cargo fmt --all -- --check` | Failed on 14 existing files; finding 14 |
| `cd gui && npm run test:release` | 58 tests passed; release identity validated as Pipeline 0.9.5 |
| `python3 scripts/release/test-paddle-parser-sidecar.py` | Stub-based parser contract passed |
| Temporary editor regressions | Two expected-correctness assertions failed, reproducing findings 01 and 05 |
| Temporary pruning-preview regression | One expected-correctness assertion failed, reproducing finding 04 |
| Temporary Rust service probe | Reproduced findings 02, 03, 06, 07, 08, 09, and 11 |
| Extracted scheduler-query probe | Reproduced selection starvation for finding 10 |

The initial release-test attempt failed because the sandbox denied a local test server's port binding. Re-running that same suite with scoped permission outside the sandbox passed all 58 tests. That first failure is environmental and is not counted as a Pipeline bug. Vite also emitted a large-chunk warning; size alone does not establish a functional defect and is not counted here.

| Area reviewed | Work performed | Qualification limits |
|---|---|---|
| Workflows and Paper Review | Run/rerun/batch paths, cancellation ownership, provider errors, extraction, report quality, artifacts, history and retention; existing Rust/frontend tests | No paid model calls, authenticated provider failures, or real end-to-end paper review |
| Workspace | Native runtime/store boundary, command gates, session and task bindings, file editing, research execution, project/studio paths | No authenticated native turn or computer-use session |
| Research programs | Follow-ups, monitors, retained context and related asset/experiment/campaign paths; targeted service probes | No live network monitor or real scientific-tool execution |
| Durable Tasks | Definition validation, state machine, scheduling, persistence, adapters, retry and recovery; missions ownership/accounting paths inspected | Not every mission transition independently exercised; no packaged background/tray qualification |
| Storage and portability | Schema migration, archives, exchange, references, retention and trash; disposable archive/fault-injection tests | No user's archive imported; no cross-device or cloud-sync qualification |
| Frontend file/PDF workspace | Async buffer updates, selection offsets, viewer lifecycle, navigation and adapter boundaries | PDF leak is source-established, not measured in a long-running Tauri session |
| Build and release tooling | Production web build, Clippy, formatting, release-script tests and parser contract | No signed/notarized installer, cross-platform build, or real Paddle model inference |

Passing existing tests does not negate the targeted failures. The audit covers the principal subsystems and several failure boundaries; it is not a claim that every source line, platform, external tool, or authenticated runtime path is bug-free.

## Reproduction material and suggested order of work

Temporary evidence remains available on this machine:

- Rust service probe (historical temporary path: `/tmp/pipeline-astra-sep7-audit/probe.rs`), editor regression source (historical temporary path: `/tmp/pipeline-astra-sep7-audit/astra_sep7_audit.test.tsx`), and preview regression source (historical temporary path: `/tmp/pipeline-astra-sep7-audit/astra_sep7_preview_audit.test.tsx`).
- Legacy schema fixture (historical temporary path: `/tmp/pipeline-astra-sep7-audit/schema12.pwrx`) and scheduler query evidence (historical temporary path: `/tmp/pipeline-astra-sep7-audit/starvation-query.txt`).
- Service probe output (historical temporary path: `/tmp/pipeline-astra-sep7-probe.log`), editor failures (historical temporary path: `/tmp/pipeline-astra-sep7-ui-probe.log`), and preview failure (historical temporary path: `/tmp/pipeline-astra-sep7-preview-probe.log`).
- Frontend baseline (historical temporary path: `/tmp/pipeline-astra-sep7-frontend.log`), Rust baseline (historical temporary path: `/tmp/pipeline-astra-sep7-rust.log`), build (historical temporary path: `/tmp/pipeline-astra-sep7-build.log`), Clippy (historical temporary path: `/tmp/pipeline-astra-sep7-clippy.log`), formatting diff (historical temporary path: `/tmp/pipeline-astra-sep7-fmt.log`), and successful release checks (historical temporary path: `/tmp/pipeline-astra-sep7-release-unsandboxed.log`).

These are temporary local artifacts, not repository dependencies. To rerun the frontend probes, copy them back into `gui/src/test/` and run the corresponding Vitest file; they intentionally assert the desired behavior and fail until fixed. To rerun the Rust service probe, temporarily place `probe.rs` at `gui/src-tauri/examples/astra_sep7_audit.rs` and run `cargo run --locked --manifest-path gui/src-tauri/Cargo.toml --example astra_sep7_audit`. It creates new stores under the system temporary directory and expects the legacy archive fixture at the path used above. Its deferred-constraint trigger must only be used in a disposable store.

Start with findings 01–03 to protect drafts and recoverability, then bind pruning to its preview and correct source offsets. The queue/monitor/scheduler fixes can remain small independent changes. Follow with archive compatibility, attempt-history preservation, PDF teardown, and formatting. Add the focused regression for each corrected behavior; no broad rewrite or new abstraction is warranted by these findings.
