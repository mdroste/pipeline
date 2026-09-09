# Archived Pipeline codebase audit report

Date: 2026-07-18

> **Archived audit snapshot.** This document preserves evidence and remediation
> notes for the tree audited on the date above. It is not the current issue
> tracker, release recommendation, supported-version policy, or test report.
> File/line references and toolchain versions are historical. See the current
> repository documentation, current CI, and GitHub issues for current status.

## Executive summary

The audit identified 18 concrete correctness, cancellation, resource-lifecycle, persistence, memory-exhaustion, and tooling defects. Each finding was subsequently reproduced or confirmed against the implementation, fixed in the order listed below, and covered by focused regression tests where practical.

At the end of that remediation pass, all 18 findings were reported resolved and
the then-current Rust/frontend suites, production build, formatting, Clippy,
and whitespace checks passed. Those results do not certify the current tree.

The file-and-line references below record the original audit evidence and may no longer match the post-remediation line numbers.

## Recorded remediation status

| # | Result | Implemented remediation |
|---:|:---:|---|
| 1 | Fixed | Parallel steps are marked terminal after dispatch even when they fail; dependent-step regression coverage added. |
| 2 | Fixed | Zero-match fan-out steps are terminal and unblock dependents; regression coverage added. |
| 3 | Fixed | Runs receive pending manifests immediately, unfinished writers finalize on drop, and legacy orphan directories are recovered. |
| 4 | Fixed | Direct API sends and streamed body reads race against per-pass/run cancellation signals. |
| 5 | Fixed | Timed-out engine installers are killed and reaped before bounded log-reader joins. |
| 6 | Fixed | Managed children use dedicated Unix process groups; provider and extraction subprocesses are cancelled, timed out, drained, and reaped. |
| 7 | Fixed | Named inputs are captured with the run and restored for re-runs; unavailable legacy inputs produce an explicit error. |
| 8 | Fixed | Re-run caches preserve every output per base step, and composite failed IDs are normalized before dependency expansion. |
| 9 | Fixed | Large diffs have bounded fallbacks, identical text has a fast path, and detailed diffs are computed lazily. |
| 10 | Fixed | Hashing, previews, imports, subprocess output, and API responses now use streaming or capped readers. |
| 11 | Fixed | Annotation loading rejects stale results, saves are serialized and flushed, backend writes are atomic, and missing runs are rejected. |
| 12 | Fixed | Run and history IDs use nanosecond/sequence uniqueness with exclusive creation and collision retry. |
| 13 | Fixed | Profile duplication clones the complete profile, including variable declarations and choices. |
| 14 | Fixed | Current-format empty profiles load and round-trip normally. |
| 15 | Fixed | Watch startup uses one captured baseline and retains only the 200 most recent jobs. |
| 16 | Fixed | Bundle imports fully validate IDs, graphs, and active-profile references, and roll back destination files/settings on failure. |
| 17 | Fixed | The declared Rust MSRV is 1.80 and CI is configured to compile with that exact version. |
| 18 | Fixed | Node support is declared and pinned, test storage is deterministic, Rust is formatted and Clippy-clean, and CI enforces the gates. |

## Ordered findings

### 1. P1 — Partial parallel failures can stall the entire pipeline

Failed parallel steps are removed from `remaining`, but only successful outputs are added to `done`. If a downstream step depends on a failed parallel step, its dependency can never become satisfied. This contradicts the code's message that downstream steps will receive incomplete inputs.

Evidence:

- `gui/src-tauri/src/pipeline/executor.rs:60`
- `gui/src-tauri/src/pipeline/executor.rs:107`
- `gui/src-tauri/src/pipeline/executor.rs:127`
- `gui/src-tauri/src/pipeline/executor.rs:801`

Recommended fix:

- Track explicit terminal states: success, failed, and skipped.
- Treat every terminal step as dependency-complete while recording failures separately.
- Add a scheduler regression test with one successful parallel step, one failed parallel step, and a downstream dependent.

