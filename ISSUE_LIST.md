# Archived Pipeline pre-release issue list

**Audit date:** 2026-07-22  
**Audited tree:** the working tree as it existed on 2026-07-22, including its
then-uncommitted release and model-catalog changes

**Historical release recommendation:** all findings below were reported remediated in that working tree, subject to the tagged-SHA quality gate and four-platform release checks.

> **Archived audit snapshot.** This is not the current issue tracker or a
> release checklist. It preserves original evidence, proposed fixes, and the
> remediation record; line numbers, dependency versions, test counts, and
> “current working tree” references are historical. See
> [CHANGELOG.md](CHANGELOG.md), [SUPPORT.md](SUPPORT.md),
> [RELEASING.md](RELEASING.md), current CI, and GitHub issues for present
> status.

## How to use this list

- **Phase 1 — release blockers:** credible paths to hangs, orphaned processes, data loss, unsafe execution, disk exhaustion, or an untested release.
- **Phase 2 — high-priority correctness:** major workflow or cross-platform behavior that can silently produce the wrong result or make a core feature unreliable.
- **Phase 3 — reliability and diagnostics:** user-visible failures, misleading status, performance cliffs, and missing provenance.
- **Phase 4 — hardening:** bounded-resource and maintainability work that should be scheduled even if it is not allowed to delay 1.0.

The order within each phase is the recommended implementation order. Line numbers identify the audited revision and may move as fixes land.

## Original audit baseline (pre-fix)

The audit covered the Rust/Tauri backend, React/TypeScript frontend, workflow/profile schema, provider adapters, extraction and artifact paths, persistence, batch/watch behavior, managed-engine installer, and build/release workflows. The following checks passed on the macOS audit host:

- `cargo test --all-targets` — 257 tests passed.
- `npm test -- --run` — 128 tests passed across 23 files.
- `npm run test:release` — 11 release-metadata tests passed.
- `cargo clippy --all-targets --all-features -- -D warnings` passed.
- `cargo fmt --check`, `cargo check --locked --all-targets`, `npm run build`, `npm ls --omit=dev --all`, and `git diff --check` passed.
- The frontend production build warned that its main JavaScript chunk is 933 kB (279 kB gzip).

At audit time, the local `npm audit` could not reach the registry in the sandbox and `cargo-audit` was not installed locally. CI defined both audits, but the release workflow was not gated on them; that gap was P1-10. Windows and Linux findings below came from code-path and packaging review, not live execution on those hosts.

## Recorded remediation status (2026-07-22)

Every item was re-read against the implementation during the 2026-07-22 pass,
and all 30 were recorded as fixed in that remediation tree. The original
evidence and required-fix text remains below as the audit record; line numbers
refer to the pre-fix revision. This statement has not been reissued as a
current-tree audit.

