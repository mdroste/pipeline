# SOL Release Audit — August 9, 2026

## Executive decision

**Current working-tree recommendation: the P0–P2 findings are closed; release only after the required remote four-platform parser qualification is green for the tagged SHA.**

The audited baseline commit (`75ab24ac1c7e267730f06a57e5a8cd2b78b77a4c`) was not releasable: its exact Clippy gate failed, and its new PaddleOCR-VL Full Parser lacked environment isolation, content-locked provisioning, bounded installer I/O, fail-closed model readiness, and real supported-platform qualification. Those findings and their baseline evidence remain documented below.

The remediation in the current working tree closes SOL-01 through SOL-09. Local Rust, frontend, release-resource, sidecar-contract, and macOS arm64 real-parser checks are green, including a fresh pinned install and authenticated extraction of a one-page PDF. The release workflow now requires the same locked-stack qualification on macOS arm64, Windows x86-64, Linux x86-64, and Linux arm64 before publication. The remaining findings are P3 test-signal and performance improvements that can be deferred if release timing requires it.

## Phase 0/1 remediation update — August 9, 2026

All P0 and P1 findings identified below have now been remediated in the working tree. The detailed findings remain as the evidence record for the audited commit; this table records the corresponding changes.

| ID | Status | Implemented remediation |
|---|---|---|
| SOL-01 | **Fixed** | Combined the duplicate math-delimiter branches. The exact locked all-target/all-feature Clippy release gate now passes with warnings denied. |
| SOL-02 | **Fixed** | Added a managed-runtime environment builder that clears ambient Python, pip, uv, Conda, and user `PATH` injection; preserves only an explicit OS/certificate/proxy allowlist; and runs Python with `-I -B`. Regression tests cover malicious Python/package-manager overrides. |
| SOL-03 | **Fixed** | Added exact CPython 3.12.13 artifacts, four PEP 751 platform locks with wheel URLs and SHA-256 values, verified wheel-only `uv pip sync`, a checksum-pinned PP-DocLayoutV3 archive, a new parser release identifier, manifest digest binding, a complete Python license inventory, and optional-runtime components in the release CycloneDX SBOM scanned by the existing release vulnerability gates. |
| SOL-04 | **Fixed** | Replaced line-based installer reads with bounded chunk drains, 64 KiB line caps, 4 MiB per-stream log caps, continued draining after truncation, and bounded post-exit/post-kill joins. Multi-megabyte unterminated-output and inherited-pipe tests cover both failure modes. |
| SOL-05 | **Fixed** | The installer now downloads and verifies the required layout model explicitly, initializes it from the local directory with external model access disabled, and treats any readiness failure as fatal. `installed: true` requires the lock, exact model files, and `layout_ready: true` manifest state. |
| SOL-06 | **Fixed** | Added an ordinary-CI sidecar contract covering probe, warm-up, authenticated server settings, extraction, assets, schema, and incomplete-model rejection. Added a reusable scheduled/manual four-platform real-parser qualification workflow; the release job depends on this matrix. A local macOS arm64 qualification completed a fresh 93-package locked install, offline model initialization, and one-page PDF extraction that reached the loopback recognition server and returned the expected schema. |

The P0/P1 release blockers are therefore closed at code level. The remote macOS, Windows, Linux x86-64, and Linux arm64 qualification matrix must still be green for the tagged SHA; the release workflow now enforces that condition. The Phase 2 findings are closed in the following update; the P3 findings remain open.

## Phase 2 remediation update — August 9, 2026

All P2 findings identified below have now been remediated in the working tree.

| ID | Status | Implemented remediation |
|---|---|---|
| SOL-07 | **Fixed** | Base and parser installs now generate bounded, deterministic per-file integrity manifests for their immutable runtime closures. Readiness validates `install.json` against release-owned constants and verifies both the complete file inventory and every file's content; repeated checks use a strong metadata identity that detects same-size replacements while added modules also fail closed. Fast and Full Parser caches now depend on immutable integrity and sidecar digests rather than path/length/mtime, and reused output is described accurately as schema-validated. |
| SOL-08 | **Fixed** | Formula numbers are matched to equations from page geometry, not array adjacency. Matching requires vertical alignment, plausible side placement, bounded distance, adequate confidence, and a mutual unambiguous nearest candidate. Leading/trailing order, two-column equations, unrelated intervening numbers, low-confidence regions, and ambiguous/orphan numbers are covered; unmatched numbers remain visible text blocks. |
| SOL-09 | **Fixed** | Disk preflight now models additional bytes for the actual install state plus staging/cache headroom: 2.9 GB for base install/repair, 4.5 GB for a fresh combined stack, 2.2 GB for a base-present parser add-on or parser repair/upgrade, and 16 MB for a sidecar-only refresh. Injected-space tests cover each state and the exact pass/fail boundary. |