### 2. P1 — A fan-out step matching zero files can stall the scheduler

`build_units` returns an empty vector and describes the step as skipped, but the scheduler never records the base step as completed. A downstream step depending on it can therefore never run.

Evidence:

- `gui/src-tauri/src/pipeline/executor.rs:452`
- `gui/src-tauri/src/pipeline/executor.rs:467`

Recommended fix:

- Produce a proper skip result or explicitly mark the base step terminal.
- Emit a skipped pass status.
- Add a zero-match fan-out dependency test.

### 3. P1 — Failed and cancelled runs leak invisible run directories

A run directory and potentially large extraction and page-image artifacts are created before orientation and execution finish. Error paths drop `RunWriter` without writing a manifest. History skips manifest-less directories, while retention only deletes runs returned by history. These directories can therefore accumulate indefinitely.

Evidence:

- `gui/src-tauri/src/commands.rs:383`
- `gui/src-tauri/src/commands.rs:577`
- `gui/src-tauri/src/commands.rs:638`
- `gui/src-tauri/src/commands.rs:686`
- `gui/src-tauri/src/runs.rs:245`
- `gui/src-tauri/src/runs.rs:400`
- `gui/src-tauri/src/runs.rs:520`

Recommended fix:

- Write a pending manifest when a run directory is created.
- Finalize it as done, partial, failed, or cancelled on every exit path.
- Alternatively, remove unfinished directories from `RunWriter::drop`.
- Add retention cleanup for pre-existing orphan directories.

### 4. P1 — Direct API calls are unresponsive to cancellation while in flight

Cancellation is checked only between HTTP requests. Once a request is awaiting a response, cancellation cannot interrupt it, and the request may remain active until the full step timeout. Per-pass cancellation has the same limitation because direct API calls have no child PID to kill.

Evidence:

- `gui/src-tauri/src/pipeline/api_common.rs:597`
- `gui/src-tauri/src/pipeline/api_common.rs:610`
- `gui/src-tauri/src/pipeline/api_common.rs:720`
- `gui/src-tauri/src/pipeline/api_common.rs:733`
- `gui/src-tauri/src/pipeline/api_common.rs:839`
- `gui/src-tauri/src/pipeline/api_common.rs:852`
- `gui/src-tauri/src/commands.rs:1191`

Recommended fix:

- Introduce per-run and per-pass cancellation tokens.
- Use `tokio::select!` around request sending and response decoding.
- Propagate the tokens through provider and tool-use loops.

### 5. P1 — Engine-install timeout handling can hang before killing the child

After `child.wait()` times out, the code awaits the stdout and stderr reader tasks before executing `child.kill()`. Those tasks can wait indefinitely for the still-running process to close its pipes, preventing the kill path from being reached.

Evidence:

- `gui/src-tauri/src/engines.rs:561`
- `gui/src-tauri/src/engines.rs:566`
- `gui/src-tauri/src/engines.rs:570`

Recommended fix:

- Match the timeout result immediately.
- On timeout, kill and reap the child before joining pipe readers.
- Bound the pipe-reader joins with a second timeout.
- Keep the child PID registered until it has been reaped.

### 6. P1 — Unix cancellation can leave descendant processes running

Cancellation sends a signal to `-pid` as though each child owns a process group, but process creation does not establish a new process group. Only the CLI leader is reliably killed; descendant tools can survive. PDF extraction and rendering subprocesses are also not registered for cancellation and have no timeout.

Evidence:

- `gui/src-tauri/src/commands.rs:187`
- `gui/src-tauri/src/pipeline/claude.rs:518`
- `gui/src-tauri/src/pipeline/extract.rs:336`
- `gui/src-tauri/src/pipeline/extract.rs:417`
- `gui/src-tauri/src/pipeline/extract.rs:758`
- `gui/src-tauri/src/engines.rs:348`