| ID | Disposition | Implemented verification/fix |
|---|---|---|
| P1-01 | Confirmed — fixed | Regular-file/no-follow ingestion, bounded folder walks and hashing, cancellation, FIFO/symlink-cycle tests. |
| P1-02 | Confirmed — fixed | Discovery children are registered, bounded, killed and reaped on timeout/cancellation; output is drained under caps. |
| P1-03 | Confirmed — fixed | Shared bounded probe runner adds timeout, process-tree cleanup, drain timeout, and stdout/stderr caps. |
| P1-04 | Confirmed — fixed | Run-critical loads fail closed; raw encrypted settings fields survive unrelated saves and active-profile edits. |
| P1-05 | Confirmed — fixed | Profile deletion stages a tombstone and rolls back if reference updates fail. |
| P1-06 | Confirmed — fixed | Semaphore waits, model resolution, merge calls, retry boundaries, and PID registration are cancellation-aware. |
| P1-07 | Confirmed — fixed | Unix cancellation signals only currently registered ownership and no longer schedules a delayed PID-only kill. |
| P1-08 | Confirmed — fixed | Central validation rejects executable/unknown tools and agents, caps cost/resource fields, and import UI reviews capabilities/call counts before activation; URL imports stay inactive. |
| P1-09 | Confirmed — fixed | Finalization rejects special/hard-linked/excess files and a live 250 ms monitor cancels CLI passes that exceed file/count/byte quotas. |
| P1-10 | Confirmed — fixed | Tagged SHA must pass audits, release tests, Rust/frontend tests, fmt, Clippy, MSRV, and four platform jobs; release stays draft. |
| P2-01 | Confirmed — fixed | Typed `merge_group`/`fan_out_item` metadata replaces slash inference; fan-out items remain distinct. |
| P2-02 | Confirmed — fixed | Fan-out now builds the full item × agent Cartesian product and merges agents only within each item. |
| P2-03 | Confirmed — fixed | Fully failed waves return failures as data, preserve prior output, and finalize a partial run. |
| P2-04 | Confirmed — fixed | `uv` streams to a capped temp archive, verifies checksum, extracts under a cap, fsyncs/version-probes staging, then atomically commits; cancellation interrupts network waits. |
| P2-05 | Confirmed — fixed | API reads use no-follow regular handles off-runtime; writes are staged atomically and quota is committed only after success. |
| P2-06 | Confirmed — fixed | All CLI readers keep draining after overflow, retain no bytes beyond the cap, and fail before parsing truncated output. |
| P2-07 | Confirmed — fixed | Batch captures one validated settings/profile snapshot and records one shared fingerprint on every job. |
| P2-08 | Confirmed — fixed | Extractors reuse the hardened typed resolver; arbitrary Windows batch files fail closed. |
| P2-09 | Confirmed — fixed | Zero matches emit `skipped` and persist a deterministic placeholder output. |
| P3-01 | Confirmed — fixed | Glob regex compiles once on a blocking task with cancellation, exclusions, and explicit time/entry/match limits. |
| P3-02 | Confirmed — fixed | Editor load errors render details with Retry and Close actions. |
| P3-03 | Confirmed — fixed | Run changes synchronously clear manifest, selection, content, and errors before loading the new default artifact. |
| P3-04 | Confirmed — fixed | Codex uses bounded `login status`; ambiguous Gemini auth is unknown; local readiness requires a bounded `/v1/models` response. |
| P3-05 | Confirmed — fixed | Retry and merge duration/tokens/attempts are accumulated and persisted, including failed-merge fallback metadata. |
| P3-06 | Confirmed — fixed | Normal and report-file-recovery success share one provenance builder. |
| P3-07 | Confirmed — fixed | Engine disk walk skips symlinks/out-of-root paths, tracks visited directories, and has entry/time caps. |
| P3-08 | Confirmed — fixed | Release matrix runs Rust and frontend tests on Linux, Windows, macOS arm64, and macOS x64. |
| P4-01 | Confirmed — fixed | Retention enforces count and byte ceilings, purges oldest completed runs, and excludes active runs. |
| P4-02 | Confirmed — fixed | Major pages and Markdown/KaTeX/highlight views are lazy chunks; largest build chunk is now under 300 kB. |
| P4-03 | Confirmed — fixed | Named backend constants and one validator cover profile bytes, steps, prompts, schemas, tools, agents, fan-out, variables, and inputs at persistence/import boundaries. |

Post-fix local verification: `cargo test --all-targets` (280 tests), `cargo clippy --all-targets --all-features -- -D warnings`, `cargo fmt --check`, frontend tests (128), frontend production build, release metadata tests, and `git diff --check`. Windows/Linux runtime behavior remains gated by the new platform CI jobs rather than claimed from the macOS host.

---

## Phase 1 — release blockers

### P1-01 — Folder and special-file ingestion can hang forever

**Severity:** Critical  
**Platforms:** all; symlink cycles and FIFOs are most readily exploitable on macOS/Linux  
**Evidence:** `gui/src-tauri/src/pipeline/extract.rs:1601-1672`, `gui/src-tauri/src/commands.rs:716-725`, `gui/src-tauri/src/commands.rs:1120-1145`

`ingest_folder` performs a synchronous depth-first walk, follows directory symlinks through `Path::is_dir`, has no visited-directory set, and caps only collected files—not directories or examined entries. A symlink cycle can therefore loop indefinitely. Non-directory entries are accepted without requiring a regular file, then opened and hashed to EOF; a FIFO/device can block forever, and a very large tree or file has no aggregate byte/time budget. The function is called directly from async run code for both primary and named folder inputs, so it can also monopolize a Tokio worker and make cancellation/UI events appear dead.

**Required fix:** use no-follow metadata, accept only regular files, track visited directory identities/canonical paths, cap directories/entries/total bytes, make hashing cancellation-aware, and move the complete walk/hash operation to a blocking worker with a deadline. Apply the same regular-file rule to single-document hashing and LaTeX reads.

**Regression coverage:** symlink cycle, FIFO, symlink-to-file, deep directory-only tree, huge sparse file, aggregate byte cap, and cancellation during hashing on Unix; equivalent junction/reparse-point cases on Windows.

