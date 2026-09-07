# September 7 release bug plan — Astra

Reviewed September 7, 2026, against the current working tree, including the uncommitted Workspace and Workflow Codex App Server implementation. This is a repair plan; no product fixes were made during the audit.

Workspace and Reviews have substantially separate storage, credentials, processes, and cancellation paths. Keep that architecture. The release risks below are concrete failures within those boundaries and at their UI entry points. None requires replacing the orchestration engine, introducing a provider abstraction, or implementing concurrent Workspace turns.

## Priorities and evidence

**P1:** fix before releasing these features for ordinary use. **P2:** genuine defects to fix next; some require longer histories, interrupted operations, or less common interactions. No P0 issue was established.

**Reproduced** means a disposable test demonstrated the current erroneous behavior. **Code-confirmed** means the stated trigger follows from the inspected implementation; it was not exercised in an authenticated native session. Suggested regression tests below should assert the corrected behavior.

| Order | Priority | Issue | Evidence |
|---|---|---|---|
| 01 | P1 | Destructive actions run before native confirmation is answered | Reproduced |
| 02 | P1 | Ambiguous Workspace submission releases ownership of a possibly running turn | Code-confirmed |
| 03 | P1 | A replaced registered folder can redirect native access into private storage | Reproduced at the store boundary |
| 04 | P1 | App-server descendants survive after their leader exits | Reproduced |
| 05 | P1 | Reconnecting drops the binding fingerprint and loses native conversation continuity | Reproduced at the store boundary |
| 06 | P1 | Attaching or relocating a folder does not invalidate the native thread binding | Reproduced at the store boundary |
| 07 | P1 | Authoritative history cannot repair an already recorded sparse completion | Reproduced |
| 08 | P1 | Streaming state crosses conversation boundaries | Reproduced |
| 09 | P1 | Approval responses omit the connection epoch | Code-confirmed |
| 10 | P1 | Storage cleanup can remove a native conversation's working directory | Reproduced at the store boundary |
| 11 | P1 | Native text prompts leave several Workspace actions unusable on macOS | Code and installed dependency confirmed |
| 12 | P1 | Archive restore is inaccessible from the empty store it requires | Code-confirmed |
| 13 | P2 | Reconciled messages are ordered by random IDs | Reproduced |
| 14 | P2 | Leaving Workspace before autosave loses the latest draft edits | Reproduced |
| 15 | P2 | Background approvals disappear when navigating between conversations | Reproduced |
| 16 | P2 | Long conversations stop loading instead of paging | Reproduced |
| 17 | P2 | Event-bridge lag can leave the global turn permit held indefinitely | Code-confirmed |
| 18 | P2 | Trash moves are not recoverable across the transaction/rename crash boundary | Code-confirmed |
| 19 | P2 | Theory promotion bypasses the existing turn guard for file mutations | Code-confirmed |

## P1 repairs

### 01. Await destructive confirmations

**Locations:** [WorkspacePage.tsx:458](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspacePage.tsx:458), [WorkspaceExchangePanel.tsx:234](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspaceExchangePanel.tsx:234), [WorkspaceReleasePanel.tsx:79](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspaceReleasePanel.tsx:79). The dialog plugin is installed in [lib.rs:74](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/lib.rs:74) and locked to 2.7.2 in [Cargo.lock:4312](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/Cargo.lock:4312).

The plugin replaces window.confirm with an asynchronous function returning a Promise. The new code checks its return value synchronously. A Promise is truthy, so the cancellation branch is never taken. Delete conversation and Empty trash proceed while the dialog is still open; clicking Cancel cannot undo the action. The archive confirmation also fails to gate the next step. Existing tests return synchronous booleans and conceal the defect.

**Reproduction:** a component test supplied the native asynchronous confirmation shape, left its Promise unresolved, and observed deleteSession execute. Resolving the confirmation to false still left one deletion call.

**Focused fix:** use await confirmDialog from the existing [DialogService.tsx:24](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/DialogService.tsx:24) for all three actions. Keep the operation disabled while confirmation is pending. Do not introduce another dialog system.

