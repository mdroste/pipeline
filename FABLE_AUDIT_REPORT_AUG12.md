# Pipeline — Pre-Release Audit (Fable, August 12, 2026)

**Tree audited:** working tree on `main` at `acecb51` (tag `v0.9.0` points at HEAD) with ~103 modified, 5 deleted, and 47 untracked uncommitted paths — i.e., the entire current feature set (62-method Auto catalog, backend module split, validate step, new run-setup UI) exists only in the working tree.

**Method:** five parallel subsystem reviews (pipeline execution core; extraction/document bundle/managed engines; commands/runs/settings/config; React frontend; release/CI/docs), with every P0/P1 finding re-verified directly against the code. Full test suites were run as a baseline.

**Baseline:**

| Check | Result |
|---|---|
| `cargo test --locked --all-targets` | ✅ 509 + 12 (CLI) pass |
| `npm test` (vitest) | ✅ 359 tests / 41 files pass |
| `npx tsc --noEmit` | ✅ clean |
| `cargo check --locked` | ✅ clean |
| `npm run test:release` | ❌ **1 of 58 fails** (see P0-3) |
| TODO/FIXME markers in src | 0 |

The codebase is in strong shape overall — the sandboxing, atomic-persistence, serde-compatibility, and bounded-I/O discipline described in CLAUDE.md is real and consistently applied (see "Verified clean" at the end). The items below are ordered by what I would fix before tagging a release.

---

## P0 — Release blockers

### P0-1. The release does not exist in git: build-required files are untracked, and a blanket `git add` would publish internal docs
- **Where:** repo root; `git status`
- **Problem:** Tracked-modified files reference untracked ones everywhere: `deps.rs` declares `mod checks; mod command;` but `gui/src-tauri/src/deps/` is untracked (same for `document_bundle/`, `engines/`, `model_catalog/`, `output/`, `pipeline_config/`, `runs/`, `settings/`); `auto_review/methods.rs` `include_str!`s 28 untracked prompt files under `prompts/auto_review/methods/`; `auto_review/legacy.rs` needs untracked `synthesis_v1.md`, `auto_review.rs` needs untracked `validate.md`; tracked-modified `AboutPage.tsx`/`RunSetupPanel.tsx`/`PipelinePage.tsx` import untracked `FlappyBirdGame.tsx`, `AgentDefaultsControl.tsx`, `RunParallelAgents.tsx`, `lib/autoReview.ts`. Any commit that misses one of these produces an unbuildable `main`; a fresh clone of HEAD builds but contains none of this release's content. Meanwhile `development/` (internal audit docs: `AUDIT_REPORT.md`, `SOL_AUDIT_AUG9.md`, `SOL_REFACTOR_LEGACY.md`, `FABLE_PAPER_PLAN.md`, `ROADMAP.md`, `ISSUE_LIST.md`) is untracked but **not** gitignored — exactly the files that were deleted from the repo root to unpublish them would come back in a blanket `git add -A`.
- **Fix:** (1) add `/development/` to `.gitignore` first; (2) make one atomic commit of all modified + untracked build inputs (eight module dirs, 30 prompt files, five frontend files, configs); (3) verify with a scratch `git clone` + full build before tagging.

### P0-2. `release.yml` publishes the draft release after the *first* platform finishes; the documented publish gate does not exist
- **Where:** `.github/workflows/release.yml:182-184`; `RELEASING.md`
- **Problem:** Every matrix job runs `gh release edit "$GITHUB_REF_NAME" --draft=false --latest` after its own upload. With `platform: all`, the fastest job publishes a release missing three of four installers. There is also a create race: parallel jobs both check `gh release view` then `gh release create` (lines 176–180). `RELEASING.md` instructs the operator not to publish until a "Verify complete draft release" job passes — no such job exists anywhere in `.github/`.
- **Fix:** remove `--draft=false` from the per-platform job; create the draft once in a pre-job; publish either manually (as RELEASING.md promises) or from a final `needs: package` job that verifies all expected assets are present.