### P1-02 — Model discovery timeouts leave provider processes running

**Severity:** Critical  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/model_catalog.rs:584-660`

`cli_version` puts `Command::output()` inside `tokio::time::timeout`. Dropping the timed-out future does not establish ownership-based process cleanup, so a hung provider can survive. `rpc_exchange` similarly returns through timeout, write, parse, and EOF `?` paths before reaching its final `child.kill()`. These discovery children are not registered with Pipeline's PID/job/process-group lifecycle. Its `BufReader::lines()` also allocates an unbounded line before parsing it, so a broken or hostile CLI can consume arbitrary memory.

**Required fix:** give every discovery child `kill_on_drop(true)` plus an RAII owner that always kills and reaps the process tree; register it with the existing cancellation/job machinery; bound stdout/stderr records and total capture; avoid early returns that bypass cleanup.

**Regression coverage:** provider that never exits, provider that never answers RPC, provider that closes stdin, oversized unterminated line, malformed JSON stream, and a child that spawns a descendant. Assert no process remains after each error.

### P1-03 — Startup and engine probes have no timeout or output bound

**Severity:** Critical  
**Platforms:** all; login-shell path discovery affects macOS/Linux  
**Evidence:** `gui/src-tauri/src/env.rs:60-93`, `gui/src-tauri/src/deps.rs:328-393`, `gui/src-tauri/src/deps.rs:510-575`, `gui/src-tauri/src/engines.rs:272-303`, `gui/src-tauri/src/engines.rs:455-511`, `gui/src/App.tsx:180-191`

PATH discovery runs the user's interactive login shell with `.output()` and no deadline. Dependency checks run provider `--version` and Claude auth commands the same way. Managed-engine status and the existing `uv --version` probe do likewise. Any shell startup file or provider/uv executable that hangs—or emits unbounded output—can pin the startup check indefinitely and keep the main Run action disabled. `spawn_blocking` moves the hang off the async runtime but does not terminate it.

**Required fix:** centralize a bounded probe runner with a short deadline, capped stdout/stderr, process-tree cleanup, and explicit `unknown/timed out` results. Startup readiness must degrade gracefully instead of gating the application forever.

**Regression coverage:** hanging login shell, hanging CLI, output flood, descendant process, invalid UTF-8, and one failed probe while other providers remain usable on each supported OS.

### P1-04 — Fallback settings loads can overwrite settings and encrypted API keys

**Severity:** Critical (data loss and unintended provider/profile selection)  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/settings.rs:463-549`, `gui/src-tauri/src/settings.rs:606-708`, `gui/src-tauri/src/commands.rs:676-704`

`load_with_warnings` substitutes all defaults after read/size/UTF-8 failures and substitutes empty API keys after encryption-key or decryption failures. `set_active_profile`, `replace_active_profile_if`, and `save_preserving_active` discard those warnings and save the fallback value. Merely switching/deleting a profile can therefore replace valid settings or encrypted credentials with defaults/empty values. Run startup also uses the fallback silently, so a transient settings failure can run a different profile/provider than the user selected.

Invalid JSON is quarantined, which preserves a recoverable copy, but subsequent mutation still writes a new default settings file. Other read and decryption failures do not even get that recovery boundary.

**Required fix:** introduce a fallible mutation load that refuses to save on any read/decrypt warning; preserve raw encrypted fields when plaintext decryption is unavailable; make run startup fail closed with an actionable settings error rather than silently changing behavior.

**Regression coverage:** oversized settings, unreadable file, invalid UTF-8/JSON, missing or mismatched encryption key, failed quarantine, and concurrent profile/settings mutations.

### P1-05 — Profile deletion is not transactional

**Severity:** High (irreversible user-data loss)  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/pipeline_config.rs:1791-1803`

`delete_profile` removes the profile file before updating `settings.active_profile`. If acquiring/writing the settings lock, loading the encryption key, or persisting settings fails, the command returns an error after the profile is already gone. The active profile may then point to a nonexistent file and all subsequent runs fail.

**Required fix:** perform deletion as a transaction: rename the profile to a recoverable temporary/trash path, update the active pointer, then commit deletion; roll back the rename on failure. Alternatively update the pointer first and roll it back if deletion fails. Keep a recoverable backup for user-created profiles.

**Regression coverage:** injected failure at every filesystem/settings step and concurrent profile switches from another window/process.

### P1-06 — Per-pass cancellation does not reliably cancel queued or merge work

**Severity:** High (unwanted cost and long-running work after user cancellation)  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/pipeline/executor.rs:735-786`, `gui/src-tauri/src/pipeline/merge.rs:103-160`, `gui/src/components/PipelineProgress.tsx:188-204`

