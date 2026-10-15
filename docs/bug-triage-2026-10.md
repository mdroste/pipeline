# Pre-release bug triage — October 2026

> Working triage for the next release after 1.0.2, audited on 2026-10-14 at
> commit `f4fff11` on `main`. Line numbers are snapshots of that commit.
> Entries are hypotheses with evidence, not shipped fixes; each one should get
> the listed regression test before its fix is merged. See
> [../CHANGELOG.md](../CHANGELOG.md) for what has actually changed since.

## How this was produced

An agent-assisted review scanned 21 backend subsystems (workflow executor,
provider adapters, Codex runtime, extraction, run store, configuration,
settings, every Workspace owner, the Tasks coordinator, and the CLI) and
produced 137 candidate defects. The 40 strongest candidates were re-examined by
five verifier agents instructed to refute each claim by tracing callers,
guards, tests, and docs; all 40 survived, usually with corrected locations,
titles, or severities. Two further agents swept areas the subsystem scans did
not reach: the frontend controller hooks and a cross-cutting Rust
security/panic/Windows/packaging pass. Thirteen headline items were then
re-read by hand; every one matched the verifier's trace.

Because every verified candidate was confirmed, treat "Confirmed by a verifier
agent" as a strong, evidenced hypothesis rather than proof. Items marked
"re-read by hand" were independently traced a second time. Items marked
"single pass" come from one sweep agent only.

## Baseline

All static release gates pass at `f4fff11`: `tsc`, `vitest` (819 tests),
`cargo test` (997 tests), `cargo clippy -D warnings`, `cargo fmt --check`,
Prettier, the source-size budget, the markdown link check, and the release
script tests. Versions agree at 1.0.2. None of the defects below is covered by
an existing test.

## Recommended before release

These thirteen are the ones worth holding the release for: user-visible data
loss, security boundaries that do not hold, or main flows that fail under
ordinary conditions.

1. **[useWorkspacePageController.ts:878](../gui/src/hooks/useWorkspacePageController.ts:878)** — A send that fails after dispatch empties the composer and then the autosave overwrites the server draft with an empty string, so the typed message is lost. (finding 1)
2. **[guard.rs:37](../gui/src-tauri/src/commands/guard.rs:37)** — With a custom data folder, the run guard and run recovery lock different files, so History refresh can rewrite the active run as interrupted and allow deleting it. (finding 2)
3. **[reuse_artifacts.rs:171](../gui/src-tauri/src/commands/reuse_artifacts.rs:171)** — Source snapshots over 50 MB (or more than 500 files) are deleted after they are fingerprinted, so every rerun or resume of such runs fails. (finding 3)
4. **[project.rs:189](../gui/src-tauri/src/workbench/project.rs:189)** — Deterministic `response_` and `zotero_` record ids collide across projects; the upsert overwrites the other project's record. (finding 4)
5. **[archive.rs:733](../gui/src-tauri/src/workbench/release/archive.rs:733)** — Archive import copies entries without bounding them by the declared size, so a crafted `.pwrx`/`.pwex` can fill the disk and crash manifest parsing. (finding 5)
6. **[archive.rs:1009](../gui/src-tauri/src/workbench/release/archive.rs:1009)** — Restore keeps blob references of the form `<blobs>/../..`, and two readers later read them, so a crafted archive can read files outside the store. (finding 6)
7. **[codex.rs:548](../gui/src-tauri/src/pipeline/codex.rs:548)** — Forked Codex `exec resume` calls carry no sandbox argument, so the user's global Codex config decides the sandbox for the review call. (finding 7)
8. **[runtime.rs:231](../gui/src-tauri/src/workbench/store/runtime.rs:231)** — Re-projecting a thread under a new binding incarnation gives every item the same timestamp with random ids, so transcript order is scrambled durably. (finding 8)
9. **[execution.rs:1477](../gui/src-tauri/src/workbench/research/execution.rs:1477)** — A symlinked `~/.zshrc` (dotfile managers) fails the launch manifest, so every `/bin/zsh` profile, including Stata, cannot preview, authorize, or run. (finding 9)
10. **[history.rs:755](../gui/src-tauri/src/runs/history.rs:755)** — Trash restore compares a canonicalized path with a non-canonical Trash directory, so Restore always fails on Windows (and on symlinked data folders). (finding 10)
11. **[useArtifactSelection.ts:186](../gui/src/components/artifact-explorer/useArtifactSelection.ts:186)** — The Sources selection request is re-applied after every navigation, so after opening a citation the user is bounced back and cannot browse Sources. (finding 11)
12. **[invocation.rs:471](../gui/src-tauri/src/pipeline/codex_server/invocation.rs:471)** — Under parallel load the shared event channel lags, and the lagged call fails non-retryably and interrupts its own turn. (finding 12)
13. **[tasks.rs:394](../gui/src-tauri/src/workbench/project/tasks.rs:394)** — Checkpoint and editor saves accept paths that capture ignores, so accepting such an edit unlinks the original file. (finding 13)

Suggested order: the composer draft wipe (1) and the run-lock split (2) first,
since both lose user work in main flows; then the two archive-import holes (5,
6) and the Codex resume sandbox (7) together, since they share owners; then the
rest.

## Findings

### Fix before release

### 1. Failed send wipes the user's message from composer, localStorage, and the server draft

- **Severity:** critical · **Category:** data-loss · **Location:** [useWorkspacePageController.ts:878](../gui/src/hooks/useWorkspacePageController.ts:878)
- **Status:** Found by a sweep agent and re-read by hand

**What happens.** User types a long question and presses Send while a project research job holds the write resources (or while the Codex runtime is disconnected). sendTurn rejects. UI shows "The submission may not have been acknowledged. It was preserved as a draft..." but the composer is empty; within ~350 ms the autosave effect PUTs draft "" to the server. Switching conversations and back shows an empty draft. The message text is unrecoverable.

<details><summary>Evidence and trace</summary>

In send(), `dispatched` is set to true before `workbenchClient.sendTurn` is awaited, so ANY rejection of sendTurn (Codex not connected, sign-in required, the explicit "project has a local job holding its write resources" conflict, turn-permit busy, storage conflict) takes the "may not have been acknowledged" branch. That branch reports "It was preserved as a draft" but never restores `draft`: the composer was emptied with setDraft("") and the localStorage pendingDraft key was removed at the top of send(). It then awaits hydrate(sessionId); because the snapshot is for the same session, `initialize` is false and the server draft (which the backend wrote via its send-draft-* operation before dispatch) is not loaded into the composer. The draft autosave effect (lines 435-469) now sees snapshot.session.draft === text but draft === "", so 350 ms later it calls persistDraft(session, "") and overwrites the server draft with an empty string. The message is gone from every store.

Guards checked: Checked that `dispatched = true` precedes the await (line 821), so no rejection takes the !dispatched restore path; hydrate's `initialize` is false for the same session so setDraft is skipped (line 310); the autosave effect's early-return conditions (pendingUser/active/submitting/contextBusy) are all false after the catch block; persistDraft/saveWorkspaceDraft has no non-empty-draft guard; localStorage key was removed at line 813. Confirmed end-to-end with a renderHook probe.

</details>

**Proposed fix.**

```text
In the dispatched-failure branch, restore the text before/after hydrate and bump the edit revision so the autosave effect cannot overwrite the server draft:

       submissionRef.current = null;
       if (sessionRef.current === sessionId) setPendingUser(null);
-      await hydrate(sessionId);
+      await hydrate(sessionId);
+      // The host wrote the text as the session draft before dispatch. Put it
+      // back in the composer; otherwise the autosave effect persists "" and
+      // destroys the preserved draft.
+      if (sessionRef.current === sessionId) {
+        draftEditRevision.current += 1;
+        setDraft(snapshotRef.current?.session.draft || text);
+        try {
+          localStorage.setItem(`pipeline.pendingDraft.${sessionId}`, text);
+        } catch {
+          /* Server draft remains authoritative. */
+        }
+      }

Alternatively (belt and braces) make the autosave effect refuse to persist an empty draft over a non-empty server draft unless the user explicitly edited (`draftEditRevision` advanced since hydrate).
```

**Regression test.** In hooks/useWorkspacePageController.test.ts add: mock sendTurn to (a) set snaps.A.session.draft = r.text and (b) reject; editDraft("my long question"); await send(); advance 600 ms; expect result.current.draft === "my long question", expect updateSession never called with draft "", expect snaps.A.session.draft still equals the text.


### 2. Pipeline guard locks ~/.pipeline/run.lock while recovery locks <data_root>/run.lock, so a custom data folder lets History rewrite and delete the active run

- **Severity:** high · **Category:** data-loss · **Location:** [guard.rs:37](../gui/src-tauri/src/commands/guard.rs:37) (first reported at `gui/src-tauri/src/runs/history.rs:337`)
- **Status:** Confirmed by a verifier agent and re-read by hand

**What happens.** Custom data folder configured; a Review is in its model-execution phase; the user opens History (or any project page: useProjectSignals.ts:162 also invokes list_runs). recover_resumable_runs acquires {data}/run.lock, flips the active run to "interrupted" with a reconstructed partial report, and History shows it as interrupted/resumable with Delete and Export enabled (delete_run only refuses status "running"). Clicking Delete renames the directory into Trash while the executor is still writing: subsequent checkpoint/artifact writes fail, the run fails, Drop cannot persist the manifest (directory gone), and the hours of model work land in Trash as a half-run. The same happens when the GUI user merely clicks Run while the CLI is executing a workflow (recovery runs before acquire_pipeline_guard rejects the launch).

<details><summary>Evidence and trace</summary>

acquire_pipeline_guard (commands/guard.rs:28-37) locks dirs::home_dir()/.pipeline/run.lock, i.e. storage::local_root() (storage.rs:34-38). recover_resumable_runs (runs/history.rs:328-337) locks runs_dir().parent()/run.lock where runs_dir() = storage::data_root()/runs (runs.rs:463-464) and data_root() is the configured folder (storage.rs:64-67 Storage::new reads config.directory; docs/storage.md:3-6 documents the Settings → Data & Storage selection). With a custom folder these are different files, so `lock.try_lock_exclusive()` at history.rs:338 succeeds while a Review runs. list_runs (commands/artifacts.rs:84) and run_pipeline (run_entry.rs:113) call recover_resumable_runs with no in-process check (PIPELINE_RUNNING is never consulted). recover_resumable_run_dir (history.rs:55-92) accepts status "running", finds no report.json, requires only context/document.md (captured_document_rel_path, history.rs:27-35), builds a partial report with no early return when checkpoints are empty, writes report.json/report.md (history.rs:233-243) and the manifest with status "interrupted" (history.rs:304-317). The Tauri delete_run command (commands/artifacts.rs:102-105) takes no pipeline guard and runs::delete_run refuses only `.task-pin` and status == "running" (history.rs:666-672), so after the flip the user can rename the live run directory into Trash (history.rs:691) while the executor still writes to it. useProjectSignals.ts:162 invokes list_runs on every project page load, so the trigger is routine. Candidate 7 is the same defect.

</details>

**Proposed fix.**

```text
Introduce one owner for the lock path and use it in all three places; additionally skip recovery while this process is running a pipeline.

// runs.rs (or runs/history.rs)
+pub fn run_lock_path() -> Result<PathBuf, String> {
+    Ok(crate::storage::data_root()?.join("run.lock"))
+}

// commands/guard.rs acquire_pipeline_guard
-        let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
-        let dir = home.join(".pipeline");
-        std::fs::create_dir_all(&dir)...;
-        ...open(dir.join("run.lock"))
+        let path = crate::runs::run_lock_path()?;
+        ...open(&path)

// runs/history.rs recover_resumable_runs and commands/orchestration/pins.rs release_abandoned_pin
-        .open(pipeline_dir.join("run.lock"))
+        .open(crate::runs::run_lock_path()?)

// runs/history.rs recover_resumable_runs, before taking the lock
+    if crate::commands::pipeline_is_running() { return Ok(0); }  // thin accessor over PIPELINE_RUNNING

Keep the recovery-at-startup call in lib.rs:87 unchanged.
```

**Fix risk.** Low. The default folder resolves to the same ~/.pipeline/run.lock, so behavior is unchanged for default storage. Review that the Workspace boundary is untouched (lock stays Review-owned).

**Regression test.** In runs/tests.rs (or commands tests with the storage root pointed at a tempdir): create a RunWriter with status "running" and context/document.md, hold the pipeline guard via acquire_pipeline_guard(), call recover_resumable_runs() and assert Ok(0) and that manifest.status is still "running" and no report.json exists. Fails today when data_root != ~/.pipeline.


### 3. register_unlisted silently deletes source-snapshot files (>50 MB or beyond 500) after they are fingerprinted, so rerun/resume of large inputs always fails

- **Severity:** high · **Category:** data-loss · **Location:** [reuse_artifacts.rs:171](../gui/src-tauri/src/commands/reuse_artifacts.rs:171)
- **Status:** Confirmed by a verifier agent and re-read by hand

**What happens.** (a) Fresh run on a 60 MB scanned PDF (allowed: MAX_STAGED_PDF_BYTES is 256 MB, and the code comments note scanned PDFs routinely exceed 32 MB). The snapshot copy is deleted immediately after fingerprinting. Later Re-run/Resume (including resuming an interrupted run) calls restore_source → copy_tree → symlink_metadata fails: the rerun aborts with "No such file or directory (os error 2)". (b) source_tree or LaTeX-project input with more than 500 files: only an arbitrary read_dir-ordered 500 survive; rerun fails with "Captured source failed its integrity check. Start a new run from the intended source revision." Forensic exports also ship a silently truncated "immutable" snapshot.

<details><summary>Evidence and trace</summary>

Path trace: commands/run.rs:456 retain_source -> reuse_artifacts.rs:146 retain_source_at -> snapshot_source (reuse_artifacts.rs:58-80) copies the staged source into {run}/source-snapshot/ via copy_tree, bounded only by MAX_RUN_OUTPUT_BYTES = 256 MiB (safety.rs:21, reuse_artifacts.rs:11) and WalkBudget (100,000 entries, safety.rs:24). The fingerprint is computed at reuse_artifacts.rs:157-161 BEFORE `writer.register_unlisted(&rel("source-snapshot"), "source")` at :171. register_unlisted (runs.rs:901) unconditionally deletes any file whose len > MAX_UNLISTED_BYTES = 50_000_000 or that pushes the total past 250_000_000 (runs.rs:955-962, `let _ = fs::remove_file(&path)`), and deletes every file after the 500th registration (runs.rs:971-976); the size check runs before the `known` check so nothing is exempt. A primary PDF between 50 MB and 256 MiB is admitted by staging (MAX_STAGED_PDF_BYTES = 256 MiB, pipeline/extract/core.rs:339) and a source_tree selection is the raw folder (latex.rs:499-512) so >500 files is trivially reachable (LaTeX closures up to 512 files, core.rs:5). restore_source (reuse_artifacts.rs:181-231) re-snapshots and verifies whenever the record names a path, regardless of `required`, and is called on every rerun/resume at rerun.rs:263-264; copy_tree's symlink_metadata on the deleted file fails with NotFound, and a 500-of-N tree fails the fingerprint with 'Captured source failed its integrity check'. No guard anywhere prevents this: the existing test at runs/tests.rs:845-855 asserts the deletion behavior for group "files", and the snapshot test at reuse_artifacts.rs:330-410 never calls register_unlisted.

</details>

**Proposed fix.**

```text
Give RunWriter a non-destructive registration for already-bounded trees and use it for the snapshot; keep register_unlisted's destructive policy for model-written output only.

// runs.rs
-    pub fn register_unlisted(&mut self, subdir: &str, group: &str) -> usize {
-        const MAX_UNLISTED: usize = 500; ...
+    pub fn register_unlisted(&mut self, subdir: &str, group: &str) -> usize {
+        self.register_tree(subdir, group, UnlistedPolicy::model_output(group))
+    }
+    /// Register a tree that copy_tree already bounded (256 MiB, no symlinks/FIFOs).
+    /// Never deletes; returns Err if any regular file could not be registered.
+    pub fn register_captured_tree(&mut self, subdir: &str, group: &str) -> Result<usize, String> {
+        self.register_tree(subdir, group, UnlistedPolicy { max_files: crate::safety::MAX_WALK_ENTRIES, max_file_bytes: crate::safety::MAX_RUN_OUTPUT_BYTES as u64, max_total_bytes: crate::safety::MAX_RUN_OUTPUT_BYTES as u64, delete_excess: false })
+    }

// reuse_artifacts.rs retain_source_at
-    writer.register_unlisted(&rel("source-snapshot"), "source");
+    writer.register_captured_tree(&rel("source-snapshot"), "source")?;
     // compute `fingerprint` here, after registration, and store the record

Move the `source_fingerprint` call below the registration so the record always describes what is on disk, and return Err instead of truncating when a limit is hit. Keep the FIFO/hard-link/symlink skips from register_unlisted in the shared walker.
```

**Fix risk.** Low–medium. The manifest gains entries for large source files (already the case for small ones). Must keep walk budgets, symlink/FIFO refusal, and deterministic ordering; do not loosen limits for the "files"/"agent_response" groups.

**Regression test.** In commands/reuse_artifacts.rs tests: create a RunWriter via RunWriter::create_in(temp, "big"), stage a 51 MB regular file (e.g. 51_000_000 zero bytes) as source_path, call retain_source, then assert the file still exists under {run}/source-snapshot/ and restore_source(run_dir, child, true) succeeds and matches the recorded fingerprint. Second test: a tree with 501 small files; assert all 501 are present and restore_source passes the integrity check. Both fail today (NotFound / integrity-check error).


### 4. put() upsert on the global id overwrites another workspace's record when deterministic response_/zotero_ ids collide

- **Severity:** high · **Category:** data-loss · **Location:** [project.rs:189](../gui/src-tauri/src/workbench/project.rs:189)
- **Status:** Confirmed by a verifier agent and re-read by hand

**What happens.** Researcher imports the same Zotero item (or the same Review findings package) into project A, edits the response decision/draft in A, then imports it into project B. In B, record() finds nothing (row belongs to A), put() runs with expected 0, the ON CONFLICT(id) branch overwrites A's row body with B's fresh initial decision / B's source id and sets revision=1; B's import then fails with 'Project object is unavailable in this Workspace'. A's researcher-edited decision is gone from the live record and A's bibliography entry now points at B's source.

<details><summary>Evidence and trace</summary>

project_records.id is the sole PRIMARY KEY (migrations/008_theory_notes.sql:4-8; 007 identical). put() (workbench/project.rs:171-190) reads the expected revision with `SELECT revision ... WHERE id=?1 AND workspace_id=?2 AND kind=?3` (line ~181) and then runs `INSERT ... ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,body_json=excluded.body_json,updated_at=excluded.updated_at` (project.rs:189) with no workspace/kind predicate. If a row with the same id exists in workspace A, a put() from workspace B sees current=None, passes the expected==0 check, and the DO UPDATE overwrites A's body_json and resets revision to 1 while workspace_id/kind stay A's; tx.commit() succeeds, then record(store, B, ...) (project.rs:147-154) fails with 'Project object is unavailable in this Workspace' after the damage is committed. Two generators produce workspace-independent ids: studio/review.rs:103-112 `response_{sha256(workflow\0run_id\0source_step_id\0finding_id)}` (Review runs are global, so importing the same run's findings into two projects is a normal flow) and studio/literature.rs:460-471 `zotero_{sha256(server\0library\0key\0version\0item_hash)}` (same Zotero item imported into two projects). In both callers the preceding record(ws,...) lookup is workspace-scoped (review.rs:114, literature.rs:472) so it finds nothing and calls put(..., 0, ...). The BEFORE UPDATE trigger project_history (008_theory_notes.sql:14-16) retains the old body in project_record_history, so the overwritten content is recoverable from history, but the live record (A's edited response decision, A's bibliography pointing at B's source id) is clobbered and B's import errors every time. No test covers cross-workspace put (project/tests.rs, home_tests.rs).

</details>

**Proposed fix.**

In put() (workbench/project.rs) look the row up by id alone and refuse a cross-scope hit, and make the DO UPDATE conditional:

```rust
let current: Option<(String, String, i64)> = tx
    .query_row("SELECT workspace_id,kind,revision FROM project_records WHERE id=?1",
        [object_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
    .optional().map_err(err)?;
if let Some((ws, k, _)) = &current {
    if ws != workspace_id || k != kind {
        return Err(WorkbenchError::conflict("This record ID belongs to another project or object kind"));
    }
}
if current.as_ref().map_or(0, |c| c.2) != expected { /* existing conflict error */ }
let changed = tx.execute("INSERT INTO project_records(...) VALUES(...) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,body_json=excluded.body_json,updated_at=excluded.updated_at WHERE project_records.workspace_id=excluded.workspace_id AND project_records.kind=excluded.kind", params![...]).map_err(err)?;
if changed != 1 { return Err(WorkbenchError::conflict("Project record belongs to another scope")); }
```

As a separate compatibility change, include the workspace id in the response_ and zotero_ hashes (review.rs:103-112, literature.rs:460-471) as spec_/bib_ already do, with a same-workspace lookup of the legacy id before creating a new record so existing imports keep deduplicating.

**Fix risk.** Low: the put() change only rejects writes that are already broken today; the id-scoping change alters dedup keys for new imports and needs the legacy-id fallback to avoid duplicate records in existing projects.

**Regression test.** Rust test in workbench/project/tests.rs: create workspaces A and B; put(A, "zotero_x", "bibliography", 0, json!({"owner":"A"})); assert put(B, "zotero_x", "bibliography", 0, json!({"owner":"B"})) returns a conflict error; assert record(A, "zotero_x", "bibliography").body["owner"] == "A" and revision == 1. Before the fix the second put errors with 'unavailable' but A's body reads "B".


### 5. Archive/exchange extraction trusts declared zip sizes: deflate bombs fill disk, and inspect_archive parses an unbounded manifest stream

- **Severity:** high · **Category:** security · **Location:** [archive.rs:733](../gui/src-tauri/src/workbench/release/archive.rs:733)
- **Status:** Confirmed by a verifier agent and re-read by hand

**What happens.** A coauthor-supplied .pwrx whose central directory declares manifest.json as 1 KiB but whose deflate stream expands to tens of GiB: the user picks it in Settings -> Data & Storage -> Restore. inspect_archive runs first (before any confirm dialog) and serde_json::from_reader keeps appending to a String until the allocator fails, aborting the whole Tauri process. With a lying database/blob entry instead, import_archive/extract() fill the system temp volume (2 GiB archive -> up to ~2 TiB) before the CRC check fails; the temp dir is only freed when the error propagates.

<details><summary>Evidence and trace</summary>

zip 8.6.0 bounds an entry's reader only by the COMPRESSED size: types.rs:267-276 `find_content` returns `reader.take(self.compressed_size)`, and read/readers.rs:265-299 `make_reader` wraps it in `Decompressor::new(.., uncompressed_size, ..)` where compression.rs:506-508 builds `flate2::bufread::DeflateDecoder::new(reader)` and ignores `uncompressed_size`. The CRC mismatch is only raised at EOF (crc32.rs:50-52). Pipeline checks only the declared sizes: archive.rs:716-719 `entry.size() > MAX_ENTRY_BYTES`, :720 `total.saturating_add(entry.size())`, then :733 `std::io::copy(&mut entry, &mut output)` with no Take; identical pattern in exchange.rs:1113-1125. The archive-file cap (`archive_size > MAX_ARCHIVE_BYTES`, archive.rs:128-134 and :660-666) limits compressed bytes to 2 GiB, so a deflate stream can still expand ~1000x into the temp dir before the CRC check fails. inspect_archive is worse: archive.rs:164-166 checks only `entry.size() > 4 MiB`, then :173 hands the unbounded decompressed stream straight to `serde_json::from_reader`; a long JSON string grows serde's scratch buffer until allocation fails (abort). inspect runs from the Restore picker before any confirm. No other guard exists (no compressed_size sanity check, no Take anywhere in release/).

</details>

