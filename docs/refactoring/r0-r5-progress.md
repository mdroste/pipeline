# R0–R5 implementation record

These stages are being implemented in order in the existing working tree.
All current and subsequent work in this section will enter one PR at the end,
as requested; stage boundaries below are validation boundaries, not separate PRs.

## R0 — baseline and reporting

- Preserved a private copy of 1,462 original owned files and SHA-256 identities
  under `/tmp/pipeline-refactor-r0-original` before editing source.
- Existing branch: `codex/nf07-nf14`; substantial prior tracked and untracked
  feature changes are retained. No resets, commits, or branch switches.
- Baseline runtime: Node 24.18.0 from `.nvmrc`, Rust/Cargo 1.97.1.
- Added `node scripts/source-size-report.mjs`, with explicit generated-file
  exceptions and a source/test baseline. It uses `rg`, never Git, and reports
  new/growing size debt without changing CI or failing unrelated work.
- Repository formatting already fails before this refactor (42 diff blocks in
  `/tmp/pipeline-r0-fmt.log`); changes include existing Workspace and retention
  work. Only files touched by the refactor will be formatted.
- Baseline: 593 frontend tests and production build passed. The parallel Rust
  run had one two-second subprocess timeout under load; a serial all-target run
  passed 893 library and 14 CLI tests (10 ignored). No source changed between
  these runs. Logs are `/tmp/pipeline-r0-{frontend,build,rust-serial}.log`.
- Size-report smoke checks passed for threshold detection, config exclusion,
  an unchanged baseline, and growth detection. R0 is complete.

## R1 — complete

- Formatted six dense Workspace/Tasks UI files with the already available
  Prettier 3.9.6; no package installation or dependency change.
- Moved inline tests out of 11 large Rust files. Split executor, command and
  profile tests by behavior; the largest moved test file is 706 lines.
- Preserved all 292 baseline test identities in the affected files. Concurrent
  feature changes added tests and were retained, including a new Workspace
  maintenance/setup test. Existing fixture paths and module references were
  adjusted for the new file locations.
- Checks: 897 library tests and 14 CLI tests passed (10 ignored); 595 frontend
  tests and production build passed. Logs: `/tmp/pipeline-r1-*.log`.
- Formatting alone reveals WorkspacePage at 1,731 lines and TasksPage at 1,265.
  Their controller decomposition remains R9; the size reporter intentionally
  shows the new physical size instead of resetting the R0 baseline.

## R2 — implemented

- Moved artifact DTOs into `lib/artifactTypes.ts` and reads into a typed client
  shared by Sources, ReportWorkspace and the read-only file adapter.
- Split selection/loading, tree and page navigation, PDF and bundle previews,
  bundle projection and report grouping into focused feature modules. Existing
  public type exports remain compatible; PDF.js and file editors remain lazy.
- All 52 focused artifact/report/file-workspace tests and production build pass
  (`/tmp/pipeline-r2-{tests,build}.log`). See the native inspection limitation below.

## R3 — implemented

- Settings now composes a serialized draft/save owner, credential-sensitive
  catalog owner, navigation and dedicated section modules.
- The workflow editor separates profile-scoped draft/history, guarded profile
  operations (including save-before-export), step actions, model catalogs and
  navigation. Disabling/removing steps share reference cleanup; extraction
  changes use one pure projection before confirmation.
- Parent sizes: ArtifactExplorer 199, SettingsPage 338, PipelinePage 303 lines.
  New feature files remain below 700 lines.
- All 67 focused Settings/editor tests, 610 frontend tests across 81 files, and
  production build pass (`/tmp/pipeline-r3-*.log`). Concurrent work accounts for
  test-count growth. Native development startup encountered a concurrent
  compilation error (subsequently fixed by that work); it will be retried after
  executor validation to avoid competing development rebuilds.

## R4 — complete

- Extracted scheduling, artifact views, checkpoints, path identity, prompts,
  fan-out units, finding lineage/collapse and schema-safe outputs. The async
  runtime remained in the facade until these checks passed.
- Step and merge records share construction in `pipeline/provenance.rs`, with
  explicit roles and unchanged per-caller accounting. Legacy stored products
  retain their original record projection.
- Added characterization coverage for fallback token/tool/attempt accounting,
  effective model provenance, merge roles, saturating attempt conversion and
  legacy records. All 286 Pipeline tests pass (`/tmp/pipeline-r4-pipeline-tests.log`).

## R5 — implemented

- Moved run ownership, supervised unit calls, parallel waves and sequential
  dispatch into separate modules; kept the executor's public API stable.
- Runtime bodies and their ordering are unchanged. Both runtime and preview
  continue to use the same scheduling helpers. Model calls still pass through
  `pipeline/call.rs` and retain their existing cancellation ownership.

The architectural contracts and validation requirements are in
[`../refactoring-plan.md`](../refactoring-plan.md).

## Final validation and qualification

- Rust: **907 library tests and 14 CLI tests passed**, 10 ignored, serial
  `cargo test --locked --all-targets -- --test-threads=1`.
- Strict `cargo clippy --locked --all-targets -- -D warnings` passed.
- All 66 Rust files touched by R0–R5 pass targeted rustfmt checks. Git's
  whitespace check passes. Unrelated repository-wide formatting debt remains
  as recorded at R0.
- Frontend: **610 tests in 81 files passed**, with a successful production build.
  Existing large-chunk build warnings remain; PDF.js/file editors still emit
  separate lazy chunks.
- Compared the bodies of execute_steps, run_parallel_wave, run_sequential_step,
  execute_step_call, capture_response, finish_response_capture and both report
  ingestion helpers against the R4 entry snapshot. They match after import-path
  and whitespace normalization.
- Native development: compiled and launched with `npm run tauri dev`, using
  port 1421 because another process owns 1420, and an isolated Workspace
  development store. The computer-use tool rejected both the absolute debug
  executable path and its process name as invalid app targets; it did not list
  the unbundled app. **Interactive UI smoke remains unverified.** The development
  process was stopped afterward. The installed Pipeline application was not
  opened. Existing invalid legacy profiles emitted startup warnings.
- Logs are `/tmp/pipeline-r5-{rust,clippy,format,diff-check}.log`,
  `/tmp/pipeline-r3-{frontend,build}.log`, and `/tmp/pipeline-refactor-native.log`.

R0–R5 implementation is complete. The [module map](module-map.md) documents the
new owners. Later stages retain the remaining oversized files. No PR or commit
was created; all current and future work in this section stays together for
one PR at the end, as requested.