A parallel unit marked `running` can wait on `Semaphore::acquire()` without selecting on its pass-cancellation signal. It notices cancellation only after another potentially long provider call releases a permit. Merge passes are displayed with cancel buttons and emit keys such as `merge/technical`, but merge calls are not scoped through `logging::with_pass`; their PID is therefore never attributed to that key. Merge code checks only global cancellation, not `is_pass_cancelled`, so the advertised cancel action is ineffective until timeout/completion.

**Required fix:** make semaphore acquisition and model resolution cancellation-selectable; do not emit `running` until a unit owns capacity (or expose a distinct queued state); wrap merge calls in the pass scope and check pass cancellation before/after every await and retry boundary.

**Regression coverage:** cancel while queued, during model discovery, during direct HTTP, during CLI execution, and during merge; verify no retry and no billable call starts afterward.

### P1-07 — Delayed Unix SIGKILL can target an unrelated reused PID/process group

**Severity:** Critical (can kill an unrelated user process)  
**Platforms:** macOS/Linux  
**Evidence:** `gui/src-tauri/src/commands.rs:479-506`

`kill_process` sends SIGTERM and then launches a detached thread that sleeps 750 ms before unconditionally sending SIGKILL to both `pid` and `-pid`. The original child can exit and be reaped during that window; if the PID or process-group ID is reused, Pipeline can kill an unrelated process. The detached killer has no owned child identity to validate.

**Required fix:** tie escalation to an owned child/process-group object and poll/wait that exact child before escalating. Use stronger identity primitives where available (for example, Linux pidfds) and never signal after ownership has been released.

**Regression coverage:** child exits immediately on SIGTERM, descendant outlives leader, PID-owner lifecycle race, repeated cancellation, and application shutdown.

### P1-08 — Shared profiles are executable capability/cost documents but are imported without validation or review

**Severity:** Critical (unsafe command execution and unbounded model spend)  
**Platforms:** all; provider behavior differs by OS and CLI  
**Evidence:** `gui/src-tauri/src/pipeline_config.rs:1496-1585`, `gui/src-tauri/src/pipeline_config.rs:1961-1993`, `gui/src-tauri/src/commands.rs:2647-2668`, `gui/src/components/PipelinePage.tsx:929-956`, `gui/src-tauri/src/pipeline/claude.rs:336-410`, `gui/src-tauri/src/pipeline/codex.rs:45-135`

Profile validation checks IDs, dependencies, and a few extraction fields but does not allowlist providers/tools or bound step count, agent count, prompt size, or fan-out maximum. URL/file imports are saved immediately without a capability/cost review. The editor exposes only `WebSearch`, so imported hidden values such as `Bash` are not visible in the normal tool UI. Claude receives arbitrary imported tool names through `--allowedTools` under noninteractive `acceptEdits`; a `Bash` tool bypasses the edit-only path policy. Codex ignores the tool allowlist except for choosing its write sandbox and explicitly permits unrestricted reads, so the same profile has materially different capabilities by provider. A profile can also request thousands of fan-out units or steps and generate a large sequence of paid calls.

**Required fix:** define and enforce a versioned capability schema, provider-specific allowlists, and strict resource caps. Treat imported prompts/tools/agents/network/shell/fan-out as untrusted and show an explicit review before activation. Shell access should require a separate local opt-in and should never be enabled merely by profile JSON. Make unsupported cross-provider semantics a validation error, not a silent fallback.

**Regression coverage:** imported `Bash`/unknown tools, unknown agents, oversized step graph, extreme fan-out, hidden tool round-trip through the editor, and one capability-matrix test per provider/transport/platform.

### P1-09 — Model-written artifacts can hang finalization and escape disk quotas

**Severity:** Critical (hang and disk exhaustion)  
**Platforms:** all; FIFO/device hang is primarily Unix  
**Evidence:** `gui/src-tauri/src/runs.rs:259-285`, `gui/src-tauri/src/runs.rs:413-493`, `gui/src-tauri/src/commands.rs:535-604`

`register_unlisted` stops walking as soon as 500 files have been registered. Files after that point are neither indexed nor deleted, so a CLI agent can leave an arbitrary number/size of files in the run directory. The walk rejects symlinks but does not require a regular file. A FIFO with metadata length zero reaches `register_existing`, whose `inspect_file` opens and reads to EOF, hanging run finalization. Quotas are checked only after model execution, so the filesystem can fill before reconciliation begins. Retention is run-count-based rather than byte-based.