Recommended fix:

- Introduce one managed subprocess runner.
- On Unix, launch each child in a dedicated process group.
- Drain stdout and stderr concurrently.
- Apply timeout and cancellation consistently.
- Kill the process tree and reap the leader.
- Use the runner for providers, Poppler, and engine installers.

### 7. P1 — Re-runs silently discard required named inputs

Fresh runs extract and pass declared extra inputs, but run manifests preserve only runtime variables. `rerun_run` accepts no replacement inputs and passes an empty input map. Unknown `{input:key}` placeholders are replaced with empty strings, so a re-run can silently execute the wrong prompt.

Evidence:

- `gui/src-tauri/src/commands.rs:638`
- `gui/src-tauri/src/runs.rs:75`
- `gui/src-tauri/src/commands.rs:784`
- `gui/src-tauri/src/commands.rs:909`
- `gui/src-tauri/src/pipeline/executor.rs:926`

Recommended fix:

- Persist safe input provenance or extracted input copies with each run.
- Require unavailable inputs to be supplied again during a re-run.
- Reject unresolved required input placeholders instead of replacing them silently.

### 8. P1 — Partial re-runs collapse multi-agent and fan-out outputs

Preloaded outputs are stored as one `StepOutput` per base ID. Later units overwrite earlier units, so a re-run can reuse only one member of a multi-agent or fan-out step. “Only failed” also seeds composite IDs such as `step/agent`, while dependency and preload logic uses base IDs; a failed unit may not be rerun at all.

Evidence:

- `gui/src-tauri/src/commands.rs:841`
- `gui/src-tauri/src/commands.rs:848`
- `gui/src-tauri/src/commands.rs:858`
- `gui/src-tauri/src/pipeline/executor.rs:45`
- `gui/src-tauri/src/pipeline/executor.rs:777`

Recommended fix:

- Preserve a vector of outputs per base step, or key cached outputs by complete unit ID.
- Normalize failed IDs consistently before dependency expansion.
- Add re-run tests for multi-agent, fan-out, and partially failed steps.

### 9. P1 — Run comparison can freeze or crash the webview

The line-diff implementation allocates an `(n + 1) × (m + 1)` JavaScript matrix. The comparison page computes this diff for every step during render, including collapsed steps. Large model outputs can therefore require hundreds of megabytes or more and block the UI thread.

Evidence:

- `gui/src/lib/diff.ts:10`
- `gui/src/lib/diff.ts:17`
- `gui/src/components/ComparePage.tsx:156`

Recommended fix:

- Replace the LCS matrix with Myers, patience diff, or another bounded-memory implementation.
- Compute a detailed diff only for the expanded step.
- Memoize results and provide an oversized-input fallback.
- Add a stress test using large step outputs.

### 10. P2 — Several size limits are enforced only after full buffering

File hashing reads the complete source file, artifact display reads the complete artifact before truncating, URL import buffers the complete response before enforcing its 10 MB limit, and API response bodies have no explicit size ceiling. Large or hostile inputs can exhaust process memory.

Evidence:

- `gui/src-tauri/src/pipeline/extract.rs:17`
- `gui/src-tauri/src/pipeline/extract.rs:336`
- `gui/src-tauri/src/runs.rs:292`
- `gui/src-tauri/src/runs.rs:606`
- `gui/src-tauri/src/commands.rs:1794`
- `gui/src-tauri/src/pipeline/api_common.rs:620`
- `gui/src-tauri/src/pipeline/api_common.rs:745`
- `gui/src-tauri/src/pipeline/api_common.rs:861`

Recommended fix:

- Stream file hashes in fixed-size chunks.
- Use capped readers for artifact previews and imported profiles.
- Reject oversized `Content-Length` values early and stop chunked responses after `limit + 1` bytes.
- Cap successful and error API response bodies.

### 11. P2 — Annotation navigation can overwrite another run or lose edits

