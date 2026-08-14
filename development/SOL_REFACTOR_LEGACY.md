# Solution plan: retire dead and legacy Pipeline code

- **Status:** Work packages 1 and 2 plus finding C7 implemented on 2026-08-11; later packages remain proposed
- **Audit date:** 2026-08-11
- **Audited version:** `0.9.0` on the local `main` worktree
- **Scope:** Rust/Tauri application code, React/TypeScript code, profile and settings persistence, report artifacts, release helpers, and direct Rust/npm dependencies

## 1. Objective

Reduce Pipeline's maintenance and build burden by deleting code that has no current caller and by retiring compatibility paths whose source formats are no longer supported. The implementation must not remove a path merely because it looks old. Every deletion must satisfy one of these tests:

1. **Dead:** no current frontend, CLI, backend, test, workflow, or documented operator entry point can reach it.
2. **Duplicate:** it stores or computes a second copy that no current reader consumes.
3. **Compatibility-only:** current writes never produce the format, and the project has explicitly ended support for the old format.
4. **Dependency-only:** a direct dependency has no import, build configuration, script, or runtime use, or a duplicate crate version can be collapsed without changing APIs.

This plan intentionally excludes ordinary redesign, performance work, and removal of features that are merely infrequently used.

## 2. Executive summary

### Implementation status (2026-08-11)

Work packages 1 and 2 are complete. Finding C7 is also complete. The implementation removed the dead
commands/stores/events/setting described in A1-A3, removed both unused npm
root dependencies, and collapsed Pipeline's direct `base64` dependency onto
0.22. The npm lockfile now contains 854 package entries instead of 900, a
reduction of 46 entries, and `base64 0.23.1` is absent from `Cargo.lock`.
The remaining portions of work packages 3-7 were not changed.

Validation completed with 501 Rust tests, 346 frontend tests, 58 release
tests, the production TypeScript/Vite build, locked all-target Cargo checks,
and the desktop startup/Settings e2e test all passing.

The audit found a small, high-confidence core that can be removed without changing the current user-visible product:

- Three registered Tauri commands have no caller: `list_history`, `check_deps`, and `read_cached_paper_text`.
- The old `~/.pipeline/history` module is reachable only through `list_history`; current history is the run store under `~/.pipeline/runs`.
- The global paper-text cache under `~/.pipeline/cache/papers` has a writer but no current reader. Its path is emitted through a `pipeline:preprocess` event that neither the GUI nor CLI consumes.
- The entire `pipeline:preprocess` event family is unobserved.
- `Settings::paddle_render_dpi` is parsed, validated, serialized, and tested but is never read by an extractor.
- `@babel/core` and `@vitest/ui` are unused direct dev dependencies.
- Pipeline directly compiles `base64 0.23.1` even though the graph already contains `base64 0.22.1` and Pipeline uses the API common to both versions.
- The executor still probes for a model-written `report.md` before and after every model attempt even though current prompts require a nonce-delimited terminal response and explicitly prohibit writing the report file.

There is also a much larger compatibility layer: Auto Review v1, pre-unified profiles, a thirteen-generation built-in migration ladder, old report fields, old prompt placeholders, mirrored legacy model-setting fields, retired extraction identifiers/readers, and a duplicate raw-document artifact. These are valid technical-debt targets, but they must be removed only after the project declares a minimum supported on-disk/export version. They are therefore a separate phase, not part of the unconditional cleanup.

### Recommended sequence

| Phase | Scope | Compatibility decision required? | Expected benefit |
|---|---|---:|---|
| 0 | Freeze baseline and define support policy | Only for later phases | Prevents accidental data loss |
| 1 | Dead commands, history store, paper cache/events, unused setting | No | Less code, disk I/O, and API surface |
| 2 | Unused npm packages and duplicate Rust crate version | No | Smaller install/compile graph |
| 3 | Legacy report-file response path and static report markers | Confirm no supported fixture depends on them | Removes per-attempt filesystem probes and branching |
| 4 | Profile, Auto Review, report, settings, and extraction compatibility | Yes | Largest source and maintenance reduction |
| 5 | Resolve orphan release helpers | Product/release-owner decision | Removes dead scripts or restores missing checks |

## 3. Baseline and audit caveat

The worktree was already being changed during this audit. In particular, a Marker extraction-engine removal was in progress across settings, engine management, extraction, frontend components, documentation, and tests. The final observed worktree passed `cargo check --locked --all-targets`, so this plan treats that removal as existing user-owned work. An implementation agent must not restore Marker merely to reproduce an older snapshot, and must not overwrite unrelated dirty files.

Before implementation, capture:

```bash
/bin/zsh -lic 'git status --short --branch'
cd gui
npm test
npm run build
cd src-tauri
cargo test --locked --lib
cargo check --locked --all-targets
```