**Required fix:** enforce write/disk quotas during execution; traverse and clean all entries independently of the registration cap; accept only no-follow regular files; bound inspection by declared and actual bytes; cap total run and global run-storage bytes; surface cleanup failures.

**Regression coverage:** more than 500 files with large files after the cutoff, FIFO/socket/device, hard links, sparse files, quota exceeded during execution, and low-disk simulation.

### P1-10 — Tagged releases are not gated on tests, lint, or dependency audits

**Severity:** High (release-process defect)  
**Platforms:** all release targets  
**Evidence:** `.github/workflows/release.yml:1-735`, `.github/workflows/build.yml:1-108`

The tag-triggered release matrix builds and uploads draft artifacts but never runs the Rust/frontend suites, formatting, Clippy, MSRV check, `cargo audit`, or `npm audit`. Those checks live in a separate push/PR workflow and are not a dependency of the tag job. A tag can therefore create manually publishable artifacts from a commit whose checks failed or never ran.

**Required fix:** call a reusable required-check workflow from the release workflow (or require successful checks for the exact tagged SHA through a protected publish job). Create/publish the release only after application tests, audits, and all four platform jobs succeed. Keep artifact creation draft-only until the final gate.

**Regression coverage:** intentionally failing Rust test, frontend test, audit, and one platform packaging job must each prevent a publishable release.

---

## Phase 2 — high-priority correctness and platform fixes

### P2-01 — Fan-out outputs are misidentified as multi-agent outputs and silently collapsed

**Severity:** High  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/pipeline/executor.rs:191-205`, `gui/src-tauri/src/pipeline/executor.rs:704-724`, `gui/src-tauri/src/pipeline/merge.rs:13-84`

The executor decides that a wave is multi-agent when any `StepOutput.step_id` contains `/`. Fan-out units use the same `step/suffix` key shape as multi-agent units, so any multi-item fan-out activates the multi-agent merge when merge is enabled. `merge_step_outputs` then groups every item under the base step ID and asks the model to merge them as if they were independent agent reports. File/item identity is largely lost, labels are misleading, and outputs that should remain one-per-item become one output.

**Required fix:** represent unit kind, agent dimension, and fan-out item as typed fields rather than parsing a slash-delimited ID. Merge only the agent dimension. If fan-out reduction is desired, make it a separate explicit workflow operation.

**Regression coverage:** one agent × many files, many agents × one file, many agents × many files, merge enabled/disabled, and item labels containing duplicate basenames.

### P2-02 — Fan-out with multiple selected agents silently ignores every agent except the first

**Severity:** High  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/pipeline/executor.rs:602-650`, `gui/src-tauri/src/pipeline_config.rs:1496-1511`

`build_units` selects `step.agents.first()` before entering the fan-out branch and creates only one unit per file. Validation rejects sequential multi-agent and sequential fan-out, but allows a parallel step to combine fan-out with multiple agents. The saved profile therefore promises a valid configuration while silently dropping all but the first agent.

**Required fix:** either create the agent × item Cartesian product with explicit grouping/reduction semantics, or reject the combination during save/import with a clear explanation.

**Regression coverage:** two agents with two items must produce the documented number and grouping of calls/outputs.

### P2-03 — A fully failed later parallel wave discards earlier successful work

**Severity:** High  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/pipeline/executor.rs:167-190`, `gui/src-tauri/src/pipeline/executor.rs:997-1022`

`run_parallel_wave` returns `Err` whenever the current wave has failures and zero results. `execute_steps` propagates that error with `?`. If an earlier dependency wave succeeded and a later wave fails completely, the whole pipeline aborts before rendering/finalizing the normal partial report, even though useful outputs exist. This differs from partial failure within a wave and from sequential-step failure handling.

**Required fix:** return wave failures as data and decide terminal status at the whole-run level. Preserve and finalize all prior outputs; abort only when the run has no useful output or when a dependency policy explicitly requires fail-fast.

**Regression coverage:** successful first wave + fully failed second wave + downstream dependency; verify a partial run, manifest, report, and failure details are retained.

### P2-04 — Managed `uv` installation can exhaust memory and replace a good binary with a partial file

**Severity:** High  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/engines.rs:455-555`

The download has a time limit but calls `response.bytes()` without a content-length or streamed byte cap. A server/proxy can force arbitrary memory growth before the checksum fails. Unpacking buffers the executable again and writes directly to the final `~/.pipeline/bin/uv` path. A crash, short write, or disk-full error can therefore destroy a previously working installation. The pre-install `uv --version` probe also has no timeout.