**Regression:** unresolved and cancelled confirmation must produce zero mutations; affirmative confirmation must produce exactly one. Cover both conversation deletion and emptying trash.

### 02. Retain ownership after an ambiguous turn/start

**Locations:** [commands.rs:1074](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:1074), especially the cleanup at [commands.rs:1256](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:1256); [supervisor.rs:530](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/codex/supervisor.rs:530).

The Workspace semaphore is released whenever the send operation returns an error. That includes a lost turn/start acknowledgement and storage errors after the native turn has already started. The supervisor records the submission only after the RPC response. A timeout does not stop the server-side turn, and this error path neither interrupts nor reconciles it before clearing active state.

**Trigger and impact:** the server accepts a turn but its response is delayed past the request deadline, or attaching its snapshots fails. A second conversation can then acquire the permit while the first native turn still runs. The first submission has no dependable Stop/unknown-outcome path, and resubmitting its restored draft may duplicate work or usage. Preserving draft text alone does not enforce the one-active-turn invariant.

**Focused fix:** persist a submission state before the RPC, retain an owner through uncertain completion, and distinguish rejection before submission from uncertainty after submission. Reconcile or interrupt the exact known thread/turn; when termination cannot be established, shut down the Workspace connection before admitting another turn. Reuse the small cleanup-owner pattern already present in [Workflow invocation.rs:39](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/pipeline/codex_server/invocation.rs:39), without sharing product runtime state.

**Regression:** simulate lost acknowledgement and a post-acknowledgement database failure. A second submission must remain blocked until cleanup establishes termination, and the original input must not be replayed automatically.

### 03. Revalidate registered folder identity before granting native access

**Locations:** [store.rs:1231](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/store.rs:1231), [commands.rs:1127](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:1127), [supervisor.rs:775](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/codex/supervisor.rs:775), [codex/process.rs:58](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/codex/process.rs:58).

Registration validates the canonical folder and records its identity, but runtime_root later checks only whether the stored path is a directory. It neither rechecks the storage/home exclusion nor compares the current folder identity with the saved identity. Native validation then compares the requested and returned paths with each other, not with the originally registered folder.

**Reproduction:** register a disposable folder, replace it with a symlink to the disposable Workspace store, and call runtime_root. It returns the substituted path, which canonicalizes to private storage. No model turn was needed to demonstrate this input-boundary failure.

**Impact:** inspect/edit permission profiles are built from this root. A moved or substituted path can grant access to a different tree, including the database, credentials, or another project.

**Focused fix:** reuse canonical_workspace_root and the existing root-identity check at every native start/resume boundary. Reject replacements until the user explicitly registers the new folder. Validate before preparing context or establishing the native binding.

**Regression:** missing roots, symlinks into private storage, and a different directory placed at the same path must fail closed; an explicitly re-registered ordinary folder must work.

### 04. Clean up descendants even after the app-server leader has exited

**Locations:** [shared process.rs:76](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/agent_runtime/codex/process.rs:76), [shared process.rs:99](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/agent_runtime/codex/process.rs:99), [session.rs:90](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/agent_runtime/codex/session.rs:90), [supervisor.rs:589](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/codex/supervisor.rs:589).

OwnedProcess.wait marks the owner no longer running and unregisters it as soon as the leader exits. Both terminate and Drop then skip process-group cleanup. A leader's exit does not imply that its descendants exited.

**Reproduction:** a disposable shell spawned a sleeping child and exited. After wait, terminate, and dropping OwnedProcess, the child was still alive. The test explicitly killed its own process group afterward.

**Impact:** a crashed or prematurely exiting Workspace App Server can leave native commands running after Pipeline believes the connection has stopped. This shared primitive also serves the Workflow App Server backend.

**Focused fix:** separate reaping the leader from finishing process-tree ownership. Perform the existing group/job cleanup before unregistering the owner, including after a successful leader wait. Preserve separation from ordinary Review cancellation.