Run all Git commands according to the repository's `AGENTS.md` instructions. Keep each phase in a separate commit so compatibility deletions can be reviewed independently of proven dead-code removal.

## 4. Audit method and confidence rules

The findings were derived from:

- searches of command registration, `invoke(...)` calls, event emitters/listeners, CLI event handling, symbols, comments, tests, workflows, and documentation;
- inspection of `package.json`, `package-lock.json`, `Cargo.toml`, `Cargo.lock`, `npm explain`, and Cargo reverse-dependency trees;
- inspection of the current run-store, rerun, report rendering, settings normalization, profile migration, extraction, and release workflows;
- successful baseline checks for `npm run build`, `cargo check --locked --all-targets`, and the npm override test.

Classifications used below:

- **A — proven dead/duplicate:** safe to remove after a final reachability search.
- **B — obsolete current behavior:** current producers no longer use the path, but a saved artifact might; verify fixtures or declare a cutoff.
- **C — compatibility-only:** deliberately supports old user state and must remain until a support cutoff is adopted.
- **Keep:** appears legacy or deprecated but is still reachable or protects a current failure mode.

## 5. Findings

### A1. Uncalled Tauri commands and the legacy history store

**Classification:** A — proven dead

**Primary files:**

- `gui/src-tauri/src/commands/config.rs`
- `gui/src-tauri/src/commands.rs`
- `gui/src-tauri/src/lib.rs`
- `gui/src-tauri/src/storage.rs`
- `gui/src-tauri/src/models.rs`
- `gui/src/lib/types.ts`
- `gui/src-tauri/src/runs.rs`

`list_history`, `check_deps`, and `read_cached_paper_text` are registered in the Tauri invoke handler, but there is no frontend `invoke` call, CLI call, or backend call to any of them.

`list_history` is the sole current entry point to `storage::list_reports`. The entire 109-line `storage.rs` module reads the old `~/.pipeline/history` directory. The active history UI and CLI use the run-store APIs under `runs.rs`. `ReportSummary` and `HistoryResponse` exist only for the old command. `runs::delete_run` still scans and removes matching files in `~/.pipeline/history`, even though current runs do not write them.

`check_deps` is also superseded. The current run setup obtains `DepsReport` through `get_execution_plan`, which evaluates the immutable run snapshot using `check_snapshot_dependencies`. `deps::check_all_for` and snapshot-specific checks remain live; only the parameterless `check_all` wrapper becomes dead when the command is removed.

**Removal plan:**

1. Repeat exact-symbol searches for the three command names immediately before editing.
2. Remove their entries from `tauri::generate_handler!` in `lib.rs`.
3. Delete `HistoryResponse`, `list_history`, `check_deps`, and `read_cached_paper_text` from `commands/config.rs`.
4. Remove `ReportSummary` and `storage` imports from `commands.rs`.
5. Remove `pub mod storage` from `lib.rs`, then delete `storage.rs`.
6. Delete Rust `models::ReportSummary` and TypeScript `ReportSummary` if the final search still finds no independent use.
7. Remove the `~/.pipeline/history` cleanup branch from `runs::delete_run`. Deleting a current run should delete only its validated canonical run directory.
8. Remove `deps::check_all` only if a post-edit search confirms that no test or non-command call remains. Preserve `check_all_for`, `check_snapshot`, and execution-plan readiness.
9. Search capability files and tests for command-name strings and remove any stale permission or mock entries.

**Acceptance criteria:**

- No production source mentions `list_history`, `HistoryResponse`, `ReportSummary`, `read_cached_paper_text`, `storage::list_reports`, or `~/.pipeline/history`.
- Run-library listing, opening, deletion, and disk-usage tests still pass.
- The run screen still receives dependency readiness from `get_execution_plan`.

### A2. Duplicate global paper cache and unobserved preprocess events

**Classification:** A — proven duplicate/dead

**Primary files:**

- `gui/src-tauri/src/commands/config.rs`
- `gui/src-tauri/src/commands/run.rs`
- `gui/src-tauri/src/lib.rs`
- `gui/src-tauri/src/emit.rs`
- `gui/src-tauri/src/bin/cli.rs`

`cache_paper_text` writes every extraction to `~/.pipeline/cache/papers/{hash}.txt`, prunes the cache, and returns a path. The only consumer of that return value is the payload of `pipeline:preprocess`. `read_cached_paper_text` is the only reader, and it has no caller. Current runs retain the canonical run-local `context/document.md`; reruns read that copy.

The backend emits `pipeline:preprocess` at extraction, cache, orientation, and bundle milestones. The React application registers listeners for log, stage/pass, routing, and usage events, but not preprocess. The CLI's event adapter does not render preprocess events. Therefore the cache path and every event in this family are currently dropped.

**Removal plan:**