## Scope and method

This audit covered the repository at commit `75ab24a` and approximately 75,600 tracked lines under the Rust backend, React/TypeScript frontend, release scripts, and GitHub workflows. I reviewed the architecture and current project instructions, then examined these release-sensitive boundaries in particular:

- provider/API dispatch, tool access, path validation, subprocess ownership, cancellation, and output bounds;
- document ingestion, extraction caches, run/artifact persistence, and structured-document generation;
- managed native-engine download, verification, installation, status, upgrade, and uninstall behavior;
- settings/secrets, update checks, frontend async state, artifact rendering, and history navigation;
- build/release identity, dependency policy, third-party notices, SBOM generation, and CI coverage.

The archived `AUDIT_REPORT.md` and `ISSUE_LIST.md` were used only as historical regression checklists. I did not carry their old findings forward without confirming them in the current code.

Priority meanings:

- **P0 — release blocker:** the release cannot pass or a core release invariant is already broken.
- **P1 — high:** credible security, integrity, availability, or shipped-feature failure; fix before release.
- **P2 — medium:** meaningful correctness, resilience, or user-impact risk; fix or explicitly accept.
- **P3 — low:** performance, maintainability, or test-signal weakness with limited immediate impact.

## Prioritized findings

| ID | Priority | Issue | Release effect |
|---|---|---|---|
| SOL-01 | P0 | The exact release Clippy gate fails | Release workflow is guaranteed to fail |
| SOL-02 | P1 | Managed Full Parser inherits ambient Python and uv configuration | Private runtime can be altered or broken by the launch environment |
| SOL-03 | P1 | Full Parser dependencies, Python build, and layout model are not content-locked | Install is not reproducible or fully auditable |
| SOL-04 | P1 | Installer output handling is unbounded and can wait forever after process exit | Install can exhaust memory or hang indefinitely |
| SOL-05 | P1 | Failed layout-model warm-up still produces an “installed” engine | Offline first use can fail after a successful install claim |
| SOL-06 | P1 | No supported platform exercises the real Full Parser install/extract path in release validation | Highest-risk new feature ships without operational evidence |
| SOL-07 | P2 | Installed-engine integrity and extraction-cache identities are too weak | Modified runtimes can be accepted and stale results called verified |
| SOL-08 | P2 | Equation-number association relies only on block adjacency | Structured equations can receive the wrong number |
| SOL-09 | P2 | Full Parser disk preflight uses total stack size even when the base engine exists | Valid installations are rejected on space-constrained machines |
| SOL-10 | P3 | Opening a report eagerly loads Sources code and up to 1 MB of source text | History navigation does unnecessary I/O and memory work |
| SOL-11 | P3 | Passing frontend tests emit unhandled React scheduling warnings | CI signal is noisy and can hide real lifecycle regressions |

## Detailed findings and fixes

### SOL-01 — P0 — The release Clippy gate fails

**Evidence**

- `.github/workflows/build.yml:136-138` and `.github/workflows/release.yml:126-128` both run `cargo clippy --locked --all-targets --all-features -- -D warnings`.
- That exact command exits 101 at `gui/src-tauri/src/document_bundle.rs:785-789` with `clippy::if_same_then_else`: the `\(...\)` and `\[...\]` branches have identical bodies.

**Impact**

This is a deterministic release blocker, not a latent risk. CI and the draft-release quality job cannot pass on the audited commit.

**Recommended fix**

Combine the two delimiter predicates and retain one stripping body. For example, compute `wrapped_in_math_delimiters` from the parenthesis/bracket alternatives, then strip once. Do not suppress the lint; the refactor is trivial and clearer.

**Acceptance criteria**

- `cargo clippy --locked --all-targets --all-features -- -D warnings` passes from `gui/src-tauri`.
- The existing formula-number parsing tests continue to pass.
- The exact command is run locally before tagging, not only after the release workflow starts.

### SOL-02 — P1 — Managed Full Parser inherits ambient Python and uv configuration

**Evidence**