`loadedRef` is not reset when `runId` changes, old annotation state is not cleared, and asynchronous loads have no stale-response guard. A debounced save can therefore write the previous run's annotations into the new run. Pending edits in the final 600 ms before unmount are discarded. The backend can also recreate a deleted run as an invisible annotation-only directory.

Evidence:

- `gui/src/components/IssuesTable.tsx:38`
- `gui/src/components/IssuesTable.tsx:45`
- `gui/src/components/IssuesTable.tsx:65`
- `gui/src-tauri/src/runs.rs:507`

Recommended fix:

- Reset load state and annotations whenever `runId` changes.
- Ignore stale asynchronous results.
- Serialize saves per run and flush pending edits on unmount.
- Require an existing run manifest before writing annotations.
- Write annotations atomically.

### 12. P2 — Run IDs can collide and reuse an existing artifact directory

Run IDs have only second-level precision, and `create_dir_all` accepts an existing directory. Rapid retries of the same input can mix or overwrite artifacts. Legacy history filenames use the same precision.

Evidence:

- `gui/src-tauri/src/commands.rs:385`
- `gui/src-tauri/src/commands.rs:871`
- `gui/src-tauri/src/runs.rs:246`
- `gui/src-tauri/src/storage.rs:23`

Recommended fix:

- Add a UUID, random suffix, or nanosecond timestamp.
- Create run directories exclusively and retry on collision.
- Apply the same uniqueness rule to legacy report files.

### 13. P2 — Duplicating a profile drops its variable declarations

The duplicate operation copies steps, merge settings, orientation settings, extraction settings, and the context template, but does not copy `source.variables`. Prompts can therefore silently lose required runtime variables.

Evidence:

- `gui/src-tauri/src/pipeline_config.rs:1385`
- `gui/src-tauri/src/pipeline_config.rs:1396`

Recommended fix:

- Clone the complete source profile and change only its name.
- Add a test asserting equality of every profile field after duplication.

### 14. P2 — Empty profiles can be saved but cannot be loaded

Saving accepts an empty step list. Loading recognizes the current profile format only if `steps` is nonempty, then falls through to legacy parsing and reports an error. The application can subsequently fall back to defaults, which resembles data loss.

Evidence:

- `gui/src-tauri/src/pipeline_config.rs:1119`
- `gui/src-tauri/src/pipeline_config.rs:1124`
- `gui/src-tauri/src/pipeline_config.rs:1288`

Recommended fix:

- Detect the current format structurally or add an explicit format version.
- Allow zero-step profiles consistently.
- Add an empty-profile round-trip test.

### 15. P2 — Watch-folder state grows without bound and has a startup race

Every completed job is retained in `WATCH_STATE.processed`, and the full vector is cloned and emitted after each addition. A long-running watcher therefore grows backend memory, event payloads, and frontend rendering work indefinitely. Files added between the initial validation scan and the asynchronous baseline scan are treated as pre-existing and never processed.

Evidence:

- `gui/src-tauri/src/commands.rs:110`
- `gui/src-tauri/src/commands.rs:1342`
- `gui/src-tauri/src/commands.rs:1351`
- `gui/src-tauri/src/commands.rs:1366`
- `gui/src-tauri/src/commands.rs:1411`

Recommended fix:

- Use the first successful scan as the actual baseline passed into the watcher task.
- Retain only a bounded deque of recent jobs.
- Store aggregate totals separately.
- Paginate or summarize frontend history.

### 16. P2 — Bundle import is incompletely validated and non-transactional

Bundle import validates unique step IDs but not dependencies, profile IDs, or whether the requested active profile exists. Profiles are saved sequentially, so a later error can leave a partial import despite the comment promising otherwise.

Evidence:

- `gui/src-tauri/src/pipeline_config.rs:1636`
- `gui/src-tauri/src/pipeline_config.rs:1645`
- `gui/src-tauri/src/pipeline_config.rs:1651`
- `gui/src-tauri/src/pipeline_config.rs:1685`