1. Remove `paper_cache_dir`, `validate_paper_hash`, `cache_paper_text`, and `read_cached_paper_text`.
2. Remove the cache write and `cached_paper_path` variable from `commands/run.rs`.
3. Remove all `app.emit("pipeline:preprocess", ...)` calls.
4. Remove the command registration from `lib.rs` and any command mocks/capability entries.
5. Update `emit.rs` and CLI comments/matches so they no longer advertise an event that cannot occur.
6. Do **not** delete or weaken the run-local artifact writes. They are the current rerun and report-workspace source.
7. Do not automatically delete users' existing `~/.pipeline/cache/papers` directory in this refactor. It is safer to stop writing it and document that old cache data may be manually removed. A later explicit storage-cleanup migration may remove it.

**Acceptance criteria:**

- A repository search finds no `pipeline:preprocess`, `cache/papers`, `cached_path`, `cache_paper_text`, or `read_cached_paper_text` in production code.
- A new run and rerun succeed using only run-local context.
- No extraction is performed twice as a side effect of removing the cache.

### A3. Unused `paddle_render_dpi` setting

**Classification:** A — proven dead

**Primary files:**

- `gui/src-tauri/src/settings.rs`
- `gui/src-tauri/src/pipeline_config.rs`
- `gui/src/lib/types.ts`
- settings and provider test fixtures

`paddle_render_dpi` has a default, accepted-value validation, persistence/import handling, TypeScript type, and tests. No extractor or engine reads it. Keeping a UI/persistence field with no effect creates a false configuration contract.

**Removal plan:**

1. Remove the field, its default helper, normalization/validation branch, settings-import copy, TypeScript property, and tests that assert its default or validate its range.
2. Confirm that no settings form currently exposes it; if a stale form element exists, remove it too.
3. Rely on Serde's default behavior of ignoring the now-unknown key when loading older settings. Add one focused test proving an older settings JSON containing `paddle_render_dpi` still loads, if old settings files remain supported.

**Acceptance criteria:** the token occurs only in a historical migration fixture, or nowhere at all.

### A4. Unused direct npm dev dependencies

**Classification:** A — proven unused direct dependencies

**Primary files:** `gui/package.json`, `gui/package-lock.json`

#### `@babel/core`

It is present only because the root project declares it. There is no Babel config, compiler plugin, import, or script. The installed `@vitejs/plugin-react` version uses the Vite/OXC toolchain and does not declare Babel core as a dependency. The direct installed folder is approximately 4.8 MB; the exact transitive lockfile savings must be measured after regeneration.

#### `@vitest/ui`

It is a root dev dependency and an optional peer of Vitest. Pipeline runs `vitest` and `vitest run`; it has no `vitest --ui` script, import, or UI configuration. The direct installed folder is approximately 1.1 MB.

**Removal plan:**

1. Remove both entries from `devDependencies`.
2. From `gui`, regenerate only the lockfile with the repository-pinned npm version, for example `npm install --package-lock-only --ignore-scripts`.
3. Run `npm ci --no-audit --no-fund` from a clean `node_modules` environment in CI or a disposable worktree to prove the lockfile is complete.
4. Verify `npm ls @babel/core @vitest/ui`. An absent optional Vitest peer is expected; neither package should remain because of Pipeline's root declaration.
5. Record the lockfile package-count and install-size delta in the implementation PR rather than promising a size from the two direct folders alone.

**Do not remove:** Tailwind/PostCSS/Autoprefixer, the Markdown/KaTeX/highlighting stack, jsdom/testing-library, or WebdriverIO. All have current source, configuration, scripts, or tests.

### A5. Duplicate direct Rust `base64` version

**Classification:** A — safe graph de-duplication

**Primary files:** `gui/src-tauri/Cargo.toml`, `gui/src-tauri/Cargo.lock`

Pipeline directly requests `base64 = "0.23"`, producing `base64 0.23.1`. The graph already compiles `base64 0.22.1` for reqwest/Tauri dependencies, and Pipeline's use of the `Engine`/`STANDARD` API is supported by 0.22. Aligning the direct requirement to 0.22 should remove one crate version from the graph. `base64 0.21.7`, pulled by `swift-rs`, is independent and is not a target of this change.

**Removal plan:**

1. Change the direct requirement to `base64 = "0.22"`.
2. Update the lockfile, targeting `base64@0.23.1` where Cargo permits a precise package update.
3. Run `cargo tree -i base64@0.23.1`; the expected result is that Cargo reports no matching package (and may return a nonzero status for that reason).
4. Run `cargo tree -i base64@0.22.1` and confirm `pipeline-gui` now appears in the reverse tree.
5. Run tests that exercise API authentication/key storage, archives, and any binary/text encoding sites found by `rg 'base64|STANDARD'`.

**Acceptance criteria:** no `base64 0.23.1` lockfile package and no compilation or serialization change.

### B1. Legacy model-written `report.md` fallback

**Classification:** B — obsolete current behavior