**Proposed fix.**

Bound every entry read by the declared size and verify the count, in archive.rs import_archive (~733), exchange.rs extract (~1125) and inspect_archive (~173):

```rust
// import_archive / extract
-        std::io::copy(&mut entry, &mut output).map_err(...)?;
+        let declared = entry.size();
+        let copied = std::io::copy(&mut (&mut entry).take(declared + 1), &mut output).map_err(...)?;
+        if copied != declared {
+            return Err(WorkbenchError::invalid("Archive entry size does not match its header"));
+        }

// inspect_archive
-    let manifest: ArchiveManifest = serde_json::from_reader(archive.by_index(index)...?)
+    let mut raw = Vec::new();
+    archive.by_index(index)...?.take(4 * 1024 * 1024 + 1).read_to_end(&mut raw).map_err(...)?;
+    if raw.len() > 4 * 1024 * 1024 { return Err(WorkbenchError::invalid("Archive manifest exceeds 4 MiB")); }
+    let manifest: ArchiveManifest = serde_json::from_slice(&raw)...?;
```
Take(size+1) still reaches inner EOF on an honest entry, so the CRC check keeps firing.

**Fix risk.** Low: honest archives produce exactly `size` bytes so behavior is unchanged; only lying headers are rejected. Keep the Take at size+1 (not size) so the CRC EOF check still runs.

**Regression test.** Build a .pwrx in a tempdir with a `manifest.json` entry whose deflate stream expands to e.g. 64 MiB of a JSON string while the central directory declares 1 KiB (write with zip::ZipWriter, then patch the central-directory uncompressed size, or write a raw deflate stream). Assert inspect_archive returns an `invalid` error quickly and that import_archive leaves the staging dir below a few MiB; before the fix inspect allocates the full stream and import writes all 64 MiB.


### 6. .pwrx restore keeps blob references with `..` traversal, later read outside immutable storage

- **Severity:** high · **Category:** security · **Location:** [archive.rs:1009](../gui/src-tauri/src/workbench/release/archive.rs:1009)
- **Status:** Found by a sweep agent and re-read by hand

**What happens.** A researcher restores a .pwrx archive received from a collaborator (or a tampered backup). Its paper_revisions.text_reference is `/Users/<name>/.pipeline/workbench/blobs/../../../.ssh/id_rsa` (the default store location plus a guessed username; several rows can try several guesses). Restore succeeds. Opening that paper in the Workspace returns the private key contents to the UI via paper_read, and the next research turn embeds it in the model prompt via research.rs context assembly.

<details><summary>Evidence and trace</summary>

During whole-store restore, blob references stored in the archived SQLite database (artifacts.storage_reference, paper_revisions.text_reference, source_versions.text_reference) are rebased unless they already "start with" the local blob root. That check is `Path::starts_with`, which is component-wise and lexical, so an archive-controlled reference such as `<store>/blobs/../../../.ssh/id_rsa` is accepted untouched (the `..` rejection at lines 1023-1027 only runs for references under `original_blob_root`). Two readers then trust the stored reference with the same lexical check and read it: workbench/research/papers.rs:549-557 (`path.starts_with(store.root_path().join("blobs"))` then `fs::read`, returned over IPC by paper_read) and workbench/research.rs:527-533 (`fs::read_to_string` spliced into model context as "Selected paper text"). workbench/project/documents.rs:51 is safe because it requires `p.parent() == blobs`.

Guards checked: safe_archive_name/blob hash checks (archive.rs:880-896) only cover files inside the zip, not DB column values. The rebase loop's `..` rejection (1023-1027) is skipped by the `continue` at 1010. canonical_workspace_root applies to workspace roots, not blob references. papers.rs and research.rs use lexical starts_with without canonicalize and fs::read follows the `..`. documents.rs:51 uses a strict parent check and is not affected. The attacker must know the victim's absolute store path; defaults under ~/.pipeline make it guessable.

</details>

**Proposed fix.**

```text
archive.rs:
-            if Path::new(&reference).starts_with(&blob_root) {
-                continue;
-            }
+            if let Ok(rest) = Path::new(&reference).strip_prefix(&blob_root) {
+                if rest.components().all(|c| matches!(c, Component::Normal(_))) {
+                    continue;
+                }
+                return Err(WorkbenchError::invalid(
+                    "Imported blob reference contains traversal",
+                ));
+            }

papers.rs / research.rs: mirror documents.rs and require `path.parent() == Some(store.root_path().join("blobs").as_path())` (or canonicalize before starts_with) before reading.
```

**Regression test.** Add an archive regression test: build a .pwrx whose paper_revisions.text_reference is `<dest>/blobs/../outside.txt` with outside.txt present next to the store; restore into an empty store and assert restore fails (or the reference is rejected), then call revision_text/paper_read and assert an error rather than the file contents.


### 7. Codex `exec resume` calls do not pin sandbox_mode, so the user's global Codex config governs the resumed review call

- **Severity:** high · **Category:** security · **Location:** [codex.rs:548](../gui/src-tauri/src/pipeline/codex.rs:548)
- **Status:** Confirmed by a verifier agent and re-read by hand

**What happens.** User has `sandbox_mode = "danger-full-access"` in ~/.codex/config.toml (common for interactive Codex use) and enables context_cache in a profile with Codex in Subscription (CLI) mode. The warm-up and thread/fork succeed; the per-unit `codex exec resume <child> -` call then executes model-generated shell commands from an untrusted paper with no sandbox and with network access, while Pipeline logs it as a sandboxed review step. With default config, write-enabled resumed calls instead run read-only and their supporting files are never written.

<details><summary>Evidence and trace</summary>

Path: call_codex_with_shared_context forks the warm thread through a transient `codex app-server` process with an explicit sandbox (codex.rs:436-450 `"sandbox": spec.sandbox`), then calls call_codex_inner with CodexSessionMode::Resume(&branch_id) (codex.rs:280). In call_codex_inner the only sandbox argument is gated on New mode (codex.rs:548-551 `if matches!(session_mode, CodexSessionMode::New) { cmd_args.push("--sandbox") ... }`); Resume mode pushes `-c sandbox_workspace_write.*` only when needs_write (codex.rs:553-560) and never `-c sandbox_mode=...` or `-c approval_policy=...`. The installed codex-cli 0.153.4 confirms `codex exec resume --help` accepts `-c`, `--json`, `-m`, `--dangerously-bypass-approvals-and-sandbox`, `--skip-git-repo-check`, `--output-schema`, `--output-last-message` but no `--sandbox`; the presence of a resume-level bypass flag shows the sandbox is decided per invocation from config.toml + overrides, not restored from the forked rollout. So the resumed per-unit call runs with whatever `sandbox_mode` the user's ~/.codex/config.toml sets (danger-full-access or workspace-write with network for interactive users), violating the read-only/no-network contract Pipeline enforces in New mode; with the Codex default (read-only) a write-enabled resumed call cannot write its supporting files. docs/workflow-codex.md:111 documents only that the App Server backend disables resume/fork caching; nothing documents sandbox inheritance for the CLI resume path. Severity high (security contract gap contingent on user config, cheap to close).

</details>

**Proposed fix.**

In call_codex_inner, pin the sandbox in both modes; keep `--sandbox` for New (unchanged) and add a config override for Resume (accepted by `codex exec resume -c`):
```rust
if matches!(session_mode, CodexSessionMode::New) {
    cmd_args.push("--sandbox".to_string());
    cmd_args.push(sandbox.to_string());
} else {
    cmd_args.push("-c".to_string());
    cmd_args.push(format!("sandbox_mode=\"{sandbox}\""));
    cmd_args.push("-c".to_string());
    cmd_args.push("approval_policy=\"never\"".to_string());
}
if needs_write {
    cmd_args.push("-c".to_string());
    cmd_args.push("sandbox_workspace_write.network_access=false".to_string());
    ... existing exclude_slash_tmp / exclude_tmpdir_env_var ...
}
```
Alternatively place `--sandbox {sandbox}` before the `resume` token (`exec --sandbox read-only resume <id>`), since the parent `exec` parser accepts it there; the `-c` form is the one verified against the installed CLI.

**Fix risk.** Low: argv-only change in the Resume branch; `-c sandbox_mode` is a documented TOML override. Run a no-model probe (`codex exec resume <id> -c sandbox_mode="read-only" --json`) to confirm the override is accepted by the pinned CLI versions on all platforms.

**Regression test.** Add a unit test over the argv built by call_codex_inner for CodexSessionMode::Resume("abc") with allowed_tools=["Read"] and with ["Read","Write"]+write_dir, asserting `cmd_args` contains the pair ("-c", "sandbox_mode=\"read-only\"") / ("-c", "sandbox_mode=\"workspace-write\"") respectively (extract the argv construction into a pure helper if needed). Fails before (no sandbox_mode override in Resume mode), passes after.


### 8. Reconciling a thread under a new binding incarnation re-projects items with one shared created_at and random ids, scrambling transcript order

- **Severity:** high · **Category:** wrong-result · **Location:** [runtime.rs:231](../gui/src-tauri/src/workbench/store/runtime.rs:231)
- **Status:** Confirmed by a verifier agent and re-read by hand

**What happens.** User has a conversation with turns (userMessage + agentMessage each). Quit and relaunch Pipeline, open the conversation. reconcile_session -> resume_thread binds a new incarnation (namespace epoch changed) -> reconcile_thread projects each turn with items sharing one created_at. conversation_snapshot/transcript_page then return the user prompt after the assistant reply in about half the turns; the frontend renders `snapshot.items.filter(isMessage)` in that order. Each relaunch produces a different shuffle. titles::first_exchange also picks the wrong 'first' message.

<details><summary>Evidence and trace</summary>

The mechanism is real but the stated trigger ('every app restart') is wrong. supervisor.rs:751 `next_epoch: AtomicU64::new(1)` means each launch's first connect() (supervisor.rs:768) yields namespace `codex-0-147-0-epoch-1` (supervisor.rs:660), and runtime.rs:22-26 bind_session returns the EXISTING binding when namespace and thread id match, so a plain relaunch re-projects via ON CONFLICT (store.rs:953) and keeps created_at. A new incarnation is created whenever the namespace differs from the active binding's: after any supervisor shutdown within a launch — ConnectionClosed (commands/codex.rs:630-638), unacknowledged dispatch (codex.rs:323), closed event stream (codex.rs:669) — the next connect() takes epoch 2, and the FOLLOWING launch (epoch 1 again) differs once more. Under a fresh incarnation, workbench_codex_reconcile_session (commands/codex.rs:365-405, called after terminal turn events from useWorkbenchEvents.ts:138 and :283; the cited useWorkspacePageController.ts call does not exist) runs resume_thread -> bind_session_projection -> reconcile_thread -> reconcile_snapshot (supervisor.rs:1030-1077), which projects every finished turn as TurnCompleted. runtime.rs:207 takes one `timestamp = now()` per event, runtime.rs:227-241 inserts every `turn.items` entry with it, store.rs:950 `new_id("item")` is random (store.rs:298-309 getrandom), and history.rs:71 orders by `i.created_at, i.id`, so items within a turn come back in random order. Worse than claimed: turns already projected under the new incarnation keep their earlier created_at while re-projected older turns get the reconcile time, so old turns sort AFTER newer ones. Rows persist, so the corruption is durable. titles::first_exchange (titles.rs:92-117) depends on this order. No test asserts order within a batch (store/tests.rs:527 projects a single-item turn).

</details>

**Proposed fix.**

In project_codex_event's TurnCompleted arm (runtime.rs:225-241), give each re-projected item a stable ordering time: inherit created_at from the superseded incarnation's row for the same (session, provider_item_id), and for genuinely new rows derive a strictly increasing timestamp from the batch time and the item index.