Recommended fix:

- Validate every profile ID, dependency graph, setting, and active-profile reference before writing.
- Stage imported files in a temporary directory.
- Commit the complete import atomically or roll back on error.

### 17. P2 — The declared Rust minimum version is incorrect

The manifest claims Rust 1.77, but the code uses `std::sync::LazyLock`, which requires Rust 1.80. Clippy's MSRV check reports the incompatibility.

Evidence:

- `gui/src-tauri/Cargo.toml:7`
- `gui/src-tauri/src/output.rs:6`
- `gui/src-tauri/src/pipeline/api_common.rs:5`

Recommended fix:

- Raise `rust-version` to at least 1.80, or replace `LazyLock` with a compatible alternative.
- Add a CI job that compiles with the declared MSRV.

### 18. P3 — Local validation is not reproducible across supported-looking toolchains

Under the local Node 25 runtime, seven UpdateBanner tests fail because `localStorage` is not the expected jsdom implementation. `package.json` declares no Node engine, while CI pins Node 20. `cargo fmt --check` also fails extensively, and strict Clippy reports 33 warnings. Formatting and Clippy are not enforced by the current test job.

Evidence:

- `gui/package.json:1`
- `.github/workflows/build.yml:46`
- `.github/workflows/build.yml:60`

Recommended fix:

- Declare and document supported Node versions.
- Add a version file such as `.nvmrc` or `.node-version`.
- Make the test setup provide a deterministic jsdom `localStorage`.
- Format the Rust tree and resolve strict Clippy findings.
- Add `cargo fmt --check`, strict Clippy, and an MSRV build to CI.

## Validation results

- Rust tests: 218 passed, 0 failed.
- Frontend tests: 128 passed across 23 files, 0 failed.
- Frontend TypeScript check and production build: passed.
- Rust formatting check: passed.
- Strict Clippy across all targets with warnings denied: passed.
- Git whitespace/error check: passed.
- The Rust 1.80 check was not executed locally because that toolchain is not installed; the dedicated CI job performs `cargo check --locked` with Rust 1.80.0.
- Vite still reports a non-failing bundle-size warning: approximately 925 KB minified and 277 KB gzip. This is a performance optimization opportunity, not one of the verified correctness defects in this report.
- Production dependency advisories were not queried locally because `cargo-audit` was unavailable and network access was restricted. CI retains Cargo and npm dependency-audit jobs.

## Completed repair sequence

1. Added regression tests for scheduler terminal states, zero-match fan-out, and partial failure.
2. Added cancellation signals and consistent managed-subprocess lifecycle handling.
3. Made run persistence lifecycle-safe and recovered pre-existing orphan directories.
4. Preserved named inputs and every multi-agent or fan-out output during re-runs.
5. Bounded file, network, artifact, subprocess-output, and diff memory usage.
6. Repaired annotation, profile, bundle-import, and watcher state handling.
7. Aligned Rust and Node toolchains and enforced formatting, Clippy, MSRV, and tests in CI.

## Completion criteria

- [x] A failed or skipped step always reaches a terminal scheduler state.
- [x] Cancelling a run or pass interrupts managed work and cleans up child processes.
- [x] Every created run directory is visible through a manifest, including recovered legacy orphans.
- [x] A re-run preserves required inputs and all outputs from multi-unit steps, or fails explicitly when an old run lacks required captured input.
- [x] User-controlled file and network inputs have enforced streaming size limits.
- [x] Switching, deleting, or closing a run cannot cross-write or silently discard pending annotations.
- [x] Empty and duplicated profiles round-trip without losing fields.
- [x] Long-running watch-folder sessions retain bounded history.
- [x] CI enforces tests, formatting, strict Clippy, the declared Rust MSRV, and the declared Node version range.