**Primary file:** `gui/src-tauri/src/pipeline/executor.rs`

`ingest_report_file_blocking` opens, bounds, reads, and removes a model-written report file under the step's write directory. `ingest_report_file` runs this in a blocking task. The executor probes before and after each attempt and carries `report_rel` through `StepCallRequest` and its call sites.

Current step execution appends a nonce-delimited terminal-response contract and explicitly instructs the model not to write the report to a file. Current sessions are run-local rather than reused from a historical template. Thus current producers cannot legitimately select this response path, while every attempt still pays for file probes and branching.

**Removal plan:**

1. Add/retain tests proving the nonce envelope accepts exactly one bounded terminal report and rejects missing, duplicate, malformed, or oversized boundaries.
2. Run any saved-run compatibility fixtures. If none writes a root `report.md`, approve removal.
3. Delete both ingest helpers.
4. Remove the pre-attempt and post-attempt ingest calls.
5. Remove `report_rel` from `StepCallRequest` and all constructors/call sites.
6. Keep run-owned write directories and artifact-writing tools; only the special interpretation of `report.md` as the model response is removed.
7. Prefer ignoring or cleaning an unexpected root `report.md` as an ordinary temporary artifact. Do not silently revive it as the response.

**Acceptance criteria:** model output is accepted only through the terminal nonce envelope, retries work, and no per-attempt code probes for `report.md`.

### B2. Static `<!-- REPORT START/END -->` response markers

**Classification:** B — old stored-output compatibility

**Primary file:** `gui/src-tauri/src/output.rs`

`strip_to_report` parses the old static HTML markers and otherwise strips a preamble. Current executor output is already cleaned through a unique nonce envelope before persistence. Current rendering nevertheless calls `strip_to_report`, so static marker handling remains as a passive reader for old reports.

**Removal plan:**

1. Treat this as part of the same compatibility decision as B1, not an independent deletion.
2. Split or retain a narrowly named `strip_preamble`/normalization helper for current calibration and display cases.
3. Replace current rendering/diff/addendum call sites with the current normalization helper.
4. Delete static marker parsing and its incomplete-marker behavior.
5. Delete marker-specific tests; retain tests for preamble stripping, math normalization, and current nonce parsing in the executor.

**Acceptance criteria:** current reports render identically, while only explicitly unsupported old static-marker reports change behavior.

### C1. Auto Review v1 implementation and embedded prompts

**Classification:** C — compatibility-only

**Primary files:**

- `gui/src-tauri/src/auto_review/legacy.rs`
- `gui/src-tauri/src/auto_review.rs`
- `prompts/auto_review/orientation_v1.md`
- `prompts/auto_review/fields/*.md`
- Auto Review migrations/tests in `pipeline_config.rs`

The legacy Rust module is 353 lines. Together with `orientation_v1.md` and thirteen v1 field prompts, it represents 492 lines and about 32 KB of source text. The prompt text is compiled into the binary. Current Auto Review uses the v2 subject/method hierarchy; v1 remains to migrate untouched old adaptive profiles and to interpret old saved selections/reports.

**Removal plan after a cutoff:**

1. Inventory supported saved profiles/runs for v1 specialist IDs and old orientation schema fields.
2. In a bridge release, migrate supported v1 Auto Review profiles to v2 and persist the result. Emit a clear backup/recovery warning if a profile cannot be mapped.
3. Once the minimum supported profile version is v2, remove the legacy module export, fallback labels, specialist-step selection, schema exposure, and prompt lookups.
4. Delete `orientation_v1.md` and `fields/*.md`; preserve `core/`, `methods/`, `subjects/`, v2 `orientation.md`, and `synthesis.md`.
5. Remove the associated known-digest constants, migrations, and fixture tests from `pipeline_config.rs`.
6. Confirm a release build no longer embeds unique v1 prompt strings.

### C2. Pre-unified profile imports and the built-in migration ladder

**Classification:** C — compatibility-only

**Primary file:** `gui/src-tauri/src/pipeline_config.rs`

The file contains:

- `LegacyRefereeConfig`, `LegacyPostStepConfig`, and `LegacyProfileData` plus converters;
- migration from `~/.pipeline/pipeline.json`, `referees.json`, `post_steps`, and old `default.json` layouts;
- imports of `type=referee`, `type=post_step`, separate arrays, bare unversioned profiles, and current envelopes;
- a built-in catalog migration ladder with markers through v13, retired profile IDs, historical prompt hashes, archive logic, and a large associated test matrix.

These branches are not dead while old user profiles/exports are supported. They are, however, the largest concentrated maintenance cost found in this audit.

**Recommended support policy:** keep one explicit, versioned import/migration boundary rather than indefinitely accepting every historical shape. Since the audited repository has a `v0.9.0` tag and notes that earlier development was not maintained as a stable release series, a reasonable policy is:

- current code reads the documented `0.9.0` artifact/profile format and the new current format;
- pre-`0.9.0` development formats must first be opened/exported through a bridge release or a standalone converter;
- unversioned/bare imports are rejected with an actionable message rather than guessed.

The release owner must confirm this policy before implementation.

**Removal plan:**

1. Build a fixture table containing the oldest officially supported profile directory, export bundle, customized built-in, and active-profile selection.
2. Give the current profile/export envelope an explicit schema/version if it does not already carry a sufficient one. Reject future versions and unsupported old versions distinctly.
3. Extract any required one-time migration into a small, boundary-only decoder or standalone conversion command. Do not keep legacy fields in the runtime `PipelineConfig` model.
4. Rewrite stock prompts to current placeholders before deleting alias support (C4).
5. Remove the legacy structs, converters, separate-array loaders, unversioned import heuristics, retired import `type` branches, and pre-cutoff tests.
6. Replace the v3-v13 built-in ladder with one current catalog baseline plus current initialization/reset behavior. Preserve user customization semantics: never overwrite a user-edited prompt just because it differs from the compiled default.
7. Replace historical-hash tests with tests for current initialization, current-version no-op, customized-built-in preservation, and explicit rejection/conversion of unsupported versions.

**Stop condition:** if the implementation cannot distinguish user edits from an obsolete stock prompt, do not delete the relevant known-digest migration until a bridge migration has persisted that distinction.

### C3. Old report data model

**Classification:** C — compatibility-only

**Primary files:**

- `gui/src-tauri/src/models.rs`
- `gui/src/lib/types.ts`
- report construction call sites in commands/runs/tests
- consumers of `PipelineReport::all_outputs` and `final_output`

`PipelineReport` still contains `referee_reports: Vec<RefereeReport>` and `editor: Option<EditorSynthesis>`. Current constructors initialize them empty. `all_outputs` clones `step_outputs` for current reports or synthesizes steps from legacy fields; `final_output` has a legacy editor branch. The TypeScript model retains the corresponding optional data.

After the supported-run cutoff:

1. Verify every supported fixture contains unified `step_outputs`.
2. Remove `RefereeReport`, `EditorSynthesis`, both `PipelineReport` fields, TypeScript fields/types, empty constructor initializers, and legacy tests.
3. Change `all_outputs` to return `&[StepOutput]`, or remove it in favor of direct `step_outputs` access. Update render, export, reconcile, ledger, run-storage, and run-entry call sites to avoid cloning current output vectors.
4. Simplify `final_output` to search `step_outputs` only.
5. Version the report schema or manifest so an unsupported old report fails with a clear message instead of appearing empty.

### C4. Legacy prompt placeholder aliases

**Classification:** C — compatibility for custom profiles

**Primary files:**

- `gui/src/lib/pipelineHelpers.ts` and tests
- `gui/src-tauri/src/pipeline/orient.rs`
- `gui/src-tauri/src/pipeline/executor.rs`
- stock prompts and migration digests in `pipeline_config.rs`

Aliases currently include:

| Legacy alias | Current token |
|---|---|
| `{paper_path}` | `{input_path}` |
| `{paper_text}` | `{input_text}` |
| `{referee_reports}` | `{prior_outputs}` |
| `{editor_synthesis}` | `{last_output}` |

Some stock prompts still use the old paper aliases, so alias removal cannot begin with the substitution code.

**Removal plan:**

1. Rewrite every stock prompt to current tokens.
2. Update the current built-in prompt digests or perform the rewrite through the final migration bridge so customized prompts are not overwritten.
3. Migrate supported custom profiles by token-aware replacement. Back up the original and avoid replacement inside unrelated prose where feasible.
4. Remove alias chips/descriptions and lint allowances from `pipelineHelpers.ts`.
5. Remove alias substitution branches from orientation/executor code.
6. Replace compatibility tests with tests that unsupported tokens are reported precisely.

### C5. Mirrored legacy model settings and settings aliases

**Classification:** C — compatibility-only, except `paddle_render_dpi` in A3

**Primary files:**

- `gui/src-tauri/src/settings.rs`
- `gui/src-tauri/src/pipeline/api_anthropic.rs`
- `gui/src-tauri/src/pipeline/gemini.rs`
- other provider/model-resolution sites
- `gui/src/lib/providers.ts`, `types.ts`, and tests
- settings import code in `pipeline_config.rs`

`claude_model`, `codex_model`, and `gemini_model` mirror the structured CLI/API `ModelSelection` fields. Loading migrates from the string fields, saving synchronizes back into them, providers fall back to them, the frontend clears/reads them, and profile bundle import copies them. This makes legacy representation part of the current runtime and wire model instead of confining it to deserialization.

**Removal plan:**