**Regression:** test a live leader with descendants and an already exited leader with descendants; test the shared primitive and retain the existing cross-mode cancellation test. This finding was reproduced on macOS, not qualified on every platform.

### 05. Preserve the harness fingerprint when rebinding the same native thread

**Locations:** [store.rs:1334](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/store.rs:1334), [supervisor.rs:459](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/codex/supervisor.rs:459), [commands.rs:1298](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:1298), [commands.rs:1154](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:1154).

On a new runtime epoch, resume_thread retires the old binding and creates another one. The new row has a null harness_fingerprint. The reconciliation command does not restore it. The next send compares that null against the current fingerprint and starts a successor thread, although the native conversation was successfully resumed.

**Reproduction:** bind a session, save its fingerprint, then bind the same provider thread under a new runtime namespace. The replacement binding has no fingerprint.

**Impact:** after reconnect and transcript refresh, a normal follow-up can lose the native conversation's prior context. The local transcript still looks continuous, making the loss difficult to recognize.

**Focused fix:** carry the verified fingerprint and instruction-source metadata into a replacement binding for the same thread, or set them immediately after successful resume. Do not copy them when intentionally creating a different native thread.

**Regression:** reconnect, reconcile, then send a follow-up; the existing provider thread must receive it unless its actual configuration changed.

### 06. Include the runtime folder in native thread compatibility

**Locations:** [research.rs:907](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/research.rs:907), [commands.rs:1154](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:1154), [supervisor.rs:459](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/codex/supervisor.rs:459), [supervisor.rs:853](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/codex/supervisor.rs:853).

Thread reuse depends only on the harness fingerprint. For a plain conversation in a project, attaching or relocating its folder changes runtime_root without changing that fingerprint. The resume request supplies new roots but no new cwd. Validation then rejects the existing thread's old cwd because it is outside the newly requested roots.

**Reproduction:** create a project without a folder, resolve its plain harness, attach a folder, and resolve again. The two fingerprints are identical while the runtime directories differ.

**Impact:** a supported project action can make an existing conversation unable to send or refresh native history. Changing an unrelated harness option happens to force a new thread, but users should not need that workaround.

**Focused fix:** include the canonical runtime root/identity in the existing compatibility decision. When it changes, deliberately start a successor with the new directory and the existing bounded handoff behavior. Keep this distinct from reconnecting an unchanged thread in issue 05.

**Regression:** attach, relocate, and detach a project folder after a conversation has a binding. Each next turn must use the requested folder and avoid repeatedly attempting an incompatible resume.

### 07. Let authoritative history repair sparse or missed event projections

**Locations:** [store.rs:1516](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/store.rs:1516), particularly [store.rs:1575](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/store.rs:1575); [supervisor.rs:981](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/codex/supervisor.rs:981).

reconcile_snapshot turns thread/read history back into ordinary turn-completed events. Their operation IDs are the same epoch/turn/completed IDs already recorded from notifications. project_codex_event returns immediately when that ID exists, before applying the fuller items from the authoritative snapshot.

**Reproduction:** project a completed turn with no items, then reconcile the same completed turn with an assistant answer. The store still has no answer. This is the recovery case needed when an item event is missed or the first terminal payload contains less history.

**Focused fix:** keep notification replay idempotent, but give authoritative reconciliation an explicit upsert path. Reapplying final items must fill missing content without creating duplicate messages or regressing a terminal state. Do not solve this with a random event ID on every refresh.

**Regression:** sparse completion followed by fuller history must recover the answer; repeating that history must leave exactly one copy. Include an in-progress snapshot with already completed items as well.

### 08. Scope transient chat state to its conversation and submission

**Locations:** [WorkspacePage.tsx:278](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspacePage.tsx:278), [WorkspacePage.tsx:375](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspacePage.tsx:375), [WorkspacePage.tsx:509](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspacePage.tsx:509).