```rust
 NormalizedEvent::TurnCompleted { turn_id, turn, .. } => {
     upsert_projected_turn(&transaction, binding_id, turn_id, turn, &timestamp, true)?;
     if let Some(items) = turn.get("items").and_then(Value::as_array) {
-        for item in items {
+        for (index, item) in items.iter().enumerate() {
             let item_id = required_json_string(item, "id", "completed turn item")?;
             let item_kind = required_json_string(item, "type", "completed turn item")?;
+            let item_timestamp = prior_item_created_at(&transaction, &binding.session_id, item_id)?
+                .unwrap_or_else(|| offset_rfc3339(&timestamp, index as i64));
             upsert_projected_item(&transaction, binding_id, turn_id, item_id, item_kind, item, true,
-                &timestamp)?;
+                &item_timestamp)?;
```
where `prior_item_created_at` is `SELECT i.created_at FROM transcript_items i JOIN session_bindings b ON b.id=i.binding_id WHERE b.session_id=?1 AND i.provider_item_id=?2 ORDER BY b.incarnation DESC LIMIT 1` and `offset_rfc3339` adds `index` nanoseconds via chrono. Alternative (also fixes #5): in bind_session, when the superseding binding has the same provider_thread_id, re-parent the prior incarnation's transcript_items/turns to the new binding so ON CONFLICT keeps created_at; or add an `ordinal` column in the next numbered migration and order on it.

**Fix risk.** Medium: touches the projection path that every event passes through; inheriting created_at changes nothing for same-incarnation ON CONFLICT updates (created_at is not in the UPDATE set) but must stay inside the existing worker/gate transaction. Re-parenting instead would touch turn-permit bookkeeping and views.rs incarnation filters, so prefer the timestamp approach.

**Regression test.** Store test: bind_session(session, ns-A, thread); project ItemCompleted events for userMessage u1 then agentMessage a1 (distinct timestamps); bind_session(session, ns-B, thread) (new incarnation); project one TurnCompleted whose `items` are [u1, a1] with ids that sort a1 < u1 lexically; read transcript_page ascending and assert order is [u1, a1] with u1.created_at < a1.created_at. Before the fix this fails roughly half the time with random ids and always fails if you fix the random ids to the reversed order.


### 9. Symlinked zsh startup files (or pyvenv.cfg) make launch_manifest fail, blocking preview/authorize/run for every /bin/zsh profile

- **Severity:** high · **Category:** wrong-result · **Location:** [execution.rs:1477](../gui/src-tauri/src/workbench/research/execution.rs:1477)
- **Status:** Confirmed by a verifier agent and re-read by hand

**What happens.** Researcher manages dotfiles with stow/chezmoi so ~/.zshrc -> ~/dotfiles/zshrc. They create a Stata profile (/bin/zsh -lic 'oldstata -q -b do analysis.do') and click Preview/Authorize. preview_host_execution -> launch_manifest -> hash_file(~/.zshrc) fails; the UI shows 'Paper input must be a regular file no larger than 200 MiB' and the profile can never be authorized or run. Any 'command' profile with argv[0]=/bin/zsh is blocked the same way.

<details><summary>Evidence and trace</summary>

execution.rs:1465-1484: when `profile.argv[0] == "/bin/zsh"` (mandatory for the Stata adapter, execution.rs:186 rejects anything else), each of .zshenv/.zprofile/.zshrc/.zlogin is checked with `path.exists()` (follows symlinks) and then `hash_file(&path)?` is called on the UNRESOLVED path. papers.rs:82-98: hash_file -> ensure_safe_regular_file uses `fs::symlink_metadata` and errors with "Paper input must be a regular file no larger than 200 MiB" for a symlink. The executable and launch scripts are canonicalized first (execution.rs:1417, 1457-1460) but the startup files and pyvenv.cfg (execution.rs:1428-1429, `config.filter(|p| p.is_file())` also follows the link) are not. launch_manifest is on the path of preview_host_execution (execution.rs:1491+), authorize/stage and run, so a researcher with a stow/chezmoi-managed ~/.zshrc can never preview, authorize or run any zsh-based profile, including every Stata profile, and gets a misleading 'Paper input' error. There is no fallback or per-file tolerance.

</details>

**Proposed fix.**

In launch_manifest, resolve host-owned configuration files through fs::canonicalize before hashing, keeping the manifest JSON shape unchanged so existing authorizations keep their hashes:

```rust
             for name in [".zshenv", ".zprofile", ".zshrc", ".zlogin"] {
                 let path = home.join(name);
-                startup.push(if path.exists() {
-                    let (hash, size) = hash_file(&path)?;
+                startup.push(if path.is_file() {
+                    let resolved = fs::canonicalize(&path)
+                        .map_err(|e| WorkbenchError::storage("Failed to resolve shell startup file", e))?;
+                    let (hash, size) = hash_file(&resolved)?;
                     json!({"path":path,"hash":hash,"size":size})

         if let Some(config) = config.filter(|p| p.is_file()) {
-            let (hash, size) = hash_file(&config)?;
+            let resolved = fs::canonicalize(&config).map_err(...)?;
+            let (hash, size) = hash_file(&resolved)?;
```
Optionally give ensure_safe_regular_file a caller-supplied label so execution errors stop saying 'Paper input'.

**Fix risk.** Low: canonicalize is already the policy for the executable and launch scripts; hashing the resolved target preserves the symlink/FIFO defense (symlink_metadata on the target still rejects non-regular files). Do not add new JSON fields or the manifest hash of existing profiles changes.

**Regression test.** In research/tests.rs, create a temp HOME with `zshrc.real` and `~/.zshrc -> zshrc.real`, set ZDOTDIR in the profile environment to that dir, create a `/bin/zsh -c true` command profile, and assert preview_host_execution succeeds and the manifest's shellStartupFiles entry for .zshrc carries the hash of zshrc.real. Before the fix it errors with 'Paper input must be a regular file'.


### 10. restore_trashed_run compares a canonical path with the uncanonicalized Trash dir; Restore always fails on Windows default storage

- **Severity:** high · **Category:** platform · **Location:** [history.rs:755](../gui/src-tauri/src/runs/history.rs:755)
- **Status:** Confirmed by a verifier agent and re-read by hand

**What happens.** Windows user with the default ~/.pipeline storage moves a report to Trash (works), then clicks Restore in History → Trash: the command returns "Invalid trashed run id" every time. The run can only be recovered by moving the folder by hand in Explorer. Retention also moves runs to Trash automatically, so automatically trashed runs are unrecoverable from the app on Windows.

<details><summary>Evidence and trace</summary>

restore_trashed_run (runs/history.rs:748-756) does `let trash = trash_runs_dir()?; let canonical = trash.join(run_id).canonicalize()?; if canonical.parent() != Some(trash.as_path()) { return Err("Invalid trashed run id") }`. trash_runs_dir (history.rs:652-660) derives from runs_dir() -> data_root(); for the default location data_root returns `self.local` verbatim (storage.rs:64-75), which is dirs::home_dir().join(".pipeline") (storage.rs:34-38), never canonicalized. On Windows std::fs::canonicalize yields a verbatim `\\?\C:\...` path whose first component is Prefix::VerbatimDisk, while the uncanonicalized home path has Prefix::Disk; Path equality compares components, so the comparison is always false and every restore is rejected. The sibling permanently_delete_trashed_run canonicalizes the Trash dir first (history.rs:774-776), so delete works and restore does not. Custom folders are canonicalized at save time (storage.rs:170-172), so only the default location is affected, but that is the common case. On macOS/Linux the same mismatch occurs only when the home or data path crosses a symlink (e.g. a tempdir under /var in a test). No test exercises restore_trashed_run (grep shows only the command wrapper at commands/artifacts.rs:113). Windows is a shipped target (unsigned installer).

</details>

**Proposed fix.**

```text
// runs/history.rs restore_trashed_run
-    let trash = trash_runs_dir()?;
+    let trash = trash_runs_dir()?
+        .canonicalize()
+        .map_err(|error| format!("Cannot resolve run Trash: {error}"))?;
     let source = trash.join(run_id);
     let canonical = source.canonicalize().map_err(|_| "Trashed run not found".to_string())?;
     if canonical.parent() != Some(trash.as_path()) || !canonical.is_dir() { ... }

sync_directory(&trash) at the end then syncs the canonical path, which is equivalent.
```

**Fix risk.** Very low; mirrors permanently_delete_trashed_run exactly.

**Regression test.** Add a round-trip test in runs/tests.rs with the data root pointed at tempfile::tempdir() (on macOS that is under /var -> /private/var, so the symlink mismatch reproduces without Windows): create a finished run, delete_run(id), assert it is listed by list_trashed_runs, then assert restore_trashed_run(id).is_ok() and the directory is back under runs/. Fails before the fix with "Invalid trashed run id"; also runs in the Windows CI job.


### 11. Artifact selection request re-applies on every selection change, bouncing the user back to the requested artifact

- **Severity:** high · **Category:** ui-state · **Location:** [useArtifactSelection.ts:186](../gui/src/components/artifact-explorer/useArtifactSelection.ts:186)
- **Status:** Found by a sweep agent and re-read by hand

**What happens.** User opens a report, clicks a source citation link (RunMarkdown onOpen) which opens Sources at context/document.md line N. They then click 'agent_response/step_1.md' in the artifact tree: the selection flips to it and immediately flips back to context/document.md. Same for compact page navigation after Issues > open evidence (page 3): clicking next page returns to page 3. Only triggering a different request escapes.

<details><summary>Evidence and trace</summary>

The effect that applies `selectionRequest` depends on `selectArtifact`, whose identity changes whenever `selected` changes (useCallback deps [selected]). After a request has been applied, any user navigation (clicking another artifact, goBack, next/previous compact page) re-runs the effect with the same request object, recomputes `next` from the request, and calls selectArtifact(next) which resets `selected` back to the requested artifact (and re-sets fileTab). Because ReportWorkspace keeps `sourceRequest` in state for the lifetime of the run (set by report source links, Issues > open evidence, "open agent reports", and initialSourceSelection from History), the Sources tab becomes unnavigable after any of these entry points.

Guards checked: No applied-request tracking (no ref/key comparison) exists in the effect; `selectArtifact` early-return only fires when next === selected, which is false after the user moves away; ReportWorkspace does not null out sourceRequest after it is consumed; existing ArtifactExplorer.test.tsx only asserts the initial application of a request (line 392), not subsequent navigation.

</details>

**Proposed fix.**

```text
Track the applied request key so the request is consumed once per key/manifest:

+  const appliedRequestKey = useRef<number | null>(null);
   useEffect(() => {
     if (!manifest || !selectionRequest) return;
+    if (appliedRequestKey.current === selectionRequest.key) return;
+    appliedRequestKey.current = selectionRequest.key;
     let next = "";
     ...
   }, [manifest, selectArtifact, selectionRequest]);

and reset `appliedRequestKey.current = null` inside the runId/manifest reset effect (line 37-71) so a re-opened run re-applies the same request. (ReportWorkspace already stamps `key: Date.now()` on every request.)
```

**Regression test.** In ArtifactExplorer.test.tsx: render with selectionRequest={{key:1, relPath:"context/document.md", line:3}}, wait for it to be selected, then userEvent.click an 'agent_response' tree entry and assert that artifact's content is shown (and read_artifact was called with its relPath) and that the selection does not revert. Add a page-based variant: request page 3, click 'next page', assert page 4 stays selected.


### 12. Review App Server call fails non-retryably and interrupts its turn when the shared broadcast channel lags during a host tool call

- **Severity:** high · **Category:** wrong-result · **Location:** [invocation.rs:471](../gui/src-tauri/src/pipeline/codex_server/invocation.rs:471)
- **Status:** Confirmed by a verifier agent and re-read by hand

**What happens.** Automatic Paper Review on a PDF with the App Server backend and default settings: 4+ reviewer steps start in parallel on one connection. Step A's model calls ReadPdfPage; while Poppler renders the page (~2 s), the other three threads stream ~150-300 delta notifications per second. Step A's receiver lags past 256 events; its next recv() yields Lagged; thread/read shows the turn still inProgress; run_connected returns '[codex-outcome-unknown] Event stream lagged...'. The step fails without retry, the cleanup owner interrupts the turn (quota consumed, no report), and the Review completes with a missing or failed reviewer.

<details><summary>Evidence and trace</summary>

Reachable as described. The Review App Server connection is a process singleton (codex_server/connection.rs:13 `static CONNECTION`) and each call takes only a shared read lease (invocation.rs:220 `c.activity.clone().read_owned()`), so parallel steps (executor/run.rs:52 semaphore from settings.max_workers, default 16 per settings/tests.rs:41) run concurrently on one connection. Every call subscribes to the same broadcast channel (invocation.rs:269 `c.native.client.subscribe()`; transport.rs:18 `EVENT_QUEUE_CAPACITY = 256`, :158 `broadcast::channel`). Per-token notifications are not filtered: wire.rs:237 normalizes `item/agentMessage/delta` into an event that is broadcast to all receivers. The receive loop (invocation.rs:341 `events.recv().await`) is the only drain point and is suspended while a call executes a host tool (invocation.rs:405 `tools.execute(...).await`, which goes through api_common::execute_native_tool for ReadPdfPage/asset batches) and around two fsync'd saves (invocation.rs:403/407 `attempt.save()` -> session.rs:170 `private_write` calls `sync_all`). When more than 256 events from sibling threads arrive during that window, recv returns Lagged; the arm at invocation.rs:471 only succeeds if thread/read already shows the turn `completed`, otherwise returns `[codex-outcome-unknown] Event stream lagged...` (invocation.rs:504). provider_error.rs:46-53 classifies `[codex-outcome-unknown]` as non-retryable, and the Attempt is dropped with terminal=false so Drop (invocation.rs:40-70) spawns `turn/interrupt`, discarding the in-flight turn. Workspace handles the same Lagged case by reconciling and continuing (workbench/codex/supervisor.rs:499, :690), so the Review path is the outlier. The transport test at transport.rs:869-878 shows lag past capacity is an expected condition. Severity high: a step in the main Review flow fails silently-non-retryably under ordinary parallel load; no data loss.

</details>

**Proposed fix.**

Two parts, both inside the Review/agent_runtime owners. (1) In run_connected (invocation.rs), make Lagged recoverable instead of terminal:
```rust
let mut lagged = false;
loop {
    match events.recv().await {
        ...
        Err(broadcast::error::RecvError::Lagged(_)) => {
            // reconcile; if already terminal return as today, else keep waiting
            if let Some(result) = reconcile_completed(&c, &thread, &turn, &mut attempt, usage, usage_available).await? { return Ok(result); }
            lagged = true;
            continue;
        }
        Ok(NormalizedEvent::TurnCompleted { .. }) if thread_id == thread && turn_id == turn => {
            let result = if status != "completed" { Err(native_error(..)) } else if lagged {
                // in-memory `items` may be incomplete; rebuild from thread/read includeTurns
                let snapshot = c.native.client.request("thread/read", json!({"threadId":thread,"includeTurns":true}), DEFAULT_REQUEST_TIMEOUT).await.map_err(|e| format!("{UNKNOWN} Reconciliation failed: {e}"))?;
                let values = snapshot["thread"]["turns"].as_array().and_then(|t| t.iter().find(|t| t["id"] == turn)).and_then(|t| t["items"].as_array()).ok_or_else(|| format!("{UNKNOWN} Reconciliation omitted items"))?;
                final_answer(values.iter())
            } else { final_answer(items.values()) };
            ...
        }
```
(2) Server requests must not be lossy: in agent_runtime/codex/transport.rs route `IncomingFrame::ServerRequest` for `item/tool/call` through a dedicated bounded mpsc per subscriber (or a side channel keyed by threadId that the Review call registers after thread/start) so a pending tool call cannot be dropped by broadcast lag; additionally raise EVENT_QUEUE_CAPACITY (e.g. 4096) since Workspace coalesces streams anyway. Keep raw DTOs inside agent_runtime/codex per CLAUDE.md.

**Fix risk.** Medium: touches the shared transport used by Workspace; the per-subscriber request channel must preserve request ordering and the existing 'respond exactly once' contract, and the lagged-reconciliation path must keep the usage accounting and response journal semantics. Requires a no-model simulator probe per CLAUDE.md since transport behavior changes.

**Regression test.** In codex_server/tests.rs with the simulator: start two Review calls on one connection; while call A has a pending item/tool/call whose host tool is blocked on a oneshot, have the simulator emit >300 item/agentMessage/delta notifications for thread B; release the tool and complete turn A with one final_answer item. Before the fix call A returns Err containing '[codex-outcome-unknown] Event stream lagged'; after the fix it returns Ok(final text) and the attempt record state is 'completed' or 'completed_reconciled'.


### 13. Checkpoint/save_text accept ignored paths that capture_changes drops, so accepting the edit unlinks the original file

- **Severity:** high · **Category:** data-loss · **Location:** [tasks.rs:394](../gui/src-tauri/src/workbench/project/tasks.rs:394)
- **Status:** Confirmed by a verifier agent and re-read by hand

**What happens.** A conversation or Markdown link opens target/notes.md (FileWorkspace.open loads any link path; editable=true since .md is an editable source). The researcher edits and presses Save (saveText with checkpointId null). save_text creates a task and a copy checkpoint containing target/notes.md, stages the new text, capture_changes produces an empty proposed set because 'target' is ignored, changed() reports target/notes.md as deleted, apply() renames the original to a claim file and clear_claim unlinks it. The UI shows saved; the file is gone from the project folder (only recoverable through Undo of the application record).

<details><summary>Evidence and trace</summary>

save_text (project/studio/manuscript.rs:62-104) validates the path only with editable() (manuscript.rs:30-36: files::relative + editable_source extension check) and, with checkpoint_id None, calls tasks::create_task + tasks::checkpoint (manuscript.rs:82-93). checkpoint() (project/tasks.rs:245-246) accepts any path passing files::relative (project/files.rs:4-21), which rejects only .git, .pipeline-tasks and .pipeline-apply-* components; `target/notes.md` or `.cache/x.md` pass. capture_changes (tasks.rs:394) builds `proposed` from inventory_at(&task_root, &[]) which skips every path where ignored() is true (project.rs:596-614: components target, node_modules, .venv, __pycache__, .cache, .git, SCRATCH, or .aux/.toc/.out/.fls/.fdb_latexmk/.synctex.gz suffixes), so the staged file is absent from proposed. changed() (tasks.rs:423-433) reports base-present/proposed-absent as a change; apply() (tasks.rs:467-487) records AppliedFile{before: Some(hash), after: None}; run_application (tasks.rs:526-533) calls SafeRoot::replace with bytes None, which renames the original to the .pipeline-apply-<claim> file and places nothing (files.rs:265-301), then clear_claim unlinks the claim file (files.rs:408-425, tasks.rs:565-567). save_text returns {saved:true, hash: hash(content)} (manuscript.rs:102). The path is reachable from the UI: the FileWorkspace 'Find a file' box opens the typed text verbatim when no listed path matches (`paths.find(...) ?? query`, file-workspace/FileWorkspace.tsx:589-594) and any FileLocation passed to open() is loaded without an ignore check (FileWorkspace.tsx:166-195; workbench_file_read uses only files::relative, file_workspace.rs:110). The same base-only-deletion also appears in the review of explicit ProjectAction::Checkpoint task copies that include an ignored path. The deleted bytes remain in the 'before' blob so Undo of the application can restore them, but the editor reports success and the file is gone from the project folder. The only ignored-path test (project/tests.rs:229-247) covers inventory, not checkpoint/capture.

</details>

**Proposed fix.**

Reject ignored paths at the two entry points (ignored() is a private fn of project.rs, visible to its child modules):

```rust
// project/tasks.rs checkpoint(), inside the `for path in paths` loop after files::relative(&path)?;
if ignored(&path, &[]) {
    return Err(WorkbenchError::invalid(
        "This path is inside a build, cache or dependency folder that task capture excludes; choose another file",
    ));
}
// project/studio/manuscript.rs editable()
files::relative(path)?;
if super::super::ignored(path, &[]) {
    return Err(WorkbenchError::invalid("Files in excluded build or cache folders cannot be edited here"));
}
```
As a belt-and-braces guard, make capture_changes re-read every checkpoint base path from the task copy even when inventory skipped it, so a base file can never be reported as deleted merely because it is ignored.

**Fix risk.** Low: only refuses paths the capture pipeline already cannot round-trip; existing valid flows are unchanged.

**Regression test.** Rust test (cfg(unix)) in project/studio/tests.rs: fixture; create `target/notes.md` = "orig"; opened = editor_file(None, "target/notes.md"); call save_text(None, "target/notes.md", &opened.hash, "edited"); assert it returns Err and `target/notes.md` still exists with "orig". Before the fix the call returns Ok(saved:true) and the file no longer exists.


### Fix soon (high severity, not release-gating)

### 14. Self-discovery run becomes terminally 'blocked' on any source-scope drift (draft, new message, research-settings edit) or a single failed child turn, with no retry control

- **Severity:** high · **Category:** ui-state · **Location:** [mod.rs:248](../gui/src-tauri/src/orchestration/discovery/mod.rs:248)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** User starts a 7-day unsupervised run from an existing project conversation (or from the auto-created 'Self-discovery research' conversation, which is an ordinary visible session). Later they open that conversation and type a note without sending it (validate_binding: 'unsent draft'), or send one message (cursor 'turnX:completed' != recorded cursor), or edit Research settings (globalConfig hash changes), or Codex returns a single 'failed' turn. At the next action dispatch the run becomes 'blocked' permanently; remaining budget is lost and only export remains.

<details><summary>Evidence and trace</summary>

orchestration/discovery/mod.rs:246-250 calls research::refresh_scope(&s, &p.run.source_scope, &p.run.source_authority) before every non-terminal dispatch; source_scope/source_authority are set once in discovery_start (discovery/commands.rs:63-71, 109-110) and never re-adopted (account() at mod.rs:102-127 only updates paper scopes). refresh_scope (workbench/missions.rs:29-49) fails when authority() (missions.rs:15-27, hashing permission profile, session overrides, selected context, globalConfig = research::load_config(store, None), workspaceConfig, data policy) differs, when the source cursor differs EXACTLY (`Some(&actual.cursor) != scope.session_cursor.as_ref()`, line 38; tasks::compatible_cursor is not used), and via tasks::validate_binding (workbench/tasks.rs:69-89) when the source session has a non-empty draft. Any Err propagates to tick() (discovery/mod.rs:76-92) which reloads the portfolio and sets state 'blocked' (line 88) or stop_outcome 'blocked' (line 85); Run::terminal() treats 'blocked' as terminal (discovery/model.rs:351-357) and discovery_control (discovery/commands.rs:225-283) only offers pause/resume/stop, no retry. The source session is an ordinary visible session: workspace::source (workbench/discovery.rs:107-141) creates a normal project session 'Self-discovery research' and the list filter (store/sessions.rs:149-151) hides only discovery_roles rows, so a user can open it, type a draft, or send a message; an existing project conversation chosen as source is visible by definition. Child failures take the same terminal path: advance() mod.rs:154-181 returns Err unless every receipt is 'completed', and the task coordinator maps any adapter Err to run 'attention' with no automatic retry (orchestration/mod.rs:558-566), so a single failed Codex turn in a 500-action run ends it as 'blocked'. Missions, by contrast, map the same conditions to a resumable 'attention' state with Retry (missions/mod.rs:58-63, missions/commands.rs:321-389). No discovery test exercises a source-scope change (discovery/tests.rs has no refresh_scope/draft/blocked case).

</details>

**Proposed fix.**

1) In tick() (discovery/mod.rs:76-92) distinguish recoverable failures from ambiguous ones: when the active child has no 'running' receipt (or there is no active child), set `p.run.state = "attention"` (add it to notice_transition and keep it non-terminal in Run::terminal) instead of 'blocked'; keep 'blocked' only for receipts still in 'running'/'unknown'.
2) Add a "retry" arm to discovery_control (discovery/commands.rs:225-283) mirroring missions/commands.rs:349-369: re-check `research::authority(&s, &session) == p.run.source_authority`, require unchanged root_identity/runtime_root/workspace, then adopt `p.run.source_scope.session_cursor = Some(actual.cursor)` and `harness_fingerprint`, clear active_child, set state 'running', due_at now.
3) In refresh_scope add a `read_only: bool` parameter (or a sibling `refresh_source_scope`) used by discovery for the source scope that compares cursors with `tasks::compatible_cursor` and skips the unsent-draft check, since discovery never submits turns to the source conversation.
```rust
// discovery/mod.rs tick()
Err(e) => {
    let id = p.run.id.clone();
    p = c.db(move |s| storage::load(&s, &id)).await?;
    p.run.reason = e;
    let ambiguous = child_has_running_receipt(c, p.run.active_child.as_deref()).await?;
    p.run.state = if ambiguous { "blocked" } else { "attention" }.into();
    p.run.due_at = None;
    save(c, &mut p, "attention", None).await?;
}
```

**Fix risk.** Medium: touches the discovery state machine, export/UI state lists and the shared refresh_scope contract; must keep the ambiguous-submission 'blocked' path and revoke grants only on true terminal states.

**Regression test.** Rust test in orchestration/discovery/tests.rs using the existing fixture: start a run, then `store.update_session(... draft: Some("note") ...)` on the source session (or insert a new completed turn row), call tick(); assert `run.state != "blocked"` and that discovery_control("retry") returns the run to 'running' and dispatches a child. Before the fix tick() leaves state == "blocked" and retry returns 'This control is unavailable'.


### 15. Forked Claude CLI calls deny Edit on their own write root because it is treated as a read root

- **Severity:** high · **Category:** wrong-result · **Location:** [claude_cli.rs:453](../gui/src-tauri/src/pipeline/claude/claude_cli.rs:453)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** Profile with context_cache.enabled=true, Claude in Subscription (CLI) mode, any step with a write root (every GUI run has one via step_write_dir). Warm-up succeeds; the step call is spawned with `--allowedTools ...,Edit(//Users/x/.pipeline/runs/r1/artifacts/by-step/s--abc/s--abc/**)` and `--disallowedTools ...,Edit(//Users/x/.pipeline/runs/r1/artifacts/by-step/s--abc/s--abc/**),...`. The model's Write to <write_dir>/files/table.csv is refused; it either reports it could not save files (schema validation may then reject the response and burn a retry) or writes into the shared workspace cwd. Downstream steps that select `Supporting files from '<producer>'` silently receive nothing.

<details><summary>Evidence and trace</summary>

Path: call_claude_with_shared_context -> call_claude_inner with ClaudeSessionMode::ResumeFork { workspace: context.workspace_dir() } (claude_cli.rs:152-166). context.workspace_dir() is a tempfile tempdir in the system temp directory (context_cache.rs:74-83), while overrides.write_dir is the run artifact write root, so the two are never ancestor/descendant. In fork mode claude_cli.rs:376-384 builds `fork_read_dirs` from extra_read_dirs + cwd + `overrides.write_dir` and calls plan_cli_workspace(Some(workspace), &fork_read_dirs, .., None). plan_cli_workspace (cli_workspace.rs:101-128) sets effective_cwd = workspace and only drops roots that are same_or_descendant of that cwd (cli_workspace.rs:124-128), so the write root stays in `workspace.read_dirs`. Then claude_cli.rs:453-457 calls cli_disallowed_tools(allowed_tools, &workspace.read_dirs, overrides.write_dir.is_some()) which, with write_enabled, emits `Edit(//<root>/**)` for every read root (claude_cli.rs:300-304) including the write root, while the allow list added at claude_cli.rs:401-404 contains the identical `Edit(//<write_dir>/**)`. Claude Code evaluates deny rules before allow rules and before the permission mode, so writes under the task's write root are refused for every forked write-enabled call; with `--permission-mode acceptEdits` (claude_cli.rs:429-430) and cwd = shared workspace, relative writes are auto-approved into the shared tempdir instead. The non-fork path is unaffected because write_dir becomes the cwd and is filtered out (cli_workspace.rs:101, :124-128). The only existing test (claude/tests.rs:20) covers the non-fork deny list. Severity high: silently missing supporting artifacts in a common secondary flow (context_cache + Claude CLI); the model still returns its text answer so the run does not fail.

</details>

**Proposed fix.**

In call_claude_inner, exclude the write root (and ancestors of it) from the deny computation, and deny edits in the shared workspace in fork mode so acceptEdits cannot auto-approve writes into the shared cwd:
```rust
let write_root = overrides.write_dir.and_then(normalize_cli_root);
let deny_roots: Vec<String> = workspace
    .read_dirs
    .iter()
    .filter(|root| write_root.as_deref().is_none_or(|w| !same_or_descendant(w, root)))
    .cloned()
    .collect();
let mut denies = cli_disallowed_tools(allowed_tools, &deny_roots, overrides.write_dir.is_some());
if let (Some(ClaudeSessionMode::ResumeFork { workspace: shared, .. }), true) = (&session_mode, overrides.write_dir.is_some()) {
    if let Some(shared) = normalize_cli_root(shared) {
        denies.push(format!("Edit({}/**)", absolute_rule_path(&shared)));
    }
}
```
(`same_or_descendant` already lives in cli_workspace.rs.)

**Fix risk.** Low: only changes the deny list derivation for the fork path; read-only calls and non-fork calls keep identical argv. Verify with the existing tests in pipeline/claude/tests.rs plus the new one.

**Regression test.** Add to pipeline/claude/tests.rs a test that builds the deny list the way call_claude_inner does for ResumeFork (plan_cli_workspace(Some("/tmp/shared"), &["/Users/x/src", "/runs/r1/by-step/s1"], None, None) then the deny derivation) with write_dir="/runs/r1/by-step/s1", and asserts no deny rule starts with "Edit(" and contains "/runs/r1/by-step/s1" while one does contain "/Users/x/src" and one contains "/tmp/shared". Fails before the fix (write root is denied), passes after.


### Backlog (medium and low)

### 16. "Open in system viewer" launches any model-written binary artifact through an unscoped shell:allow-open

- **Severity:** medium · **Category:** security · **Location:** [default.json:13](../gui/src-tauri/capabilities/default.json:13)
- **Status:** Single pass (sweep agent only, confidence 0.5); not independently verified

**What happens.** A reviewed manuscript contains an injected instruction; a Codex/Claude CLI step writes `summary.jar` (or `report.exe` on Windows, `report.pkg` on macOS) into its artifact directory. The Artifact explorer shows a Binary card with "Open in system viewer"; the researcher clicks it and the payload executes with their privileges.

<details><summary>Evidence and trace</summary>

The main window is granted `shell:allow-open` with no open scope, so the shell plugin's `open` accepts arbitrary local paths (the app relies on this for ArtifactPreviews, FileWorkspace and fileWorkspaceClient opening absolute paths). runs.rs:486-508 classifies every non-text, non-image, non-PDF artifact as "binary" (including .exe, .jar, .pkg, .dmg, .msi), and ArtifactPreviews.tsx:63 BinaryCard calls `openExternal(content.abs_path)` for all of them. CLI-provider steps (Claude Code / Codex CLI with sandbox roots) can write arbitrary bytes into their artifact root, so a prompt-injected document can leave an executable in the run that a single click launches via `open`/ShellExecute. Files written locally carry no quarantine attribute, so Gatekeeper does not intervene on macOS.

Guards checked: runs/artifacts.rs:3-22 confines abs_path to the canonical run directory (so no traversal, but executables inside the run are still reachable). Text, image and PDF kinds never reach BinaryCard. No extension allowlist exists on either side; no `scope` is configured for shell:allow-open in capabilities/default.json or tauri.conf.json plugin config, and the frontend already opens absolute file paths successfully, confirming no regex validation is in effect. SafeMarkdownLink's http/https/mailto guard does not apply to this path. A user click is required.

</details>

**Proposed fix.**

runs/artifacts.rs read_artifact: add `openable: bool` to ArtifactContent, true only for an allowlist of document types (pdf, docx, xlsx, pptx, csv, zip, images) and false for everything else; ArtifactPreviews BinaryCard: when !openable, render "Reveal in Finder/Explorer" (existing commands/artifacts.rs:339 `file_viewer::reveal`) instead of `openExternal`. Alternatively extend detect_kind with an "executable" kind for exe/msi/bat/cmd/ps1/jar/pkg/dmg/app/command/sh and refuse to open it.

**Regression test.** Unit test in runs/tests.rs: write `payload.exe`, `payload.jar`, `payload.pkg` into a temp run dir and assert read_artifact reports openable=false (or kind="executable"); a frontend test that BinaryCard renders Reveal rather than Open for those entries.


### 17. CLI panics (exit 101) when stdout is a closed pipe because println! is used with SIGPIPE ignored

- **Severity:** medium · **Category:** crash · **Location:** [cli.rs:909](../gui/src-tauri/src/bin/cli.rs:909)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** User runs `pipeline-cli run --profile auto-review --input paper.pdf | head -40` (or pipes into `less` and quits early). The multi-minute review completes and is saved, then the CLI panics while printing the Markdown, prints a panic message to the terminal, never prints the 'Run: <id>' line, and exits 101 instead of 0/1. Same for `pipeline-cli workflow schema | head` and `pipeline-cli profiles --json | head -1`.

<details><summary>Evidence and trace</summary>

All stdout output in bin/cli.rs uses println!/print! (finish_single_run at cli.rs:901 and :909, plus the schema/profile/engine paths). The Rust runtime sets SIGPIPE to SIG_IGN before main on Unix; `grep -rn 'sigpipe|SIGPIPE|catch_unwind|panic::set_hook'` over gui/src-tauri/src finds nothing, and Cargo.toml only lists libc as a dependency without any signal setup. println! on an EPIPE write panics with 'failed printing to stdout: Broken pipe'. main (cli.rs:280-286) runs dispatch inside rt.block_on, which resumes the unwind on the calling thread, so the process exits 101, skips codex_server::shutdown(), and never prints the 'Run: <id>' line at cli.rs:911-913. The run itself is already persisted by then (the report was finished before finish_single_run), so impact is limited to the exit status and the missing run-id line, which matters for scripted pipelines checking exit codes.

</details>

**Proposed fix.**

```text
Add one stdout helper and route every report/JSON print through it:

+fn emit_stdout(text: &str) -> bool {
+    use std::io::Write as _;
+    let mut out = std::io::stdout().lock();
+    match out.write_all(text.as_bytes()).and_then(|_| out.write_all(b"\n")).and_then(|_| out.flush()) {
+        Ok(()) => true,
+        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => false, // reader left; stop printing
+        Err(e) => { eprintln!("Error: cannot write to stdout: {e}"); false }
+    }
+}

// finish_single_run
-        Some("-") => println!("{markdown}"),
+        Some("-") => { emit_stdout(markdown); }
-        None => println!("{markdown}"),
+        None => { emit_stdout(markdown); }

Do the same for the check JSON, workflow schema/template, profiles, and engines paths. Do not reset SIGPIPE to SIG_DFL (child stdin pipes rely on EPIPE errors).
```

**Fix risk.** Low; purely CLI output plumbing. Keep the 'Run: <id>' stderr line and exit code computation unchanged.

**Regression test.** Integration test in bin/cli tests (unix): spawn the built pipeline-cli with `profiles --json` (or `workflow schema`) with stdout piped, drop the read end immediately, wait, and assert the exit status is 0 (not 101) and stderr contains no 'panicked'. Alternatively unit-test emit_stdout against a pipe whose reader is closed and assert it returns false without panicking.


### 18. CLI stores relative --input/--input-dir paths in run manifests, so the GUI cannot associate those runs with a project or automate from them

- **Severity:** medium · **Category:** wrong-result · **Location:** [cli.rs:797](../gui/src-tauri/src/bin/cli.rs:797) (first reported at `gui/src-tauri/src/bin/cli.rs:1004`)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** User cds into ~/Papers/draft and runs `pipeline-cli batch --input-dir . --profile auto-review`. manifest.json for each run has input_path "./paper.pdf". In the desktop app, the project attached to ~/Papers/draft lists none of these reviews (underRoot fails), and 'Automate follow-up' from a CLI-created review creates an automation targeting './paper.pdf', which fails when the task runs because the GUI process cwd differs.

<details><summary>Evidence and trace</summary>