**Required fix:** stream to a size-capped temporary archive, verify the checksum, extract to a temporary executable, fsync/validate/version-probe it, then atomically replace the destination. Preserve the previous binary until commit.

**Regression coverage:** oversized response, missing content length, checksum mismatch, short archive, disk full, cancellation, crash before rename, and old binary preserved after every failure.

### P2-05 — Direct-API file tools can block on special files and failed writes consume quota

**Severity:** High  
**Platforms:** all; special-file behavior primarily Unix  
**Evidence:** `gui/src-tauri/src/pipeline/api_common.rs:218-305`, `gui/src-tauri/src/pipeline/api_common.rs:312-377`

Read validation canonicalizes paths and checks reported length but does not require a regular file. Opening a FIFO/device in `read_bytes_limited` can block despite the byte cap. Write validation rejects symlinks but can target an existing FIFO and block in `std::fs::write`. Both are synchronous operations inside the API tool loop. In addition, `WRITE_COUNT` and `WRITE_BYTES` are incremented before path validation, directory creation, and the write; rejected paths and failed writes permanently consume the run quota.

**Required fix:** use no-follow, regular-file-only handle validation; perform blocking I/O off the async runtime with cancellation/deadlines; reserve quota only after validation and roll it back on failure (or commit accounting only after a successful write).

**Regression coverage:** FIFO/device/socket, symlink replacement race, failed parent creation, invalid traversal, disk full, and repeated rejected writes followed by a valid report write.

### P2-06 — CLI output overflow can be accepted as a truncated success

**Severity:** High  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/pipeline/claude.rs:501-535`, `gui/src-tauri/src/pipeline/codex.rs:211-288`, `gui/src-tauri/src/pipeline/gemini.rs:243-266`

Each stdout reader breaks once collected output exceeds 50 MB, closes its end of the pipe, and returns the partial buffer without an overflow error. Depending on the CLI, the child may fail on the broken pipe, keep running until timeout, or exit successfully. In the success case Pipeline can parse/use a truncated report as if it were complete; Claude's truncated JSON envelope can also fall back to raw text. The retained string can exceed the advertised cap by the last accepted record.

**Required fix:** keep draining and discarding after the capture limit, record an overflow flag, then fail deterministically when the child exits. Alternatively terminate the process immediately on overflow, but still reap it and drain/close pipes safely. Never pass an overflowed buffer to report parsing.

**Regression coverage:** sustained stdout beyond 50 MB, one oversized record, simultaneous stderr flood, and provider that exits only after all stdout is consumed.

### P2-07 — Batch jobs do not share one immutable profile/settings snapshot

**Severity:** High (non-reproducible and mixed-result batch)  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/commands.rs:1906-1985`, `gui/src-tauri/src/commands.rs:667-710`, `README.md:103-106`

`start_batch` captures only variables and named inputs. Every item calls `run_pipeline_inner`, which reloads the current on-disk settings and active profile. Editing/switching profiles or providers while a batch is running changes later jobs, despite the UI describing the operation as running one active profile over many inputs. Required variables/inputs and model selection can also change mid-batch.

**Required fix:** snapshot the complete validated profile and settings once at batch start and pass them to each run; record one batch-level profile/version identifier. If dynamic behavior is desired, expose it as an explicit option rather than an accident of persistence timing.

**Regression coverage:** switch profile/provider and edit prompts between batch items; all jobs must retain the initial snapshot.

### P2-08 — Windows extraction command resolution bypasses the hardened `.cmd` shim resolver

**Severity:** High on Windows  
**Platforms:** Windows  
**Evidence:** `gui/src-tauri/src/pipeline/extract.rs:500-530`, `gui/src-tauri/src/pipeline/extract.rs:405-480`, `gui/src-tauri/src/deps.rs:27-67`

The extraction resolver treats `.exe`, `.cmd`, and `.bat` files as launchable and returns a raw path. Marker/Poppler launch code passes that path directly to `std::process::Command`. Provider discovery correctly uses `deps::ResolvedCommand`, whose documented purpose is to avoid direct npm `.cmd` execution and preserve untrusted argument boundaries. Extraction therefore has a separate Windows behavior: a discovered script can fail to launch or reintroduce command-shell parsing that the provider path deliberately removed.