Switching conversations does not reset or key pendingUser, stream, or the buffered deltas. The event matcher accepts every thread while submitting, and every thread when the selected conversation has no binding. Hydration changes active state to match the newly selected conversation, leaving the old transient content behind.

**Reproduction:** send in conversation A, receive part of its answer, select an unbound conversation B, and receive another A delta. B displays A's prompt and combined answer. A submission queued behind another conversation is also vulnerable to that other turn's completion being treated as its own.

**Focused fix:** associate pending input, streaming content, and completion handling with a session/submission identity. Require an exact thread match once known; buffer pre-acknowledgement events against the submitted conversation. Clear the displayed transient state when selecting another conversation and ignore late promise completions for an obsolete selection. A small keyed state object is sufficient.

**Regression:** switch between two bound conversations and from a running conversation to an unbound one; queue B behind A; return to A. No text, completion, restored draft, or Stop target may cross conversations.

### 09. Carry the epoch in approval responses

**Locations:** [workbenchTypes.ts:203](/Users/Mike/Documents/GitHub/pipeline/gui/src/lib/workbenchTypes.ts:203), [WorkspacePage.tsx:546](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspacePage.tsx:546), [commands.rs:155](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:155), [commands.rs:1349](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:1349).

Events carry an epoch, but ResolveServerRequest does not. The backend looks up the current pending request by request ID and compares that request's epoch with the current supervisor. It cannot compare the user's response with the epoch of the card they actually saw.

**Trigger and impact:** a response from an old connection arrives after reconnect, and the new server has reused the same request ID and method. Validation succeeds against the new pending request. An old Allow response can therefore authorize a different action.

**Focused fix:** send event.epoch in the typed response DTO and compare submitted epoch, stored epoch, method, and request ID before responding. Key frontend pending cards by epoch and request ID. Also keep a card available for retry when response delivery fails; currently resolve removes it before learning whether the response was accepted.

**Regression:** a delayed response from epoch N must not resolve a request with the same ID/method in epoch N+1. Failed delivery must leave an actionable pending card.

### 10. Protect native conversation directories during storage cleanup

**Locations:** [store.rs:1252](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/store.rs:1252), [retention.rs:152](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/release/retention.rs:152), [retention.rs:288](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/release/retention.rs:288), [commands.rs:878](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:878).

Unfiled conversations and projects without a registered folder use jobs/conversations/<session> as their native runtime directory. The retention scan classifies every immediate child of jobs as disposable job_scratch. Its guard checks research_executions only, and the command takes no Workspace turn guard.

**Reproduction:** create a normal conversation runtime directory with a working file and apply job_scratch pruning. The directory is moved away. Command inspection establishes that an active native turn would not prevent this operation; the fixture did not run a model.

**Impact:** cleanup can remove the cwd and files of a running native conversation. It also treats all conversation directories as one jobs/conversations cleanup entry.

**Focused fix:** exclude live conversation runtime directories from disposable execution scratch, and use the existing turn guard for destructive retention operations. Coordinate pruning with writers through the existing database gate so a newly adopted reference cannot race the reference check. Keep explicit cleanup of genuinely disposable finished-job material.

**Regression:** pruning while a native turn is active must be rejected or exclude its resources; another conversation's working files must survive pruning finished execution scratch.

### 11. Replace unsupported native text prompts

**Locations:** [WorkspacePage.tsx:442](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspacePage.tsx:442), [WorkspaceResearchPanel.tsx:135](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspaceResearchPanel.tsx:135), [WorkspaceRecipesPanel.tsx:165](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspaceRecipesPanel.tsx:165), [WorkspaceRecipesPanel.tsx:257](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspaceRecipesPanel.tsx:257), [WorkspaceReleasePanel.tsx:85](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspaceReleasePanel.tsx:85).

These actions depend on window.prompt. The installed Wry macOS WKUIDelegate has no JavaScript text-input dialog handler, and Tauri's dialog plugin supplies alert/confirm shims but no prompt shim. The repository's earlier native qualification already records this failure for project creation; the same API remains in rename, inspector imports, recipe cloning/completion, and archive root mapping. Browser-mocked tests do not establish native support.