cli.rs:797 (run) and cli.rs:1004 (batch) forward the raw path; scan_input_files (commands/batch.rs:204-236) returns entry.path() of read_dir(dir), i.e. `<input-dir>/<name>`, which is relative when --input-dir is relative (e.g. './paper.pdf'). commands/run.rs:194 stores `input_path: paper_path.to_string()` in the manifest and run.rs:681 stores extra_input_sources verbatim. The GUI treats manifest input_path as absolute: useProjectSignals.ts:165 and project-surface/ProjectReviews.tsx filter with underRoot(run.input_path, root), and underRoot (src/lib/projectOverview.ts:109-115) is a plain string-prefix test against the absolute project root, so relative paths never match and CLI runs vanish from the project's Reviews. ReportWorkspace.tsx:848-855 seeds automatePath from runManifest.input_path, so a follow-up Task would validate the relative path against the desktop process cwd (run_context.rs:533-535 fs::metadata) and fail. Lineage (load_latest_report_for_input, history.rs canonicalize) is unaffected. docs/cli/README.md says nothing about path resolution, so there is no documented absolute-path requirement. Real but narrow: only CLI invocations with relative paths.

</details>

**Proposed fix.**

```text
Absolutize in the CLI before building HeadlessRunOptions (shared with the fix for candidate 6):

// bin/cli.rs cmd_run / cmd_check / cmd_batch
+    let input = args.input.map(|p| absolutize(&p)).transpose()?;
     ...
-        input_path: args.input.unwrap_or_default(),
+        input_path: input.unwrap_or_default(),

// cmd_batch, after scan_input_files
+    let files = files.iter().map(|f| absolutize(f)).collect::<Result<Vec<_>, _>>()?;

and apply absolutize to each --extra-input value. Optionally normalize once more in run_pipeline_inner_with_snapshot so any future headless caller is covered, but keep the GUI's absolute dialog paths untouched.
```

**Fix risk.** Low. Manifests from the GUI are unchanged; CLI manifests become absolute, matching the GUI contract.

**Regression test.** In bin/cli tests: run parse+cmd path with --input 'paper.pdf' from a temp cwd (or unit-test absolutize) and assert the HeadlessRunOptions.input_path is absolute and starts with the cwd; for batch, assert scan_input_files('.') results are absolutized before HeadlessRunOptions is built. A vitest for underRoot('./paper.pdf', '/abs/root') === false documents why the Rust side must absolutize.


### 19. recovered_result reads root-level document.md instead of context/document.md, so Task reconcile of a finished Review always errors

- **Severity:** medium · **Category:** wrong-result · **Location:** [orchestration.rs:95](../gui/src-tauri/src/commands/orchestration.rs:95)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** A Task runs a Review step; its receipt becomes "unknown" while the run goes on to finish with status done (the task hit its context budget and marked running receipts unknown at orchestration/mod.rs:452-469, the app was killed between RunWriter::finish and the review-raw.json write, or atomic_json for review-raw.json failed). The task sits in "attention"; the user clicks Reconcile. recover_review finds review-run.json, calls recovered_result -> read_run_file("document.md") -> Err. The reconcile command fails with 'Cannot resolve document.md from run: No such file or directory', the receipt stays "unknown", and the completed Review (visible as done in History) can never be adopted by the task; the only way forward is to re-run and pay for the review again.

<details><summary>Evidence and trace</summary>

commands/orchestration.rs:95 builds the recovered value with `read_run_file(run_id, "document.md")`. read_run_file (commands/rerun.rs:31-46) canonicalizes run_dir.join(rel) and returns Err("Cannot resolve document.md from run: ...") when it does not exist. No run writes a root-level document.md: the captured text path is DOCUMENT_TEXT_PATH = "context/document.md" (runs.rs:38; legacy context/extracted_text.md per history.rs:36-37), and `grep '"document.md"'` finds only the export package writer (commands/export.rs:758) and tests. Reachability: mark_run (commands/orchestration.rs:69-86) writes actions/<op>/review-run.json for every task-owned Review; recover_review (orchestration/adapters.rs:670-698) calls recovered_result when review-raw.json is absent and propagates the error with `?`; Coordinator::recover_results (orchestration/mod.rs:607-612) does `.await?`; task_control "reconcile" (orchestration/commands.rs:148-149) does `m.recover_results(&mut run).await?`. So any receipt in state "unknown" whose run finished done/degraded can never be adopted: reconcile errors and the task stays in attention. extracted_text is only optionally consumed (adapters.rs:590-592 `.as_str().filter(..)`), so the field is not even required. No test covers recovered_result.

</details>

**Proposed fix.**

```text
// commands/orchestration.rs recovered_result
-    Ok(Some(json!({"run_id":run_id,"report":load_run_report(run_id)?,"markdown":read_run_file(run_id,"report.md")?,"extracted_text":read_run_file(run_id,"document.md")?})))
+    let dir = crate::runs::runs_dir()?.join(run_id);
+    let text_path = crate::runs::captured_document_rel_path(&dir, &manifest).ok();
+    let extracted_text = text_path.and_then(|rel| read_run_file(run_id, rel).ok());
+    Ok(Some(json!({
+        "run_id": run_id,
+        "report": load_run_report(run_id)?,
+        "markdown": read_run_file(run_id, "report.md")?,
+        "extracted_text": extracted_text,
+    })))

(captured_document_rel_path is pub(crate) in runs/history.rs:27; re-export it from runs.rs if needed.) The text is optional downstream, so a missing or over-cap document must not fail reconciliation.
```

**Fix risk.** Low. Only the Task recovery path changes; the normal execute path still supplies extracted_text from the live run result.

**Regression test.** In commands/orchestration tests: RunWriter::create_unique in a temp runs root, add_text(DOCUMENT_TEXT_PATH, ...), write report.json/report.md, finish with status "done"; assert recovered_result(&run_id) returns Ok(Some(v)) with v["extracted_text"] equal to the captured text. Fails today with 'Cannot resolve document.md from run'.


### 20. Staged LaTeX runs drop bundle figure assets whose staged key differs from the root-level lookup (graphicspath rewrites and chapter-file references)

- **Severity:** medium · **Category:** wrong-result · **Location:** [adapters.rs:180](../gui/src-tauri/src/document_bundle/adapters.rs:180)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** Project main.tex contains `\graphicspath{{figures/}} \input{sections/model}`; sections/model.tex contains `\includegraphics{irf}`. Staging copies figures/irf.png and records the key ("sections", "figures/irf"). At bundle build, tex_graphic_names(expanded text) yields "irf"; captured_reference(root, root, "irf") returns Some(None); the figure is skipped. document_bundle.json lists no figure assets and artifacts/document/figures is never created, with no warning.

<details><summary>Evidence and trace</summary>

Every app run stages the source (commands/run.rs:114-115 and 692-693 call `extract::stage_selected_source` then `snapshot_source`), so `_pipeline_source_map.json` is present at the staged root. document_bundle/bundle.rs:6-7 calls `copy_tex_assets` for latex sources; adapters.rs:210-222 sets root = parent of the staged main file and resolves each `\includegraphics` argument via `resolve_tex_graphic` (adapters.rs:179-182), which calls `captured_reference(root, root, raw)` = staged.rs:57-64 `reference`, looking up key("", raw) and returning `Some(None)` when the manifest exists but has no such key (staged.rs:55-56 comment: never fall back). Staging records graphics under different keys: latex.rs:437-445 inserts `(directory of the referencing file, reference.target)`, and for `\graphicspath` entries `reference.target` is the rewritten `"{dir}/{file}"` (latex.rs:114-124). Concrete failing case that the repo's own staging fixture uses (pipeline/extract/tests.rs:146-159: main.tex with `\graphicspath{{figures/}}\includegraphics{irf}` and figures/irf.png): the bare reference `irf` (latex.rs:87-95) does not resolve at root (resolve_latex_reference 249-278 tries root/irf.*), the graphicspath variant `figures/irf` resolves and is recorded as key("", "figures/irf"); at bundle time tex_graphic_names yields `irf`, key("", "irf") is absent, `Some(None)` is returned and the staged figure is skipped with no warning. Likewise a chapter file sections/model.tex with `\includegraphics{figures/irf}` is recorded as key("sections", "figures/irf") (current_dir is the chapter directory, latex.rs:425) and dropped at bundle time. Correction to the candidate's own scenario: `graphics_paths` is computed per file (latex.rs:96-107 over the same `visible`), so a `\graphicspath` in main.tex does not apply to `\includegraphics{irf}` inside sections/model.tex; that figure is not staged at all, so the bundle omission there is consistent rather than a lookup mismatch. The mismatch is real for same-file graphicspath and for chapter-level root-relative references.

</details>

**Proposed fix.**

Record a root-level alias for every graphics reference at staging time so the bundle's root lookup matches what the author wrote:

```rust
// pipeline/extract/latex.rs: LatexReference gains the authored argument
 pub(super) struct LatexReference {
     pub target: String,
+    /// The `\includegraphics` argument as written, for root-level lookups.
+    pub alias: Option<String>,
     ...
 }
// in latex_references: bare graphics -> alias: Some(target.to_string()); graphicspath variants (line 115) -> alias: Some(target.to_string()); other references -> alias: None

// stage_latex_project, after source_map.insert(...) at line 437-445
+                    if let Some(alias) = &reference.alias {
+                        let relative = mapped.strip_prefix(destination_root).unwrap().to_path_buf();
+                        source_map.insert(Path::new(""), alias, relative);
+                    }
```
`reference(root, root, raw)` computes relative_dir "" so key("", raw) now resolves for both graphicspath rewrites and chapter-file references. Optionally also emit a quality note from copy_tex_assets when a name yields `Some(None)`.

**Fix risk.** Low-medium: adds manifest entries only; if two files reference the same bare name to different files, the last staged wins for the root alias (document in a comment). Manifest size is bounded by MAX_LATEX_SIZE (staged.rs:19-21).

**Regression test.** document_bundle test: stage the extract/tests.rs:146-159 project (main.tex with `\graphicspath{{figures/}}\includegraphics{irf}`), run extract_latex on the staged main, call document_bundle::build(&extraction, run_dir); assert `added_artifacts` contains an `artifacts/document/figures/001_irf.png` entry. Fails today (empty, directory created but no copy).


### 21. Detached runtime verification keeps the engine gate held after cancellation, so an immediate re-run fails with 'operation already running'

- **Severity:** medium · **Category:** concurrency · **Location:** [installer_io.rs:91](../gui/src-tauri/src/engines/installer_io.rs:91)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** User starts a Review with PaddleOCR-VL extraction on the first use of the session; the log shows 'Verifying the PaddleOCR-VL runtime...'. After 10 s they cancel and immediately start the run again (or Tasks retries it). The second run's extraction fails with 'A managed-engine operation is already running' and the run ends in error, even though nothing is installing; retrying after the detached hash completes succeeds.

<details><summary>Evidence and trace</summary>

extract_paddle_full (pipeline/extract/paddle_full.rs:20-25) wraps `tokio::task::spawn_blocking(crate::engines::paddle_full_parser_lease)` in commands::await_or_cancel (commands/lifecycle.rs:324-344), which returns Err on cancellation and drops the JoinHandle; the blocking task keeps running detached, as the comment at paddle_full.rs:12-16 acknowledges. paddle_full_parser_lease (engines/runtime.rs:445) takes `acquire_engine_guard(false)` first, which does a fail-fast `ENGINE_OPERATION_RUNNING.swap(true)` returning 'A managed-engine operation is already running' when set (installer_io.rs:91-93) and a try_lock on ~/.pipeline/engine.lock (:103-104); the guard is released only in InstallGuard::drop (installer_io.rs:129-135), i.e. when the detached task's `(paths, lease)` result is dropped after the multi-GB content hash completes. A new run that starts extraction in that window calls paddle_full_parser_lease again, hits the swap and fails the run instead of waiting. Note the gate is also held for the entire extraction by design (runtime.rs:439-440 doc comment), so two simultaneous PaddleOCR-VL runs already fail fast; the cancel-then-retry case is the narrower variant where nothing useful holds the gate. Severity medium: edge case, confusing error, recoverable by retrying later.

</details>

**Proposed fix.**

Let extraction wait for a lease that is merely held by a detached verification, while installs keep try-lock semantics. Minimal change in extract_paddle_full: retry acquisition with bounded backoff while no install is running:
```rust
// paddle_full.rs
let lease_future = async {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        match tokio::task::spawn_blocking(crate::engines::paddle_full_parser_lease).await
            .map_err(|e| format!("PaddleOCR-VL runtime verification task failed: {e}"))? {
            Ok(v) => break Ok::<_, String>(v),
            Err(e) if crate::engines::engine_gate_busy_without_install() && std::time::Instant::now() < deadline => {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            Err(e) => break Err(e),
        }
    }
};
let (paths, engine_lease) = crate::commands::await_or_cancel(lease_future, None).await??;
```
with `pub(crate) fn engine_gate_busy_without_install() -> bool { ENGINE_OPERATION_RUNNING.load(SeqCst) && !INSTALL_RUNNING.load(SeqCst) }` in installer_io.rs. Optionally make sha256 verification cooperative by checking a cancellation flag in its read loop so the detached hash exits early.

**Fix risk.** Low: only extraction's acquisition changes; install/uninstall/cleanup keep fail-fast behavior. Bound the wait so a stuck holder cannot hang a run past the extraction timeout.

**Regression test.** In engines/tests.rs: acquire an InstallGuard via acquire_engine_guard(false) on one thread, hold it for ~1 s, and concurrently call the new waiting acquisition helper from extraction; assert it returns Ok once the first guard drops. Before the fix the second acquisition returns Err('A managed-engine operation is already running') immediately.


### 22. Parser install falls through to a full multi-GB rebuild when the sidecar is already current, after passing only the 16 MB sidecar-refresh space check

- **Severity:** medium · **Category:** wrong-result · **Location:** [paddle_install.rs:193](../gui/src-tauri/src/engines/paddle_install.rs:193)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** Parser runtime verified and sidecar current, but ~/.pipeline/native/paddleocr-vl is missing or corrupt (user deleted it to free space, or it failed verification). The panel shows 'not installed'; the user clicks Install with 1 GB free. install_engine computes plan ParserSidecarRefresh (16 MB) and passes the space check; install_paddle_full_parser downloads the 1.9 GB base stack with no check, then renames the healthy parser to .<release>-backup and starts re-provisioning it; the uv sync fails with ENOSPC after several GB of writes, the install reports a confusing 'parser package sync failed', and the previously good parser was needlessly torn down and rolled back. The same wasteful rebuild happens for `pipeline-cli engines install paddleocr-vl-parser` on a healthy install.

<details><summary>Evidence and trace</summary>

install_engine (engines/operations.rs:23-26) sets lightweight_parser_refresh = parser_runtime_reusable_for_sidecar_refresh(root), which is true whenever the pinned runtime verifies (runtime.rs:358-360), and then budgets only PADDLE_SIDECAR_REFRESH_HEADROOM_MB = 16 (engines.rs:340, :374-377) for the free-space check (operations.rs:43-49). install_paddle_full_parser (paddle_install.rs:157-198) first installs the base engine when missing with no space check (paddle_install.rs:166-173), then calls refresh_paddle_parser_sidecar_locked, which returns Ok(false) both when the runtime is not reusable (runtime.rs:392-394) and when the sidecar and manifest already match the embedded script (runtime.rs:407-414). The `Ok(false) => {}` arm at paddle_install.rs:193 falls through to ensure_uv, renames the healthy versions/<release> directory to a backup (paddle_install.rs:209-223) and re-provisions CPython, the uv sync and the layout model, none of which was covered by the 16 MB plan. Callers: Settings `install_engine` command (commands/config.rs:642) and `pipeline-cli engines install` (bin/cli.rs:1480), so the healthy-parser/missing-base scenario and a healthy reinstall both reach it. engines/tests.rs:591-592 asserts the 16 MB plan but nothing ties the plan to the installer's actual decision. Severity medium: wasteful and can fail mid-way on ENOSPC, but the backup/rollback keeps the previous parser.

</details>

**Proposed fix.**

Make the refresh outcome three-state and short-circuit the already-current case; keep the space plan consistent with the base-engine download.
```rust
// runtime.rs
pub(super) enum SidecarRefresh { NotReusable, AlreadyCurrent, Refreshed }
pub(super) fn refresh_paddle_parser_sidecar_locked(root: &Path) -> Result<SidecarRefresh, String> {
    if !parser_runtime_reusable_for_sidecar_refresh(root) { return Ok(SidecarRefresh::NotReusable); }
    ...
    if installed_digest == expected && manifest.sidecar_sha256 == expected { return Ok(SidecarRefresh::AlreadyCurrent); }
    ...
    Ok(SidecarRefresh::Refreshed)
}
// paddle_install.rs
match refresh_paddle_parser_sidecar_locked(&parser_root) {
    Ok(SidecarRefresh::Refreshed | SidecarRefresh::AlreadyCurrent) => { /* verify via paddle_full_parser_paths_at and return Ok(()) as today */ }
    Ok(SidecarRefresh::NotReusable) => {}
    Err(error) => log(...),
}
// operations.rs: charge the base download when it is missing
let lightweight_parser_refresh = ... && base_installed;
```
Update the paddle_full_parser_lease caller (runtime.rs:449-454) to ignore the enum.

**Fix risk.** Low-medium: changes a pub(super) return type used by two callers; a deliberate 'repair' of a healthy parser would need an explicit force path if that behavior is wanted (today it is indistinguishable from install).

**Regression test.** In engines/tests.rs, create a temp parser root with a verified runtime fixture whose sidecar already matches PADDLE_PARSER_SCRIPT (reuse the fixtures behind existing refresh tests), call refresh_paddle_parser_sidecar_locked and assert SidecarRefresh::AlreadyCurrent; and assert install_space_requirement("paddleocr-vl-parser", false, true, true) no longer yields ParserSidecarRefresh/16 once the plan requires base_installed. Before the fix the first returns Ok(false) (indistinguishable from NotReusable) and the second returns the 16 MB plan.


### 23. Every self-discovery action creates a new role directory with full copies of the input files that are never removed

- **Severity:** medium · **Category:** resource-leak · **Location:** [mod.rs:284](../gui/src-tauri/src/orchestration/discovery/mod.rs:284)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** Researcher supplies 40 MiB of input files and runs the default 75-candidate portfolio; ~200 reviewer/action folders each receive a 40 MiB copy (~8 GB) plus evidence copies; after a few runs the private store consumes tens of GB with no UI to reclaim it, since terminal runs only disable the roles.

<details><summary>Evidence and trace</summary>

For every phase that is neither a persistent paper phase nor Acquire/ExternalReview/Deliver, advance() builds `role = format!("action-{}", p.run.actions_reserved)` and calls workspace::role (orchestration/discovery/mod.rs:281-289); storage::admit increments actions_reserved on every dispatch (discovery/storage.rs:205), so each action gets a unique role key. workspace::role (workbench/discovery.rs:144-262) creates a new session per key (lines 164-184) and, when not yet configured, a new directory `<store root>/discovery/<sha256(run/role_key)>` (lines 240-241 via directory(), 17-43) and inserts a discovery_roles row. advance() then calls materialize for every such action (mod.rs:290-297) and again with evidence + manuscript artifacts for reviewer actions on a paper (mod.rs:298-314); materialize (discovery/artifacts.rs:112-152) reads each artifact (up to 64 MiB + 1) and project::materialize_research_files (project.rs:509-534) writes full byte copies into the role root. Termination only calls workspace::revoke, which executes `UPDATE discovery_roles SET enabled=0` (discovery.rs:85-94); the only remove_dir_all calls under workbench are retention trash and archive conversation_files, and nothing references the discovery directory, so every per-action directory and its input copies persist indefinitely across runs. With the documented defaults (75 proposals in batches of five, two reviewers over up to 20 candidates, revision + reassessment, two referees per manuscript version, challenges per round) the per-run directory count is on the order of 150-250, each holding a full copy of the inputs; the docs describe cleanup only for execution grants, not folders.

</details>

**Proposed fix.**

Remove read-only role directories when a run terminates (writer/paper roots can stay for export) and consider sharing one read-only input root per run:

```rust
// workbench/discovery.rs
pub fn revoke(store: &Store, run: &str) -> WorkbenchResult<()> {
    let conn = store.connection()?;
    conn.execute("UPDATE discovery_roles SET enabled=0 WHERE run_key=?1", [run]).map_err(error)?;
    let mut q = conn.prepare("SELECT root_key FROM discovery_roles WHERE run_key=?1 AND writable=0").map_err(error)?;
    for key in q.query_map([run], |r| r.get::<_, String>(0)).map_err(error)? {
        let dir = directory(store, &key?)?; // validates 64-hex, real directory, no symlink
        let _ = std::fs::remove_dir_all(&dir); // best effort; grants are already disabled
    }
    Ok(())
}
```
Called from tick() (discovery/mod.rs:93-98) and discovery_control (commands.rs:285-290) as today. Optionally key non-paper roles as `format!("reviewer-{phase}")` so materialize reuses one directory per phase instead of one per action.

**Fix risk.** Low-medium: deletion must stay inside <store>/discovery/<hex> via directory() validation; disabled roles cannot run (role_policy rejects them), so removing their roots after revoke does not affect live work. Export reads artifacts from the artifact store, not these roots.

**Regression test.** Rust test in orchestration/discovery/tests.rs (or workbench/discovery/tests.rs): run the deterministic lifecycle fixture to a terminal state, then assert that `<store root>/discovery` contains no directory for a `discovery_roles` row of that run with writable=0. Before the fix one directory per reviewer/action role remains with the materialized input copies.


### 24. Mission Retry without an active child never re-adopts the planner conversation cursor, so a steering message makes every Retry fail

- **Severity:** medium · **Category:** ui-state · **Location:** [commands.rs:383](../gui/src-tauri/src/orchestration/missions/commands.rs:383)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** Mission stops with attention 'Repeated investigations are no longer adding evidence' (active_child None). The researcher opens the planner conversation from Activity and sends a steering message; the planner's latest turn id changes. They press Retry: advance -> refresh_scope -> 'The mission conversation or folder changed outside its recorded work' -> attention again. Every subsequent Retry fails identically; the mission can only be stopped.

<details><summary>Evidence and trace</summary>

mission_control 'retry' (orchestration/missions/commands.rs:321-389) re-validates authority and adopts the current cursor only in the active-child branch (lines 325-381: `target.session_cursor = Some(actual.cursor)`). The else branch (lines 382-388) only sets state queued / phase Plan. advance() then uses the planner scope (missions/mod.rs:212-225) and calls workspace::refresh_scope, which requires `Some(&actual.cursor) == scope.session_cursor` exactly (workbench/missions.rs:38) and returns 'The mission conversation or folder changed outside its recorded work. Inspect the conversation before continuing.'; tick() turns that Err into 'attention' (mod.rs:58-63) with active_child still None, so every subsequent Retry takes the same else branch and fails identically. Attention-without-child is reachable: stagnation sets attention inside apply_challenge (mod.rs:605-610) after advance() already cleared next.active_child (mod.rs:104), and any refresh_scope failure during dispatch does the same. Role conversations are ordinary visible project sessions created via tasks::occurrence_session (workbench/missions.rs:142; workbench/tasks.rs:207+), not discovery_roles, so the researcher can open the planner conversation and send a steering message, which changes tasks::cursor (workbench/tasks.rs:40-45). There are no retry tests in missions/tests.rs.

</details>

**Proposed fix.**

Factor the authority/binding re-adoption from the active-child branch into a helper and apply it to the Plan-phase planner scope in the else branch:

```rust
} else {
    m.phase = Phase::Plan;
    let session = m.planner_scope.session_id.clone().ok_or("Mission conversation missing")?;
    let expected = m.authority_fingerprints[&session].clone();
    let old = m.planner_scope.clone();
    let actual = run_store(move |s| {
        if workspace::authority(&s, &session)? != expected {
            return Err(WorkbenchError::conflict("Mission authority changed; prepare a new mission"));
        }
        let b = crate::workbench::tasks::binding(&s, &session)?;
        if Some(&b.root_identity) != old.root_identity.as_ref() || Some(&b.runtime_root) != old.runtime_root.as_ref() {
            return Err(WorkbenchError::conflict("Mission folder changed"));
        }
        crate::workbench::tasks::validate_binding(&s, &b)?;
        Ok(b)
    }).await.map_err(|e| e.message)?;
    m.planner_scope.session_cursor = Some(actual.cursor);
    m.planner_scope.harness_fingerprint = Some(actual.harness_fingerprint);
    m.state = "queued".into();
    m.due_at = Some(now());
    m.reason = "Reconsidering the agenda within the remaining budget".into();
    m.stagnant_rounds = 0;
}
```

**Fix risk.** Low: reuses the exact checks already applied in the active-child branch; permissions, roots and selected context still cannot change silently.

**Regression test.** Rust test in orchestration/missions/tests.rs: drive a mission to attention with active_child None (stagnation), then insert a new completed turn for the planner session (changing tasks::cursor), call mission_control(retry) and tick(); assert the mission dispatches (state 'running' with an active child). Before the fix tick() returns the mission to 'attention' with reason 'The mission conversation or folder changed outside its recorded work'.


### 25. Synthesized Check/Experiment finding copies a >4000-byte instruction into tested_domain, so adoption fails after host execution and every Retry re-runs the check

- **Severity:** medium · **Category:** wrong-result · **Location:** [mod.rs:439](../gui/src-tauri/src/orchestration/missions/mod.rs:439)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** Planner proposes an experiment candidate whose 'instruction' is 5 KB (allowed, prompt asks for a 'concrete bounded investigation'); the captured plan executes successfully (receipt passed); consume() rejects the synthesized finding; mission enters attention; each Retry re-executes the plan and fails again until the action budget is exhausted.

<details><summary>Evidence and trace</summary>

validate::plan accepts candidate instructions up to 12000 bytes (orchestration/missions/validate.rs:158 `text(&c.instruction, 12000)?`). For Check/Experiment investigations consume() synthesizes a Finding with `tested_domain: c.instruction.clone()` (missions/mod.rs:439) and immediately calls validate::finding (mod.rs:441), which enforces `text(&f.tested_domain, 4000)?` (validate.rs:275) and returns 'Required text is empty or exceeds 4000 bytes' (validate.rs:6-10). This happens only after the child TaskRun finished and the host receipt was journaled (advance() mod.rs:91-102 consumes a 'finished' child). consume() works on a clone (`next`), so the Err propagates to tick() which sets state 'attention' (mod.rs:58-63) while m.active_child still references the finished child. Retry (missions/commands.rs:321-381) re-validates the binding, adopts the cursor, clears active_child and requeues with the phase unchanged (still Investigate); advance() then calls child_chain (mod.rs:302-321) which dispatches the same Action::Check / Action::CapturedCheck, consuming another action and host run, and consume() fails identically. No guard bounds the instruction before synthesis (prompts::short exists but is private and unused here).

</details>

**Proposed fix.**

Bound the synthesized text with the existing excerpt helper (make `short` pub(super) in missions/prompts.rs) so the finding always satisfies validate::finding:

```rust
// missions/mod.rs consume(), Investigate branch
Finding {
    summary: format!("Executed the selected check: {}", prompts::short(&c.question, 3800)),
    outcome: ...,
    method: ...,
    tested_domain: prompts::short(&c.instruction, 3800), // short() appends a ~70-byte excerpt marker; stays under 4000
    ...
}
```
Alternatively validate synthesized host findings with the plan limit (12000) via a `validate::host_finding` variant; the excerpt is simpler and keeps the full instruction available in the round's selected candidate.

**Fix risk.** Low: only affects the synthesized finding text for host executions; the exact instruction remains in round.selected and the execution receipt.

**Regression test.** Rust test in orchestration/missions/tests.rs: build a mission with a Check capability, apply a plan whose selected candidate has a 5000-byte instruction (validate::plan passes), simulate a finished child whose output has a completed receipt, call consume(); assert Ok and `rounds.last().finding.tested_domain.len() <= 4000`. Before the fix consume() returns Err('Required text is empty or exceeds 4000 bytes').


### 26. Mission prompts are stored inline and re-parsed by render_prompt, so '{{input:...}}'/'{{output:...}}' text in researcher or source content fails every action permanently

- **Severity:** medium · **Category:** wrong-result · **Location:** [storage.rs:168](../gui/src-tauri/src/orchestration/missions/storage.rs:168) (first reported at `gui/src-tauri/src/orchestration/missions/mod.rs:273`)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** The source conversation's last exchange (pulled by last_exchange into sourceContext) or the mission background mentions this product's own binding syntax, e.g. 'use {{input:document}}'. The dispatched Workspace action fails before submission with 'Missing task input: {{input:document}}', the child goes to attention, the mission goes to attention, and Retry reproduces the same failure forever.

<details><summary>Evidence and trace</summary>