**Required fix:** reuse one typed command resolver for providers, extractors, and managed tools. Accept native executables directly and translate only validated npm shims to `node.exe` plus a fixed entry point; reject arbitrary batch files.

**Regression coverage:** `.EXE`, standard npm `.CMD`, arbitrary `.BAT`, spaces/metacharacters in PDF paths, mixed-case `PATHEXT`, and managed-vs-system precedence on Windows.

### P2-09 — Zero-match fan-out is marked complete without an output or a pass event

**Severity:** High  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/pipeline/executor.rs:608-622`, `gui/src-tauri/src/pipeline/executor.rs:176-190`

When a fan-out glob matches nothing, `build_units` only logs a warning and returns no units. The scheduler marks the step done, emits no skipped/error pass, and produces no placeholder output. Dependents then run with missing context, while the progress UI may never show that the step existed. This is unlike `run_if` skips, which create a structured skipped output.

**Required fix:** define an explicit zero-match policy (`skip with placeholder`, `error`, or `allow empty`) in the schema; default to a visible skipped output and event, and make downstream substitution deterministic.

**Regression coverage:** zero matches with and without dependents, merge enabled, and a workflow containing only the zero-match step.

---

## Phase 3 — reliability, diagnostics, and platform confidence

### P3-01 — Fan-out glob expansion can freeze a run on large repositories

**Severity:** Medium  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/pipeline/glob.rs:45-96`, `gui/src-tauri/src/pipeline/executor.rs:608-625`

Glob expansion synchronously walks up to 100,000 entries on the async executor path and recompiles the same regex for every file. It skips hidden directories but not common large trees such as `node_modules`, `target`, virtual environments, or profile-configured exclusions. There is no cancellation check.

**Required fix:** compile once, move the walk to a blocking task, add cancellation and directory exclusions, and expose entry/time budgets in the result rather than silently treating truncation as ordinary matching.

### P3-02 — Pipeline editor shows a permanent spinner after its initial load fails

**Severity:** Medium  
**Platforms:** all  
**Evidence:** `gui/src/components/PipelinePage.tsx:79-94`, `gui/src/components/PipelinePage.tsx:152-159`

The initial `Promise.all` catch sets `loading` false but leaves `config` null. Rendering checks `loading || !config` and continues showing the spinner forever, with no error, retry, or way back. A transient settings/profile parse problem therefore looks like a hung window.

**Required fix:** store/render a load error with Retry and Close actions; do not use null data as a loading sentinel.

### P3-03 — Artifact Explorer retains stale manifest/selection across run changes

**Severity:** Medium  
**Platforms:** all  
**Evidence:** `gui/src/components/ArtifactExplorer.tsx:241-285`

When `runId` changes, the component starts loading the new manifest but does not reset the old manifest, `manifestError`, selected path, content, or load error. The content effect can request the previous run's selected artifact against the new run ID and display stale content/error until another selection. A prior manifest failure can also force fallback mode for the next valid run.

**Required fix:** reset all run-scoped state synchronously in the `runId` effect or key the explorer by run ID; default selection only after the new manifest arrives.

### P3-04 — Readiness checks can report authenticated/reachable when the first real call will fail

**Severity:** Medium  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/deps.rs:394-505`

Codex authentication is considered valid when `~/.codex/auth.json` merely exists; Gemini uses substring checks for `apiKey`/`oauth`. Neither verifies readability, structure, expiry, or an actual CLI auth status. The local-server check only opens a TCP connection to the first resolved address, so an unrelated listener can be reported ready and IPv6/IPv4 ordering can cause false negatives.

**Required fix:** use provider-supported noninteractive auth/status commands with the bounded probe runner; treat ambiguous state as unknown. For local servers, probe a bounded OpenAI-compatible endpoint and try all resolved addresses while reporting HTTP/protocol errors separately.

### P3-05 — Merge/retry accounting understates duration and token/cost usage

**Severity:** Medium  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/pipeline/merge.rs:103-180`, `gui/src-tauri/src/pipeline/executor.rs:792-965`

Merged `StepOutput` values leave duration and token fields at defaults even though merge is a paid provider call. Parallel retries record only the successful/last attempt's measured duration and usage in the output; aggregate per-step provenance cannot reconstruct the true cost or elapsed time. This makes reports and comparisons understate expensive or flaky runs.

**Required fix:** measure merge calls through the same usage wrapper, accumulate all attempts, and record attempt count plus per-attempt/total duration and tokens. Ensure the run-level event counter and persisted step metadata reconcile.

### P3-06 — Report-file recovery omits model provenance