**Focused fix:** use small controlled React forms, following WorkspaceProjectDialog. For imports, a prefilled filename-based title is enough. Give archive root mappings a row per root with a folder picker and a detach choice. Keep unresolved recipe issues as an editable field instead of silently converting a failed/cancelled prompt into an empty list.

**Regression:** verify these forms in component tests and exercise rename, one import, recipe cloning, and root mapping in the Tauri development app. No generic form framework is needed.

### 12. Make archive restore available before creating a conversation

**Locations:** [WorkspacePage.tsx:668](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspacePage.tsx:668), [WorkspaceResearchPanel.tsx:166](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspaceResearchPanel.tsx:166), [WorkspaceReleasePanel.tsx:120](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspaceReleasePanel.tsx:120), [archive.rs:585](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/release/archive.rs:585).

The restore button is inside the conversation research inspector, which requires a conversation snapshot. import_archive rejects a destination containing any session or workspace. Thus a fresh store cannot expose the restore button, and creating the conversation needed to reach it makes restore fail. Correcting the prompt and confirmation bugs does not resolve this contradiction.

**Focused fix:** expose whole-store export/restore in Workspace connection/settings or the empty Workspace surface, independent of a conversation. Retain the backend's empty-destination rule; do not add overwrite/merge restore as a workaround.

**Regression:** launch with an empty disposable store and restore a valid archive through the UI without first creating any app-owned records. Verify the imported projects and conversations appear afterward.

## P2 repairs

### 13. Preserve provider item ordering when rebuilding transcripts

**Locations:** [store.rs:1190](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/store.rs:1190), [store.rs:1583](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/store.rs:1583), [store.rs:2050](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/store.rs:2050), [archive.rs:274](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/release/archive.rs:274).

Items restored from one terminal turn all receive the same creation timestamp. Their local IDs are random, yet hydration and archive transcript export sort by creation time and ID. A rebuilt turn can show the assistant's reply before the user's question or reorder commentary and final answers.

**Reproduction:** reconcile 32 ordered provider items. The hydrated order differed immediately; one run began message-23, message-21, message-20, message-14 instead of message-00 through message-03.

**Focused fix:** persist a stable turn/item position from provider history and preserve it during upserts. Add one migration for ordering metadata and use it consistently in display/export. Do not rely on UUID lexical ordering or fabricated timestamps.

**Regression:** a multi-item turn must retain its exact order after reconnect, repeated reconciliation, application restart, and Markdown/archive export.

### 14. Flush drafts when leaving the Workspace route

**Locations:** [WorkspacePage.tsx:262](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspacePage.tsx:262), [WorkspacePage.tsx:375](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspacePage.tsx:375), [App.tsx:729](/Users/Mike/Documents/GitHub/pipeline/gui/src/App.tsx:729), [App.tsx:949](/Users/Mike/Documents/GitHub/pipeline/gui/src/App.tsx:949).

The draft effect cancels its 350 ms save timer on unmount. Internal conversation switches explicitly save, but switching to Reviews or connection settings unmounts WorkspacePage without calling that save path. App-level navigation protection covers workflow/settings drafts only.

**Reproduction:** type a draft and immediately unmount the route. No updateSession call occurs, and the stored draft remains empty.

**Focused fix:** register an awaited draft flush with the existing page-navigation/close path, preserving the serialized save queue. Keep the last text in a ref so the flush uses the latest edit. A local fallback may cover abrupt shutdown, but is not a substitute for awaiting an ordinary navigation save.

**Regression:** type and immediately navigate to Reviews and Settings, then return; the complete draft must survive. Cover an in-flight autosave and a failed save as well.

### 15. Retain approvals for conversations that are not selected

**Locations:** [WorkspacePage.tsx:212](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspacePage.tsx:212), [WorkspacePage.tsx:314](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspacePage.tsx:314), [WorkspacePage.tsx:391](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspacePage.tsx:391), [WorkspacePage.tsx:577](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspacePage.tsx:577).