- `gui/src-tauri/src/pipeline/claude.rs:1343-1360` builds subprocesses without clearing inherited environment variables.
- `gui/src-tauri/src/engines.rs:1169-1187` sets a handful of uv/Python variables but does not remove `PYTHONPATH`, `PYTHONHOME`, `UV_INDEX`, `UV_DEFAULT_INDEX`, `UV_INDEX_URL`, `UV_FIND_LINKS`, `UV_CONSTRAINT`, `UV_PYTHON_INSTALL_MIRROR`, `UV_ASTRAL_MIRROR_URL`, or related variables.
- `UV_NO_CONFIG=1` only disables configuration-file discovery; uv still recognizes environment-based index, constraint, mirror, and Python-path settings. See the official [uv environment variable reference](https://docs.astral.sh/uv/configuration/environment/).
- `gui/src-tauri/src/engines.rs:1289-1298` adds overrides to the inherited environment during installation.
- `gui/src-tauri/src/pipeline/extract.rs:3627-3675` likewise launches the managed Python directly for extraction and adds only the small environment returned by `paddle_full_parser_env`; it does not use isolated Python mode or remove inherited Python variables.

**Impact**

The runtime is stored in an app-owned directory, but its behavior is not actually private. A shell, IDE, corporate bootstrap, or unrelated Python installation can inject modules through `PYTHONPATH`/`sitecustomize`, redirect package or Python downloads through uv variables, select constraints or find-links inputs, or make the interpreter unusable through `PYTHONHOME`. This creates correctness and supply-chain risk and contradicts the code comment that a global Python installation cannot alter extraction.

**Recommended fix**

Create a dedicated managed-runtime command builder rather than using the general provider command builder:

1. Start from a sanitized environment. Preserve only required OS variables such as `PATH`, temp-directory variables, locale, certificate/proxy variables deliberately supported by policy, and Windows essentials such as `SystemRoot`.
2. Explicitly remove all `PYTHON*`, `PIP_*`, `UV_*`, virtual-environment, Conda, and package-index variables before setting Pipeline-owned values.
3. Supply the package index and Python download source explicitly. Document whether enterprise proxy and custom-CA variables remain supported.
4. Invoke managed Python with isolation flags where compatible (`-I`, and `-B` if bytecode writes are unnecessary) for inventory, probe, warm-up, and extraction.
5. Ensure logging redacts credentials if an allowed proxy/index URL can contain them.

**Acceptance criteria**

- Tests set a malicious `PYTHONPATH` containing a shadow `paddleocr`/`sitecustomize.py` and prove it is not imported.
- Tests set uv index, constraint, find-links, and Python-mirror variables and prove the generated command ignores them.
- The clean environment works on macOS arm64, Windows x86-64, and Linux x86-64/aarch64.

### SOL-03 — P1 — Full Parser provisioning is not content-locked or reproducible

**Evidence**

- The uv executable itself is versioned and checksum-verified, and the two top-level Python packages are version-pinned (`gui/src-tauri/src/engines.rs:37-46`). Those are good controls.
- The environment is created with `uv venv --python 3.12 --managed-python` (`engines.rs:1605-1613`), which does not select a specific CPython patch build or bind an app-known artifact digest.
- Packages are installed with two top-level requirements only (`engines.rs:1615-1629`). There is no checked-in lock, wheel-only policy, constraints file, artifact hash set, or `--require-hashes` equivalent for the transitive closure.
- `packages.txt` is generated only after resolution (`engines.rs:1639-1665`). Hashing that file proves that the inventory file has not changed later; it does not prove that the installed files came from an expected closure or match expected wheel hashes.
- PP-DocLayoutV3 is downloaded as a side effect of constructing `PaddleOCRVL` (`engines.rs:1674-1694`; `paddle_parser_sidecar.py:623-644`). Pipeline records no expected model revision, file manifest, or digest.
- `THIRD_PARTY_LICENSES.md` explicitly excludes this resolved Python/model closure from the bundled Cargo/npm/Poppler SBOM and advisory process.

**Impact**

Two users installing the same Pipeline release at different times can receive different CPython patch builds, transitive packages, wheels, or model files. A package re-publication, compromised index/mirror, yanked dependency, incompatible new transitive release, or upstream model replacement can make an old signed Pipeline installer behave differently. The release SBOM and `cargo audit`/`npm audit` results do not cover the code that this feature later downloads and executes with user privileges.

**Recommended fix**

1. Generate and check in a platform-specific lock for every supported OS/architecture, including an exact CPython build identifier and all Python artifacts.
2. Install from that lock in frozen mode, prefer wheels only, and validate every downloaded artifact with a release-owned SHA-256.
3. Pin PP-DocLayoutV3 to an immutable revision and maintain a file/hash manifest. Refuse activation if it does not match.
4. Store the lock digest, Python artifact digest, wheel digests, and model-manifest digest in `install.json`.
5. Produce an installed-runtime CycloneDX inventory and license inventory from the same lock, and scan it in release CI. It can remain separate from the bundled-app SBOM, but it should be versioned release evidence.
6. Treat any dependency/model update as a new parser release identifier so caches and upgrades cannot silently cross environments.

**Acceptance criteria**

- A fresh install on each supported target consumes only artifacts named in a committed lock/manifest.
- Reinstalling the same Pipeline release yields the same lock and component digests.
- A changed wheel, Python archive, or model file fails before activation.
- Release evidence includes the optional runtime's packages, licenses, hashes, and advisory scan.

### SOL-04 — P1 — Installer output handling is unbounded and can hang

**Evidence**

- `gui/src-tauri/src/engines.rs:1317-1337` reads stdout and stderr with `AsyncBufReadExt::lines()`. A line is allocated until a newline arrives; there is no per-line or total byte ceiling.
- After a timeout, the code awaits both reader tasks without another timeout (`engines.rs:1346-1360`). It does the same after a normal leader exit (`engines.rs:1366-1376`).
- A child can exit successfully after spawning a descendant that inherited its stdout/stderr pipe. In that case `child.wait()` completes but `lines().next_line()` never reaches EOF, so the install future can remain stuck after the nominal one-hour step timeout.
- The repository already solves both problems elsewhere: `gui/src-tauri/src/process.rs:32-51,111-140` and `gui/src-tauri/src/pipeline/extract.rs:140-154,262-299` drain in chunks with a byte cap and bound post-exit pipe joins.

**Impact**

A buggy or hostile package/build subprocess can emit a huge unterminated line and exhaust backend/webview memory. A leaked descendant can keep a pipe open forever and make install cancellation or timeout appear ineffective. This is especially consequential because package installation is network-driven and the step timeout is 3,600 seconds.

**Recommended fix**

Unify `run_install_step` with a shared bounded async process supervisor:

- read fixed-size chunks, frame lines only within a strict per-line limit, and cap the total logged bytes;
- continue draining after the display cap so the child cannot block on a full pipe;
- mark truncation in the log;
- after leader exit, allow a short EOF grace period, then terminate the owned process group/job and bound the reader joins;
- guarantee PID deregistration on spawn, wait, timeout, cancellation, reader failure, and panic paths;
- consider `UV_NO_PROGRESS=1` to reduce carriage-return/progress traffic.

**Acceptance criteria**

- A test subprocess emitting a multi-megabyte line completes without proportional memory growth.
- A leader that exits while a descendant holds both pipes cannot delay completion beyond the configured grace period.
- Timeout and cancellation tests prove the whole process tree is reaped on Unix and Windows.

### SOL-05 — P1 — Failed layout-model warm-up still produces an installed engine

**Evidence**

- The installer says constructing `PaddleOCRVL` downloads and validates PP-DocLayoutV3 (`gui/src-tauri/src/engines.rs:1674-1684`).
- Any non-cancellation warm-up failure is reduced to a warning, after which the `models` phase is emitted as `done` (`engines.rs:1685-1695`).
- The installer then writes `install.json` and succeeds (`engines.rs:1697-1717`).
- `paddle_full_parser_paths_at` validates the Python/script/inventory/manifest files but does not require or validate the layout-model cache (`engines.rs:367-419`). `engine_statuses` therefore reports the engine installed (`engines.rs:694-715`).

**Impact**

The UI can tell a user that installation completed even though a required model is absent. The first document run then retries a network download, which can fail offline, change behavior after release, or surprise a user who reasonably expected installation to be the only network-dependent step.

**Recommended fix**

Choose one honest state model:

- preferred: make model provisioning mandatory and roll back the parser install if warm-up or hash verification fails; or
- introduce an explicit `packages_installed/model_pending` state, do not allow selection, and provide a resumable model-download action.

In both cases, status should verify the pinned model manifest from SOL-03, and the normal extraction path should never initiate an undeclared download.

**Acceptance criteria**

- Simulated model-download failure cannot result in `installed: true` and a selectable engine.
- After a successful install, extraction of a fixture succeeds with network access disabled.

### SOL-06 — P1 — Release validation never exercises the real Full Parser path

**Evidence**

- Rust tests validate registry entries, checksum syntax, sidecar-refresh manifest logic, parsing helpers, and structured-output normalization (`gui/src-tauri/src/engines.rs:1840-1964`, `pipeline/extract.rs:5500+`).
- Frontend tests mock `install_engine` and engine events (`gui/src/components/EnginesPanel.test.tsx`).
- The build/release workflows run unit tests, compile the app, and exercise general packaged-app UI smoke tests, but never install the managed Python environment, import the real pinned packages, warm the model, or extract a document with the Full Parser.
- The feature declares four supported target combinations, including three operating systems (`engines.rs:196-210`).

**Impact**

The release's largest new platform-dependent dependency stack can pass every gate even if a wheel is unavailable, the Python build is incompatible, a model contract changed, `PaddleOCRVL` arguments drifted, Windows process behavior differs, or the sidecar output no longer matches Rust's schema.

**Recommended fix**

Use two layers of validation:

1. **Per-PR contract test:** run the actual sidecar against a tiny stub `paddleocr` package and a fake authenticated loopback recognition endpoint. Exercise probe, warm-up, extraction, settings serialization, asset emission, malformed output, cancellation, and schema ingestion without downloading gigabytes.
2. **Scheduled/manual release qualification:** on every supported target, perform a fresh locked install, disconnect the network, extract a small PDF fixture, validate deterministic structural invariants, uninstall, and reinstall. Preserve logs, component digests, and output hashes as release artifacts.

The release checklist should require green evidence for every advertised target, not merely successful compilation.

**Acceptance criteria**

- The sidecar contract runs in ordinary CI without external model downloads.
- A full qualification job has passed on macOS arm64, Windows x86-64, Linux x86-64, and Linux arm64 for the exact parser lock intended for release.

### SOL-07 — P2 — Engine integrity checks and extraction-cache identities are too weak

**Evidence**

- The base Q8 installer writes strong expected hashes into `install.json` (`gui/src-tauri/src/engines.rs:1482-1498`), but `paddle_paths_at` later checks only that a server, model, and projector file exist (`engines.rs:343-355`). It does not read the manifest or validate those files.
- Full Parser status verifies the bundled sidecar digest and the digest of `packages.txt`, but not the Python executable, installed `site-packages` contents, or layout-model cache (`engines.rs:367-419`).
- Extraction cache fingerprints identify executables/models only as `path:length:mtime` (`gui/src-tauri/src/pipeline/extract.rs:2032-2040`). The Full Parser fingerprint does not include the package inventory digest or layout-model identity (`extract.rs:3427-3456`).
- Cached structured output is logged as a “verified” cache when it deserializes and passes semantic checks (`extract.rs:3715-3729`), even if the actual parser package/model environment changed without changing the Python launcher path.

**Impact**

File corruption or manual/package modification can leave an engine marked installed. More importantly, a changed Python package or layout model can produce different extraction results while the old cache is still reused because neither appears in the fingerprint. Length/mtime identity is also not a content-integrity boundary.

**Recommended fix**

- Validate `install.json` against release-owned constants before reporting either engine installed.
- Record hashes for the extracted native runtime closure, locked wheel set, exact Python build, and model files. Verify them before first execution after install/update; cache a successful verification result keyed by safe file metadata if hashing multi-GB files on every status refresh is too expensive.
- Make the cache fingerprint depend on immutable manifest/lock/model digests, not mutable paths or the Python launcher's metadata.
- Rename the cache log to “schema-validated” unless content provenance is actually verified.

**Acceptance criteria**

- Altering any package or model file invalidates engine readiness or changes the cache fingerprint.
- Replacing a file while preserving its size and mtime cannot reuse a prior extraction cache.
- Untouched installations do not incur multi-GB hashing on every UI status poll.

### SOL-08 — P2 — Equation-number association relies only on adjacency

**Evidence**

- The sidecar preserves block order, bounding boxes/polygons, and confidence (`gui/src-tauri/src/paddle_parser_sidecar.py:510-553`).
- `add_paddle_structured_nodes` assigns a `formula_number` block to the immediately preceding or following equation solely by array adjacency (`gui/src-tauri/src/document_bundle.rs:922-959`). It does not inspect horizontal alignment, vertical overlap, distance, confidence, or a provider-supplied association.

**Impact**

In multi-column pages, aligned equations, or layouts containing an unrelated numbered region next to an equation in reading order, the bundle can attach the wrong number. That error propagates to source navigation and to downstream LLM context as if it were structured parser truth.

**Recommended fix**

Prefer a stable provider relationship/identifier if Paddle exposes one. Otherwise match candidates on the same page using geometry: require vertical overlap or a small normalized vertical distance, equation-number-like horizontal placement, a maximum distance, and an unambiguous nearest candidate. Preserve unmatched `formula_number` blocks as text/metadata rather than dropping or forcing an association.

**Acceptance criteria**

- Fixtures cover leading/trailing numbers, two adjacent equations, multi-column layout, an intervening unrelated number, low-confidence geometry, and an orphan number.
- Ambiguous cases remain unassociated and visible rather than being silently mislabeled.

### SOL-09 — P2 — Full Parser disk preflight is not incremental

**Evidence**

- The Full Parser's `est_disk_mb` is 3,800 MB and explicitly includes the base Q8 stack (`gui/src-tauri/src/engines.rs:240-250`).
- `install_engine` requires that full amount of currently free space for every non-lightweight parser install (`engines.rs:1747-1769`).
- If the 2,300 MB base engine is already installed, the parser reuses it (`engines.rs:1539-1548`) and needs only the parser runtime/model plus staging headroom, yet the preflight still requires 3,800 MB free.

**Impact**

A machine with the base engine installed and enough space for the parser add-on can be incorrectly rejected. This is likely on laptops because the feature already consumes several gigabytes.

**Recommended fix**

Calculate required *additional* bytes from the components actually missing, plus archive/staging/rollback overhead and a safety margin. Keep total installed size as a separate UI field. Model fresh install, base-present add-on, parser repair, and parser upgrade separately.

**Acceptance criteria**

- Unit tests inject available-space values and cover all four installation states.
- A base-present install succeeds when free space exceeds the parser delta plus headroom but is below 3.8 GB.
- Preflight still conservatively accounts for simultaneous old/new parser directories during rollback-safe upgrades.

### SOL-10 — P3 — Report history eagerly preloads Sources

**Evidence**

- On every run open, `ReportWorkspace` dynamically imports the Artifact Explorer and immediately invokes both `get_run_manifest` and `read_artifact(context/document.md)` (`gui/src/components/ReportWorkspace.tsx:392-418`).
- This happens while the active tab is still `report`; the Sources component itself is configured with `deferInitialArtifact` (`ReportWorkspace.tsx:657-670`).
- Display reads permit up to 1,000,000 bytes of text (`gui/src-tauri/src/runs.rs:24-26`), so the preload is bounded but not free.

**Impact**

Browsing history pays the parse/transfer/allocation cost for a code-split viewer and a potentially 1 MB document even when the user never opens Sources. Fast history navigation can also start obsolete I/O that cannot be cancelled. This weakens the value of `deferInitialArtifact` and code splitting.

**Recommended fix**

Keep the small manifest preload only if Provenance needs it immediately. Load the Artifact Explorer and `context/document.md` on Sources hover/focus/click, or through a cancellable idle prefetch. Abort or ignore obsolete requests when `runId` changes.

**Acceptance criteria**

- Opening a report does not call `read_artifact` for `context/document.md` before Sources is requested.
- Opening Sources still feels immediate after optional hover/idle prefetch.

### SOL-11 — P3 — Frontend tests are not warning-clean

**Evidence**

All 306 frontend tests pass, but the run emits React `act(...)` warnings from:

- `BatchPanel.test.tsx` — preloaded active workflow while synchronization continues;
- `ReportWorkspace.test.tsx` — Sources preload;
- `ArtifactExplorer.test.tsx` — switching artifacts during load;
- `AboutPage.test.tsx` — asynchronous `AboutFooter` update.

Several negative-path tests also print expected error stacks directly to stderr. The current CI treats all of this as success.

**Impact**

`act` warnings usually mean the assertion did not fully synchronize with a state update. Even where behavior is correct, recurring noise makes new unhandled updates and unexpected console errors harder to detect during release review.

**Recommended fix**

Await the relevant state transitions with `act`, `findBy*`, or `waitFor`; explicitly settle mocked listeners/promises and timers before test teardown. Add a test setup guard that fails on unexpected `console.warn`/`console.error`, while individual negative-path tests spy on and assert their expected log messages.

**Acceptance criteria**

- `npm test -- --reporter=dot` produces no React scheduling warnings.
- Unexpected warnings/errors fail the responsible test.

## Verification results

| Check | Result |
|---|---|
| `cargo test --locked --all-targets --all-features` | **Pass** — 461 library tests and 2 CLI tests |
| `npm test -- --reporter=dot` | **Pass with warnings** — 36 files, 306 tests; see SOL-11 |
| `npm run build` | **Pass** — TypeScript and Vite production build |
| `cargo check --locked --all-targets --all-features` | **Pass** |
| `cargo +1.88.0 check --locked --all-targets --all-features` | **Pass** — declared MSRV |
| `cargo fmt --all -- --check` | **Pass** |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | **Pass** — SOL-01 regression closed |
| `npm run test:release` | **Pass** — 57 release-script tests and identity validation for Pipeline 1.0.1 |
| `npm run prepare:release-resources` | **Pass** — generated notices and a 1,442-component CycloneDX SBOM |
| Generated optional-runtime SBOM evidence | **Pass** — 95 PyPI components, 95 wheel hashes, no missing licenses, PP-DocLayoutV3 present, and 98 parser license entries including CPython, uv, and the model |
| `python3 scripts/release/test-paddle-parser-sidecar.py` | **Pass** — ordinary-CI parser contract |
| `python3 scripts/release/qualify-paddle-parser.py` on macOS arm64 | **Pass** — fresh exact CPython/93-package install, verified offline model warm-up, one-page PDF extraction, and one authenticated recognition request |
| Remote four-platform parser qualification | **Pending for release SHA** — required by the release workflow before publication |
| `npm audit --audit-level=low` | **Pass** — 0 known vulnerabilities across 867 dependencies |
| `cargo audit` | **Pass under repository policy** — 0 unignored vulnerabilities/warnings; configured GTK/Tauri maintenance advisories remain explicitly documented in `.cargo/audit.toml` |
| Python AST parse of `paddle_parser_sidecar.py` | **Pass** |
| `git diff --check` | **Pass** |

## What is already strong

Several controls deserve to be preserved while making the fixes above:

- native llama.cpp, uv, GGUF, and projector downloads are pinned and checksum-verified before activation;
- engine installs use staging/backup paths and avoid replacing a working install until validation succeeds;
- subprocess ownership and bounded-output helpers already exist for provider probes and extraction, providing a clear pattern for SOL-04;
- path traversal, symlink, special-file, size, entry-count, and time budgets are applied broadly to user and run artifacts;
- the release pipeline pins third-party GitHub Actions, validates identity, generates offline notices/SBOMs, audits Cargo/npm inputs, and exercises packaged builds;
- the current automated test base is large and fast enough to support focused regression additions.

## Remaining release sequence

1. Cut a clean release candidate and run the required real-parser qualification for the exact SHA on macOS arm64, Windows x86-64, Linux x86-64, and Linux arm64.
2. Rerun all ordinary quality, release-resource, dependency, packaged-app smoke, signing, and provenance gates on the candidate. The P3 frontend work can follow without blocking the release candidate.
3. Tag only after the remote qualification matrix and all required release gates are green.

## Release exit criteria

A release candidate should not be tagged until:

- all P0/P1 findings are closed with regression tests;
- the exact CI and release Clippy command passes;
- managed Python/uv execution is demonstrably isolated from ambient configuration;
- Python, wheel, and layout-model inputs are frozen to release-owned hashes and represented in release evidence;
- install success guarantees offline readiness, or the UI exposes an honest non-ready state;
- the installer is bounded against huge output and inherited-pipe hangs;
- Full Parser qualification has passed on every advertised target;
- all other verification rows above are green, with any remaining warnings explicitly accepted.

## Audit limitations

This was a source, workflow, and local dynamic audit, not a formal security proof. The exact managed parser stack was provisioned and exercised locally on macOS arm64, including real Paddle model initialization and PDF extraction through a controlled loopback recognition endpoint. I did not exercise signed installers on clean Windows or Linux hosts, run the remote four-platform matrix for a tagged SHA, call paid external model providers, perform fuzzing, or complete a manual accessibility/usability study. The release workflow now makes the omitted cross-platform parser qualification a publication prerequisite rather than assuming compilation and unit tests establish runtime compatibility.