**Severity:** Medium  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/pipeline/executor.rs:920-965`

If a provider call reports an error but successfully wrote the expected report file, the executor treats the file as success. That `StepOutput` records the resolved model but leaves `model_transport`, `model_policy`, `model_source`, and catalog timestamp at defaults, unlike the ordinary success path. The same executed model can therefore appear unauditable solely because its stdout/exit behavior differed.

**Required fix:** construct success metadata through one shared builder for stdout and file-recovery paths and add equality tests for provenance fields.

### P3-07 — Managed-engine disk-size walk follows symlinked directories without bounds

**Severity:** Medium  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/engines.rs:210-228`, `gui/src-tauri/src/engines.rs:304-316`

`dir_size` uses `entry.metadata()`, which follows symlinks, and recursively traverses any resulting directory with no visited set or entry cap. A corrupted/tampered managed stack can make the Engines page scan outside `~/.pipeline`, loop through a symlink cycle, or spend a very long time walking unrelated storage.

**Required fix:** use `file_type`/`symlink_metadata`, skip symlinks and reparse points, track visited directories, and cap entries/time.

### P3-08 — Application tests run only on Linux before platform packaging

**Severity:** Medium (release confidence)  
**Platforms:** Windows and macOS risk  
**Evidence:** `.github/workflows/build.yml:38-83`, `.github/workflows/release.yml:13-55`

The regular application test job runs only on Ubuntu. The release matrix compiles/packages on macOS and Windows but does not run Rust or frontend tests there. The highest-risk code in this audit—process groups/jobs, command resolution, path normalization, filesystem types, and packaging—is platform-specific and cannot be validated adequately by host-independent unit simulations alone.

**Required fix:** run at least `cargo test --all-targets`, frontend tests, and targeted process/path/extraction integration suites on Windows, macOS arm64, macOS x64, and Linux for the tagged SHA. Add installed-bundle launch smoke tests where practical.

---

## Phase 4 — hardening and performance backlog

### P4-01 — Run retention has no global byte budget

**Severity:** Medium  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/commands.rs:617-631`, `gui/src-tauri/src/runs.rs:438-493`

Retention limits only the number of runs. Even after fixing P1-09's per-run leak, legitimate page images, extracted context, model files, and reports can make each retained run large. A count such as 100 can consume many gigabytes with no automatic pressure response.

**Recommended fix:** support both count and byte ceilings, purge oldest completed runs until both hold, never delete an active run, and expose predicted/current disk use in Settings.

### P4-02 — The frontend ships as one oversized main chunk

**Severity:** Low/Medium  
**Platforms:** all, especially older machines  
**Evidence:** production build warning; `gui/src/App.tsx` eagerly imports major pages/components

The audited build produces a 933 kB main JS chunk (279 kB gzip), above Vite's 500 kB warning threshold. This is not a correctness failure, but it increases webview parse/startup memory and makes every app launch pay for editor/history/math/highlighting code that may not be used.

**Recommended fix:** lazy-load Pipeline Editor, History, engines, and heavy Markdown/KaTeX/highlight paths; define manual chunks and add a bundle-size budget in CI.

### P4-03 — Profile/schema limits are scattered rather than centralized

**Severity:** Low/Medium  
**Platforms:** all  
**Evidence:** `gui/src-tauri/src/pipeline_config.rs:1496-1585`, `gui/src-tauri/src/commands.rs:2623-2676`

Some fields have strong validation (step IDs, variable keys, import bytes), while other resource-bearing fields do not (profile name length, prompts/templates, number of profiles/steps/agents/tools, output schemas, merge prompt, fan-out size). P1-08 requires immediate safety caps; the longer-term issue is that limits are not represented in one schema shared by backend, frontend, import/export, and tests.

**Recommended fix:** define a versioned validation contract with named constants and structured errors, enforce it at every persistence/import boundary, mirror limits in the UI, and fuzz/profile-test the complete decoder/validator.

---

## Recommended release gate

1. Fix P1-01 through P1-10 and add the stated regression tests.
2. Fix P2-01 through P2-09 before a broad public beta; if any item is deferred, disable or clearly label the affected feature rather than shipping silent behavior.
3. Run the new tag-gated suite on all four release targets, complete `cargo audit` and `npm audit`, and perform manual smoke tests for install, first launch, provider setup, run/cancel, batch, profile import, history deletion, and uninstall.
4. Triage Phase 3 with owners and a target release. Phase 4 can remain a documented backlog only after disk/resource ceilings from P1-08 and P1-09 are in place.