1. Make structured `ModelSelection` the only runtime/provider/frontend representation.
2. If old settings remain supported for one bridge release, deserialize them into a private `RawSettings`/versioned settings-file DTO, convert once, and persist the current shape. Do not serialize legacy fields again.
3. Remove provider fallbacks and make model resolution consume only the selected structured value.
4. Remove frontend fallbacks/patches and TypeScript legacy fields.
5. Update settings fingerprinting, import/export, defaulting, validation, catalog discovery, and tests.
6. After the bridge window, remove the boundary DTO fields and reject unsupported unversioned settings with a recovery message.

Other compatibility aliases in settings include the Serde alias `llm_provider`, accepted plaintext API-key fields, and the retired extractor string `paddleocr-vl`. Handle them deliberately:

- migrate plaintext keys into the credential store before failing closed; do not silently discard credentials;
- keep `llm_provider` until old settings are outside the cutoff;
- remove the `paddleocr-vl` setting alias only with the old extraction-reader work in C6.

### C6. Retired extraction identifier and old structure-cache reader

**Classification:** C — compatibility-only

**Primary files:**

- `gui/src-tauri/src/pipeline/extract/structured.rs`
- `gui/src-tauri/src/document_bundle.rs`
- `gui/src-tauri/src/settings.rs`
- `gui/src-tauri/src/pipeline_config.rs`

There is a passive reader for an old direct Paddle cache/layout and compatibility handling for extraction method `paddleocr-vl`, while current full parsing uses `paddleocr-vl-full`.

After the artifact cutoff:

1. Add current full-parser structure and bundle fixtures.
2. Remove the old cache-root reader/fallback from `structured.rs`.
3. Remove `paddleocr-vl` deserialization/method normalization and old method branches in document-bundle provenance handling.
4. Update tests and documentation to accept only current public extraction methods.

**Important:** the managed recognition engine/root identifier `paddleocr-vl` is still used internally by the current full parser. Do not rename or delete that current engine merely because its identifier resembles the retired public extraction method. Trace each occurrence by type and call site.

### C7. Duplicate raw-document artifact

**Classification:** C — current artifact-schema compatibility

**Implementation status (2026-08-11): Complete.** New manifests carry
`artifact_schema_version: 1`, write the exact extraction to
`context/document.md` immediately after workspace creation, and never write or
export `extracted_text.md`. The enriched bundle Markdown remains a transient
model input; `document_bundle.json` and `blocks.jsonl` retain the structured
views without duplicating the full text. Rerun and recovery resolve captured
text through a single versioned boundary: schema 1 requires `document.md`,
while schema 0 requires `extracted_text.md` because its old `document.md` may
contain a bundle preamble. Tests cover the early bundle/orientation failure
window and both sides of the version boundary.

**Primary files:** run/rerun storage, export, artifact lists, report workspace, documentation

At audit time, runs retained both `context/extracted_text.md` and `context/document.md`; the latter was the readable document projection with metadata/node context, while the former was the exact extraction view and rerun source. This duplicated the full document for each run and export.

This can be simplified, but only by making one file canonical throughout the lifecycle:

1. Choose `context/document.md` as the canonical current readable text.
2. Ensure it is written immediately after extraction, before orientation/bundle work that may fail, so interrupted runs remain rerunnable.
3. Change rerun, report workspace, primary artifact selection, export, manifest validation/recovery, and tests to use `context/document.md`.
4. Preserve exact extracted content requirements. If node markers/metadata make `document.md` unsuitable as the rerun input, define one canonical raw artifact instead of deleting the wrong copy.
5. Version the run manifest. For supported older runs, either fall back to `extracted_text.md` at a boundary or migrate it once; after the cutoff, remove that fallback.
6. Stop writing and exporting `extracted_text.md`, update UI export copy and docs, then remove old artifact-list/recovery branches.

**Stop condition:** do not remove `extracted_text.md` until bundle/orientation failure tests prove a rerun always has a complete canonical document.

### C8. Manifest-less and per-page artifact recovery

**Classification:** Keep

Some comments describe old page-artifact compatibility and manifest recovery, but these paths also protect current interrupted writes and crash windows. They are not proven dead. Revisit only if artifact publication becomes transactional and tests prove no current run can expose a partial manifest.

### C9. Orphan release helpers

**Classification:** dead operational entry points, but deletion requires release-owner review

**Files:**

- `scripts/release/smoke-poppler.mjs`
- `scripts/release/test-paddle-parser-sidecar.py`
- `scripts/release/validate-macos-machos.sh`
- `scripts/release/validate-signed-tag-binding.sh`

No active workflow, npm script, or release script invokes these files. They should not remain indefinitely as misleading checks. However, two are security/packaging validations whose absence from the workflow may be a regression rather than evidence that the check is unwanted:

- `validate-signed-tag-binding.sh` verifies an annotated, cryptographically verified tag points at the immutable workflow SHA. The active `validate-release-identity.mjs` checks the tag name/version and monotonicity, but not signature verification or tag-object binding.
- `validate-macos-machos.sh` verifies architecture, deployment target, signatures, and relocatable dependency paths. The active workflow signs bundled Poppler but does not run this validator.
- `smoke-poppler.mjs` functionally exercises bundled `pdftotext` and `pdftoppm`, while the active bundler performs only a version probe.
- `test-paddle-parser-sidecar.py` is a fast stubbed sidecar contract test; the active qualification workflow exercises the real qualification script but does not run this unit contract.

**Resolution plan:**

1. For each helper, have the release owner choose **wire in** or **delete** and record the choice in the PR.
2. Wire in validators that encode required release guarantees, with platform guards and tests that assert the workflow calls them.
3. Delete helpers whose guarantee is explicitly out of scope, plus stale documentation/tests mentioning them.
4. Do not label deletion of a missing security check as harmless dead-code cleanup.

## 6. Items reviewed and intentionally retained

| Item | Why it remains |
|---|---|
| `save_all_artifacts` | A comment calls it legacy, but `ExportControls.tsx` invokes it for current core exports. Rename/comment cleanup is optional; deletion is not. |
| WebdriverIO/Tauri e2e packages and Rust `e2e` feature | `test:e2e`, `wdio.conf.mjs`, startup specs, and optional Tauri WebDriver plugins form a current manual e2e path even if normal CI does not run it. Decide test strategy separately. |
| Tailwind, PostCSS, Autoprefixer | Current CSS/build configuration uses them. |
| Markdown, remark/rehype, KaTeX, and highlight dependencies | Imported by the current report renderer. |
| `glob@10.5.0`, `whatwg-encoding` | These were the only lockfile entries carrying npm deprecation notices. They arrive through the live WebdriverIO/Mocha graph. Upgrade their owners; do not delete the e2e stack merely to remove a transitive warning. |
| `character-entities-legacy` | Despite its package name, it is a current Markdown transitive package, not Pipeline compatibility code. |
| Deprecated model-policy catalog entries | They drive current UI/support behavior for known models. Remove only when the policy itself changes. |
| Internal `paddleocr-vl` recognition engine ID | Still part of the current full-parser implementation; distinct from the retired public extraction method. |
| Page-artifact fallback and manifest recovery | Protect current interrupted-run/crash behavior as well as old artifacts. |
| `base64 0.21.7` | Required transitively by `swift-rs`; unrelated to Pipeline's direct 0.23 duplication. |

## 7. Implementation work packages

The following packages are sized so another agent can implement and review them independently.

### Work package 1 — Remove dead API/storage paths

Implements A1-A3. Keep this commit free of schema-cutoff changes.

Checklist:

- [x] Re-run reachability searches.
- [x] Remove three commands and registrations.
- [x] Delete old history module/types/cleanup.
- [x] Delete paper cache and preprocess event family.
- [x] Delete `paddle_render_dpi` while retaining permissive old-JSON loading.
- [x] Update focused Rust/TS tests and stale documentation.
- [x] Verify the affected run, settings, dependency, and desktop startup paths.

### Work package 2 — Slim dependency graphs

Implements A4-A5, preferably as a standalone commit so lockfile changes are auditable.

Checklist:

- [x] Remove `@babel/core` and `@vitest/ui` direct declarations.
- [x] Regenerate npm lockfile with pinned npm.
- [x] Align direct Rust `base64` to 0.22 and regenerate Cargo lockfile.
- [x] Record before/after npm package counts and verify the Cargo reverse tree.
- [x] Run frontend, Rust, release, and encoding/credential tests.

### Work package 3 — Remove obsolete report response adapters

Implements B1-B2 after supported report fixtures are checked.

Checklist:

- [ ] Strengthen nonce-envelope tests.
- [ ] Remove report-file probes and `report_rel` plumbing.
- [ ] Remove static report markers from current rendering.
- [ ] Keep preamble/math normalization.
- [ ] Verify success, retry, timeout, malformed response, diff, addendum, render, and export behavior.

### Work package 4 — Establish the compatibility boundary

This is a release/product task and must precede C1-C7.

Deliverables:

- [ ] A written minimum supported version for settings, profiles/exports, reports/runs, and document artifacts.
- [ ] Representative fixtures at that minimum version.
- [ ] A decision between one bridge release and a standalone converter.
- [ ] Backups and actionable error messages for unsupported user state.
- [ ] A schema/version field wherever the current format is otherwise guessed structurally.

### Work package 5 — Collapse settings/profile compatibility

Implements C1, C2, C4, and C5 in this order:

1. Move old settings/profile parsing to versioned boundary DTOs.
2. Migrate stock and supported custom prompt tokens.
3. Persist current settings/profiles in the bridge version.
4. Remove runtime legacy fields and provider fallbacks.
5. Remove pre-unified imports and the built-in migration ladder.
6. Remove Auto Review v1 code and embedded prompts.