The event listener discards a server request when it belongs to another bound conversation. The pending-request registry is loaded when the surface mounts/changes, but not when sessionId changes. Returning to the waiting conversation therefore does not recover its approval card.

**Reproduction:** open B, emit a command approval for A, then select A. The card is absent, and pendingRequests was called only at initial mount. The backend can still be waiting for the user's response.

**Focused fix:** retain all epoch-scoped pending requests and filter only their presentation, or refresh the authoritative pending registry on conversation selection. Keep a visible route to the conversation needing attention. Combine the request keys with issue 09's epoch fix.

**Regression:** an approval arriving for background conversation A must appear when A is selected, resolve once, and clear its attention indicator correctly.

### 16. Page long histories instead of rejecting the conversation

**Locations:** [store.rs:1132](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/store.rs:1132), [WorkspacePage.tsx:552](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspacePage.tsx:552), [commands.rs:926](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:926).

The frontend's 200-message window operates on an already hydrated full snapshot. The backend returns an error above 500 turns or 2,000 transcript items instead of returning a page. Tool-heavy conversations can reach the item cap much sooner than 500 exchanges. Draft persistence and individual Markdown export also call conversation_snapshot, so the cap affects more than rendering.

**Reproduction:** seed 501 completed turns into a disposable store. conversation_snapshot fails with “Conversation turn history exceeds its hydration limit.”

**Focused fix:** return a bounded latest page with a cursor and load older items explicitly. Fetch session metadata for draft saves without hydrating transcript items. Export by bounded database iteration. Keep size limits; do not simply raise or remove the caps.

**Regression:** histories beyond both thresholds must open at their latest messages, page backward, save drafts, and export. A tool-heavy history should be part of this test.

### 17. Recover active-turn state when the event bridge lags

**Locations:** [transport.rs:18](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/agent_runtime/codex/transport.rs:18), [commands.rs:52](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:52), [commands.rs:1398](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:1398), [commands.rs:1539](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:1539).

The broadcast queue holds 256 events. On lag, the event bridge only emits workbench:event-lagged; no frontend code listens to that event. If the discarded portion included the matching turn completion, the bridge never calls complete_active_turn and never releases the permit. The separate transcript projector may repair database history, but does not repair this process-global lifecycle state. Lost server requests similarly cannot be recovered by repainting the transcript.

**Focused fix:** on bridge lag, reconcile the exact active thread/turn and terminal state. Recover pending requests when supported; otherwise fail the connection explicitly rather than leaving an invisible wait. Use the same cleanup owner as issue 02. Merely increasing the queue is insufficient.

**Regression:** force a lag that includes a terminal notification and verify a subsequent turn becomes available after reconciliation. Also test a lost approval request. This is a deterministic simulator test, not a reason to run a long paid conversation.

### 18. Commit the trash journal before moving files

**Locations:** [retention.rs:341](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/release/retention.rs:341), [retention.rs:362](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/release/retention.rs:362), [retention.rs:398](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/release/retention.rs:398).

Pruning inserts a trash row in an uncommitted SQLite transaction, renames the file/directory, then commits. A process death or commit failure between rename and commit leaves the original path gone and no durable row identifying the moved data. list_trash and empty_trash consult only committed rows, so the supposedly recoverable object is stranded. Restore has the corresponding rename-before-state-update gap.

**Focused fix:** commit a small “moving” journal record first, perform the rename, then mark it “trashed.” Reconcile incomplete moves by inspecting source/destination existence at startup or when opening storage controls. Apply the same state transition discipline to restore. Extend the existing journal rather than building a new transaction system.

**Regression:** inject termination immediately before and after each rename and database commit. On reopening, every object must have an intelligible retained/restorable/deleted state; no original may disappear without a recoverable journal entry.

### 19. Apply the file-mutation turn guard to theory promotion

