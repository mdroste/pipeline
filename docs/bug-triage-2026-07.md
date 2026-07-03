# Bug triage — July 2026 dust-off

Candidate issues from a full source review (Rust backend + React frontend + CI) on 2026-07-02.
These are **unverified hypotheses** from code reading, not confirmed bugs. Each needs
verification before fixing. Delete entries as they are confirmed-fixed or refuted.

## P1 — verified 2026-07-02, confirmed items FIXED same day

All three confirmed items below were fixed on 2026-07-02 with regression tests
(`settings::tests::quarantine_moves_corrupt_file_aside`,
`api_common::tests::validate_tool_path_allows_temp_files` + `_rejects_outside_allowed_dirs`,
`api_anthropic::tests::resolve_model_uses_dateless_aliases`). 95/95 tests pass.

1. **CONFIRMED, FIXED — Malformed settings.json wipes API keys.** [settings.rs:193](../gui/src-tauri/src/settings.rs)
   falls back to `Settings::default()` on parse failure (and plain `load()` discards the
   warning). Any routine save then persists the defaults: `switch_profile()`
   (pipeline_config.rs:897), `delete_profile()` (:889), `import_bundle()` (:1102) all do
   `load()` → mutate → `save()`. One stray byte in settings.json + one profile switch =
   encrypted API keys destroyed. Fix: back up the corrupt file (e.g. `settings.json.corrupt`)
   before falling back, and/or refuse to save over an unparseable file.
2. **REFUTED — Temp-file lifetime race.** All three CLI wrappers (`claude.rs:75`,
   `codex.rs:38`, `gemini.rs:38`) hold the `NamedTempFile` guard in the same async fn that
   awaits `child.wait()`. `commands.rs` holds the paper-text tmp and `_orient_tmp` for the
   whole run. No race.
3. **CONFIRMED, FIXED — Retired Anthropic model IDs.** `api_anthropic.rs:16-17` maps ""/"sonnet" →
   `claude-sonnet-4-20250514` and "opus" → `claude-opus-4-20250514`. Both were deprecated
   with retirement June 15, 2026 (now past) — direct-API Claude calls with default model
   settings 404. Haiku ID is fine. Fix: use dateless aliases (`claude-sonnet-4-6` /
   `claude-opus-4-6` or newer). The `gpt-4.1` / `gemini-2.5-flash` defaults exist and were
   a false alarm, though worth keeping configurable.
4. **PARTLY CONFIRMED (different bug), FIXED — temp-dir check broken on macOS/Windows.** The two
   hypothesized holes were refuted (poisoned-lock fallback recovers the real list;
   canonicalized-allowed-dir comparison is correct). But `api_common.rs:95` compares the
   canonicalized file path against a NON-canonicalized `env::temp_dir()`: on macOS
   `$TMPDIR` is `/var/folders/…` while canonical paths are `/private/var/folders/…`, so
   `starts_with` is always false (verified empirically). `set_allowed_dirs` receives only
   the paper's source dir (commands.rs:135), so in direct-API mode the LLM cannot read the
   extracted-paper-text temp file, the orientation map, or long-prompt instruction files —
   **direct API mode is effectively broken on macOS** (and likely Windows: `canonicalize`
   returns `\\?\`-verbatim paths). Fix: canonicalize `temp_dir` once before comparing.
5. **REFUTED — Executor error reporting.** Sequential-step failure records a `StepFailure`,
   breaks, and returns prior outputs; "No steps produced output" only fires on truly-empty
   output. Parallel waves error only if every step fails; partial failures continue with a
   log warning. Minor gap → moved to P3: parallel-step failures aren't added to
   `failed_steps`, so the report banner misses them (log-only).

## P2 — degraded behavior, verify second

6. **Silent 50 MB stdout truncation** in `claude.rs` (~199): oversized LLM output truncated
   with no error or warning.
7. **usePipeline listener-registration race** (`usePipeline.ts:36-121`): unmount during
   `Promise.all` listener setup may leak listeners.
8. **`deps.rs` codex auth check** (~115) returns `Some(false)` when nothing was actually
   probed — UI shows "not authenticated" instead of "unknown".
9. **Marker/pdftotext timeout floor** (`extract.rs` ~199): timeout = max(step_timeout/2, 120s)
   overrides a user-set short timeout.
10. **Merge failure fallback** (`merge.rs` ~216): concatenation banner interpolates agent
    names unsanitized. Check whether ammonia in `print_report_html` already covers this;
    if so, refute.
11. **Effort setting silently ignored** for Anthropic/Google direct-API calls — should at
    least be surfaced in the UI or docs.
12. **Log truncation** (usePipeline keeps last 8k of 10k lines) and **math-render fallback**
    (ReportViewer drops KaTeX entirely on one bad formula) happen without user notice.

## P3 — hygiene / hardening backlog

- Duplicate step IDs not validated on profile import (`pipeline_config.rs`) — merge grouping
  keys on `{id}/{agent}` would collide.
- `kill_process` PID validation is thin (`commands.rs` ~79); relies on registration always
  being valid.
- Codex stderr drained silently (`codex.rs` ~115) — capacity warnings lost.
- Windows keyfile `icacls` failure is best-effort and unreported (`settings.rs` ~364).
- `updates.rs` doesn't handle GitHub rate-limit responses distinctly.
- Untested components: PipelineProgress, ReportViewer, PromptEditor, WaveDiagram,
  PipelinePage, SettingsPage (no `.test.tsx`).
- `BUILTIN_PROFILES` const (`pipeline_config.rs:465`) lists only deep-review/quick-review but
  three built-ins are created — check what that const gates.
- Parallel-step failures are logged but not recorded in `failed_steps` (executor.rs ~390),
  so the report's warning banner only reflects sequential failures.

## Baseline (run 2026-07-02)

- `cargo test`: 91/91 pass. `cargo clippy`: 22 style warnings, no errors.
- `npm test` (vitest): 35/35 pass across 6 files.
- `npm audit`: 4 vulns (2 high) — all dev-tooling (vite, esbuild, undici, babel);
  `npm audit fix` available. Nothing in shipped code. cargo-audit runs in CI only.

## Verification protocol

1. Baseline: `cargo test`, `cargo clippy`, `npm test`, `npm audit`, `cargo audit` — confirm
   the tree still builds green after months idle (toolchain/dependency rot).
2. For each P1/P2 item: read the exact code path, write a failing test where feasible,
   confirm or refute. Record verdict here.
3. Runtime smoke test: `npm run tauri dev`, run Quick Review on a small paper via the
   Claude CLI path; if API keys available, one direct-API run.
4. Fix confirmed items in priority order, one commit per fix, tests included.