child_chain (orchestration/missions/mod.rs:264-269, 271-274, 309) embeds the fully rendered prompt as Action::Workspace{prompt}. prompts::packet/render (missions/prompts.rs:18-53) serialize untrusted text into that prompt: definition.background/objective/criteria, sourceContext = m.context (built in workbench/missions.rs:190-192 from h.context_preview plus last_exchange, which is the raw payload_json of the source conversation's last four user/agent transcript items, lines 196-210), prior findings and questions. JSON serialization and the '<' escaping (prompts.rs:47) preserve '{{'. missions::storage::admit (missions/storage.rs:138-176) stores `inputs: json!({})` and leaves the prompt inline, unlike discovery::storage::admit (discovery/storage.rs:159-165) which moves the prompt into inputs['discoveryPrompt'] and substitutes '{{input:discoveryPrompt}}' exactly once. definition::validate (definition.rs:450-457 via prompt_bindings 495-508) rejects any `{{output:X}}` whose X is not a step id at admit time (advance Err -> attention), and `{{input:X}}` passes validation but fails at execution: adapters.rs:337-338 runs render_prompt(prompt, outputs, inputs) whose regex `\{\{(output|input):([a-zA-Z0-9_-]+)(#[^}]*)?\}\}` (definition.rs:357-378) returns Err('Missing task input: ...') for any unresolved reference; the coordinator then marks the child 'attention' (orchestration/mod.rs:558-566), advance() sees child.state attention and returns Err (missions/mod.rs:135-143), the mission goes to attention, and Retry re-dispatches the identical prompt. The product itself documents this syntax to researchers (definition.rs catalog 'contextBindings'), so background text like 'use {{input:document}}' is a realistic, if uncommon, input. Mechanically confirmed; frequency is low, hence medium.

</details>

**Proposed fix.**

Mirror discovery/storage.rs in missions::storage::admit so the rendered prompt is substituted opaquely once:

```rust
pub fn admit(s: &Store, m: &mut Mission, mut chain: Chain, scope: Scope) -> Result<TaskRun> {
    ...
    let mut inputs = serde_json::json!({});
    if let super::super::definition::Action::Workspace { prompt, .. } = &mut chain.steps[0].action {
        inputs["missionPrompt"] = serde_json::json!(std::mem::replace(prompt, "{{input:missionPrompt}}".into()));
    }
    super::super::definition::validate(&chain)?;
    let run = TaskRun { chain, scope, inputs, ... };
    ...
    super::super::store::insert_run(&tx, &run, &op, &hash(&(&run.chain, &run.scope, &run.inputs))?)?;
```
render_prompt replaces in a single pass, so braces inside the substituted value are never re-scanned.

**Fix risk.** Low: only new mission children change shape; the idempotency hash now includes inputs, which is already the discovery convention. Existing queued children keep their inline prompt.

**Regression test.** Rust test in orchestration/missions/tests.rs: create a mission whose definition.background contains "see {{input:document}} and {{output:nope}}", call child_chain + storage::admit, assert the admitted child's step prompt is "{{input:missionPrompt}}" and that definition::render_prompt(prompt, &json!({}), &run.inputs) succeeds and contains the literal "{{input:document}}". Before the fix admit() fails validation on {{output:nope}} (or, without it, render_prompt returns 'Missing task input: {{input:document}}').


### 27. Slot-blocked task rewrites its body and appends an identical event every 2 s while waiting

- **Severity:** medium · **Category:** resource-leak · **Location:** [mod.rs:438](../gui/src-tauri/src/orchestration/mod.rs:438)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** A daily scheduled 'Follow up' automation becomes due while the researcher is in a 90-minute interactive Workspace turn (task_turn_available() is false). The occurrence sits in 'queued' and tick() saves it every 2 s: ~2,700 events rows and ~2,700 rewrites of the run body (which can be several MiB once Review outputs are retained) before the slot frees. Repeated daily, the events table grows by tens of thousands of rows per task and the task's Activity log becomes unusable; two automations that both contain Workspace steps block each other the same way.

<details><summary>Evidence and trace</summary>

In tick() (orchestration/mod.rs:291-318) a ready, non-control step that cannot dispatch (slot busy: active.len() >= 4 or same-kind active, or adapter unavailable) sets run.due_at = now()+2, reason = 'Waiting for an execution slot' and `changed = true`, then after the loop `if changed { self.save(&mut run, "queued", "Waiting for an execution slot").await? }` (mod.rs:438-441) runs unconditionally every tick. Coordinator::save -> Store::save rewrites the whole body (`UPDATE runs SET ... body=?6`, store.rs:266) and inserts an events row (store.rs:271-275) on every call. The run loop sleeps until next_due clamped to >= 1 s (mod.rs:158-165) and ready_ids selects runs with due_at <= now (store.rs:383), so the same run is re-saved every ~2 s with an identical state/reason for as long as the slot stays busy. No guard compares previous state/reason (the Waiting branch at mod.rs:282-288 does compare, the Ready branch does not), and the events table has no pruning (only paginated reads at store.rs:406). Severity medium: unbounded event growth and body rewrites while a task waits behind a long Workspace turn; no incorrect results.

</details>

**Proposed fix.**

Persist the queued transition once and let subsequent re-polls be in-memory only:
```rust
Advance::Ready(ready) => {
    let was_queued = run.state == "queued" && run.reason.as_deref() == Some("Waiting for an execution slot");
    let mut queued_due = None;
    let mut blocked = false;
    for pending in ready {
        ...
        if !control && (busy || !adapters::available(..)) {
            queued_due = Some(now() + 2);
            run.due_at = queued_due;
            if run.state != "running" { run.state = "queued".into(); }
            run.reason = Some("Waiting for an execution slot".into());
            blocked = true;
            continue;
        }
        ...
    }
    if changed || (blocked && !was_queued) {
        self.save(&mut run, "queued", "Waiting for an execution slot").await?;
    } else if blocked {
        // keep the durable due_at fresh without an event row, at most every 30 s
        if run.due_at.is_none_or(|d| d + 30 < now()) { self.db(move |s| s.touch_due(&id, queued_due)).await?; }
    }
}
```
where `changed` is set only by dispatches/decisions (remove the `changed = true` at mod.rs:316) and `Store::touch_due` is a small `UPDATE runs SET due_at=?2 WHERE id=?1` without an events insert. The completing child already calls coordinator.notify() (mod.rs:434) so the wake does not depend on due_at.

**Fix risk.** Low-medium: the coordinator relies on due_at for recovery after restart; keep a periodic durable due_at (the touch) so a restart still re-polls the queued run. Recheck the existing orchestration tests for queued-state assertions.

**Regression test.** In orchestration tests, create a run whose first step is a Workspace action while `active` already holds a ("workspace", _) entry; call tick() three times with the clock advanced 2 s each; assert `SELECT COUNT(*) FROM events WHERE run_id=? AND kind='queued'` is 1 and the run revision advanced once. Before the fix the count is 3 and the revision advanced three times.


### 28. Nested If/ForEach named 'done' (or 'enter' inside While) collides with the parent's synthetic decision keys

- **Severity:** medium · **Category:** wrong-result · **Location:** [state.rs:76](../gui/src-tauri/src/orchestration/state.rs:76)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** Chain: If 'gate' { then: [ If 'done' { then: [delay 'a'] }, workspace 'b' ] }, deliver 'c'. Tick 1: 'gate' records decisions['gate']=true, inner If records decisions['gate/done']=<its choice>, dispatches 'a'. Tick 2: at step 'gate' the check `contains_key("gate/done")` is now true, so the whole outer branch is treated as complete; 'b' never runs and 'c' is dispatched immediately with the task reporting 'All steps completed'. Similarly, While 'w' { steps: [ If 'enter' {...} ] } always takes the If's then-branch because decisions['w/0/enter'] (the loop entry flag, true) is read as the If's recorded choice and its condition is never evaluated; a ForEach named 'enter' there fails with 'Invalid saved iteration input'.

<details><summary>Evidence and trace</summary>

walk() (orchestration/state.rs:66-80) addresses each step as `{prefix}{id}` and skips a step when `p.decisions` contains `{address}/done` (state.rs:76). An If records its own choice at exactly `address` (state.rs:85-98) and its completion at `{address}/done` (state.rs:111); children of If and Chain are walked with prefix `{address}/` (state.rs:100-107, :235), so a child If or ForEach whose id is `done` records its decision at `{parent}/done`. ForEach also records at its own address (state.rs:206) and While records entry at `{address}/{i}/enter` (state.rs:168-179) while its children use prefix `{address}/{i}/`, so a child If/ForEach named `enter` reads the loop flag as its own decision (state.rs:85 `v.as_bool()` on `true`; ForEach fails with 'Invalid saved iteration input' at state.rs:218). validate_steps (definition.rs:254-267) only checks charset `[A-Za-z0-9_-]`, length and chain-wide uniqueness, so `done` and `enter` are accepted. Trace for the candidate's chain: tick 1 records decisions['gate']=true, inner If records decisions['gate/done']=<bool>, returns pending leaf; tick 2 hits state.rs:76 for 'gate' with 'gate/done' present and skips the whole branch, so later siblings inside the branch never run. Leaf steps named `done` do not collide because they use `p.receipts`, not decisions (state.rs:239). Severity medium: silently wrong task flow but requires a specific reserved id.

</details>

**Proposed fix.**

Reject the reserved ids at validation time (cheapest, keeps saved decision keys stable for existing runs) and, for new definitions, separate synthetic keys from the id namespace. In definition.rs validate_steps:
```rust
if step.id.is_empty()
    || step.id.len() > 64
    || matches!(step.id.as_str(), "done" | "enter")
    || step.id.bytes().all(u8::is_ascii_digit)
    || !step.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    || !ids.insert(step.id.clone())
{
    return Err("Step IDs must be unique identifiers of at most 64 characters and may not be 'done', 'enter' or purely numeric".into());
}
```
(Numeric-only ids are rejected because Repeat/Parallel/While/ForEach children use `{address}/{i}/` prefixes that could collide with an If child named like an index.) Changing the key separator (e.g. `{address}#done`) would also work but changes the durable decisions map for in-flight runs, so prefer validation unless a migration of saved runs is planned.

**Fix risk.** Low: validation-only change; an existing saved chain using these ids would fail validation on next edit, which is the intended outcome.

**Regression test.** In orchestration tests: build a chain `If 'gate' { then: [ If 'done' { then: [Delay 'a'] }, Workspace 'b' ] }` and assert validate() returns Err after the fix; separately (before the fix) drive walk() twice with 'a' completed and assert the second walk returns Ready containing 'gate/b' — it currently returns Finished/Ready for the next top-level step, demonstrating the skip.


### 29. Quality ledger counts every extra provider call in multi-agent and fan-out steps as a schema retry, marking first-try runs degraded

- **Severity:** medium · **Category:** wrong-result · **Location:** [quality.rs:61](../gui/src-tauri/src/output/quality.rs:61)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** Settings → parallel agents = [claude, codex] (or any workflow with a `for_each` fan-out step). Every call succeeds on its first attempt. The saved report nevertheless shows `Quality status: Degraded`, a limitation line `Provider responses required N additional attempt(s).`, and `**Schema retries:** N` in the Quality section, and History/ReportWorkspace label the run degraded.

<details><summary>Evidence and trace</summary>

output/quality.rs:61-62 adds `output.attempt_count.saturating_sub(1)` per non-skipped output as `schema_retry_events`, and lines 133-135 and 151-158 turn any nonzero value into the 'Provider responses required N additional attempt(s).' limitation and status "degraded". models.rs:402-404 documents StepOutput.attempt_count as 'Number of provider attempts represented by the totals above', and the assemblers sum it across calls: merge.rs:185-187 sums per-agent attempts and merge.rs:460 sets the merged output to `original_attempts + 1`; merge.rs:615-617/632 sums per-agent attempts for unmerged multi-agent groups; executor/findings.rs:372 sums per-unit attempts for fan-out collapse. Per-call attempt counts exist on StepCallRecord (models.rs:357, filled from usage.provider_attempts at provenance.rs:37). So a two-agent parallel step with a merge call, all first-try, yields attempt_count 3 and is reported as 2 schema retries and a degraded run. Parallel agents are a Settings-level default (settings.rs:576, up to four providers), so this is an ordinary configuration, but the effect is a wrong quality label rather than wrong findings.

</details>

**Proposed fix.**

Count retries per call when call records exist:

```rust
// output/quality.rs build_report_quality
-        schema_retry_events =
-            schema_retry_events.saturating_add(output.attempt_count.saturating_sub(1));
+        let retries = if output.calls.is_empty() {
+            output.attempt_count.saturating_sub(1)
+        } else {
+            output.calls.iter().fold(0u32, |total, call| {
+                total.saturating_add(call.attempt_count.saturating_sub(1))
+            })
+        };
+        schema_retry_events = schema_retry_events.saturating_add(retries);
```
Legacy outputs without call records keep the old arithmetic.

**Fix risk.** Low: pure reporting change; single-call outputs produce identical numbers.

**Regression test.** output/quality test: build a StepOutput with `calls` = two StepCallRecords (attempt_count 1 each) plus a merge record (attempt_count 1) and output.attempt_count = 3; call build_report_quality with no failures/notes; assert schema_retry_events == 0 and status == "done". Fails today with 2 and "degraded".


### 30. Fan-out collapse assembles a bare {findings} object that violates the step's findings-v2 contract and drops the validation ledger

- **Severity:** medium · **Category:** wrong-result · **Location:** [findings.rs:333](../gui/src-tauri/src/pipeline/executor/findings.rs:333)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** Workflow: step `validate` with for_each.artifact {step:"synthesis", pointer:"/findings"} and output_schema {"x-pipeline-schema":"findings-v2-validation","x-pipeline-preserve-findings-from":"synthesis"}. Each unit passes check_validation_dispositions and returns findings + one disposition with hashes. After the wave, the assembled product is `{"findings":[...]}`. Consequences: (1) build_run_products -> parse_value sees no schema_version, parses as legacy v1 (strict_v2=false), products.schema_version=1 and validation_dispositions=[] so the quality report's manual_review_items is 0 and the audit ledger is gone from report.json/exports; (2) on any later resume (only_failed or from_step), incompatible_reuse_ids canonicalizes the cached artifact against the step schema, it fails (missing required fields), and the validate step plus every dependent is forcibly re-run with a log line claiming the cached artifact 'does not satisfy the active output contracts' even though nothing changed; (3) a downstream step that preserves findings from `validate` gets a product whose ranks are all 1.

<details><summary>Evidence and trace</summary>

pipeline/executor/run.rs:388-402 calls `collapse_findings_fan_out` after every parallel wave and checkpoints its result. findings.rs:333 builds `serde_json::json!({ "findings": combined })` and findings.rs:350-357 marks it `structured_json: true` without any schema check. The step's resolved contract (findings.rs:254 `effective_output_schema`, structured.rs:69-96) is `output_schema_for_taxonomy` whose root `required` is `["schema_version","taxonomy","findings"]` (src/findings.rs:168) or `validation_output_schema_for_taxonomy` which additionally requires `validation_dispositions` (src/findings.rs:214-217). Per-unit `validation_dispositions` are never read (findings.rs:306-312 only extracts `findings`). Consequences verified: (1) src/findings.rs:331-343 `build_run_products` -> `parse_validation_dispositions` (src/findings.rs:408-416) reads `validation_dispositions` from the assembled raw_text, which is absent, so the ledger is empty and `manual_review_items` (output/quality.rs:85-94) is 0; `parse_value` (src/findings.rs:508-513) sees no `schema_version`, sets strict_v2=false and products.schema_version=1 (src/findings.rs:667). (2) commands/rerun.rs:160-173 `incompatible_reuse_ids` canonicalizes each cached output against the resolved step schema; the assembled artifact lacks required root fields, so the step and dependents are forced to re-run on resume. (3) Each unit's schema requires `rank` equal to array position within its single-element array (src/findings.rs:189), so the concatenation carries duplicate rank 1 values. No built-in uses `for_each` (rg over builtins.rs/auto_review.rs is empty) and no test references `collapse_findings_fan_out` outside findings.rs/run.rs, so this affects user-authored fan-out workflows only.

</details>

**Proposed fix.**

In `collapse_findings_fan_out`, assemble the root from the resolved schema, renumber ranks, carry the ledger, and validate before accepting:

```rust
-        let combined_value = serde_json::json!({ "findings": combined });
+        let mut dispositions = Vec::new();
+        // inside the per-element loop, alongside unit_findings:
+        //   if let Some(ledger) = unit_value.get("validation_dispositions").and_then(Value::as_array) { dispositions.extend(ledger.iter().cloned()); }
+        //   else if element.is_object() { dispositions.push(json!({"finding_id": element["id"], "disposition": "deferred_manual_review", "reason": "fan-out unit produced no usable verdict"})); }
+        for (index, finding) in combined.iter_mut().enumerate() {
+            if let Some(object) = finding.as_object_mut() {
+                object.insert("rank".into(), serde_json::json!(index + 1));
+            }
+        }
+        let mut root = serde_json::Map::new();
+        root.insert("schema_version".into(), serde_json::json!(crate::findings::FINDING_SCHEMA_VERSION));
+        root.insert("taxonomy".into(), schema.get(crate::pipeline::structured::FINDINGS_TAXONOMY_KEY).cloned().unwrap_or_else(|| serde_json::json!(crate::findings::CATEGORIES)));
+        root.insert("findings".into(), serde_json::Value::Array(combined));
+        if schema.get(crate::pipeline::structured::VALIDATION_LEDGER_KEY).is_some() {
+            root.insert("validation_dispositions".into(), serde_json::Value::Array(dispositions));
+        }
+        let combined_value = serde_json::Value::Object(root);
         let raw_text = match serde_json::to_string_pretty(&combined_value) { ... };
+        if let Err(error) = crate::pipeline::structured::canonicalize(&schema, &raw_text) {
+            emit WARNING "assembled fan-out findings for step '{}' do not satisfy its output contract ({error}); keeping per-unit outputs.";
+            kept.extend(units); outputs = kept; continue;
+        }
```
Keep the existing budget fallback. This respects the executor rule that rejected products never become downstream context.

**Fix risk.** Medium-low: changes the persisted artifact shape for fan-out steps (now contract-conformant); resume compatibility improves. Ledger synthesis for failed units must use the disposition enum exactly (src/findings.rs:226-230).

**Regression test.** Executor test: step `validate` with for_each.artifact {step:"synthesis", pointer:"/findings"} and output_schema {x-pipeline-schema-ref: findings-v2-validation, x-pipeline-preserve-findings-from: synthesis}; feed two unit outputs each with one finding (rank 1) and one disposition; call collapse_findings_fan_out; assert `structured::canonicalize(&resolved_schema, &assembled.raw_text).is_ok()`, ranks are [1,2], and `validation_dispositions.len() == 2`. Fails today (missing schema_version/taxonomy/validation_dispositions).


### 31. Bare relative .tex input passes `check` but fails staging because Path::parent of a bare filename is Some(""), not None

- **Severity:** medium · **Category:** other · **Location:** [latex.rs:533](../gui/src-tauri/src/pipeline/extract/latex.rs:533) (first reported at `gui/src-tauri/src/bin/cli.rs:797`)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** From the manuscript directory: `pipeline-cli check --profile auto-review --input paper.tex` exits 0 (ready); `pipeline-cli run --profile auto-review --input paper.tex` exits 1 with 'Error: Failed to resolve : No such file or directory (os error 2)'. `--input ./paper.tex` or an absolute path works, so the message gives no hint of the cause.

<details><summary>Evidence and trace</summary>

cli.rs:797 passes args.input verbatim into HeadlessRunOptions; no absolutization exists anywhere in bin/ or commands/ (`grep current_dir()|std::path::absolute` hits only scan_input_files). commands/run.rs:61 wraps it as Path and :114 calls extract::stage_selected_source(paper, ...). In latex.rs:517-518 a `.tex` file becomes latex_main, and :530-534 computes `project_root = selected.parent().unwrap_or_else(|| Path::new("."))`. For a bare name, Path::parent returns Some(""), not None (verified with a throwaway rustc program: parent=Some(""), canonicalize -> Err("Failed to resolve : No such file or directory (os error 2)")), so stage_latex_project (latex.rs:366-368) fails with exactly that message. The preflight validate_primary_input_selection (run_context.rs:533-535) uses fs::metadata on the relative path, which succeeds, so `check` reports ready. The failure occurs after the run directory was created (run.rs:114 is after preflight_run_storage), leaving a failed run behind. Same for `--extra-input key=appendix.tex` through run.rs:690-692. The GUI always passes absolute dialog paths, so only the CLI is affected.

</details>

**Proposed fix.**

```text
Harden the fallback and absolutize CLI paths.

// pipeline/extract/latex.rs stage_selected_source
-            selected.parent().unwrap_or_else(|| Path::new("."))
+            selected
+                .parent()
+                .filter(|parent| !parent.as_os_str().is_empty())
+                .unwrap_or_else(|| Path::new("."))

// bin/cli.rs, before building HeadlessRunOptions in cmd_run/cmd_check/cmd_batch
+fn absolutize(path: &str) -> Result<String, String> {
+    let p = Path::new(path);
+    if p.is_absolute() { return Ok(path.to_string()); }
+    let cwd = std::env::current_dir().map_err(|e| format!("Cannot resolve the current directory: {e}"))?;
+    Ok(cwd.join(p).to_string_lossy().into_owned())
+}
   input_path: args.input.map(|p| absolutize(&p)).transpose()?.unwrap_or_default(),
   extra_inputs: args.extra_inputs.into_iter().map(|(k, v)| absolutize(&v).map(|v| (k, v))).collect::<Result<_, _>>()?,
```

**Fix risk.** Low. The latex.rs change only affects an empty parent. Absolutizing in the CLI also fixes candidate 8; manifests from the GUI are unchanged.

**Regression test.** Unit test in pipeline/extract/latex.rs: chdir-free variant — create temp dir with main.tex, call stage_selected_source(Path::new("main.tex"), "document", private) after setting the process cwd to the temp dir (serialize with a test mutex), assert Ok. Fails today with 'Failed to resolve : No such file or directory'.


### 32. Prompt-refresh migrations propagate validation errors from user-edited built-ins, making ensure_migrated fail on every launch

- **Severity:** medium · **Category:** compat · **Location:** [migrations.rs:592](../gui/src-tauri/src/pipeline_config/migrations.rs:592)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** v0.9.5 user kept the stock auto-review orientation prompt but edited auto_consistency's prompt to say 'Use the {var:journal} house style' without declaring `journal` (0.9.5 had no placeholder check; it was added in validation.rs after v0.9.5). First 1.0.x launch: parsimony replaces the orientation prompt, sets changed=true, validate_profile_data returns 'step 'auto_consistency' prompt references undeclared variable', migrate_builtin_catalog returns Err, no marker is written, and list_profiles()/every run fails with the same error on every subsequent launch.

<details><summary>Evidence and trace</summary>

migrations.rs:918 `migrate_builtin_catalog(&profiles)?` inside `ensure_migrated`, which is called with `?` from profiles.rs:39 `list_profiles`, persistence.rs:91 `load_required_workflow_for` and profiles.rs:186 `export_profile_data`. Inside the catalog, every prompt-refresh body parses a built-in, patches a field and calls `validate_profile_data(&profile)?` (migrations.rs:180, 215, 283, 317, 493, 524, 592, 634), and each marker is written only after its body returns Ok (e.g. lines 123-136 for `.prompt-parsimony-v1`). So one built-in that is valid JSON but fails current validation turns into an error on every launch; `load_profile` (persistence.rs:11) treats the same file as skippable, the migrations do not. Reachability of the parsimony scenario verified: `.prompt-parsimony-v1` and `.structured-grant-review-contract-v1` are absent from v0.9.5 migrations.rs (git show), the parsimony digest `e0b2d3d9...` equals sha256 of v0.9.5 prompts/auto_review/orientation.md (computed), v0.9.5 validation.rs has no `{var:` placeholder check (added in 5c08807, after the v0.9.5 tag), and the structured-auto-review-contract-v2 migration (migrations.rs:480-489) does not replace a v0.9.5 auto-review.json because v0.9.5 already used `AUTO_REVIEW_CONTRACT = "auto-review-v2"` (git show v0.9.5 auto_review.rs:28), so the user's edited steps survive into the parsimony body. A 0.9.5 auto-review.json with the stock orientation prompt and any step prompt containing an undeclared `{var:x}` therefore makes `refresh_parsimonious_prompt_defaults` fail at line 592 on every launch. Severity medium: requires a 0.9.x-era edit that current rules reject, but the result is a persistent, UI-unrepairable failure of the Workflows list and every run.

</details>

**Proposed fix.**

Treat a built-in that fails current validation as "leave alone" in every catalog body so the marker is still recorded and load_profile's skip semantics are matched:

```rust
// migrations.rs, in refresh_parsimonious_prompt_defaults (both the auto loop at 591 and the grant block at 633):
         if changed {
-            validate_profile_data(&profile)?;
+            if let Err(error) = validate_profile_data(&profile) {
+                eprintln!("WARNING: left '{}' unchanged by the prompt-parsimony migration: {error}", path.display());
+                continue; // (for the grant block: skip the write instead)
+            }
             let json = serde_json::to_string_pretty(&profile)...;
             restore_profile_bytes(&path, json.as_bytes())...;
         }
```
Apply the same pattern at lines 180, 215, 283, 317, 493 and 524 (the v7 body at 317 is not in a loop: wrap the whole update in `if validate_profile_data(&profile).is_ok()`). Keep `?` for I/O and serialization errors.

**Fix risk.** Low: only the error path changes; profiles that validate are migrated exactly as before. A skipped profile still shows as 'skipped invalid profile' in list_profiles, which the user can then reset once candidate 1 is fixed.

**Regression test.** pipeline_config migration test: create a profiles dir with auto-review.json whose orientation_prompt is the v0.9.5 stock text (digest e0b2d3d9...) and whose auto_consistency prompt contains `{var:journal}` with no variables; write all CATALOG_MIGRATION_MARKERS except `.prompt-parsimony-v1`; call migrate_builtin_catalog(&profiles); assert Ok and that `.prompt-parsimony-v1` now exists. Fails today with 'references undeclared variable'.


### 33. Grant Review migration keys on step ids that never shipped, so upgraded installs keep the free-form grant suite

- **Severity:** medium · **Category:** compat · **Location:** [migrations.rs:377](../gui/src-tauri/src/pipeline_config/migrations.rs:377)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** A v0.9.5 user upgrades to 1.0.2. On first launch has_legacy_skeleton is false (no grant_significance step), so grant-review.json is left as the 5-step suite. Running Grant Proposal Review produces no structured findings/validation stage and the Issues view stays empty; the marker file prevents the migration from ever running again. Only a manual 'Reset to defaults' (which works for this profile) installs the typed suite.

<details><summary>Evidence and trace</summary>

migrations.rs:377-385 requires all of grant_significance, grant_approach, grant_investigators, grant_clarity, grant_synthesis; line 395 returns early otherwise. `git show v0.9.5:gui/src-tauri/src/pipeline_config/builtins.rs | rg grant_` shows the shipped ids were grant_aims, grant_feasibility, grant_clarity, grant_consistency, grant_synthesis (lines 306-338), and `git log -S grant_significance` returns only 5c08807, the commit that added this migration; `rg grant_significance` in the current tree hits only migrations.rs, so no builtin ever wrote that skeleton and no test exercises refresh_stale_grant_review_suite. The current builtins.rs:312-355 grant_review_profile adds grant_validate with findings_step/primary_step. Flow on a 0.9.5 upgrade: migrations.rs:144-157 runs refresh_stale_grant_review_suite once, has_legacy_skeleton is false, the file is left as the 5-step free-form suite, and the `.structured-grant-review-contract-v1` marker is written so it never re-runs; refresh_parsimonious_prompt_defaults (600-640) only swaps prompts, not steps. Only a manual reset installs the typed suite. The comment at migrations.rs:138-143 states the intent to archive and replace, so the shipped behavior contradicts the design.

</details>

**Proposed fix.**

Detect the shape that actually shipped and re-key the migration so 1.0.0-1.0.2 installs get it too:

```rust
// migrations.rs refresh_stale_grant_review_suite
-    let has_legacy_skeleton = ["grant_significance", "grant_approach", "grant_investigators", "grant_clarity", "grant_synthesis"]
-        .iter().all(|step_id| profile.steps.iter().any(|step| step.id == *step_id));
+    let has_legacy_skeleton = profile.steps.iter().any(|step| step.id == "grant_synthesis")
+        && !profile.steps.iter().any(|step| step.id == "grant_validate");

// migrate_builtin_catalog (line 144) and CATALOG_MIGRATION_MARKERS (line 362)
-    let structured_grant_review_marker = profiles.join(".structured-grant-review-contract-v1");
+    let structured_grant_review_marker = profiles.join(".structured-grant-review-contract-v2");
     ...
     ".structured-grant-review-contract-v1",
+    ".structured-grant-review-contract-v2",
```
The `is_current` check at 386-394 already protects an up-to-date file, and `archive_retired_profile` (line 399) preserves user edits.

**Fix risk.** Low-medium: a custom profile that reuses the `grant-review` id with a grant_synthesis step but no grant_validate would be archived and replaced; that matches the documented intent and the archive keeps the copy recoverable.

**Regression test.** Migration test: write grant-review.json from `git show v0.9.5` shape (steps grant_aims, grant_feasibility, grant_clarity, grant_consistency, grant_synthesis; no output schemas), write all markers except the grant one, run migrate_builtin_catalog; assert grant-review.json now contains grant_validate with x-pipeline-schema-ref findings-v2-validation and that an archived copy exists. Fails today (file unchanged).


### 34. Reset to defaults silently no-ops on an invalid profile because save_for re-loads and re-validates the broken file

- **Severity:** medium · **Category:** wrong-result · **Location:** [profiles.rs:28](../gui/src-tauri/src/pipeline_config/profiles.rs:28)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** User on v0.9.5 had declared a variable `topic` in auto-review and referenced `{var:topic}` in a step prompt, then removed the variable (accepted by 0.9.5 validation). After upgrading to 1.0.x the file fails `step 'auto_contribution' prompt references undeclared variable '{var:topic}'`: the workflow disappears from the list, the editor shows Auto Review defaults for it, Save fails with 'Profile is invalid', and 'Reset workflow' reports success (dirty flag cleared) but the file is untouched; next launch repeats. Built-in cannot be deleted, so the only fix is manual file surgery.

<details><summary>Evidence and trace</summary>

Path trace: commands/config.rs:488-490 `reset_pipeline_config` -> pipeline_config/profiles.rs:22-30 `reset_defaults()` builds the stock config and calls `let _ = save(&d);` (line 28), discarding the error. profiles.rs:4-8 `save` -> profiles.rs:10-18 `save_for`, which at line 15 does `let name = load_profile(profile_id)?.name;`. persistence.rs:3-13 `load_profile` runs `validate_profile_data(&profile)` (line 11) on every current-format file, and validation.rs:379-660 enforces the full rule set on load, including `validate_profile_steps` (unique ids, line 73) and the undeclared `{var:...}` check (validation.rs:648-652). So any on-disk profile that fails current validation makes `save_for` return Err before writing, `reset_defaults` still returns the stock config, and useProfileOperations.ts:382-404 `handleReset` receives a successful invoke, calls `setConfig`, `setDirty(false)` and overwrites `savedConfigRef`, showing a successful reset while the file is untouched. The scenario is reachable: `git merge-base --is-ancestor 5c08807 v0.9.5` shows the placeholder check (commit 5c08807, 2026-08-27) postdates v0.9.5 (2026-08-17), and `git show v0.9.5:.../validation.rs` has no `{var:` check, so a 0.9.5 profile referencing an undeclared variable loads fine there and fails on 1.0.x; hand edits reach the same state. Recovery is blocked from the UI: list_profiles skips it (profiles.rs:55-58), switch/rename/export go through `load_profile` (profiles.rs:113,178,187), and delete_profile refuses built-ins (profiles.rs:121-123).

</details>

**Proposed fix.**

Write the stock ProfileData directly under the profile lock without re-reading the existing file, and surface errors:

```rust
// pipeline_config/profiles.rs
-pub fn reset_defaults() -> PipelineConfig {
-    let d = match get_active_profile_id().as_str() { ... };
-    let _ = save(&d);
-    d
-}
+pub fn reset_defaults() -> Result<PipelineConfig, String> {
+    let _ = ensure_migrated();
+    let id = crate::settings::load_persisted_required()?.active_profile;
+    let _lock = lock_profile_mutations()?;
+    let stock: ProfileData = match id.as_str() {
+        "auto-review" => auto_review_profile(),
+        "auto-review-quick" => quick_auto_review_profile(),
+        "grant-review" => grant_review_profile(),
+        _ => {
+            // Keep the custom profile's display name, read leniently.
+            let name = read_profile_file(&profile_path(&id)?).ok()
+                .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
+                .and_then(|v| v.get("name")?.as_str().map(str::to_string))
+                .unwrap_or_else(|| id.clone());
+            ProfileData::from_config(name, &defaults())
+        }
+    };
+    save_profile_unlocked(&id, &stock)?;
+    Ok(stock.into())
+}

// commands/config.rs:489
-    Ok(pipeline_config::reset_defaults())
+    pipeline_config::reset_defaults()
```
The command name and return shape to the frontend are unchanged (a Result<PipelineConfig, String> already).

**Fix risk.** Low: the write path still goes through save_profile_unlocked (validated, locked). Behavior change only when the existing file was unreadable/invalid, which previously failed silently.

**Regression test.** In pipeline_config/tests: write auto-review.json whose auto_contribution prompt contains `{var:topic}` with no `topic` variable, set active_profile = auto-review, assert load_profile fails, call reset_defaults(), then assert it returns Ok and load_profile("auto-review") succeeds and equals auto_review_profile(). Fails today because save_for returns Err and the file is unchanged.


### 35. Bundle import validates raw imported settings without the loader's normalization, rejecting 0.9.x backups

- **Severity:** medium · **Category:** compat · **Location:** [profiles.rs:388](../gui/src-tauri/src/pipeline_config/profiles.rs:388)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** User exported 'pipeline-settings-backup.json' from v0.9.3 (file contains "antigravity_access_mode": "subscription"), reinstalls on 1.0.2 and uses Import All: the call fails with 'Antigravity subscription mode is disabled: ...' and none of their custom profiles are restored, even though the app itself would have normalized that value on load.

<details><summary>Evidence and trace</summary>

profiles.rs:378-388 `import_bundle` deserializes the bundle's Settings and calls `imported_settings.validate()?` on the raw value. The persisted loader settings.rs:1324-1337 `load_raw_settings_required` (and the UI loader at 1144-1146) first apply `migrate_legacy_model_fields`, `normalize_access_modes` (which unconditionally sets `antigravity_access_mode = "api"`, settings.rs:1024) and `drop_unknown_providers` (settings.rs:1033-1052, strips e.g. "gemini"), with the comment at 1333-1335 explaining that validate() on the launch path rejects such values. `Settings::validate` rejects `antigravity_access_mode` other than ""|"api" (settings.rs:624-631) and any provider outside claude/codex/antigravity/local (567-575, 591-593). `git show v0.9.3:settings.rs` shows `antigravity_access_mode: "subscription"` as the default (line 534) and v0.9.0 allowed "gemini" (lines 537, 666-688); `export_bundle` (profiles.rs:205-225) serializes the whole Settings struct, so 0.9.x bundles carry those values. The import is all-or-nothing (the Err at 388 precedes any profile write at 395+), and the rejected field is not among those copied into `current` (398-420), so valid custom profiles are refused over an irrelevant setting.

</details>

**Proposed fix.**

Normalize the imported settings the same way the loader does before validating:

```rust
// settings.rs (impl Settings)
+    /// Apply the same compatibility normalization the persisted loader uses.
+    pub fn normalized_for_import(mut self) -> Self {
+        self.migrate_legacy_model_fields();
+        self.normalize_access_modes();
+        self.drop_unknown_providers();
+        self
+    }

// pipeline_config/profiles.rs import_bundle
-            let validated = validate_bundle_profiles(&profiles, &active_profile)?;
-            imported_settings.validate()?;
+            let validated = validate_bundle_profiles(&profiles, &active_profile)?;
+            let imported_settings = imported_settings.normalized_for_import();
+            imported_settings.validate()?;
```
Alternatively validate only the merged `current` after the field copy, which already excludes access modes.

**Fix risk.** Low: normalization is the same code the app runs on every settings load; access modes derived from (empty) API keys in the bundle are not copied into `current`.

**Regression test.** pipeline_config test: build a Bundle envelope JSON whose settings contain `"antigravity_access_mode": "subscription"` and `"preferred_provider": "gemini"` plus one valid custom profile; call import_bundle; assert Ok and that the profile file exists. Fails today with 'Antigravity subscription mode is disabled'.


### 36. Undecryptable stored API key blocks every Review launch and Settings offers no way to discard it

- **Severity:** medium · **Category:** other · **Location:** [settings.rs:1462](../gui/src-tauri/src/settings.rs:1462) (first reported at `gui/src-tauri/src/settings.rs:1309`)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** User once saved a Google API key, later stops using Google. They restore ~/.pipeline/settings.json from a backup (or a different machine) but not keyfile. get_settings shows the warning and an empty Google key field. Every Review launch now fails with 'Cannot start run because settings could not be loaded safely: Could not decrypt Google API key: Decryption failed'. User opens Settings, leaves Google key empty, clicks Save: prepare_secret_for_save keeps the stale ciphertext, so the next launch fails identically. Runs stay blocked until the user guesses to enter a throwaway key and then clear it.

<details><summary>Evidence and trace</summary>

settings.rs:1302-1319 `load_persisted_required` decrypts all four stored keys and returns Err on the first failure (lines 1309-1316), regardless of which provider the run uses. It gates run launch at commands/run_context.rs:119, rerun.rs:191, artifacts.rs:266, orchestration.rs:144, and profile save/export/import (profiles.rs:6,207,397). The UI loader settings.rs:1149-1178 `load_with_warnings` turns an undecryptable key into an empty string plus a warning, so Settings shows an empty field. The Settings save path is commands/config.rs:123 -> settings.rs:1374-1388 `save_preserving_active` -> settings.rs:1480-1516 `save_unlocked_preserving_raw_secrets` -> settings.rs:1451-1478 `prepare_secret_for_save`, which at lines 1462-1467 deliberately re-persists the old ciphertext when the UI value is empty and the ciphertext does not decrypt. No command or UI path writes an empty value for such a key (rg finds no discard/clear command), so the only way out is to type a throwaway key then clear it (second save hits the decrypt-ok branch and encrypts ""). Trigger is settings.json restored without ~/.pipeline/keyfile, keyfile loss, or the crypto.rs:56-83 corrupt-size path (which renames the keyfile aside and a later save generates a new one). Reachable but requires a keyfile mismatch, and a workaround exists, so medium rather than high.

</details>

**Proposed fix.**

Keep load_persisted_required strict (its contract says it never substitutes), but make a deliberate discard expressible without typing a dummy key:

```rust
// settings.rs: new locked mutation, same pattern as set_active_profile (1393-1402)
+pub fn discard_undecryptable_secret(provider: &str) -> Result<(), String> {
+    let path = settings_path()?;
+    let (_guard, lock_file) = acquire_settings_write_lock(&path)?;
+    let result = load_raw_settings_required(&path).and_then(|mut current| {
+        let key = load_or_create_key()?;
+        let field = match provider { "anthropic" => &mut current.anthropic_api_key, "openai" => &mut current.openai_api_key, "google" => &mut current.google_api_key, "local" => &mut current.local_api_key, other => return Err(format!("Unknown provider '{other}'")) };
+        if !field.starts_with(ENC_PREFIX) || decrypt_string(field, &key).is_ok() { return Err("Stored key is readable; clear it from Settings instead".into()); }
+        field.clear();
+        save_raw_unlocked(&path, &current)
+    });
+    let _ = fs2::FileExt::unlock(&lock_file);
+    result
+}
```
Expose it as a Tauri command, and have `get_settings` return the list of providers whose key failed to decrypt (derived from the warnings at settings.rs:1151-1178) so the Settings page can show a "Discard stored key" action next to the warning. Optionally scope the hard failure in load_persisted_required to keys whose provider is actually selected for the run.

**Fix risk.** Low: adds a new explicit mutation under the existing write lock; existing preserve-ciphertext behavior for accidental empty saves is unchanged.

**Regression test.** settings tests: encrypt a google key with key A, write settings.json, swap the keyfile for key B; assert load_persisted_required() is Err; call discard_undecryptable_secret("google"); assert the persisted google_api_key is empty and load_persisted_required() is Ok. Also assert a Settings-page save with an empty value still preserves the ciphertext (existing behavior).


### 37. PDF page_count is one short for multi-page PDFs because the trailing form feed is trimmed, so the last page cannot be recorded as inspected

- **Severity:** medium · **Category:** wrong-result · **Location:** [documents.rs:94](../gui/src-tauri/src/workbench/project/documents.rs:94)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** Researcher opens a 12-page build PDF in Manuscript, navigates to page 12 and clicks 'Record inspection of build page 12'. inspect_pages computes count=11 and returns 'Choose existing PDF pages'; the final page cannot be recorded. The reader also reports 11 pages.

<details><summary>Evidence and trace</summary>

read_document (project/documents.rs:93-97) derives page_count as `text.matches('\u{000c}').count().max(1)` for pdf revisions. The retained text comes from extract_import_text (workbench/research/papers.rs:198-205) which calls crate::pipeline::extract::extract_pdftotext; that function trims the whole output (pipeline/extract/pdf.rs:40 `String::from_utf8_lossy(&output.stdout).trim().to_string()`), and U+000C is Unicode White_Space so the trailing form feed pdftotext emits after the last page is removed. Verified on this machine with the repository fixture gui/test.pdf: pdfinfo reports 71 pages, `pdftotext gui/test.pdf -` contains 71 U+000C and ends with "\n\n\f", so the trimmed text contains 70 form feeds and page_count becomes 70 for every PDF with >= 2 pages (1-page PDFs are rescued by max(1)). inspect_pages (studio/manuscript.rs:565-572) rejects any page > count with 'Choose existing PDF pages'; the frontend page number is a free numeric input (research-studio/Manuscript.tsx:101, 776) sent as `pages: [pdfPage]` (Manuscript.tsx:836-839), so the last compiled page can never be recorded as inspected. DocumentView.pageCount (projectClient.ts:164) has no frontend consumer today, so the visible impact is limited to inspect_pages (and any future reader use). No test covers page_count.

</details>

**Proposed fix.**

Keep the shared Workflow extractor unchanged and count pages on the Workspace side:

```rust
// project/documents.rs read_document()
let page_count = if revision.input_kind == "pdf" && !text.is_empty() {
    let breaks = text.matches('\u{000c}').count();
    // pdftotext ends every page with U+000C; trimming removed the final one.
    Some(if text.ends_with('\u{000c}') { breaks } else { breaks + 1 }.max(1))
} else {
    None
};
```
Note: trim() also removes multiple trailing form feeds when the last pages are blank; if that matters, have papers.rs store the raw pdftotext output trimmed only of '\n', '\r', ' ' and '\t' (a Workspace-local wrapper), leaving pipeline/extract/pdf.rs untouched.

**Fix risk.** Low: changes only the derived page count returned to the reader and inspect_pages; retained text and fingerprints are unchanged.

**Regression test.** Rust test in workbench/project (documents): import a pdf revision whose stored text is "p1\u{000c}p2\u{000c}p3" (what extract_pdftotext yields for a 3-page PDF after trim) and assert read_document(...).page_count == Some(3), then assert inspect_pages(..., vec![3], ...) succeeds. Before the fix page_count is Some(2) and inspect_pages returns 'Choose existing PDF pages'.


### 38. Every plain editor save leaves a permanent open 'Edit {path}' task and an orphaned .pipeline-tasks copy directory

- **Severity:** medium · **Category:** wrong-result · **Location:** [manuscript.rs:82](../gui/src-tauri/src/workbench/project/studio/manuscript.rs:82)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** A researcher saves main.tex 60 times during a writing session. The project overview's next step becomes 'Edit main.tex', the action-items list shows 60 open 'Edit main.tex' tasks, the model context lists 60 'Open task ...: Edit main.tex' lines (after 500 saves, real open tasks are dropped from context with 'older open tasks are omitted'), and .pipeline-tasks/ inside the research folder accumulates 60 directories each holding a copy of main.tex that are never deleted.

<details><summary>Evidence and trace</summary>

save_text with checkpoint_id None (project/studio/manuscript.rs:80-93) calls tasks::create_task, which stores a ResearchTask with status 'open' (project/tasks.rs:72-85), then tasks::checkpoint which creates <root>/.pipeline-tasks/<checkpoint_id>/ with a copy of the file (tasks.rs:305, 349-362) and stage/capture/apply. Nothing afterwards changes the task: update_task is only reached from ProjectAction::UpdateTask (project.rs:893-897), apply()/reject() only mutate checkpoint/application records (tasks.rs:498-507, 665-680), and no code under workbench/project removes a task directory (the only remove_dir_all calls are in release/retention.rs and release/archive/conversation_files.rs). Consequences are user- and model-facing: project/index.sql:29-33 selects the most recently updated task whose status is not completed/rejected as next_task (rendered as 'Next: ...' in ProjectIndexPage.tsx:205-213); project_context (project.rs:289-296) appends 'Open task {id}: Edit {path}' for every open task into the research harness context, capped at 500 with a truncation note; ProjectActionItems lists data.tasks. The existing test editor_saves_are_conflict_aware_reversible_and_task_scoped (studio/tests.rs:133-164) does not assert task state afterwards.

</details>

**Proposed fix.**

Close the auto-created task after a successful apply and discard the copy:

```rust
// manuscript.rs save_text()
let (cp, auto_task) = if let Some(cp) = checkpoint_id { (cp.to_string(), None) } else {
    let task = tasks::create_task(store, ws, format!("Edit {path}"), None, vec![path.into()], vec!["Rebuild the accepted file combination".into()])?;
    (tasks::checkpoint(store, ws, &task.id, vec![path.into()], "copy")?.id, Some(task))
};
...
let application = if let Some(task) = auto_task {
    let applied = tasks::apply(store, ws, &cp, vec![path.into()], captured.revision)?;
    tasks::update_task(store, ws, &task.id, task.revision, "completed")?;
    tasks::discard_copy(store, ws, &cp)?; // new: validates task_root_path() then fs::remove_dir_all for the copy backend
    Some(applied)
} else { None };
```
Alternatively tag auto-created tasks (e.g. objective prefix or a `kind` flag) and exclude them from next_task and project_context.

**Fix risk.** Low-medium: removing the copy directory must go through task_root_path() validation so only the app-owned .pipeline-tasks/<id> folder is deleted; Undo still works from retained blobs.

**Regression test.** Rust test (cfg(unix)) in studio/tests.rs: after StudioAction::SaveText with checkpoint_id None, assert task_pages::page(&store, &ws, None, true).records is empty (no open task) and that f.root.join(".pipeline-tasks") contains no subdirectory for the checkpoint. Before the fix one open task 'Edit main.tex' and one directory remain.


### 39. Re-importing a package with ledger objects reports every evidence link (and unpackaged-output executions) as an unresolvable conflict because import-rewritten fields are not excluded from the fingerprint

- **Severity:** medium · **Category:** wrong-result · **Location:** [exchange.rs:98](../gui/src-tauri/src/workbench/release/exchange.rs:98) (first reported at `gui/src-tauri/src/workbench/release/exchange.rs:1815`)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** Coauthor A exports a package with includeLedger=true (evidence links have freshness 'current', staleReason null). B imports it into a new Workspace (all 'new'). B re-imports the identical package into that Workspace, as the design doc says is idempotent: preview shows N conflicts for the N evidence links, import records N open exchange_conflicts, and 'Take imported' errors for each. The test import_into_nonempty_store_remaps_is_idempotent_and_records_conflicts does not include ledger objects, so this is untested.

<details><summary>Evidence and trace</summary>

Export: load_object's evidence arm serializes the full EvidenceRecord (exchange.rs:534 `serde_json::to_value(&e)`), which includes `freshness` and `staleReason` (ledger.rs:44-59, camelCase). Import: exchange.rs:1815 inserts evidence_links with the literal `'unknown'` freshness and the fixed stale_reason string (present once in the file; documented in docs/workbench/project-exchange.md:76-77). Re-import: plan() pass 1 finds the row via local_object's direct SQL (exchange.rs:1427-1433 -> "existing"), pass 2 compares `fingerprint(&local) == fingerprint(&value)` (exchange.rs:1484-1488) and fingerprint strips only the keys at exchange.rs:101-115, not freshness/staleReason, so local ('unknown' + reason) != package ('current'/null) -> "conflict" for every evidence link. resolve_conflict's take_imported for evidence hits the immutable-object refusal (exchange.rs:1944-1946). Executions with unpackaged outputs get `a["unavailable"]` added at import (exchange.rs:1755-1756), which the package copy lacks, so the same mismatch applies. docs/workbench/project-exchange.md:72-74 promises idempotent reimport. No test sets include_ledger (only exchange.rs:687 references it).

</details>

**Proposed fix.**

Exclude the import-rewritten bookkeeping from fingerprint():

```rust
                 for key in [
                     "workspaceId", "updatedAt", "createdAt", "revision", "textReference",
                     "storageReference", "cwd", "sessionId", "profileId", "artifactId",
                     "compiledArtifactId", "entrypoint", "paperId",
+                    "freshness", "staleReason", "unavailable",
                 ] {
```
Alternatively, in plan() pass 2 first compare the package value against the stored exchange_bases.value_json for (namespace, kind, source_id) and classify as identical when equal.

**Fix risk.** Low: freshness/staleReason are derived state the importer always overwrites, so ignoring them cannot hide a content change; `unavailable` only appears in output manifests the importer rewrote. fingerprint is pub, so check its other callers still want these keys ignored (they are volatile everywhere).

**Regression test.** Extend the exchange tests: create a claim with one evidence link (freshness 'current'), export with include_ledger=true, import into an empty Workspace, then preview the same package again and assert the evidence decision outcome is "identical" and no exchange_conflicts row is recorded. Before the fix the outcome is "conflict".


### 40. load_object resolves objects through LIMIT/retired-filtered list views, so large or retired objects export as 'unavailable' and re-import aborts on a primary-key collision

- **Severity:** medium · **Category:** wrong-result · **Location:** [exchange.rs:362](../gui/src-tauri/src/workbench/release/exchange.rs:362) (first reported at `gui/src-tauri/src/workbench/release/exchange.rs:363`)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** (a) A Workspace with 620 notes exports with includeNotes=true: 120 notes are missing from the package and listed as 'no longer available' with no error. (b) A researcher exports the ledger, later retires one claim, then imports a coauthor's updated copy of the same package into the same Workspace: plan() marks the retired claim 'new', `INSERT INTO claims` hits `UNIQUE constraint failed: claims.id`, and the entire import fails with 'Project exchange failed' instead of recording a conflict.

<details><summary>Evidence and trace</summary>

index() enumerates ids with unbounded SQL (exchange.rs:264 notes, :296 claims including retired ones) and export queues them all (exchange.rs:696-711). load_object then resolves by scanning bounded list views: notes via research::list_notes (notes.rs:128/130 LIMIT 500), papers via list_papers (papers.rs:524 LIMIT 500), sources (sources.rs:130 LIMIT 500), claims via research_ledger (ledger.rs:343 `workflow_state <> 'retired' ... LIMIT 500`), evidence (ledger.rs:349 LIMIT 1000). A miss returns None and export records 'Referenced object is no longer available in this Workspace' (exchange.rs:705-711) with no error. On import, local_object finds the row's workspace by direct SQL (its match arm for claims at `SELECT workspace_id FROM claims WHERE id=?1`) but then returns `load_object(...)?.map(..)` which is None for a retired claim or a beyond-limit note, so plan() takes the `local.is_none()` branch and classifies it "new" (exchange.rs:1427-1431); import_exchange then runs a plain `INSERT INTO claims(...)` (claim arm just before exchange.rs:1790) which violates the primary key, `.map_err(err)?` propagates and the transaction rolls back, failing the whole import instead of recording a conflict.

</details>

**Proposed fix.**

Load by id instead of scanning list views, and make the planner robust to a row that exists but cannot be loaded. Minimal safe step in local_object:

```rust
     let ix = index(store, &ws)?;
     ...
-    Ok(load_object(store, &ws, &ix, kind, id, &mut scratch)?.map(|(value, _)| LocalObject { workspace_id: ws, value }))
+    let value = load_object(store, &ws, &ix, kind, id, &mut scratch)?
+        .map(|(value, _)| value)
+        .unwrap_or(Value::Null); // row exists; treat as existing (fingerprint mismatch => conflict, never INSERT)
+    Ok(Some(LocalObject { workspace_id: ws, value }))
```
Then replace the list scans in load_object with direct queries (`SELECT ... FROM research_notes WHERE id=?1 AND workspace_id=?2`, claims including retired ones joined to claim_versions, evidence_links by id) so exports include every selected object.

**Fix risk.** Low for the planner change (only affects rows that previously crashed the import). The direct-query rewrite touches export content for notes/papers/sources/claims/evidence; keep field names identical to the list-view records so package fingerprints of existing exports stay stable.

**Regression test.** Exchange test: create a claim, export with include_ledger=true, retire the claim locally, then import the same package into the same Workspace and assert the import succeeds and records one conflict for the claim (before the fix it errors with a UNIQUE/PRIMARY KEY failure). Second test: 520 notes with include_notes=true, assert manifest.objects contains 520 notes and external_references is empty (before the fix 20 are 'no longer available').


### 41. paper_search excerpts span the whole surrounding line, so long-line text papers produce unbounded tool results and receipts

- **Severity:** medium · **Category:** resource-leak · **Location:** [papers.rs:610](../gui/src-tauri/src/workbench/research/papers.rs:610)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** Researcher imports results.json (a 12 MiB minified single-line file with a trailing newline) as a paper with role 'other'. The model calls workbench_paper_search {query:"coef", limit:20}. Each of the 20 hits has excerpt = the entire 12 MiB line -> a ~240 MiB JSON tool response is built in memory, written into tool_receipts.result_json, and pushed over the App Server JSON-RPC pipe, stalling or failing the turn and bloating the private store.

<details><summary>Evidence and trace</summary>

papers.rs:609-614: excerpt_start is the previous newline (or start-160 only when none exists) and excerpt_end is the next newline (or end+160 only when none exists); nothing caps the span when a newline is found, so a single-line document yields the whole line per hit. Text papers accept json/csv/tsv/txt (papers.rs:192) up to MAX_TEXT_BYTES = 16 MiB (research.rs:28), and MAX_SEARCH_RESULTS = 100 (research.rs:29, clamp at papers.rs:603). The dynamic tool path (research/tools.rs:204-213) serializes the Vec<PaperSearchHit> with no truncation, writes it to tool_receipts.result_json (tools.rs:374) and returns it to the App Server; the only size limits in tools.rs are on arguments (:55) and the records tool (:132-139). The UI command commands/research.rs:107-114 uses the same function. Severity is lower than claimed: it needs a long-line file and produces slow/bloated turns rather than wrong results.

</details>

**Proposed fix.**

Clamp the window inside the line and cap the per-call payload in paper_search:

```rust
-        let mut excerpt_start = text[..start].rfind('\n').map_or(start.saturating_sub(160), |v| v + 1);
-        let mut excerpt_end = text[end..].find('\n').map_or((end + 160).min(text.len()), |v| end + v);
+        let line_start = text[..start].rfind('\n').map_or(0, |v| v + 1);
+        let line_end = text[end..].find('\n').map_or(text.len(), |v| end + v);
+        let mut excerpt_start = line_start.max(start.saturating_sub(160));
+        let mut excerpt_end = line_end.min(end + 160);
```
and stop collecting once `hits.iter().map(|h| h.excerpt.len()).sum::<usize>() > 64 * 1024` (returning what fits).

**Fix risk.** Low: excerpts for ordinary prose lines under 320 bytes are unchanged; start/end locators are untouched so anchors remain stable.

**Regression test.** Unit test: import a text revision whose single line is 1 MiB with 'coef' repeated; call paper_search(.., "coef", 20) and assert every excerpt.len() <= 160 + 4 + 160 and the total serialized result is under 64 KiB. Before the fix each excerpt is ~1 MiB.


### 42. Project search returns nothing when the query contains a punctuation-only token such as '&', '-', '=' or an em dash

- **Severity:** medium · **Category:** wrong-result · **Location:** [search.rs:363](../gui/src-tauri/src/workbench/search.rs:363)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** Researcher searches the project for `tariffs & welfare` or `x = y` or pastes a title containing ` - `. search() builds `"tariffs" AND "&" AND "welfare"`; FTS5 returns no rows even though the index contains matching notes, so the Library search reports nothing found.

<details><summary>Evidence and trace</summary>

search.rs:346-363 splits on whitespace, wraps each token as an exact FTS5 phrase (`"&"`), and joins with ` AND `; the MATCH at search.rs:366 runs against research_fts with unicode61. Empirically on this machine (sqlite 3.45.3 CLI; rusqlite bundles a newer SQLite with the same FTS5 semantics): `"tariffs" AND "welfare"` -> 1 row, `"tariffs" AND "&" AND "welfare"` -> 0, `"tariffs" AND "-" AND "welfare"` -> 0. The tokenizer yields an empty phrase for pure-punctuation tokens and an empty phrase matches no rows, so the AND kills the query. Nothing upstream filters tokens (check_text at :333 only bounds length).

</details>

**Proposed fix.**

Drop tokens with no alphanumeric content before building phrases, and return an empty page if nothing remains:

```rust
     let tokens = r.query.split_whitespace()
+        .filter(|s| s.chars().any(char::is_alphanumeric))
         .take(32)
         .map(|s| { ... })
         .collect::<Vec<_>>();
+    if tokens.is_empty() {
+        return Ok(SearchPage { items: Vec::new(), next_cursor: None, ..Default::default() });
+    }
     let query = tokens.join(" AND ");
```

**Fix risk.** Low: only changes queries that previously could never match; keep the cursor identity hash on the raw query so pagination is unaffected.

**Regression test.** In the search tests, index a note containing 'tariffs and welfare', then assert search("tariffs & welfare") and search("tariffs - welfare") each return that note and search("&") returns an empty page without error. Before the fix the first two return zero items.


### 43. Automation-driven turn on the viewed conversation leaves a phantom streaming bubble and never refreshes the transcript

- **Severity:** medium · **Category:** ui-state · **Location:** [useWorkbenchEvents.ts:275](../gui/src/hooks/useWorkbenchEvents.ts:275)
- **Status:** Single pass (sweep agent only, confidence 0.65); not independently verified

**What happens.** A session-scoped automation (Automate > chain with a Workspace step bound to conversation A) fires while the user has conversation A open. Streaming text appears as a live assistant bubble with the pulsing indicator; when the turn completes the bubble stays frozen with the partial markdown, the real message is not shown in the transcript, and the composer remains enabled so a user send collides with the running turn permit.

<details><summary>Evidence and trace</summary>

agentMessageDelta is accepted when `!owner` and the thread matches `selectedSnapshot.activeBinding.providerThreadId` (no active turn required), but the matching turnCompleted branch additionally requires `activeRef.current?.turnId === payload.turnId`. Turns started by the Tasks coordinator on a session-scoped automation resume the same session binding thread (orchestration/adapters.rs Workspace action -> submit_turn(automated) -> resume_thread(active_binding.provider_thread_id)) and emit the same workbench:event stream. `turnStarted` is not handled by this hook, so `active` is never set for such turns. Deltas therefore accumulate into `stream` and render as an in-progress assistant bubble, while turnCompleted neither clears the stream nor reconciles/hydrates, so the finished message never appears until the user switches conversations or sends.

Guards checked: Checked matchesThread fallback to the snapshot's active binding thread (lines 186-189); checked hydrate() only marks `active` for turns already in progress at hydration time (useWorkspacePageController.ts:295-324) — a turn that starts after hydration is never picked up; WorkspaceTaskCards/useProjectIndex listen to turnCompleted but only refresh cards/index, not the conversation snapshot; confirmed in Rust that task turns reuse the session's active binding thread when the harness fingerprint matches (workbench/commands/codex.rs:209-218).

</details>

**Proposed fix.**

```text
Treat an unowned completion on the matched thread as terminal regardless of activeRef, and adopt unowned turnStarted events on the matched thread:

+      if (
+        payload.kind === "turnStarted" &&
+        !owner &&
+        !activeRef.current &&
+        matchesThread(payload.threadId) &&
+        typeof payload.turnId === "string"
+      ) {
+        const turn = { threadId: String(payload.threadId), turnId: payload.turnId };
+        activeRef.current = turn;
+        setActive(turn);
+      }
 ...
         else if (
           !owner &&
           matchesThread(payload.threadId) &&
-          activeRef.current?.turnId === payload.turnId
+          (!activeRef.current || activeRef.current.turnId === payload.turnId)
         ) {
```

**Regression test.** In hooks/useWorkspacePageController.test.ts (listeners harness): hydrate session A with activeBinding.providerThreadId = "thread-A" and no turns; emit agentMessageDelta {threadId:"thread-A", turnId:"turn-X", delta:"hi"} then turnCompleted for the same ids with status completed; assert stream === "" afterwards and reconcileSession/conversationSnapshot were called again for A.


### 44. connectionClosed never clears an active turn restored by hydration, leaving the composer disabled

- **Severity:** medium · **Category:** ui-state · **Location:** [useWorkbenchEvents.ts:202](../gui/src/hooks/useWorkbenchEvents.ts:202)
- **Status:** Single pass (sweep agent only, confidence 0.6); not independently verified

**What happens.** Send a message, open Reviews, come back while it is still running (header shows working, composer disabled — correct). The Codex app-server then crashes or the account is signed out: connectionClosed arrives, nothing clears `active`. The conversation stays locked with "ChatGPT is working…" until the user selects a different conversation and returns.

<details><summary>Evidence and trace</summary>

When the Workspace page is unmounted (navigating to Reviews/Settings) while a turn is running, the submission owner ref is lost. On return, hydrate() restores `active` from the still-running turn (no owner). If the Codex connection then drops, the bridge emits only connectionClosed (no synthesized turnCompleted; see src-tauri workbench/commands/codex.rs:627-639), and this hook only finishes the turn for a matching owner. `active` stays set: the header shows "ChatGPT is working…", the composer and send button are disabled, and Stop calls interruptTurn on a dead connection with its rejection swallowed (WorkspacePage.tsx:509 `void workbenchClient.interruptTurn(...)`). Recovery requires switching conversations.

Guards checked: Checked the turnCompleted branch (272-286) handles the unowned-but-active case only for turnCompleted, not connectionClosed; checked the Rust bridge emits no turnCompleted on ConnectionClosed (close_active_connection resets native state only); checked the [surface, sessionId] effect does not re-run on connection loss; chatgpt:account-changed listener only updates models. Existing test 'clears a restored active turn when reconciliation confirms it ended' covers reconcile on re-entry, not this event.

</details>

**Proposed fix.**

```text
if (owner?.epoch === payload.epoch)
           finishSubmission("connection closed");
+        else if (!owner && activeRef.current) {
+          // A turn adopted from hydration cannot finish once its connection is gone.
+          activeRef.current = null;
+          setActive(null);
+          setStream("");
+          const selected = sessionRef.current;
+          if (selected)
+            void workbenchClient
+              .reconcileSession(selected)
+              .catch(() => false)
+              .finally(() => void hydrate(selected));
+        }

Also give WorkspacePage.tsx:509/906 onStop a `.catch((cause) => setError(workbenchErrorMessage(cause)))` so a failed interrupt is visible.
```

**Regression test.** In useWorkspacePageController.test.ts: hydrate A with activeBinding {id:"b", providerThreadId:"thread-A"} and turns [{bindingId:"b", providerTurnId:"turn-A", terminalAt:null}] so `active` is restored; emit connectionClosed {epoch:1}; assert result.current.active === null and reconcileSession was called for A.


### 45. Superseded binding incarnations keep a full duplicate transcript projection that is never reclaimed

- **Severity:** low · **Category:** resource-leak · **Location:** [runtime.rs:27](../gui/src-tauri/src/workbench/store/runtime.rs:27) (first reported at `gui/src-tauri/src/workbench/store.rs:953`)
- **Status:** Confirmed by a verifier agent (refute-first review)

**What happens.** A 300-item conversation is opened once per day. Each day a new incarnation is bound and 300 new transcript_items rows plus turn/change_log rows are written; after a year the store holds ~110k item rows for one conversation, most invisible. The SQLite file and .pwrx archives grow without bound; transcript_page and conversation_snapshot subqueries scan the growing set.

<details><summary>Evidence and trace</summary>

The 'every app launch' trigger is refuted: supervisor.rs:751 resets `next_epoch` to 1 per process, so a plain relaunch reuses the existing binding (runtime.rs:22-26) and re-projection hits ON CONFLICT (store.rs:953). The leak itself is real: whenever the namespace changes (supervisor reconnect within a launch via commands/codex.rs:323/:637/:669 -> connect() epoch 2 at supervisor.rs:768, and then the next launch's epoch-1 again), runtime.rs:27-33 retires the old binding and inserts a new one, reconcile_snapshot (supervisor.rs:1030-1077) re-projects every turn/item as fresh rows plus a change_log row per turn (runtime.rs:207 operation id includes the epoch), history.rs:71 and views.rs:67 hide older incarnations with NOT EXISTS, and nothing deletes them: release/retention.rs contains no reference to transcript_items/superseded bindings (grep empty). Growth is one full transcript copy per incarnation change, which is bounded by reconnect frequency, hence low severity.

</details>

**Proposed fix.**

After a successful reconcile under a new incarnation (or at the end of bind_session when the superseded binding shares provider_thread_id), prune rows that a newer incarnation already covers:

```rust
// store/runtime.rs, new pub(crate) fn prune_superseded_projection(&self, session_id)
DELETE FROM transcript_items WHERE id IN (
  SELECT i.id FROM transcript_items i JOIN session_bindings b ON b.id=i.binding_id
  WHERE b.session_id=?1 AND b.retired_at IS NOT NULL AND EXISTS (
    SELECT 1 FROM transcript_items n JOIN session_bindings nb ON nb.id=n.binding_id
    WHERE nb.session_id=b.session_id AND n.provider_item_id=i.provider_item_id AND nb.incarnation>b.incarnation));
-- same shape for turns keyed on provider_turn_id
```
called from reconcile_snapshot's run_store closure after the loop. If the re-parenting alternative from #2 is chosen instead, this becomes unnecessary.

**Fix risk.** Low-medium: must run inside the worker/gate path and only touch retired bindings; keep the active binding's rows untouched so the turn permit and views are unaffected. Order it after the #2 fix so the kept rows carry the inherited created_at.

**Regression test.** Store test: bind ns-A, project a 3-item turn, bind ns-B, project the same TurnCompleted, call the prune (or reconcile) and assert `SELECT COUNT(*) FROM transcript_items` equals 3 and transcript_page still returns the 3 items. Before the fix the count is 6.


### 46. Conversation export and Stop failures are swallowed (no error shown)

- **Severity:** low · **Category:** ui-state · **Location:** [WorkspacePage.tsx:693](../gui/src/components/WorkspacePage.tsx:693)
- **Status:** Single pass (sweep agent only, confidence 0.75); not independently verified

**What happens.** User chooses Export conversation, picks a location in a directory without write permission; the backend rejects; nothing appears in the UI and no file is written.

<details><summary>Evidence and trace</summary>

exportCurrent() awaits the native save dialog and workbenchClient.exportConversation; the menu item invokes it as `void exportCurrent()` with no catch, unlike every other conversation action on this page which attaches `.catch((cause) => setError(...))`. A write failure (read-only folder, path error) produces an unhandled rejection and no feedback; the user believes the Markdown export succeeded. The same pattern applies to onStop at lines 509 and 906 (`void workbenchClient.interruptTurn(...)`).

Guards checked: Confirmed exportCurrent (controller lines 735-744) has no internal try/catch; confirmed the MenuItem call site attaches no .catch; no global unhandledrejection handler surfaces errors (grep found none in gui/src).

</details>

**Proposed fix.**

```text
-              <MenuItem onSelect={() => void exportCurrent()}>
+              <MenuItem
+                onSelect={() =>
+                  void exportCurrent().catch((cause) =>
+                    setError(workbenchErrorMessage(cause)),
+                  )
+                }
+              >

and likewise `.catch((cause) => setError(workbenchErrorMessage(cause)))` on both interruptTurn call sites.
```

**Regression test.** In WorkspacePage.test.tsx mock exportConversation to reject and the save dialog to return a path; open the conversation menu, click Export, assert an alert with the error text is rendered.


## Coverage and limits

Scanned with a dedicated agent and verified as described above: workflow
executor and merge/reconcile; supervised calls and the Anthropic, OpenAI, and
Google adapters; Claude and Codex CLI adapters; the Review App Server adapter
and the shared Codex runtime including the managed ChatGPT account; extraction
and document bundles; engines, dependency checks, updates, and storage
selection; orientation, findings, and report output; the immutable run store;
the Review command boundary; workflow configuration and migrations; settings,
credential encryption, and the project ledger; the Workspace command layer,
store, Codex integration, research services, project files and tasks, release
archives, programs and desk; missions and self-discovery; the Tasks
coordinator; the CLI.

Only sampled: the React frontend (one agent read the controller hooks and
grep-swept the rest; the editor, results, project surface, research, and
settings components were not read in full) and the cross-cutting Rust
security/panic/Windows/packaging pass (grep-driven, two findings reported).

Not verified: the 97 lower-ranked candidates in the appendix. They were
reported by the subsystem scans with evidence but no verifier re-examined
them. They are listed so they can be triaged later, not because they are
known to be real.

Not exercised: no model turns, packaged installers, or Windows hosts were run
during this review. Platform claims were established from the code.

## Appendix: unverified candidates

| Area | Candidate | Location | Finder severity | Confidence |
|---|---|---|---|---|
| review-commands | cancel_batch leaves BATCH_CANCEL set when no batch task is alive, cancelling every later foreground run and Task review | [batch.rs:195](../gui/src-tauri/src/commands/batch.rs:195) | medium | 0.45 |
| review-commands | cleanup_print_export() after App::run is unreachable; printed report HTML lingers in the temp dir | [lib.rs:424](../gui/src-tauri/src/lib.rs:424) | low | 0.8 |
| review-commands | Dock icon click cannot restore the hidden main window in background-tasks mode (RunEvent::Reopen unhandled) | [lib.rs:417](../gui/src-tauri/src/lib.rs:417) | low | 0.55 |
| review-commands | Importing a profile whose name slugifies to more than 64 characters fails with a profile-ID validation error | [profiles.rs:336](../gui/src-tauri/src/pipeline_config/profiles.rs:336) | low | 0.5 |
| engines-deps | Installer validation probes are registered in CHILD_PIDS and get SIGKILLed by any unrelated kill_all_children() | [paddle_install.rs:54](../gui/src-tauri/src/engines/paddle_install.rs:54) | low | 0.5 |
| engines-deps | Base-engine install phases are never recorded in the progress snapshot, so a remounted panel shows no progress during the 2 GB base download | [installer_io.rs:69](../gui/src-tauri/src/engines/installer_io.rs:69) | low | 0.6 |
| runs-store | Rerun copies parent bundle assets with fs::copy, which follows symlinks planted inside the model-writable artifacts/ tree | [rerun.rs:533](../gui/src-tauri/src/commands/rerun.rs:533) | medium | 0.55 |
| runs-store | copy_tree applies the 5-second WalkBudget wall-clock limit to a copy that fsyncs twice per file | [reuse_artifacts.rs:13](../gui/src-tauri/src/commands/reuse_artifacts.rs:13) | medium | 0.45 |
| runs-store | recover_resumable_runs parses every completed run's report.json under a 5-second budget; with many runs it aborts and interrupted runs stay stuck as "running" | [history.rs:349](../gui/src-tauri/src/runs/history.rs:349) | medium | 0.45 |
| runs-store | Automatic retention only moves runs to Trash, which is never bounded, so the configured size cap cannot free disk space | [retention.rs:275](../gui/src-tauri/src/runs/retention.rs:275) | medium | 0.45 |
| runs-store | History, retention order and latest-report lookup sort the local-offset RFC3339 `created` string lexically, which misorders runs across DST or timezone changes | [history.rs:437](../gui/src-tauri/src/runs/history.rs:437) | low | 0.6 |
| runs-store | Trash shows the run's last-modified time as its deletion time, so the Trash list is not ordered by when runs were moved | [history.rs:714](../gui/src-tauri/src/runs/history.rs:714) | low | 0.6 |
| direct-api | OpenAI insufficient_quota 429 is classified as transient; usage-limit fallback never triggers | [retry.rs:79](../gui/src-tauri/src/pipeline/api_common/retry.rs:79) | medium | 0.65 |
| direct-api | Gemini functionCall without an args object fails the whole step instead of yielding a tool error | [models.rs:811](../gui/src-tauri/src/pipeline/api_common/models.rs:811) | medium | 0.4 |
| direct-api | Provider API keys (x-api-key, x-goog-api-key) and prompts follow cross-origin redirects on the shared HTTP client | [api_common.rs:28](../gui/src-tauri/src/pipeline/api_common.rs:28) | medium | 0.5 |
| direct-api | PDF Read between 18 and 20 MiB is read and charged, then always rejected by the encoded-media budget | [execution.rs:586](../gui/src-tauri/src/pipeline/api_common/execution.rs:586) | low | 0.6 |
| direct-api | Write tool creates directories through a planted symlinked intermediate before the containment check | [models.rs:369](../gui/src-tauri/src/pipeline/api_common/models.rs:369) | low | 0.6 |
| direct-api | Shared-context warm-up capped at 32 output tokens fails on reasoning/thinking models and silently drops the OpenAI prompt_cache_key | [api_openai.rs:313](../gui/src-tauri/src/pipeline/api_openai.rs:313) | low | 0.4 |
| cli-providers | Deadline-killed CLI calls are reported as 'killed by the system' and forked Claude calls spawn an orphan fallback process after timeout | [claude_cli.rs:756](../gui/src-tauri/src/pipeline/claude/claude_cli.rs:756) | medium | 0.65 |
| cli-providers | Large system prompts / output schemas are passed as single argv strings and exceed OS command-line limits | [claude_cli.rs:431](../gui/src-tauri/src/pipeline/claude/claude_cli.rs:431) | medium | 0.55 |
| cli-providers | Provider child PIDs stay registered when the call future is aborted, so later kills can target a reused PID | [cli_process.rs:197](../gui/src-tauri/src/pipeline/cli_process.rs:197) | low | 0.45 |
| extraction | Staged LaTeX runs never emit the 'read from outside the selected directory' quality note | [latex.rs:670](../gui/src-tauri/src/pipeline/extract/latex.rs:670) | low | 0.6 |
| extraction | Managed llama-server bearer token is passed on the command line | [pdf.rs:149](../gui/src-tauri/src/pipeline/extract/pdf.rs:149) | low | 0.5 |
| extraction | Full-parser cache activation failure leaks the kept staging directory | [paddle_full.rs:191](../gui/src-tauri/src/pipeline/extract/paddle_full.rs:191) | low | 0.6 |
| extraction | Bundle never records the visual-companion PDF origin for staged LaTeX runs | [bundle.rs:63](../gui/src-tauri/src/document_bundle/bundle.rs:63) | low | 0.55 |
| extraction | Quality notes from extra named inputs are discarded | [run.rs:707](../gui/src-tauri/src/commands/run.rs:707) | low | 0.6 |
| codex-runtime | Attempt receipt archive grows without pruning until startup hard-fails all Workflow Codex calls | [journal.rs:47](../gui/src-tauri/src/pipeline/codex_server/journal.rs:47) | medium | 0.6 |
| codex-runtime | A second concurrent token-refresh request closes the whole runtime connection non-retryably | [transport.rs:413](../gui/src-tauri/src/agent_runtime/codex/transport.rs:413) | medium | 0.4 |
| codex-runtime | Write-queue backpressure timeout leaks the pending reply slot | [transport.rs:265](../gui/src-tauri/src/agent_runtime/codex/transport.rs:265) | low | 0.75 |
| codex-runtime | Request issued during connection close waits the full deadline instead of failing fast | [transport.rs:254](../gui/src-tauri/src/agent_runtime/codex/transport.rs:254) | low | 0.5 |
| pipeline-config | Settings backup silently omits any profile that fails current validation | [profiles.rs:213](../gui/src-tauri/src/pipeline_config/profiles.rs:213) | medium | 0.65 |
| pipeline-config | Global default model/effort policies overwrite a step's explicit overrides when the step inherits its agents | [pipeline_config.rs:364](../gui/src-tauri/src/pipeline_config.rs:364) | medium | 0.55 |
| pipeline-config | Legacy pipeline.json/referees.json migration propagates validation errors, bricking ensure_migrated on every launch | [migrations.rs:871](../gui/src-tauri/src/pipeline_config/migrations.rs:871) | medium | 0.45 |
| pipeline-config | Editor receives stock Auto Review defaults when the active profile fails to load, and calibration drafting targets the wrong step | [persistence.rs:74](../gui/src-tauri/src/pipeline_config/persistence.rs:74) | low | 0.6 |
| pipeline-config | Profile names longer than ~64 characters or without ASCII letters cannot be created, duplicated or imported | [profiles.rs:71](../gui/src-tauri/src/pipeline_config/profiles.rs:71) | low | 0.85 |
| orient-findings-output | More than 50 host extraction notes make orientation unsatisfiable, failing the run after three model calls | [orient.rs:546](../gui/src-tauri/src/pipeline/orient.rs:546) | medium | 0.45 |
| orient-findings-output | Undeclared findings product is inferred from a parallel step even after the sequential report step failed | [findings.rs:336](../gui/src-tauri/src/findings.rs:336) | medium | 0.45 |
| orient-findings-output | Report header always reports Model: Automatic / Effort: default regardless of configured role models | [render.rs:26](../gui/src-tauri/src/output/render.rs:26) | low | 0.5 |
| orient-findings-output | Run summary table cells are not escaped, so a `\|` in a step label breaks the Markdown table | [summary.rs:312](../gui/src-tauri/src/output/summary.rs:312) | low | 0.5 |
| orient-findings-output | Adaptive-review settings validation does not check selection_notes bounds against the agent range | [auto_review.rs:141](../gui/src-tauri/src/auto_review.rs:141) | low | 0.4 |
| executor | Preloaded-step wave checkpoints use a post-push ordinal that collides with the next step's wave, misordering recovered reports | [run.rs:244](../gui/src-tauri/src/pipeline/executor/run.rs:244) | low | 0.5 |
| executor | A panicked merge task silently drops the whole multi-agent step from the wave | [merge.rs:499](../gui/src-tauri/src/pipeline/merge.rs:499) | low | 0.4 |
| executor | Revision reconciliation retries non-retryable provider errors, including outcome-unknown submissions | [reconcile.rs:126](../gui/src-tauri/src/pipeline/reconcile.rs:126) | low | 0.55 |
| executor | Partially reserved merge bytes are never released when the merged wave exceeds the run output budget | [run.rs:362](../gui/src-tauri/src/pipeline/executor/run.rs:362) | low | 0.5 |
| executor | Supporting-file selection silently truncates at 500 matches with no log or manifest note | [artifact_context.rs:371](../gui/src-tauri/src/pipeline/executor/artifact_context.rs:371) | low | 0.5 |
| settings | Project restore leaves an invisible trash entry that permanently blocks deleting the project again | [projects.rs:462](../gui/src-tauri/src/projects.rs:462) | medium | 0.55 |
| settings | Ledger 'latest occurrence' and run ordering use lexical comparison of local-offset RFC3339 timestamps | [ledger.rs:853](../gui/src-tauri/src/projects/ledger.rs:853) | medium | 0.55 |
| settings | Issue ledger retains all history without pruning, then permanently fails its own write limits | [ledger.rs:904](../gui/src-tauri/src/projects/ledger.rs:904) | medium | 0.5 |
| settings | merge_project_issues leaves an archived target archived after absorbing live occurrences | [ledger.rs:1095](../gui/src-tauri/src/projects/ledger.rs:1095) | low | 0.45 |
| workbench-commands | Pre-dispatch validation failures in submit_turn tear down the Workspace Codex process as if the turn were ambiguously sent | [codex.rs:257](../gui/src-tauri/src/workbench/commands/codex.rs:257) | medium | 0.55 |
| workbench-commands | send-ack draft clear is single-attempt and its failure (draft_cleared=false) is ignored, so a sent message can persist as the conversation draft | [codex.rs:287](../gui/src-tauri/src/workbench/commands/codex.rs:287) | low | 0.4 |
| workbench-store | More than 500 active conversations makes list_sessions fail, silently emptying the sidebar | [sessions.rs:164](../gui/src-tauri/src/workbench/store/sessions.rs:164) | medium | 0.55 |
| workbench-store | Deleting a conversation leaves its private working-files directory on disk, and retention refuses to remove it | [sessions.rs:287](../gui/src-tauri/src/workbench/store/sessions.rs:287) | low | 0.55 |
| workbench-codex | thread/resume omits cwd so a changed project folder permanently breaks existing conversations | [supervisor.rs:526](../gui/src-tauri/src/workbench/codex/supervisor.rs:526) | medium | 0.55 |
| workbench-codex | Event bridge task never exits after ConnectionClosed, leaking a task and the dead supervisor per reconnect | [codex.rs:627](../gui/src-tauri/src/workbench/commands/codex.rs:627) | low | 0.6 |
| workbench-codex | Native prompt cache is opened before it is checked to be a regular file, so a FIFO or symlink wedges the command | [native_prompts.rs:60](../gui/src-tauri/src/workbench/codex/native_prompts.rs:60) | low | 0.5 |
| workbench-programs-desk | Follow-up whose submission fails after dispatch is stuck in 'attention' forever and consumes queue capacity | [followups.rs:268](../gui/src-tauri/src/workbench/programs/followups.rs:268) | medium | 0.6 |
| workbench-programs-desk | Accepted-file monitors and campaign status fail while any local job runs because the project lock is held for the whole execution | [monitors.rs:68](../gui/src-tauri/src/workbench/programs/monitors.rs:68) | medium | 0.55 |
| workbench-programs-desk | Dataset blob is written non-atomically and a partial file permanently blocks re-import of that content | [mod.rs:299](../gui/src-tauri/src/workbench/data/mod.rs:299) | medium | 0.55 |
| workbench-programs-desk | Experiment tick can launch a grid attempt after the family was cancelled or paused (no state guard in the commit) | [experiments.rs:380](../gui/src-tauri/src/workbench/programs/experiments.rs:380) | low | 0.45 |
| workbench-programs-desk | Dataset import only rejects symlinks, so a FIFO or device path blocks a database worker thread indefinitely | [mod.rs:217](../gui/src-tauri/src/workbench/data/mod.rs:217) | low | 0.5 |
| workbench-programs-desk | IRF figures with more than four series reuse identical line style and gray level, making series indistinguishable | [plot.py:23](../gui/src-tauri/src/workbench/programs/plot.py:23) | low | 0.55 |
| workbench-programs-desk | Wide CSV imports allowed by the parser fail at the desk record size limit after the blob is already stored | [mod.rs:329](../gui/src-tauri/src/workbench/data/mod.rs:329) | low | 0.45 |
| workbench-programs-desk | Symbol title can exceed the desk title limit even though notation and scope individually pass validation | [theory.rs:31](../gui/src-tauri/src/workbench/programs/theory.rs:31) | low | 0.6 |
| workbench-release | .pwrx restore copies the archive database verbatim into the live store, including any triggers/views/extra objects a crafted archive carries | [archive.rs:1084](../gui/src-tauri/src/workbench/release/archive.rs:1084) | medium | 0.5 |
| workbench-release | Importing into a new Workspace creates and commits the Workspace before the plan or transaction can fail, leaving empty orphan Workspaces on every failed attempt | [exchange.rs:1593](../gui/src-tauri/src/workbench/release/exchange.rs:1593) | low | 0.7 |
| workbench-release | list_conflicts returns the newest 500 conflicts of any state, so older open conflicts become invisible and unresolvable once resolved ones accumulate | [exchange.rs:1891](../gui/src-tauri/src/workbench/release/exchange.rs:1891) | low | 0.6 |
| workbench-release | resolve_conflict 'take imported' writes package note bodies without the note validation used everywhere else | [exchange.rs:1937](../gui/src-tauri/src/workbench/release/exchange.rs:1937) | low | 0.45 |
| workbench-research | A completed execution whose declared output exceeds 200 MiB is recorded as outcome_unknown with stdout/stderr discarded and a misleading 'Paper input' message | [execution.rs:691](../gui/src-tauri/src/workbench/research/execution.rs:691) | medium | 0.65 |
| workbench-research | Source-tree paper import silently drops non-UTF-8 and over-budget text files while reporting extraction 'complete' | [papers.rs:367](../gui/src-tauri/src/workbench/research/papers.rs:367) | medium | 0.6 |
| workbench-research | Content-addressed text/PDF blobs are written non-atomically, so a crash leaves a truncated blob that is never rewritten | [papers.rs:442](../gui/src-tauri/src/workbench/research/papers.rs:442) | medium | 0.5 |
| workbench-research | Re-importing a file that matches an existing revision returns that revision but leaves the paper's current revision, title and role unchanged | [papers.rs:479](../gui/src-tauri/src/workbench/research/papers.rs:479) | medium | 0.45 |
| workbench-research | verify_evidence_results forces stale evidence back to 'current' regardless of what the evidence targets | [execution.rs:1304](../gui/src-tauri/src/workbench/research/execution.rs:1304) | medium | 0.45 |
| workbench-research | Launch-time failures before any process starts are recorded as outcome_unknown instead of failed | [execution.rs:909](../gui/src-tauri/src/workbench/research/execution.rs:909) | low | 0.5 |
| workbench-research | Rendered page images are persisted base64-encoded inside tool_receipts, duplicating blobs and growing the SQLite store without retention | [tools.rs:374](../gui/src-tauri/src/workbench/research/tools.rs:374) | low | 0.5 |
| workbench-research | workbench_paper_page truncates the requested page with `as u32`, rendering the wrong page for out-of-range values | [tools.rs:220](../gui/src-tauri/src/workbench/research/tools.rs:220) | low | 0.5 |
| workbench-research | Stata log validation reads every declared .log output fully into memory with no size bound | [execution.rs:668](../gui/src-tauri/src/workbench/research/execution.rs:668) | low | 0.45 |
| workbench-project | inventory_at aborts the whole inventory on one unreadable or vanished entry | [project.rs:638](../gui/src-tauri/src/workbench/project.rs:638) | medium | 0.5 |
| workbench-project | checkpoint() leaks the git worktree/branch and task directory when a later step fails | [tasks.rs:364](../gui/src-tauri/src/workbench/project/tasks.rs:364) | low | 0.6 |
| workbench-project | Missing inventory entries are carried forward forever, so the inventory record grows without bound | [project.rs:715](../gui/src-tauri/src/workbench/project.rs:715) | low | 0.45 |
| workbench-project | Windows SafeRoot reads follow intermediate directory junctions/symlinks outside the project root | [files.rs:157](../gui/src-tauri/src/workbench/project/files.rs:157) | low | 0.5 |
| workbench-project | Journalled file placement requires hard links, so edits fail on exFAT/SMB project folders with a misleading recovery error | [files.rs:327](../gui/src-tauri/src/workbench/project/files.rs:327) | low | 0.5 |
| workbench-project | mutate() persists the operation before validation, so an identical retry returns the 'unknown' placeholder as a success value | [project.rs:869](../gui/src-tauri/src/workbench/project.rs:869) | low | 0.4 |
| missions-discovery | Mission prompt renderer cannot shrink goals, questions or criteria, so a long mission eventually fails to render and is stuck in attention | [prompts.rs:41](../gui/src-tauri/src/orchestration/missions/prompts.rs:41) | medium | 0.6 |
| missions-discovery | Evidence text preview truncates at a byte boundary and silently returns null for any text file over 128 KiB that straddles a multibyte character | [commands.rs:474](../gui/src-tauri/src/orchestration/missions/commands.rs:474) | low | 0.8 |
| missions-discovery | A paper record exceeding 4 MiB makes the discovery save fail and permanently blocks the run | [storage.rs:125](../gui/src-tauri/src/orchestration/discovery/storage.rs:125) | low | 0.4 |
| orchestration | Input signals bypass the 8 MiB progress budget, after which one run stalls every tick and can block coordinator startup | [store.rs:463](../gui/src-tauri/src/orchestration/store.rs:463) | medium | 0.5 |
| orchestration | Review retention pin is orphaned if release_pin fails or the process dies after the completion commit | [mod.rs:541](../gui/src-tauri/src/orchestration/mod.rs:541) | medium | 0.6 |
| orchestration | Action timeout is reported to the researcher as 'Task stopped', hiding the real cause | [mod.rs:405](../gui/src-tauri/src/orchestration/mod.rs:405) | low | 0.7 |
| orchestration | Stopping a task with a running action emits a spurious 'needs attention' notice | [mod.rs:565](../gui/src-tauri/src/orchestration/mod.rs:565) | low | 0.65 |
| cli | CLI never shuts down the shared ChatGPT app server it spawns; orphaned on exit and on Ctrl+C | [cli.rs:283](../gui/src-tauri/src/bin/cli.rs:283) | medium | 0.45 |
| cli | A run that finishes during the interrupt grace period is reported as cancelled and its report discarded | [cli.rs:753](../gui/src-tauri/src/bin/cli.rs:753) | low | 0.7 |
| cli | After the first Ctrl+C, further Ctrl+C presses are swallowed for the rest of the process | [cli.rs:752](../gui/src-tauri/src/bin/cli.rs:752) | low | 0.6 |
| cli | --force overwrites the existing report non-atomically and follows symlinks created after validation | [cli.rs:1132](../gui/src-tauri/src/bin/cli.rs:1132) | low | 0.6 |
| cli | Run id is withheld when the --out / --out-dir write fails, hiding where the completed review was saved | [cli.rs:905](../gui/src-tauri/src/bin/cli.rs:905) | low | 0.7 |
| cli | A symlinked --workflow file is rejected with an ELOOP message instead of a clear reason | [cli.rs:1165](../gui/src-tauri/src/bin/cli.rs:1165) | low | 0.45 |

96 candidates listed.