**Locations:** [commands.rs:1845](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/commands.rs:1845), [studio/theory.rs:462](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/project/studio/theory.rs:462), [studio/theory.rs:582](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/src/workbench/project/studio/theory.rs:582).

The studio command acquires the global turn guard for SaveText and GenerateValues, but omits PromoteTheory. Promotion writes into the same task directory through stage_text and captures the changed files. Its project lock does not exclude native Codex file operations, which do not hold that lock.

**Trigger and impact:** promote a derivation into a task copy while its conversation is editing that copy. The UI mutation and native tools can interleave, and the captured review can include unfinished native edits. Equivalent editor saves are already blocked during the turn.

**Focused fix:** add PromoteTheory to the existing guarded file-mutating action list. No new lock manager is necessary.

**Regression:** promotion must be refused while a Workspace turn holds the permit and must succeed after the turn finishes. Retain the existing promotion/assumptions tests.

## Implementation sequence

1. **Remove immediate data-loss and access hazards:** 01, 02, 03, 04, 09, 10. Keep native ownership and Review cancellation separate.
2. **Repair conversation continuity together:** 05, 06, 07, 08, 13, 15, 17. These changes should share a focused reconnect/navigation fixture, while keeping their regression assertions distinct.
3. **Make the ordinary desktop paths usable:** 11, 12, 14, 16. Reuse existing React dialogs and bounded storage helpers.
4. **Finish recovery and the missed mutation guard:** 18, 19.

For each item, implement the smallest correction and add the stated regression. Add migrations only where persisted ordering or journal state requires them; do not edit already applied migrations. Do not combine this work with renaming storage namespaces, new providers, more concurrency, or broader Research Studio features.

## Validation performed and remaining release work

The baseline working tree passed:

- Frontend suite: **515 tests across 63 files**.
- Rust all-target suite: **784 library tests and 14 CLI tests**; **7 tests ignored**. Probe targets contain no ordinary tests.
- Frontend production build: **passed**.

Twelve additional temporary reproduction tests demonstrated current defects: four component tests, seven Workspace store tests, and one shared process test. Their success means the assertions reproduced the bugs, not that the bugs were fixed. Temporary additions were removed, and the original source files were restored byte-for-byte. Fixtures used disposable stores and an explicitly cleaned-up child process. No user research store, account, or native model turn was used.

Audit evidence is retained locally under [/tmp/sep7-astra-evidence](/tmp/sep7-astra-evidence), including the test sources, restoration runners, and result logs. Baseline logs are [/tmp/sep7-astra-frontend-tests.log](/tmp/sep7-astra-frontend-tests.log), [/tmp/sep7-astra-rust-tests.log](/tmp/sep7-astra-rust-tests.log), and [/tmp/sep7-astra-build.log](/tmp/sep7-astra-build.log). These temporary paths are audit aids, not release dependencies; commit corrected regression tests alongside their eventual fixes.

After repair, run the ordinary frontend/Rust suites and production build. Shared Codex process/transport changes require both Workspace and Workflow regression coverage. Then exercise the corrected flows in the **Tauri development application**, especially asynchronous confirmation, restore into an empty store, navigation during a turn, reconnect, and draft preservation.

Authenticated sign-in/model/tool behavior, packaged abrupt-death recovery, and Windows/Linux qualification were **not** performed in this audit. They remain the explicit gates in [Workspace release qualification](/Users/Mike/Documents/GitHub/pipeline/docs/workbench/release-qualification.md) and [Workflow Codex qualification](/Users/Mike/Documents/GitHub/pipeline/docs/workflow-codex.md). Include a simultaneous Workspace/Review run to verify independent Stop and account behavior. Also verify the native tools and web-search availability actually match the Workspace UI's advertised restrictions; the audit does not treat unobserved runtime defaults as a confirmed defect.

Already documented feature boundaries—one active Workspace turn, optional Review handoffs, disabled Windows file acceptance, and the opt-in Workflow App Server backend—are not bugs by themselves. The work above repairs promised behavior without expanding those boundaries.