This ordering keeps migration code available until the last supported profile has been converted.

### Work package 6 — Collapse report/run artifact compatibility

Implements C3, C6, and C7 after the boundary exists.

Checklist:

- [ ] Make unified `step_outputs` the only supported report body.
- [ ] Remove old report structs/fields and avoid current-output cloning.
- [ ] Remove retired public extraction-method readers while preserving the current internal recognition engine.
- [x] Select one canonical document artifact and migrate/fallback at the version boundary (C7 complete).
- [ ] Update manifest validation, rerun, export, report workspace, artifact explorer, docs, and fixtures together.

### Work package 7 — Resolve release-script orphans

Implements C9 as a separate release-hardening review. Do not mix these decisions into application cleanup.

## 8. Validation matrix

Run the relevant subset after each work package and the full matrix before merge.

### Static reachability checks

```bash
rg -n 'list_history|read_cached_paper_text|pipeline:preprocess|cache/papers|ReportSummary' gui
rg -n 'paddle_render_dpi' gui
rg -n 'ingest_report_file|report_rel|REPORT START|REPORT END' gui/src-tauri/src
rg -n 'LegacyRefereeConfig|LegacyPostStepConfig|LegacyProfileData' gui/src-tauri/src
rg -n '\{paper_path\}|\{paper_text\}|\{referee_reports\}|\{editor_synthesis\}' gui prompts
rg -n 'claude_model|codex_model|gemini_model' gui/src gui/src-tauri/src
```

Expected results depend on phase: phase 1 tokens should disappear immediately; phase 4 tokens remain until the explicit cutoff work.

### Frontend

```bash
cd gui
npm test
npm run build
npm run test:release
```

When commands, settings transport, run setup, or export behavior changes, build
the embedded e2e-mode binary and then run the desktop spec:

```bash
npm run tauri -- build --debug --features e2e --no-bundle --config src-tauri/tauri.e2e.conf.json
npm run test:e2e
```

### Rust/Tauri

```bash
cd gui/src-tauri
cargo fmt --check
cargo test --locked --lib
cargo check --locked --all-targets
cargo tree -d
```

Run focused tests for:

- settings load/normalize/save and old-settings boundary behavior;
- profile initialization/import/customized-built-in preservation;
- current and minimum-supported run/report fixtures;
- execution nonce parsing, retries, and timeouts;
- new run, rerun, deletion, artifact list/read, export, and recovery;
- current full-parser bundle/provenance behavior.

### Manual acceptance

1. Launch Pipeline with a clean `~/.pipeline` test home and create a run.
2. Restart and open the run from the library.
3. Rerun one step, produce a diff, and export all supported formats.
4. Delete the run and confirm its canonical directory is removed.
5. Launch with a minimum-supported settings/profile fixture and confirm bridge migration or the documented rejection path.
6. Launch with an unsupported fixture and confirm Pipeline gives a precise recovery/conversion message rather than silently dropping fields.

## 9. Expected impact and measurement

The unconditional phase should remove:

- one Rust module (`storage.rs`) and several command/model types;
- one duplicate global document-cache writer/reader and its pruning logic;
- an unused event family and related serialization/emission work;
- one no-op settings field;
- two direct npm dev dependencies;
- one duplicate compiled `base64` crate version;
- after fixture confirmation, two filesystem probes/tasks per model attempt and old static report parsing.

The compatibility phase has the larger source reduction: Auto Review v1 alone accounts for roughly 492 lines/32 KB across code and prompts, and `pipeline_config.rs`, `settings.rs`, `models.rs`, and `output.rs` contain substantial additional branching and tests. Do not use line count as the success criterion. Measure:

- direct and transitive npm package count after a clean install;
- duplicate Cargo packages (`cargo tree -d`);
- clean frontend build time and clean Rust check/release build time on the same machine;
- release binary size, especially after removing embedded legacy prompts;
- per-run artifact bytes before/after document de-duplication;
- number of accepted persistence/import schema variants and migration branches.

## 10. Definition of done

The refactor is complete when:

1. Every phase-1 deletion has a final no-caller search and passing tests.
2. Dependency lockfiles contain no root `@babel/core`/`@vitest/ui` and no `base64 0.23.1`.
3. Current run, rerun, report, diff, export, and deletion flows pass without global paper cache/history state.
4. Current model responses use only the nonce-delimited terminal contract.
5. Any compatibility removal is backed by a documented minimum version, fixtures, migration/conversion path, and explicit unsupported-version error.
6. Current settings/profile/report runtime types contain only current representations; compatibility decoding, if still in a bridge release, is isolated at file/import boundaries.
7. The release owner has either wired in or deleted each orphan helper, with no misleading uncalled validator left behind.
8. Documentation describes only the resulting current artifact, event, setting, and import contracts.