### P0-3. `npm run test:release` is red: refactor fallout in `paddle-parser-lock.test.mjs`
- **Where:** `scripts/release/paddle-parser-lock.test.mjs:100` (fails); `gui/src-tauri/src/engines/paddle_install.rs:243,417`
- **Problem:** The test asserts `engines.rs` contains `include_str!(".../runtime-lock.json")`, `"runtime_lock_sha256": runtime_lock_sha256`, and `"layout_ready": true` — but the uncommitted module split moved that code into `engines/paddle_install.rs`, leaving `engines.rs` a facade. The release-gating test suite fails on the exact tree you intend to ship.
- **Fix:** point the test at the `engines/` module tree (scan the directory, or the specific new files) instead of the single facade file; re-run `npm run test:release` to green before tagging.

### P0-4. Read-only Claude CLI calls can write into the user's original source folder (prompt-injection → file modification)
- **Where:** `gui/src-tauri/src/pipeline/claude.rs:689-721`; `gui/src-tauri/src/pipeline/orient.rs:127-131`
- **Problem:** Every `claude -p` call passes `--permission-mode acceptEdits`, which (per the code's own comment) auto-approves edits in the cwd and every `--add-dir` directory. The `Edit({dir}/**)` deny rules are added **only when `overrides.write_dir.is_some()`** — read-only calls get no deny at all. The folder-input orientation survey is exactly such a call: cwd and read root are the user's original selected folder (not a staged copy). A prompt-injected instruction inside a surveyed repo gets its Edit/Write auto-approved into the user's real source tree — in the folder-input built-ins (code review, replication audit) this app explicitly targets. Merge/reconcile calls share the gap with cwd = the shared temp dir. Codex (read-only sandbox) and Gemini (plan mode) are not affected.
- **Fix:** when `write_dir` is `None`, drop `acceptEdits` (or add `--disallowedTools Edit,Write` plus `Edit({dir}/**)` denies for cwd and every read root). Add a regression test asserting the deny list is non-empty for read-only calls.

### P0-5. Webview-invocable commands write arbitrary files with caller-controlled path and content
- **Where:** `gui/src-tauri/src/commands/artifacts.rs:130-134` (`save_text_file`), `commands/config.rs:275-278` (`export_item`), `commands/export.rs:5-9` (`save_report_md`), `commands/config.rs:288-291,339-342` (`export_profile`/`export_bundle`)
- **Problem:** `save_text_file` and `export_item` are literally `std::fs::write(&path, content)` — no validation, overwrite semantics, follows symlinks. The frontend picks paths via the dialog plugin, but the commands accept anything. This app renders LLM output derived from untrusted papers, so webview compromise is the stated threat model — and these commands turn it into write-anywhere: `~/.zshrc`, `~/.pipeline/prompts/*.md` (silently overriding every compiled-in prompt), profiles, or the keyfile.
- **Fix:** move the save dialog to the Rust side (dialog plugin's Rust API) and write only to the dialog-returned path — the webview should never supply a filesystem path. At minimum: refuse `~/.pipeline/**` destinations, open with create-new + `O_NOFOLLOW`.

### P0-6. Ship/cut decision: Flappy Bird easter egg with caricatures of real AI-industry CEOs
- **Where:** `gui/src/components/FlappyBirdGame.tsx` (690 lines; `type CharacterSkin = "dario" | "sam"`, lines 73–123, 612–627), reached via Konami code in `AboutPage.tsx:11-53,75`; `gui/src/lib/flappyScore.ts`
- **Problem:** The Help/About page mounts a full game whose two selectable "pilots" are recognizable caricatures labeled "DARIO" and "SAM" flying over a San Francisco skyline. In a tool for academic economists whose stated values are "plain, not promotional," likeness-based parody of living, named industry figures is a reputational and likeness-rights liability with no product value. It is currently undocumented in README/CHANGELOG and would ship silently inside the big catch-up commit.
- **Fix:** decide consciously. Recommended: cut it from the release build (delete the two files + the AboutPage Konami hook + tests). If you keep it, rename/redraw the skins to generic characters and note the egg in the changelog.

### P0-7. Version identity: `v0.9.0` is already tagged/published at HEAD, but the working tree rewrites the 0.9.0 changelog in place
- **Where:** `CHANGELOG.md` (uncommitted diff edits the "## 0.9.0 — 2026-08-10" entry: 32→62 method roles, four→five-step skeleton, etc.); `gui/package.json` / `Cargo.toml` / `tauri.conf.json` all still `0.9.0`
- **Problem:** The new work would ship under a version whose tag already points at a different tree. `RELEASING.md` itself forbids moving a published tag, and `validate-release-identity.mjs` compares versions against existing tags — a rebuilt `v0.9.0` will likely be rejected, and if it weren't, two different binaries would claim the same version.
- **Fix:** bump to 0.9.1 (or 0.10.0 given the scope), restore the 0.9.0 changelog entry as shipped, and record the new work under the new version.

---

## P1 — High priority (fix before release if at all possible)

### P1-1. Claude CLI steps inherit the user's ambient Claude Code configuration; only WebSearch is denied
- **Where:** `gui/src-tauri/src/pipeline/claude.rs:703-709`
- **Problem:** The code already recognizes that `--allowedTools` controls auto-approval rather than availability and denies `WebSearch` — but stops there. `Bash`, `WebFetch`, user-configured MCP servers, and hooks all remain loadable/approvable through the user's own `~/.claude` allow rules in `-p` mode (no `--strict-mcp-config`, no settings isolation). A permissive personal allowlist silently grants every pipeline step shell/network access; a hostile paper can exfiltrate content via WebFetch. Starkly asymmetric with the Gemini path (full settings override + admin policy file) and Codex (OS sandbox).
- **Fix:** extend `--disallowedTools` with `Bash`, `WebFetch`, and `mcp__*` when not granted, and pass `--strict-mcp-config` (or point the CLI at an isolated settings source), mirroring `gemini.rs:192-284`.

### P1-2. Decrypted API keys cross IPC, and `get_model_catalog` gives a CSP-proof exfiltration/SSRF channel
- **Where:** `gui/src-tauri/src/commands/config.rs:45-49` (`get_settings` returns all four keys decrypted), `config.rs:59-69` + `model_catalog.rs:581-599` (`get_model_catalog` accepts an **unsaved** `Settings` from the webview); `settings.rs:986-1028` (`validate_local_base_url` allows any HTTPS host)
- **Problem:** A compromised webview reads keys via `get_settings`, then has the *backend* HTTP client send `Authorization: Bearer <local_api_key>` to any HTTPS host it writes into `local_base_url` — bypassing the strict CSP entirely. It can also probe loopback services.
- **Fix:** return masked keys from `get_settings` (write-only updates from the UI); make local-provider discovery use only the *persisted* `local_base_url`, or require the passed value to equal the saved one.

### P1-3. CLI death by signal is misclassified as user cancellation, suppressing retries
- **Where:** `pipeline/claude.rs:852`, `codex.rs:846`, `gemini.rs:630`; `executor.rs:1724-1734` (`is_cancellation_error` substring match)
- **Problem:** All three wrappers map `exit 143 || status.code().is_none()` to `Err("Pipeline cancelled")`. On Unix any signal death (OOM-kill, CLI segfault, external kill) has `code() == None`, so a transient crash is labeled "cancelled" and the step's remaining retries are skipped. Worse, `is_cancellation_error` is a substring match on "cancelled", so a provider error like "your plan was cancelled" also aborts retries.
- **Fix:** treat signal exits as cancellation only when `is_cancelled() || is_pass_cancelled(pass_key)` is actually set; otherwise return a retryable "provider terminated by signal" error. Replace substring matching with a typed error or sentinel prefix.

### P1-4. Run preflight passes when a profile-required provider is missing (fails mid-run after paid steps)
- **Where:** `gui/src-tauri/src/deps.rs:98-103` (`dependency_group_ready` uses `.any()`), `deps/checks.rs:766-772`, gated at `commands/run_context.rs:184`
- **Problem:** Providers are treated as an OR-group: a profile with a step declaring `agents: ["gemini"]` passes preflight with only Claude installed, then deterministically fails mid-run — after the earlier (paid) steps completed. The same module already computes the exact per-provider `required` set (`checks.rs:323`); `ready` just ignores it.
- **Fix:** AND over each provider dependency whose `required` flag is set (`dependency_ready` already exists for exactly this).

### P1-5. Full Parser verification silently disappears when the pdftotext baseline is unavailable; the LLM path fails closed in the identical case
- **Where:** `pipeline/extract/pdf.rs:564-577, 383-401, 424-430`; `deps/checks.rs:314`
- **Problem:** If `pdftotext_page_baseline` returns `None` (poppler broken/missing, encrypted or malformed PDF), the 300-page cap, the pages-vs-PDF count check, and `paddle_char_count_is_suspicious` are all skipped — any structure with non-empty pages is accepted unverified. `extract_llm` explicitly refuses to run in the same situation. This contradicts CLAUDE.md's "verifies its page count … and conservative baseline completeness" and "an incomplete text-heavy page still fails closed."
- **Fix:** fail closed like the LLM path (preferred), or at minimum enforce the page-count cap from Paddle's own reported page count and record a prominent quality note that completeness was unverified.

### P1-6. Documented Paddle page-recovery behavior does not exist: no `finish_reason=length` detection, no recursive bisection, no inline warning blockquote
- **Where:** `gui/src-tauri/paddle_parser_sidecar.py:224-346` vs CLAUDE.md; `pipeline/extract/core.rs:17` (`PADDLE_REGION_WARNING_PREFIX`)
- **Problem:** The sidecar's only recovery is N whole-page layout retries plus one whole-page no-layout pass, triggered solely by the char-count heuristic. It never inspects `finish_reason`, never bisects a page along its long axis, and never emits the warning blockquote that Rust filters for (`PADDLE_REGION_WARNING_PREFIX` is unproducible dead code). Either the bisection machinery was lost in the Fast→Full migration or CLAUDE.md describes the retired extractor.
- **Fix:** reconcile before release — implement the documented recovery, or correct CLAUDE.md and delete the dead prefix-filtering in `core.rs`.

### P1-7. macOS/Linux Poppler inputs are unpinned and never provenance-checked in CI, contradicting the published license/provenance claims
- **Where:** `scripts/release/bundle-poppler.sh:18` (plain `brew install poppler`), lines 41–59 (Linux copies whatever apt produced); `THIRD_PARTY_LICENSES.md` (asserts exact version/commit/snapshot anchors); `scripts/release/poppler-provenance.mjs` (invoked by nothing)
- **Problem:** Only Windows is checksum-locked. A Homebrew or Ubuntu version bump silently ships binaries that falsify the published GPL provenance table — a compliance problem, not just drift.
- **Fix:** verify the built bundle against `poppler-lock.json`-style pins for all three platforms (wire `poppler-provenance.mjs` into `release.yml`), or soften the documented claims to match what is actually guaranteed.

### P1-8. Parallel-wave outputs are not checkpointed until the entire wave (and merge) completes
- **Where:** `pipeline/executor.rs:929-1003` (`checkpoint_outputs` at line 1002)
- **Problem:** CLAUDE.md promises "each completed or failed step is checkpointed immediately." In a 5-unit wave where four units finish in minutes and one runs 30 more, a crash or cancel during that window (or during a long merge call) loses all completed provider work — resume re-runs and re-bills those units.
- **Fix:** checkpoint each unit's `StepOutput` in its task success path (before merge), letting finalize/merge overwrite with the merged artifact; or checkpoint unmerged outputs before invoking `merge_step_outputs`.

---

## P2 — Medium priority

### P2-1. The one-run Parallel override is silently dropped from the run fingerprint
- **Where:** `commands/run_context.rs:99-101` vs `:137-140`; call order in `run_entry.rs:96-105` and `config.rs` is overrides → runtime → foreground
- **Problem:** `bind_parallel_overrides` folds the override into `snapshot.fingerprint`, but `bind_runtime_snapshot` then recomputes the digest from `snapshot.config_fingerprint` — overwriting it. CLAUDE.md says the transient override "is included in … the run fingerprint"; it isn't, so the `expected_profile_snapshot_id` staleness check cannot detect overrides changed between RunPreview and launch. Existing tests never chain the two binders.
- **Fix:** digest `snapshot.fingerprint.as_str()` in `bind_runtime_snapshot`; add a test that chains all three binders.

### P2-2. Artifact-quota breach is recorded as "Pass cancelled," hiding the real cause
- **Where:** `pipeline/claude.rs:117-127` (`supervise_artifact_writes` → `cancel_pass`), `executor.rs:1574-1591`
- **Problem:** The quota violation cancels the pass; on the next loop iteration the cancellation check fires first, so `StepFailure.error` becomes "Pass 'x' cancelled" instead of the quota message. A host-enforced safety stop is indistinguishable from a user click in the run record.
- **Fix:** kill the pass's children directly without marking it user-cancelled, and short-circuit the retry loop on the quota error itself as a non-retryable class.

### P2-3. Managed uv reuse check compares the extracted binary's hash against the *archive* checksum — always mismatches
- **Where:** `engines/installer_io.rs:841-842` vs `:884`
- **Problem:** `artifact.sha256` is the digest of the release `.tar.gz`/`.zip` (the same constant `download_verified` uses), so the "existing install verified" gate can never pass. Every install/repair discards a valid uv, logs the alarming "Replacing managed uv after a checksum mismatch" (falsely implying tampering), and re-downloads ~40 MB.
- **Fix:** record the extracted binary's own SHA-256 (or a version stamp) at install time and compare against that; keep `artifact.sha256` for the archive only.

### P2-4. Multi-gigabyte synchronous integrity hashing runs on the async executor
- **Where:** `pipeline/extract/pdf.rs:563,586-587,687` → `engines/runtime.rs:301` → `engines/integrity.rs:380-411`
- **Problem:** First parser resolution per session hashes the whole closure (Q8 model 936 MB + projector 882 MB + cpython/venv; 8 GB cap) with blocking `std::fs` reads on a tokio worker thread, plus 50 MB structure reads and cache renames on the same path. The rest of the codebase carefully uses `spawn_blocking` for this kind of work.
- **Fix:** wrap `paddle_full_parser_paths()` and the structure read/activation in `tokio::task::spawn_blocking`.

### P2-5. Find-bar highlights corrupt React-managed report content when the markdown changes
- **Where:** `gui/src/hooks/useFindBar.ts:37-56,80-96`; `ReportViewer.tsx:381`; triggered from `ReportWorkspace.tsx:575,914`
- **Problem:** `applyHighlights` replaces React-owned text nodes; `clearHighlights` runs in a passive-effect cleanup, which executes *after* React commits updates for a prop change. Toggling clean/raw output (or switching compared steps) with highlights active makes React reconcile against detached nodes: stale text silently persists or a `NotFoundError` throws and the error boundary downgrades the report to the plain-text fallback. `parent.normalize()` extends the stale window past find-bar close.
- **Fix:** remount the markdown container on content change (`key={normalizedMarkdown}` on `.report-content`), or move to the CSS Custom Highlight API (no DOM mutation).

### P2-6. Log lines buffered from a dying run flush into the next run's console
- **Where:** `gui/src/hooks/usePipeline.ts:163,474-481,519-526`
- **Problem:** `startPipeline`/`rerunPipeline` reset `logs` but never clear `logBuffer.current`; lines in the 200 ms flush window at cancel/failure time (exactly the stderr tails you need for triage) are attributed to the *new* run with fresh timestamps.
- **Fix:** `logBuffer.current = []` alongside `setLogs([])` in both callbacks.

### P2-7. Eight release-integrity scripts are dead code
- **Where:** `scripts/release/`: `poppler-provenance.mjs`, `smoke-poppler.mjs`, `smoke-packaged-app.mjs`, `validate-macos-machos.sh`, `validate-signed-tag-binding.sh`, `artifact-integrity.mjs`, `prepare-notices.mjs`, `validate-release-assets.mjs`
- **Problem:** Referenced by no workflow, npm script, tauri hook, or doc — only their own unit tests run. CLAUDE.md still describes `scripts/release/` as performing these checks.
- **Fix:** wire back in at minimum the packaged-app smoke test and release-asset validation (given the recent AppImage-launch fixes), or delete the scripts plus their tests; update CLAUDE.md either way.

### P2-8. `list_runs` can quarantine-and-replace a healthy manifest without the run lock
- **Where:** `runs/history.rs:425-441` (`recover_broken_manifest`) vs the locked path at `:236-244`
- **Problem:** Any manifest that is valid JSON but fails `RunManifest` deserialization (e.g., written by a *newer* app version) is renamed `manifest.corrupt-*` and replaced with a synthetic `status:"failed"` manifest — from a read path holding no lock. A downgrade or side-by-side install converts healthy runs into "failed" rows.
- **Fix:** rewrite only under `run.lock`, and only when the file is not valid JSON at all.

### P2-9. Batch cancel landing during job startup is silently erased
- **Where:** `commands/lifecycle.rs:8-15` (`begin_run_state` clears `CANCEL_FLAG` unconditionally); window between `commands/batch.rs:101-104` and `commands/run.rs:12`
- **Problem:** A cancel in that window kills zero children (none spawned yet) and its flag is cleared — one full job of real model spend runs to completion.
- **Fix:** re-check `BATCH_CANCEL` (or a cancel epoch captured at guard acquisition) immediately after `begin_run_state`.

### P2-10. Hardening: unscoped `shell:allow-open`; llama-server API key on argv; port-reservation TOCTOU
- **Where:** `gui/src-tauri/capabilities/default.json` (`shell:allow-open` with no URL scope); `pipeline/extract/pdf.rs:141-142` (`--api-key` in argv, world-readable via `ps`), `:91-97` (bind port 0, drop, reuse)
- **Fix:** add a URL scope (https + mailto) to `shell:allow-open`; pass the llama key via `LLAMA_API_KEY` env like the sidecar already does; detect the child's bound port instead of pre-reserving.

### P2-11. Shared-context warm-up holds the slot mutex across the whole primer call, risking correlated sibling timeouts
- **Where:** `pipeline/claude.rs:467-514`, `codex.rs:184-231`, `api_anthropic.rs:229-271` (same pattern in the other two APIs)
- **Problem:** Siblings block on `slot.lock().await` while their own `step_timeout_secs` clocks are already running; a slow warm-up can push every sibling in the wave into timeout/fallback at once.
- **Fix:** give the warm-up its own shorter deadline, or start each sibling's timeout clock after lock acquisition.

---

## P3 — Low priority / cleanup

1. **LaTeX `\input{}` expands inside comments and disagrees with the staging resolver** — `pipeline/extract/latex.rs:426-434`: `extract_latex` scans raw content (a commented `%\input{old_draft}` is inlined) and its regex requires `{` immediately after the command while `latex_references` accepts whitespace, so staged read roots and inlined text can diverge. Fix: strip comments first; align the regexes.
2. **Login-shell PATH capture ingests the shell's entire stdout** — `env.rs:78-90`: an `echo` in `.zshrc` corrupts the cached PATH for the process lifetime. Fix: print a sentinel (`__PATH__=$PATH`) and parse it, or take the last non-empty line.
3. **Dangling `parent_id: page-0001` on markdown-derived bundle nodes** — `document_bundle.rs:421`; no node/page carries that id and `validate()` never checks parents (`to_model_index_pretty` silently filters them). Fix: emit matching page ids, drop the field, or validate parent references.
4. **llama.cpp tar extraction has no entry-count cap** (Python archive path caps at 100k) — `engines/installer_io.rs:539-617`. Defense-in-depth only (archive SHA-256 is verified first). Fix: mirror `MAX_PYTHON_ARCHIVE_ENTRIES`.
5. **`refresh_paddle_parser_sidecar` mutates the installed runtime without the engine lock, via two non-atomic writes** — `engines/runtime.rs:353-392`. A crash between the writes leaves script/manifest digests mismatched until repair. Fix: take the lock; temp-file + rename both files.
6. **Run output budget double-counts multi-agent wave outputs** — `executor.rs:987-991` vs `:2413`: post-merge, every returned output is reserved again and originals are never released; effective budget ≈ half of 256 MB. Fix: reserve only net-new merged text.
7. **Sequential prompt expansion re-substitutes placeholders inside model-produced upstream text** — `executor.rs:2542-2563,2727`: tokens embedded in a prior step's output get expanded (bounded by `MAX_EXPANDED_PROMPT_BYTES`, no cross-view leak). Fix: single-pass substitution over the template only, splicing outputs as opaque payloads.
8. **8 MB single-line cap silently overrides the 50 MB stdout cap for JSON-envelope providers** — `logging.rs:21`, `cli_process.rs:82-92`: Claude/Gemini envelopes arrive as one line, so the effective cap is 8 MB and the error text misstates the limit. Fix: align constants or the message.
9. **Oversized single Codex `agent_message` misreported as total-stdout overflow** — `codex.rs:73-81,708-713,799-805`. Fix: distinct error string.
10. **Rename/retag lost to concurrent finalization write** — `runs.rs:829-841` vs `runs/history.rs:445-467` (acknowledged in comments). Fix: route both through one advisory lock.
11. **Preprocessing transcript deleted even when its copy into the run failed** — `commands/run.rs:134-138` vs `run_storage.rs:36-59` (copy failure only warns). Fix: delete the source only after a successful copy.
12. **Dead code:** `RunWriter::create` (`runs.rs:503-505`); `CallUsage::add`, `emit_model_round_trips`, `emit_tool_calls` (`pipeline/logging.rs:107-110,338-364`); `PADDLE_REGION_WARNING_PREFIX` filtering if P1-6 resolves doc-side. Fix: remove or wire up.
13. **"Provider default" can't clear a legacy plain-provider model/effort override** — `AgentDefaultsControl.tsx:64-77,111,153`: only the `provider:transport` key is deleted while display falls back to the plain key, so the select snaps back. Fix: delete both keys on inherit.
14. **`pipeline:stage` handler can flip a finished workspace back into progress** — `usePipeline.ts:222-244` lacks the `prev.kind` guard the pass handler (`:261-267`) has. Fix: same guard.
15. **Fallback run duration recomputed from `Date.now()` at render time** — `App.tsx:834` → `runProvenance.ts:265`: for runs with no persisted dir, Provenance shows a duration that keeps growing. Fix: capture elapsed once on transition to `done`.
16. **Model dropdown options embed the section label** ("GPT Exact · Agents for this report") — `AgentDefaultsControl.tsx:143`; `RunParallelAgents.test.tsx:71-73` locks the odd copy in. Fix: drop `· {label}`; update the test.
17. **Inconsistent localStorage guards** — `App.tsx:270`, `usePersistentPanelWidth.ts:14,27`, `RunSetupPanel.tsx:167` unguarded while `flappyScore.ts` try/catches; a throwing storage area (WebKitGTK with storage disabled) crashes theme toggle/panel resize. Fix: wrap like `flappyScore.ts`.
18. **Konami tracker rejects the code after a stray leading ArrowUp** — `AboutPage.tsx:43` (moot if P0-6 is cut).

---

## Documentation drift (consolidated)

These are claims in shipped/operator docs that the code contradicts. Each is cheap to fix but expensive to leave wrong:

| Doc | Claim | Reality | Action |
|---|---|---|---|
| `RELEASING.md` | Windows signing required; Grype/Syft SBOM scans; tag-triggered builds; "Verify complete draft release" gate | None exist in the current workflow (`workflow_dispatch` only; Windows intentionally unsigned) | Rewrite to match the simplified pipeline (with P0-2) |
| CLAUDE.md | "each completed or failed step is checkpointed immediately" | Successes checkpoint only after the whole wave + merge | Fix code (P1-8) or wording |
| CLAUDE.md | Paddle page bisection / `finish_reason=length` / inline warning blockquote | Not implemented in the sidecar | P1-6 |
| CLAUDE.md | resources/poppler "populated and provenance-checked by CI" | Only Windows is checked | P1-7 |
| CLAUDE.md | Parallel override "included in … the run fingerprint" | Dropped by binder ordering | P2-1 |
| CLAUDE.md | `resources/notices/` = "licenses, SBOM, and Poppler provenance" | Bundle contains only NOTICE/license/third-party files; SBOM generator removed (stale generated files still on disk locally) | Update text; have `prepare-package-notices.mjs` clean the dir |
| CLAUDE.md | build.yml covers "maintenance audits/MSRV"; build instructions say `cargo test --all-targets` | build.yml is test-only (a hardening test now asserts audits are absent) and CI runs `--lib` | Correct the comment; decide `--lib` vs `--all-targets` in CI |
| CLAUDE.md (header/tree) | CLAUDE.md/AGENTS.md are repo files | Both are gitignored ("local only") and untracked — fresh clones and other contributors get no project instructions | Track them, or correct the self-description |

---

## Verified clean (checked, no findings)

- **Terminal-report nonce contract** (`output.rs`): exactly-two-marker rule, refusal detection, compatibility-file path all fail closed; merge/reconcile use the same envelope.
- **Direct-API tool sandbox** (`api_common.rs`): per-call `ToolAccess`, canonicalization + `O_NOFOLLOW`, traversal/symlink/FIFO handling, quota rollback, atomic persist — all correct.
- **Cancellation/PID lifecycle** (`cli_process.rs`, `commands/lifecycle.rs`): process groups, register-after-cancel kill, drain-with-escalation, `CallTask` drop semantics — no leaked-child path found (P1-3/P2-9 are classification/race issues, not leaks).
- **Codex fork validation**: cwd, sandbox, network, writable roots, model all checked before resume.
- **`safety.rs` / `process.rs`**: `O_NOFOLLOW`+post-open type checks, allocation-checked replacement, walk budgets, drain-past-cap bounded runner — solid and matching their contracts.
- **No cross-method extraction fallback** anywhere; retired methods fail closed. LLM chunking math and retry-overwrite logic correct; extraction cache fingerprints cover all relevant settings.
- **Serde backward compatibility**: `#[serde(default)]` discipline, schema_version gating, deserializer-level migrations, hash-gated prompt migrations — consistent throughout.
- **Atomic persistence + keyfile crypto**: temp-file + fsync + rename (+ dir sync) everywhere it matters; no-clobber keyfile creation; corrupt files quarantined, never overwritten.
- **Retention/deletion**: id validation, canonicalization under the runs root, running-run protection, digest-gated manual purge — no wrong-deletion path found.
- **Auto Paper Review**: plan validation (category-correct allowlisted IDs, counts, duplicate/pairing rules), materialization, and re-validation are solid; catalog↔disk consistency holds in both directions (every referenced prompt exists — but 30 of them are untracked, see P0-1).
- **`storage.rs` deletion**: no dangling references or half-migrated duplicates.
- **Frontend XSS surface**: no rehype-raw; KaTeX `trust:false` with bounded macros; highlight.js escapes; link allowlist; data-URI images — no model-output injection path found. **Type drift**: none found across the major mirrored types.
- **Tailwind**: genuinely wired (config + postcss + directives + pervasive utility classes), not scope creep.
- **npm lockfile** in sync; **workflow actions** SHA-pinned; toolchain pins consistent (Rust 1.97.1, Node 24.18.0).

---

## Suggested sequencing

1. Gitignore `development/` → decide the easter egg (P0-6) → bump version + restore 0.9.0 changelog (P0-7).
2. Fix the two security P0s (P0-4, P0-5) — both are small, localized changes.
3. One atomic commit of the full working tree; scratch-clone build to verify (P0-1).
4. Fix `paddle-parser-lock.test.mjs` (P0-3) and the release.yml publish gate (P0-2); rewrite RELEASING.md.
5. Work through P1 (sandbox hardening, misclassified cancellation, preflight AND, Paddle verification/doc reconciliation, poppler provenance, wave checkpointing).
6. P2/P3 and the doc-drift table as follow-ups — none individually block a tag, but P2-1…P2-6 are all small, high-value fixes worth batching before it.
