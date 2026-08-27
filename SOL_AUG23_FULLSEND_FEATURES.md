# Pipeline Full-Send Pre-Release Product Audit

**Audit date:** August 23, 2026  
**Scope:** The Pipeline repository as presented on the audit date, including the Rust/Tauri engine, React interface, workflow format, built-in profiles and prompts, report assembly, persistence, exports, tests, release automation, and release documentation.  
**Decision:** **Do not publish the next public release until the P0 gate in this report is closed.**

Pipeline already has a serious foundation: host-enforced structured-output schemas, source snapshots, a response journal, explicit workflow selectors, resumable run storage, deterministic report assembly, atomic export behavior, and a broad Rust and frontend test base. The remaining problem is not a lack of features. It is that the product currently proves execution structure more convincingly than it proves report truth, evidence integrity, privacy claims, exact reproducibility, and release completeness.

This report therefore favors fewer, stronger promises. It identifies functionality to remove where it expands attack surface or distracts from the core product; modifications needed to make existing behavior trustworthy; and additions that make report quality measurable, extensible, and useful in real review workflows.

Line references are navigation aids and will drift as the repository changes. Recommendation IDs should remain stable until the associated acceptance criteria are met.

---

## 1. Audit method and observed baseline

The audit combined a repository-wide architecture review with targeted inspection of the highest-risk paths:

- document extraction and preprocessing;
- provider and model selection;
- adaptive Auto Review routing;
- prompt/tool contracts and provider adapters;
- structured findings, validation, evidence, and rendering;
- run manifests, recovery, history, projects, and exports;
- settings, secrets, network egress, and Tauri capabilities;
- first-run, run setup, progress, failure, and report-reading UX;
- workflow portability and extension points;
- CI, packaging, release publication, support, privacy, and security documentation.

### Test and build baseline

| Check | Observed result | Release interpretation |
|---|---:|---|
| `cargo test --locked --all-targets` | **646 passed** (632 library + 14 CLI) | Strong engine baseline. |
| `cargo fmt --check` | **Passed** | No formatting drift. |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | **Passed** | Strong static Rust baseline. |
| Frontend production build | **Passed** | Build emits large-chunk warnings; `ReportViewer` is approximately 553 kB minified. |
| Release-script tests | **58 passed** | Some passing assertions intentionally require weak CI/release behavior and must be inverted; see R-03. |
| Full frontend suite | **411 passed, 1 timed out**, reproduced twice | The 1,000-match report-search case exceeds the default timeout under full-suite contention; isolated runs pass but are still slow. |
| End-to-end coverage | One startup/settings smoke specification | Too shallow and not wired into the release workflow. |

The audit did **not** perform an external penetration test, a clean-machine installer test, or a live-provider report-quality benchmark, because the repository does not yet contain the harness needed to make those exercises repeatable. Adding those harnesses is part of the release gate.

### Implementation update — August 23, 2026

The first implementation pass addressed the recommendation set requested after this audit. The table below records shipped code, not aspirational status; broader benchmark, packaged-app, assistive-technology, and release-gate proof remains governed by the original **Done when** clauses.

| ID | Implemented in this pass | Remaining proof or follow-on from the full recommendation |
|---|---|---|
| **R-10** | Replaced the shared merge prompt with a domain-neutral contract and made migration recognize the retired academic prompt without overwriting unrelated customization. | Exercise the generic fixture across the legal, policy, business, and technical benchmark set. |
| **M-06** | Added run-store/disk preflight; made the pending manifest, captured context, canonical document, checkpoints, report, and final manifest required durable writes; removed the best-effort ephemeral fallback; and emit Complete only after finalization succeeds. | Packaged fault-injection coverage for permission loss, disk exhaustion, and process interruption on every supported OS. |
| **M-07** | Added required, optional, and quorum dependency policy; separated attempted/completed work from successful dependency satisfaction; blocked downstream synthesis when required coverage fails; and configured Auto Review and Grant Review with explicit requirements. | Surface a richer pre-run quorum editor and exercise partial/failure combinations in packaged E2E. |
| **M-08** | Added the strict host-validated `findings-v2` contract with non-empty IDs and prose, canonical rank/taxonomy/severity/confidence/verification fields, contributing reviewer/call IDs, and locator-bearing discriminated evidence. Invalid semantics fail after provider-schema validation. | Semantic source/quote/hash verification remains M-09 rather than being falsely claimed by schema validation alone. |
| **M-14** | Replaced first-N-byte orientation with a deterministic bounded sample spanning the document start, detected section starts, stratified interior positions, and tail; persisted exact byte ranges, reasons, and omissions; and exposed the coverage in report quality. | Add bundle-native page/asset sampling, routing benchmarks, and the optional pre-dispatch specialist override panel. |
| **M-15** | Added a validator disposition ledger keyed to every candidate finding, including retained, revised, merged, rejected, missing-evidence, and deferred-manual-review outcomes, with before/after hashes and completeness checks. | Benchmark adjudication accuracy rather than treating ledger completeness as proof of correctness. |
| **M-16** | Migrated Grant Review to typed specialist findings, strict synthesis, a dedicated `findings-v2-validation` stage, dependency requirements, canonical products, Issues compatibility, and grant-specific orientation; preserved customized legacy profiles during migration. | Add domain benchmark corpora and renderer-parity fixtures for every built-in workflow. |
| **M-17** | Added a deterministic `ReportQuality` ledger with extraction/orientation coverage, step outcomes, requested/observed tools, providers, schema retries, evidence counts, manual-review counts, limitations, reproducibility hash, and typed terminal status; rendered it in Markdown and the report Provenance panel. | Extend capability negotiation data under M-12 and semantic verification counts under M-09. |
| **M-18** | Bumped the portable schema to v11; added required/secret/validated variables, constrained named inputs, typed named products and export policy, strict placeholder declaration/runtime checks, explicit optional-value markers, secret redaction/re-entry, strict shape validation, and an updated published schema. Arbitrary code remains excluded. | Compile-time subworkflows, signed package metadata, minimum-version/capability declarations, and supported/degraded/migration-required/incompatible import classification remain a later contract increment. |
| **M-22** | Disabled automatic byte retention by default; protected incomplete, resumable, project-linked, annotated, pinned, and manually retained runs; moved manual/automatic deletions to Trash; added restore and deliberate permanent-delete controls for reports and projects; preserved project issue ledgers through Trash; and added project autosave, save-before-selection, and a before-unload draft guard. | Add configurable Trash expiry/audit history, reversible issue-merge history, and atomic searchable multi-project assignment from History. |
| **M-23** | Replaced all production browser `alert`/`confirm` paths with one accessible dialog/toast service; added focus restoration, Escape behavior, live status, progress busy state, roving Arrow/Home/End tab behavior, reduced-motion rules, and forced-color support. | Complete axe plus manual keyboard, VoiceOver/NVDA, zoom, scaling, and minimum-window certification before release. |
| **M-24** | Added a default Basic editor and opt-in Advanced controls, general 100-state undo/redo, Cmd/Ctrl+S and Cmd/Ctrl+Z shortcuts, per-workflow crash-recovery drafts, adjacent save-blocking status, keyboard tab navigation, and retained the DAG overview. | Add jump-to-field validation summaries, call/capability estimates, migration diff preview, and compare/reset history. |
| **M-27** | Replaced competing export buttons with one explicit menu for a safe shareable report, sensitive forensic archive, and custom selection; shareable packages use a strict allowlist with verified evidence, limitations, quality, and redacted provenance; every package is staged atomically with a file manifest and SHA-256 ledger; the UI shows progress, path, checksum, and a backend-restricted Reveal action; print behavior is labelled as a system print/save-PDF flow. | Portable archive import, schema migration, and cross-installation round trips remain A-12. Packaged visual QA must still certify native dialogs on every supported OS. |
| **M-28** | Added History workflow/status/provider/project/date filters, sorting, pagination, usage summaries, and compact overflow actions while retaining Open/Resume; added Older/Newer provenance cards, change summaries, reconciliation disclosure, and diff export; added issue search/decision filters, readiness gating, bulk rationale, unreviewed counts, and complete decision export; made console completion-aware, valid, labelled, and clipboard-error-visible; added responsive reading typography and controls, lazy Sources, drag/drop/recent/clear/replace input selection, and shared Help metadata. | Run automated visual/performance fixtures at 1024×700 and large-history/report scale, then complete manual keyboard, text-scaling, and minimum-window QA on all supported platforms. Dollar cost cannot be shown truthfully in History until the lightweight run index carries model-specific priced call data. |
| **M-29** | Replaced the process-wide discovery mutex with a bounded, 15-minute keyed single-flight registry over provider, transport, normalized endpoint fingerprint, credential fingerprint, and catalog/policy revision. Same-key callers share one cancellation-resistant task and result; unrelated keys run concurrently; completed/expired entries are removed; raw endpoints and credentials never enter the registry. | Add packaged live-provider latency telemetry and cancellation/failure soak tests; the deterministic concurrency and invalidation contract is covered locally. |

Verification after this pass: **651 Rust library tests and 14 CLI tests passed**, **412 frontend tests passed across 44 files**, Rust formatting and Clippy with warnings denied passed, and the TypeScript/Vite production build passed. The existing approximately 554 kB `ReportViewer` chunk warning remains tracked by M-21.

### Priority and effort legend

- **P0 — release blocker:** can expose secrets, misstate privacy, publish incomplete artifacts, corrupt trust in evidence, or prevent a defensible report-quality claim.
- **P1 — required for a high-quality public release:** materially affects usability, recovery, accessibility, portability, or professional output.
- **P2 — near-term hardening:** valuable for scale and polish, but can follow a deliberately limited 1.0 if documented.
- **S / M / L / XL:** roughly less than two days / up to one week / one to three weeks / multi-sprint work, including tests and documentation.

### Action legend

- **REMOVE:** delete a behavior, promise, permission, compatibility path, or product surface.
- **MODIFY:** keep the capability, but change its contract or implementation.
- **ADD:** introduce a missing capability or release control.

---

## 2. Executive release gate

The following conditions should be release-blocking. A release candidate is not ready merely because it compiles or completes a sample review.

The detailed inventory contains **53 recommendations: 10 removals, 29 modifications, and 14 additions**.

| Gate | Required outcome | Primary recommendations |
|---|---|---|
| **G-01 Atomic release** | Every platform builds immutable artifacts; one downstream job verifies the complete set and creates one draft. No matrix job can publish or overwrite assets. | R-01, R-02, A-02 |
| **G-02 Continuous verification** | Required PR/main checks run on supported operating systems and include frontend build/tests, Rust all-target tests, format/lint, dependency/security review, workflow validation, and packaged smoke coverage. | M-01, A-02 |
| **G-03 Truthful release claims** | Release, security, support, privacy, and changelog claims exactly match automated behavior and actual signing/support status. | R-02, M-02 |
| **G-04 Secret boundary** | Saved secrets never round-trip through the webview; endpoint discovery cannot be used to send a saved bearer token to an arbitrary frontend-supplied host. | M-03, M-04 |
| **G-05 Tool isolation** | CLI providers cannot inherit ambient shell, web, MCP, hook, or write capabilities outside Pipeline's explicit contract. | M-05 |
| **G-06 Authoritative egress preview** | The backend resolves every planned call to provider, transport, host, and locality; the preview discloses the exact external destinations and degraded capabilities. | M-04, M-11, M-12 |
| **G-07 Measured report quality** | A versioned benchmark corpus measures major-defect recall, clean-document false positives, routing, evidence accuracy, deduplication, citation integrity, and validation behavior. | A-01, Section 8 |
| **G-08 Verifiable findings** | Every released finding follows a strict v2 schema and contains at least one host-verified locator. Unverified claims cannot masquerade as verified evidence. | M-08, M-09 |
| **G-09 Renderer parity** | Every shipped full-fidelity surface—including canonical JSON, Markdown, print/PDF, and Issues—preserves the same stable IDs, canonical rank, evidence, provenance, and verification status. | M-10, A-07 |
| **G-10 Exact run record** | Each run stores a complete effective workflow, materialized Auto Review graph, exact prompt hashes, resolved provider/model/tool plan, inputs, overrides, and capability state. | M-11 |
| **G-11 Reliable report reader** | The complete frontend suite is deterministic; large-document find does not synchronously create 1,000 DOM nodes per keystroke or freeze the viewer. | M-21 |
| **G-12 Packaged-product proof** | Every advertised installer passes a clean-machine launch, import, fixture review, report rendering, resume, and export smoke test before the draft exists. | A-02 |
| **G-13 Safe retention** | Default settings never permanently purge completed work without informed opt-in; pinned, project-linked, annotated, and recoverable runs are protected and deletions have a grace period. | M-22 |

The gate should be represented as a machine-readable release manifest, not a checklist that can be bypassed by memory.

### Stop-ship epics

The individual P0 labels provide implementation-level acceptance criteria. For go/no-go management, group them into nine stop-ship epics so “P0” remains an executable priority rather than a flat list of unrelated tasks.

| Epic | Scope | Recommendation set |
|---|---|---|
| **E-01 Release integrity** | Continuous checks, truthful claims, immutable artifacts, one verified publisher | R-01, R-02, R-03, M-01, M-02, A-02 |
| **E-02 Secret and egress boundary** | Backend-owned secrets, constrained endpoint drafts, authoritative destinations | M-03, M-04 |
| **E-03 Agent authority** | Isolated CLI configuration/tools and narrow webview open capability | M-05, R-09 |
| **E-04 Durable user data** | Durable-before-complete, in-place recovery, safe retention/Trash | M-06, M-20, M-22 |
| **E-05 Truthful orchestration** | Dependency quorum, capability negotiation, typed degradation/limitations | R-05, M-07, M-12, M-17 |
| **E-06 Finding and evidence integrity** | Strict product contract, semantic verification, citation capture, explicit adjudication | R-06, M-08, M-09, M-13, M-15 |
| **E-07 Reproducible report surfaces** | Actual-call metadata, effective specification, renderer parity, visible quality | R-04, M-10, M-11, A-05 |
| **E-08 Measured report quality** | Versioned corpus, statistical gates, controlled model/prompt promotion | A-01, A-14 |
| **E-09 Reader and packaged reliability** | Deterministic large-report UI and clean-machine end-to-end proof | M-21, A-02 |

Related recommendations intentionally have different verbs but not duplicate ownership: a **REMOVE** item shrinks the unsafe surface immediately; its paired **MODIFY** item defines the durable replacement; an **ADD** item supplies the user-facing or release consumer. In particular, R-05 removes blanket search while M-12 owns capability negotiation; M-17 owns the typed quality ledger while A-05 displays it; M-18 owns the declarative workflow language while A-10 owns the permissioned extension/tool boundary; M-08/M-09/M-13 establish the minimum finding-to-source/call link while A-08 adds the richer human-revision graph; and R-02/M-02/A-02 are one release-integrity epic with separate removal, truth, and automation deliverables.

---

## 3. Foundations to preserve

These mechanisms are worth building upon rather than replacing:

1. **Host-enforced schema dialect.** Pipeline owns the structured-output subset and adapts it across providers instead of trusting provider-specific behavior.
2. **Response journal and call records.** Terminal responses, usage, and call metadata provide the beginning of a durable audit trail.
3. **Source snapshots and run directories.** Inputs are captured within a run, which is the right foundation for locator verification and replay.
4. **Validation pass and deterministic assembly.** A separate validation stage plus host rendering is a better architecture than asking one model to write the final truth directly.
5. **Explicit selector allowlists.** Workflow expressions are constrained rather than becoming arbitrary code execution.
6. **Resumable run state.** The engine already has the primitives needed for user-facing recovery and durable batch execution.
7. **Atomic exports and path checks.** Existing export code takes data loss and path boundaries seriously.
8. **Broad engine tests.** The Rust test suite, formatting, and lint baseline are unusually strong for a pre-1.0 desktop product.

The recommendations below tighten the contracts around these strengths.

---

## 4. REMOVE before release

### R-01 — Remove publication from platform matrix jobs

**Priority / effort:** P0 / M  
**Risk:** Users can see or download a partial release; concurrent jobs can race while creating the same release.

**Evidence**

- `.github/workflows/release.yml:146-184` lets each platform job upload its own asset and then set the release to non-draft.
- The same jobs use `gh release upload --clobber`, allowing an already-named asset to be replaced.
- Parallel jobs contain create-if-missing behavior, so release creation itself can race.

**Remove**

- `gh release create`, `gh release edit`, publication, and `--clobber` from all matrix jobs.
- Any release design in which a platform job has authority to make assets public.

**Replace with**

Matrix jobs should build, sign, hash, and upload immutable workflow artifacts only. A single aggregator, dependent on every required platform job, should verify an expected-artifact manifest and create exactly one draft. Publication should be a separate protected action.

**Done when**

- Killing any platform job leaves no public GitHub release.
- Duplicate asset names fail closed.
- A concurrency test proves two release attempts for the same version cannot mutate one another.

### R-02 — Remove unsigned installers from the official supported set

**Priority / effort:** P0 / L  
**Risk:** The project advertises support and signing assurances that its workflow does not deliver.

**Evidence**

- `.github/workflows/release.yml:139-144` explicitly builds Windows as unsigned.
- `RELEASING.md:16-31` and `RELEASING.md:107-122` describe a substantially stronger signed and attested release process.
- `SUPPORT.md:16-25` describes cross-platform launch verification and an offline Windows WebView2 experience, while `gui/src-tauri/tauri.conf.json:53-58` uses the bootstrapper configuration.

**Remove**

- Unsigned Windows packages from an official, generally available release.
- Unsupported platform/signing claims from user-facing documentation until the automation actually enforces them.

**Decision rule**

Either fund code signing and clean-machine verification for a platform, or label that artifact experimental and keep it out of the official supported matrix. Do not make documentation carry a control that automation lacks.

**Done when**

The artifact manifest records identity, signature verification, checksum, installer smoke result, and support tier for every published asset.

### R-03 — Remove tests that protect weak release behavior

**Priority / effort:** P0 / S  
**Risk:** A green suite currently treats missing controls as intentional invariants.

**Evidence**

`scripts/release/workflow-hardening.test.mjs:42-91` asserts, among other things, manual-only Linux CI, absence of Clippy/audit/MSRV gates, no SBOM/provenance/attestation/smoke gate, unsigned Windows behavior, `--clobber`, and per-job release editing.

**Remove or invert**

Delete assertions that require the release process to remain weak. Replace them with assertions for required PR triggers, protected release aggregation, immutable artifacts, signature verification, checksums, SBOM/provenance, and packaged smoke tests.

**Done when**

The release tests fail if any matrix job can publish, if a required artifact is missing, or if a claimed security control is absent.

### R-04 — Remove misleading provider/model metadata from report mastheads

**Priority / effort:** P0 / S  
**Risk:** A professional report can name the wrong model/provider and a hard-coded effort level, undermining the audit trail.

**Evidence**

- `gui/src-tauri/src/output/render.rs:25-46` derives masthead metadata from preferred settings and hard-codes effort as `default`.
- Actual runs can use multiple providers, fallbacks, transports, models, and efforts; those facts live in call records (`gui/src-tauri/src/models.rs:317-434`).

**Remove**

The current single preferred-provider/preferred-model/`default`-effort claim.

**Replace with**

A concise host-generated provenance summary based on actual calls: workflow/version, completion state, provider/model set, degraded capabilities, effective-configuration hash, and a link to detailed provenance.

**Done when**

Fallback and multi-provider fixture runs produce mastheads that exactly match their call records.

### R-05 — Remove blanket WebSearch from roles that do not require it

**Priority / effort:** P1 / M  
**Risk:** Unneeded web access adds prompt-injection exposure, latency, cost, provider variance, and irreproducible external context.

**Evidence**

- Auto Review assigns web capability broadly in `gui/src-tauri/src/auto_review.rs:970-1055`.
- `prompts/auto_review/synthesis.md:3-16` tells synthesis to use supplied reports, so external search is unnecessary there.
- `prompts/auto_review/validate.md:14` does have a legitimate conditional use: checking candidate findings about external literature against primary sources.
- Provider support is unequal: direct OpenAI/local paths intentionally lack hosted search while other adapters expose it.

**Remove**

WebSearch from consistency, exposition, synthesis, and other steps unless the workflow author declares a substantive need for external sources. Validation should request search only when candidate findings actually contain external-literature claims; without a compatible search capability, those claims should become visibly unverified rather than being silently accepted or causing unrelated findings to receive web access.

**Replace with**

Per-step and, where practical, per-finding `required`, `preferred`, and `forbidden` capability declarations. Literature/contribution roles may opt in; synthesis should normally be `forbidden`; validation should receive a bounded primary-source verification task only for the affected candidate IDs.

**Done when**

The effective run plan and final limitations section show exactly which steps had external access and why.

### R-06 — Remove silent structural repair from current execution paths

**Priority / effort:** P1 / M  
**Risk:** Silent ID creation and implicit product selection make findings unstable and conceal malformed model output.

**Evidence**

- The stock findings-v1 schema already requires a non-empty ID (`gui/src-tauri/src/findings.rs:69-84`). However, loose/custom and legacy parsing at `gui/src-tauri/src/findings.rs:232-346` can create ordinal IDs, and duplicate suffixing remains reachable for current parsed output.
- `gui/src-tauri/src/findings.rs:111-138` can treat any parseable structured step as the findings product when `outputs.findings_step` is missing.

**Remove**

- Ordinal repair for current loose/custom workflows and duplicate-suffix repair for current output paths.
- Implicit findings-product discovery for newly authored workflow schema versions.

**Retain only as**

A versioned legacy import adapter that records the coercion in migration warnings. Current malformed output should trigger one targeted repair attempt and then fail or be marked incomplete.

**Done when**

Duplicate/invalid-ID fixtures fail validation; legacy fixtures migrate deterministically with a visible warning.

### R-07 — Remove the dormant Antigravity CLI implementation and misleading name

**Priority / effort:** P1 / M  
**Risk:** Dead provider code expands maintenance/security surface, while the UI conflates a disabled CLI product with the supported Google API transport.

**Evidence**

- `gui/src-tauri/src/pipeline/antigravity.rs` is a large compiled and tested wrapper but is intentionally unreachable.
- Settings force the Antigravity-named provider through the API path.
- Provider UI labels continue to expose “Antigravity” even though the durable capability is Gemini via Google API.

**Remove**

- The unreachable CLI wrapper, probes, CLI-only settings, and tests.
- “Antigravity” as the durable public provider identity.

**Replace with**

A versioned migration from provider ID `antigravity` to `google`, displayed as **Gemini API**. Preserve old workflow imports through migration, not dead runtime code.

**Done when**

No production path or UI copy mentions the disabled CLI, and old profiles migrate without changing the selected model.

### R-08 — Remove the branded Flappy Bird easter egg from the release build

**Priority / effort:** P2 / S  
**Risk:** A large unrelated game adds bundle weight, global keyboard handling, test/maintenance surface, and person-specific branding to a professional review product.

**Evidence**

- `gui/src/components/AboutPage.tsx:18-66` registers a global Konami-code listener and `gui/src/components/AboutPage.tsx:92` mounts the game.
- `gui/src/components/FlappyBirdGame.tsx` is more than 800 lines and uses `dario`/`sam` pilot identities.

**Remove**

The game, trigger, related assets, and tests from production. If a developer easter egg is genuinely wanted, keep a generic version behind a compile-time development flag and outside release chunks.

**Done when**

The production bundle contains no game code, person-specific identifiers, or global trigger.

### R-09 — Remove blanket `shell:allow-open` from the main webview

**Priority / effort:** P1 / M  
**Risk:** Safe link components validate normal user actions, but a compromised webview can invoke the plugin permission directly.

**Evidence**

- `gui/src-tauri/capabilities/default.json:5-14` grants `shell:allow-open` to the main window.
- Multiple components call the shell plugin directly, including `gui/src/components/SafeMarkdownLink.tsx`, `gui/src/components/ArtifactExplorer.tsx`, `gui/src/components/SettingsPage.tsx`, `gui/src/components/DepsCheck.tsx`, `gui/src/components/UpdateBanner.tsx`, and `gui/src/components/AboutPage.tsx`.

**Remove**

The generic permission and direct component calls.

**Replace with**

Narrow backend commands for:

1. an allowlisted set of Pipeline support/update URLs;
2. validated local artifacts inside the current run; and
3. a user-gesture-bound external citation/link opener.

The third path must preserve legitimate report links currently supported by `gui/src/components/SafeMarkdownLink.tsx:11-48`: validate HTTP(S) and `mailto` schemes, reject embedded credentials/control characters/local-file and custom schemes, show the destination host/address in the UI, and prohibit programmatic bulk opening. Log the destination class/host in diagnostics without recording document-derived query text or mail body.

**Done when**

Direct shell invocation from webview JavaScript is denied, while approved Pipeline links, user-clicked validated citations/mail links, and in-run artifacts still open through tested commands.

### R-10 — Remove academic assumptions from generic merge behavior

**Priority / effort:** P2 / S  
**Risk:** Portable profiles inherit domain language that can distort non-academic workflows.

**Evidence**

- `prompts/merge.md:1-16` speaks specifically about an academic paper.
- Generic profiles inherit this prompt through `gui/src-tauri/src/pipeline_config/builtins.rs:269-286`.

**Remove**

Academic-paper assumptions from the default merge contract. Put domain-specific language in the Academic Review profile.

**Done when**

The generic merge fixture can combine business, legal, policy, and technical inputs without academic terminology; Academic Review output remains unchanged through its explicit prompt.

---

## 5. MODIFY before release

### M-01 — Make CI continuous, cross-platform, and release-representative

**Priority / effort:** P0 / L  
**Risk:** The repository's strongest tests are not required on the changes that can reach users.

**Evidence**

- `.github/workflows/build.yml:3-61` is manually dispatched, Ubuntu-only, and runs a subset of frontend/Rust checks.
- It does not build the production frontend, run Rust all-target/all-feature lint gates, exercise macOS/Windows, install a package, or launch the packaged application.
- `gui/e2e/startup.spec.mjs:1-17` only covers launch, dependency-modal dismissal, and opening Settings; the release workflow does not run it.

**Modify**

- Trigger required checks for pull requests, protected-branch pushes, and release candidates.
- Use a fast Linux lane for every change and supported-platform lanes for packaging-sensitive changes; require the full supported matrix before release.
- Run the production TypeScript build, complete frontend suite, release-script suite, Rust all-target tests, format, Clippy with warnings denied, dependency/license/security policy, schema fixtures, and workflow validation.
- Add packaged launch/import/render/export tests rather than treating a development-server launch as installer proof.

**Done when**

Branch protection requires the checks, the same commit is promoted into release packaging, and a red platform or quality lane makes release creation impossible.

### M-02 — Make release, security, support, and changelog text executable truth

**Priority / effort:** P0 / M  
**Risk:** Material discrepancies between documentation and automation create legal, security, and support exposure.

**Evidence**

`RELEASING.md:3-5,16-31,107-122`, `SECURITY.md:48-67`, `SUPPORT.md:16-25`, and `CHANGELOG.md:244-268` claim controls such as signed tags/installers, broad platform verification, SBOMs, scans, checksums, provenance/attestation, retained drafts, and offline runtime behavior that are not enforced by `.github/workflows/release.yml`. By contrast, `PRIVACY.md:47-54` candidly distinguishes provider/API behavior and notes that an OpenAI-compatible endpoint is local only when its URL actually is; preserve that language and make the UI equally precise.

**Modify**

1. Define a single machine-readable release policy: supported platform, artifact name, signing identity, required tests, hash, SBOM, provenance, smoke result, and support tier.
2. Generate the human release checklist and artifact table from that policy.
3. Fail documentation tests when prose claims a control absent from the policy/workflow.
4. Until controls exist, narrow the prose rather than implying manual assurance.

**Done when**

Every objective release/security claim points to an automated job or a clearly identified human approval record.

### M-03 — Keep persisted secrets behind the Rust boundary

**Priority / effort:** P0 / L  
**Risk:** A webview compromise can read provider keys because saved plaintext values are deliberately returned to JavaScript.

**Evidence**

- `gui/src-tauri/src/commands/config.rs:39-49` returns the complete Settings value.
- `gui/src-tauri/src/settings.rs:1124-1167` decrypts all saved keys before that return.
- `gui/src/components/SettingsPage.tsx:125-135` stores the values and a JSON snapshot in React state.
- `gui/src/lib/types.ts:658-670` includes all key values in the webview-visible type.

**Modify**

- Introduce a `PublicSettings` DTO containing non-secret settings, `has_*_key`, and at most a non-sensitive masked suffix.
- Implement explicit secret operations: **unchanged**, **replace**, and **clear**. Do not use an empty form field to ambiguously mean two states.
- Keep persisted credentials backend-owned for model discovery and calls.
- Scrub secrets from logs, error chains, diagnostics, clipboard actions, crash payloads, and serialized frontend state.
- Add IPC contract tests that deserialize every settings/catalog response and reject secret-shaped fields.

**Done when**

No command callable by the main webview can retrieve a saved plaintext credential, and a memory/IPC snapshot test contains none.

### M-04 — Re-contract custom endpoints and egress disclosure

**Priority / effort:** P0 / L  
**Risk:** The current custom-endpoint flow can send a bearer token to a frontend-supplied HTTPS host, while the preview cannot state the actual network destination.

**Evidence**

- `gui/src-tauri/src/commands/config.rs:56-68` accepts a complete unsaved Settings object for catalog discovery.
- `gui/src-tauri/src/model_catalog.rs:581-601` performs an authenticated `/models` request with the supplied endpoint/key.
- `gui/src-tauri/src/settings.rs:1186-1230` permits arbitrary HTTPS hosts; `gui/src-tauri/src/settings/tests.rs:329-348` explicitly accepts `https://models.example.com/v1`. Redirects being disabled in `gui/src-tauri/src/pipeline/api_common.rs:29-52` is a useful existing defense, but does not close the webview-to-host channel.
- `gui/src/components/RunPreview.tsx:147-182` inserts `default provider` and reasons from provider labels. The green local-only branch is effectively unreachable, yet the exact remote host remains hidden.
- `gui/src-tauri/src/commands/config.rs:137-152,247-263` returns no resolved endpoint/locality per planned call.

**Modify**

- Rename the UI concept **Local** to **OpenAI-compatible endpoint**.
- Parse and classify the endpoint in Rust as loopback, private-network HTTPS, or public HTTPS; display **On this computer** or the exact remote hostname.
- Keep saved keys backend-owned. For a newly typed key, create a short-lived backend draft bound to endpoint, revision, window/session, and expiry; catalog calls use an opaque draft token.
- Require explicit acknowledgement before a bearer token or document can be sent to a non-loopback custom host.
- Generate an immutable backend plan listing every logical call's provider, model, transport, hostname, credential use, artifact classes, hosted tools, fallback, retry, merge, and validation calls. Bind confirmation to its fingerprint.

**Done when**

Preview tests cover loopback, private and public HTTPS, defaults, fallbacks, merges, orientation, and WebSearch, and the UI never infers locality from provider ID alone.

### M-05 — Isolate every CLI provider's tools and configuration

**Priority / effort:** P0 / XL  
**Risk:** A hostile document can exploit tools or integrations inherited from a user's Claude or Codex configuration outside Pipeline's declared contract.

**Evidence**

- `gui/src-tauri/src/pipeline/claude.rs:652-658` correctly notes that `--allowedTools` controls approval rather than complete availability.
- The deny construction at `gui/src-tauri/src/pipeline/claude.rs:659-688` blocks some web/write paths, but not Bash, WebFetch, user MCP tools, or hooks.
- Calls use `--permission-mode acceptEdits` (`gui/src-tauri/src/pipeline/claude.rs:794-830`) and inherit normal configuration/environment (`gui/src-tauri/src/pipeline/claude.rs:869-880,1512-1585`).
- Codex applies useful search and filesystem-sandbox overrides, but still launches from the normal configuration/environment without an explicit empty MCP/config-source boundary (`gui/src-tauri/src/pipeline/codex.rs:500-630`).

**Modify**

- Launch each CLI with isolated settings/config sources and a strict empty MCP configuration where it supports them, while retaining only the authentication material required for the selected account.
- Start from deny-all and explicitly permit the exact Pipeline-owned tools for the step. Deny Bash, WebFetch, MCP namespaces, hooks, and all undeclared built-ins.
- Scope reads to concrete snapshotted inputs/artifacts, not bare global `Read`.
- Permit writes only in a producer-owned output directory and only for steps whose contract requires an artifact.
- If this isolation cannot be guaranteed for the installed CLI version, mark that transport unavailable rather than silently inheriting ambient authority.

**Done when**

Integration fixtures with hostile project/user Claude and Codex settings plus prompt-injected document content cannot invoke shell/network/MCP, read outside selected roots, or write outside the producer directory.

### M-06 — Make run persistence durable-before-complete

**Priority / effort:** P0 / L  
**Risk:** Pipeline can spend model time, show **Complete**, and return a report that was never durably saved.

**Evidence**

- Persistence is described as optional/best-effort in `gui/src-tauri/src/commands/run_storage.rs:3-23` and `gui/src-tauri/src/runs.rs:586-588`.
- Workflow/document writes warn and continue (`gui/src-tauri/src/commands/run.rs:188-212`); checkpoint failures only log (`gui/src-tauri/src/pipeline/executor.rs:91-109,176-193`).
- Final report/manifest write failures can return no run ID (`gui/src-tauri/src/commands/run_storage.rs:148-225,277-287`), while `gui/src-tauri/src/commands/run_entry.rs:61-84` still emits the terminal Complete stage.
- Extraction begins before workspace creation (`gui/src-tauri/src/commands/run.rs:107-164`), so paid work can precede discovery that storage is unusable.

**Modify**

1. Preflight the run-store path, atomic rename support, permissions, and conservative disk headroom before extraction/model work.
2. Treat the pending manifest, source/workflow snapshot, canonical document, checkpoints, `report.json`, `report.md`, provenance, and final manifest as required writes.
3. Emit Complete only after the final manifest is durable.
4. If finalization fails, keep checkpoints and return a recoverable **Could not save final report** state with run location and retry action.
5. If ephemeral execution is desired, make it an explicit opt-in mode and watermark the result **Not saved**.
6. Surface startup recovery failures currently ignored at `gui/src-tauri/src/commands/run_entry.rs:103-105`.

**Done when**

Disk-full, permission-loss, interrupted-write, and final-rename fixtures never produce a false Complete state and always leave either a recoverable run or no paid work.

### M-07 — Add required/optional/quorum semantics to dependencies

**Priority / effort:** P0 / L  
**Risk:** A schema-valid final report can be synthesized after required reviewers failed, with missing inputs silently omitted.

**Evidence**

- A dispatched parallel step is placed in the done set even when it fails (`gui/src-tauri/src/pipeline/executor.rs:1008-1037`).
- Downstream readiness checks the done set (`gui/src-tauri/src/pipeline/executor.rs:1280-1289`), while missing artifacts disappear during staging (`gui/src-tauri/src/pipeline/executor.rs:471-508`).
- The warning at `gui/src-tauri/src/pipeline/executor.rs:3288-3301` does not prevent synthesis.
- Auto Review synthesis selects prior reports (`gui/src-tauri/src/pipeline_config/builtins.rs:225-263`) and can therefore run on an incomplete evidence base.

**Modify**

- Distinguish order-only dependencies from required data, optional data, and quorum groups.
- Require all core Auto Review passes plus a declared specialist quorum before consolidation.
- Stop before synthesis when quorum fails; preserve partial reports and offer resume.
- Pass optional failures downstream as structured inventory, never as silent absence.
- Include dependency satisfaction and omitted inputs in the deterministic limitations section.

**Done when**

Fixtures with failed core reviewers cannot generate a complete report; optional failure fixtures generate a visibly degraded report with the exact missing coverage.

### M-08 — Introduce a strict, versioned `findings-v2` contract

**Priority / effort:** P0 / XL  
**Risk:** Structurally valid findings can be empty, unlocatable, unstable, and too academic-specific for extension.

**Evidence**

- `gui/src-tauri/src/findings.rs:44-109` does not require a locator within each evidence object and permits an empty evidence array.
- Specialist schema construction at `gui/src-tauri/src/auto_review.rs:804-835` can accept `[{}]` as evidence; several required prose fields do not have non-empty constraints.
- The source limit is eight (`gui/src-tauri/src/findings.rs:18-21`) even though Full Auto can have up to nine contributing reviewer reports.

**Modify**

Define a versioned product schema with:

- non-empty stable finding ID, title, problem, consequence, recommended action, and evidence;
- canonical global rank, severity, confidence, verification status, and a workflow-declared taxonomy;
- source call IDs and contributing reviewer IDs without an arbitrary cap below the workflow's fan-out;
- an optional disagreement/adjudication record, not a claim that reviewer agreement proves correctness;
- a discriminated evidence union:
  - **document:** source hash, page and/or line range, node ID, exact quote;
  - **artifact:** run-relative path, asset ID, content hash, fragment;
  - **web:** URL or DOI, title, publisher, access time, query/call ID, captured snippet hash.

Keep provider schemas within Pipeline's supported dialect; enforce cross-field and semantic rules host-side.

**Done when**

`{}`, empty strings, empty evidence, duplicate IDs, invalid union variants, and out-of-contract categories fail fixtures consistently on every provider adapter.

### M-09 — Verify evidence semantically before it reaches the report

**Priority / effort:** P0 / XL  
**Risk:** A plausible locator can be parsed and displayed without proving that it points to the cited material.

**Evidence**

`gui/src-tauri/src/findings.rs:349-412` bounds/coerces fields but does not verify page existence, line/node/asset identity, quote match, or content hash. Pipeline already performs useful best-effort capture for safe cited `source_path` files and writes a saved `artifact_path` back into evidence (`gui/src-tauri/src/runs.rs:715-864`; call site `gui/src-tauri/src/commands/run.rs:749-759`). The remaining gap is that capture is source-path-specific and best-effort; it does not semantically verify page, line, node, asset, quote, or hash locators across every finding.

**Modify**

Create a host-owned evidence verifier that:

- validates page and line ranges against the normalized bundle;
- resolves node IDs, asset IDs, and source paths inside the immutable run snapshot;
- verifies source/content hashes;
- exact-matches normalized quotes, or uses a documented fuzzy threshold while retaining the matched fragment and score;
- validates URL/DOI syntax, captured metadata, citation span, and originating call;
- records `verified`, `partially_verified`, `unverified`, or `invalid` with a machine reason.

Invalid locators should trigger at most one targeted repair call. Claims that remain unverified may be retained only in a clearly separated **Items requiring manual verification** section, never as normal evidence.

Define quote verification precisely: Unicode, whitespace, soft-hyphen, line-break, and documented OCR normalization may produce `verified_with_normalization` only when the structural locator and source hash also resolve. At least 98% of quote-bearing benchmark locators should match exactly after that normalization. The remaining at most 2% may use a preregistered fuzzy threshold and must carry the match score; anything below it is `partial`/`unverified` and moves out of the main report.

**Done when**

The benchmark contains no out-of-range page, nonexistent ID, path escape, mismatched source hash, or fabricated verified citation.

### M-10 — Preserve canonical rank, evidence, and provenance in every surface

**Priority / effort:** P0 / L  
**Risk:** The same run tells different stories depending on whether it is read as Markdown, PDF, or Issues.

**Evidence**

- `gui/src-tauri/src/findings.rs:482-526` renders category, a new sequential number, title, and body while dropping stable ID, priority, sources/reviewers, and evidence.
- It groups by category even though array order is authoritative.
- `gui/src/components/IssuesTable.tsx:196-202` re-sorts by severity.
- Evidence navigation in `gui/src/components/ReportWorkspace.tsx:570-577` primarily handles page/artifact paths, not every node/asset/source locator.

**Modify**

- Make canonical global rank the default everywhere; category/severity grouping is an explicit alternate view.
- Render the same stable ID, rank, severity, problem, consequence, recommendation, evidence, source reviewers/calls, confidence, verification, and limitations in every full-fidelity format.
- Deep-link page, line, node ID, asset ID, source path, and captured web source.
- Test parity from one canonical JSON fixture rather than independently snapshotting divergent renderers.
- Add golden visual fixtures covering long titles, equations, tables, evidence blocks, page breaks, footers/page numbers, links, dark/light UI, and print output. Fail on clipped content, orphan headings, split evidence records, unreadable contrast, missing glyphs, or pagination drift outside an approved tolerance.

**Done when**

Finding IDs and order are byte-for-byte identical in canonical JSON and structurally identical across every shipped full-fidelity surface (currently Markdown, print/PDF, and Issues). Any later HTML, DOCX, or other full-fidelity renderer must pass the same semantic and visual fixtures before it is enabled.

### M-11 — Persist the complete effective run specification and make reruns explicit

**Priority / effort:** P0 / XL  
**Risk:** The stored fingerprint cannot reconstruct what actually ran, and rerun can silently use a changed active profile.

**Evidence**

- Stored workflow JSON omits inherited settings (`gui/src-tauri/src/pipeline_config/workflow.rs:4-45`).
- Run overrides/defaults are applied after parts of canonicalization (`gui/src-tauri/src/commands/run_context.rs:147-243`; `gui/src-tauri/src/commands/run.rs:48-59,188-204`).
- The manifest lacks the complete materialized Auto Review graph and effective configuration (`gui/src-tauri/src/runs.rs:222-294`).
- Rerun intentionally uses the current profile instead of the saved workflow (`gui/src-tauri/src/commands/rerun.rs:5-12,211-245`) and can start without the normal plan review (`gui/src-tauri/src/commands/rerun.rs:14-28`).

**Modify**

Write immutable `effective-workflow.json` and `execution-plan.json` artifacts containing:

- fully expanded steps and adaptive specialists;
- exact expanded prompt/system text per call, or content-addressed immutable snapshots of every template/source plus expansion inputs sufficient to reconstruct the exact bytes; retain hashes for identity and deduplication rather than treating hashes alone as replay data;
- normalized tool/output schemas and request-envelope fields that materially affect provider behavior;
- resolved agent, provider, model, effort, transport, endpoint classification, timeout, retry, and fallback;
- requested/delivered tools and capabilities;
- extraction settings, variables, extra inputs, one-run overrides, and merge/quorum policy;
- catalog revision plus prompt/catalog/workflow schema versions;
- normalized source, extraction, and artifact hashes.

Store the effective-spec hash in the manifest, report, and exports. Rerun should offer two explicit choices:

1. **Replay saved specification** — preserve workflow/provider policy where still compatible.
2. **Run with current workflow** — create a new revision and show a diff plus fresh immutable preview.

Neither choice should start before readiness, egress, reuse/invalidation, and capability changes are reviewed.

**Done when**

A deterministic replay test reconstructs the same graph, exact prompt bytes, inputs, request contracts, and capability plan from the complete forensic export alone. Shareable exports may redact prompt/source content but must state that they are not replay-complete.

### M-12 — Negotiate model/transport capabilities and record every degradation

**Priority / effort:** P0 / XL  
**Risk:** Pipeline can silently run a materially weaker review than the workflow promises.

**Evidence**

- Auto Review requests WebSearch broadly (`gui/src-tauri/src/auto_review.rs:970-1053`).
- OpenAI Chat Completions does not expose it (`gui/src-tauri/src/pipeline/api_openai.rs:20-75,586-588`), while Anthropic/Google may retry without hosted search (`gui/src-tauri/src/pipeline/api_anthropic.rs:319-349`; `gui/src-tauri/src/pipeline/api_google.rs:275-309`).
- Local servers retry some tool-bearing 400 responses without tools (`gui/src-tauri/src/pipeline/api_common.rs:1412-1498`), which can also remove the only path to a document supplied by file.
- The catalog has a capability field (`gui/src-tauri/src/model_catalog.rs:26-46`) that is dropped from `ResolvedModel` (`gui/src-tauri/src/model_catalog.rs:77-88`; `gui/src-tauri/src/model_catalog/resolution.rs:186-200`).

**Modify**

- Carry capabilities through catalog resolution at provider + transport + model granularity.
- Let workflows declare each capability `required`, `preferred`, or `forbidden`.
- Required mismatch fails preflight with model/provider remedies. Preferred mismatch requires visible consent and creates a degradation record. Forbidden tools must be unavailable, not merely unapproved.
- Never retry without a tool that is the only route to required input. Inline bounded content or fail.
- Normalize typed degradations such as `search_unavailable`, `tool_removed`, `missing_upstream`, `merge_failed`, `context_cache_fallback`, `truncated_input`, and `persistence_failed`.
- Use terminal states `done`, `degraded`, `partial`, `failed`, `cancelled`, and `unsaved` rather than deriving completion solely from `failed_steps`.

**Done when**

Every requested, delivered, denied, and downgraded capability appears per call and in the host-generated report limitations. No provider adapter can silently weaken a required contract.

### M-13 — Preserve hosted-search citations as first-class evidence

**Priority / effort:** P0 / L  
**Risk:** Reports can cite external material without a durable, auditable connection to the provider call and captured source.

**Evidence**

Provider responses are deserialized, but usable citation provenance is reduced or discarded: Anthropic terminal handling extracts text from content blocks without retaining citation fields (`gui/src-tauri/src/pipeline/api_common.rs:1156-1167,1333-1341`), while Google grounding metadata is reduced to unique query counts (`gui/src-tauri/src/pipeline/api_common.rs:1196-1242`). The durable findings contract lacks a normalized web-citation object, and the response journal does not retain the complete citation chain (`gui/src-tauri/src/pipeline/response_journal.rs:166-263`).

**Modify**

For every external citation, persist:

- query and provider tool/call ID;
- result title, canonical URL and/or DOI, publisher/host, and access time;
- response citation span and finding ID;
- permitted captured snippet plus hash;
- verification/resolution result and any redirect/canonicalization metadata.

Do not claim that a URL's existence validates the substantive claim. Distinguish **source resolved**, **quote matched**, and **claim corroborated**.

**Done when**

Every external assertion in the benchmark traces to a durable citation object; missing or fabricated citations are rejected before release rendering.

### M-14 — Make orientation and routing representative, bounded, and evaluable

**Priority / effort:** P1 / L  
**Risk:** Long-document tails and overloaded routing prompts can omit methods, results, robustness sections, conclusions, and appendices that determine specialist selection.

**Evidence**

- Orientation is limited to the first 250,000 bytes (`gui/src-tauri/src/pipeline/orient.rs:4,120-137`).
- `prompts/auto_review/orientation.md:1-45` asks one call to inventory the document, enumerate formal content, and select roles from a large catalog.
- Run-specific materialization lives in `gui/src-tauri/src/auto_review.rs:1103-1233`, with routing prompt expansion/schema construction at `gui/src-tauri/src/auto_review.rs:1246-1398`; neither has a benchmarked accuracy contract.

**Modify**

1. Build a deterministic section-aware inventory from the document bundle.
2. Sample abstract/introduction, every section heading/start, methods, results, conclusion, appendix headings, asset/table/equation inventory, and the document tail.
3. Produce a bounded role shortlist from deterministic features/catalog metadata.
4. Use a separate structured adjudication step only for ambiguous role choices.
5. Persist included pages/byte ranges, omitted material, candidates, scores/reasons, and final selection.
6. Add an optional pre-dispatch panel for high-stakes runs so users can inspect and override chosen specialists without editing workflow JSON.

**Done when**

Routing has independent subject/method accuracy metrics, appendix-dependent fixtures choose the correct specialist, and user overrides become immutable provenance.

### M-15 — Make validation dispositions explicit and auditable

**Priority / effort:** P1 / L  
**Risk:** A final subsequence does not explain whether a finding was fixed, merged, rejected, or lost.

**Evidence**

- `prompts/auto_review/validate.md:1-18` asks for a filtered/repaired final set.
- `gui/src-tauri/src/pipeline/executor.rs:1692-1761` mainly verifies relative-order/subsequence properties rather than semantic evidence or disposition completeness.

**Modify**

Require a validator ledger keyed by every input finding ID with one disposition:

- `retained`;
- `revised` (with before/after hash);
- `merged_into` another stable ID;
- `rejected_false_positive`;
- `unverified_missing_evidence`;
- `deferred_manual_review`.

Keep the ledger in provenance, not the author-facing issue list. Separate adjudication from prose cleanup so the benchmark can score false-positive rejection and true-positive retention.

**Done when**

Every proposed finding has exactly one disposition, all merges are traceable, and no item can disappear without a machine reason.

### M-16 — Bring every built-in review profile to the same quality contract

**Priority / effort:** P1 / L  
**Risk:** “Grant Proposal Review” and generic workflows can produce free-form Markdown without the evidence, validation, annotation, and export guarantees advertised elsewhere.

**Evidence**

- Grant Review is configured as free-form Markdown without canonical findings or a dedicated validation product (`gui/src-tauri/src/pipeline_config/builtins.rs:300-343`).
- Generic profile behavior inherits assumptions and compatibility fallbacks that are not visible to users.

**Modify**

- Define a structured taxonomy and evidence contract for every shipped review workflow.
- Give each a validation/adjudication stage, canonical findings product, limitations section, and Issues compatibility.
- State profile-specific quality claims and benchmark coverage in the profile picker.
- Keep free-form output only as an explicitly named auxiliary product, not the source of truth.

**Done when**

Every built-in profile passes schema/evidence/render parity fixtures and has benchmark documents covering its intended domain.

### M-17 — Add a deterministic coverage, degradation, and limitations section

**Priority / effort:** P0 / M  
**Risk:** A polished report can conceal failed reviewers, weak extraction, missing appendices, unavailable search, truncation, or unresolved evidence.

**Evidence**

- Current rendering exposes only coarse failed-step warnings (`gui/src-tauri/src/output/render.rs:51-62`).
- Extraction quality exists in the document bundle but is buried in Sources.
- Capability removal, context-cache fallback, merge failure, and persistence problems are mostly warning strings rather than typed report state.

**Modify**

Have the host—not synthesis prose—render:

- input/extraction coverage, recovered pages/blocks/assets, truncation and missing-content warnings;
- completed, failed, skipped, cancelled, and quorum-omitted reviewers;
- requested versus delivered capabilities and external destinations;
- schema repair/refusal/narration events;
- verified/partial/unverified evidence counts;
- unresolved disagreements and manual-review items;
- final run status and reproducibility hash.

Put a one-line status near the report title and full details in a Quality/Provenance panel and export.

**Done when**

Synthetic degradation fixtures produce identical host-authored limitations regardless of provider wording.

### M-18 — Generalize the portable workflow contract without allowing arbitrary code

**Priority / effort:** P1 / XL  
**Risk:** The current schema is powerful for built-in academic reviews but too implicit for a safe extension ecosystem.

**Evidence**

- Inputs, variables, steps, output selectors, and profile metadata are limited in `gui/src-tauri/src/pipeline_config.rs:46-60,127-201,348-519,577-585`.
- `OutputConfig` mainly identifies a primary and findings step (`gui/src-tauri/src/pipeline_config.rs:436-440`).
- `ProfileData`/summary expose little compatibility metadata (`gui/src-tauri/src/pipeline_config.rs:495-519,653-659`).
- Missing runtime `{var:key}` and `{input:key}` placeholders silently become empty strings (`gui/src-tauri/src/pipeline/executor.rs:3503-3546`).
- `CURRENT_SCHEMA_VERSION` is 9 (`gui/src-tauri/src/pipeline_config.rs:663-673`), while architecture documentation still describes version 8.

**Modify**

- Add typed variables with validation, explicit optionality/defaults, and secret classification.
- Add input MIME/extensions, size/cardinality/schema, required/optional semantics, and artifact sensitivity.
- Lint every placeholder against declarations and supplied inputs; unresolved required placeholders fail preflight.
- Add reusable, compile-time-expanded subworkflows while retaining a flat, inspectable runtime DAG.
- Add typed named products with schemas, viewer/export policies, and sensitivity.
- Extend profile metadata with semantic version, description, author/source, license, tags, minimum Pipeline version, required/preferred/forbidden capabilities, input/output contract, changelog, and content hash/signature.
- Version deterministic host transforms and taxonomies.
- Keep arbitrary shell and arbitrary executable extensions excluded from the portable format.
- Correct stale schema-version documentation as part of every migration.

**Done when**

Validation can classify an imported workflow as supported, degraded, migration-required, or incompatible before execution, with no silent empty placeholder substitution.

### M-19 — Separate document preparation from paid review

**Priority / effort:** P1 / L  
**Risk:** Users discover extraction problems only after launching a run, when time or model cost may already have been incurred.

**Modify**

Add a cached **Prepare document** stage before immutable plan confirmation:

- run the selected extractor and build the normalized document bundle;
- show page/block/table/equation/asset coverage and extraction warnings;
- preview representative text and missing/low-confidence regions;
- let the user inspect preprocessing logs, retry the same extractor, or explicitly switch extractors;
- compute the true context/call plan from the prepared bundle;
- reuse the verified preparation artifact in the eventual run.

Preserve the current fail-closed extractor policy in `gui/src-tauri/src/pipeline/extract/dispatch.rs:17-27` and PDF completeness checks at `gui/src-tauri/src/pipeline/extract/pdf.rs:1329-1371`; do not introduce silent cross-extractor fallback.

**Done when**

No model review begins before the user can see extraction status, and changing the source or extractor invalidates the prepared bundle deterministically.

### M-20 — Preserve failed/cancelled run context and expose recovery in place

**Priority / effort:** P0 / L  
**Risk:** A recoverable partial run is presented as a terminal red error whose only action is to start over.

**Evidence**

- `PipelineState.error` retains message/stage but not run ID, partial outputs, or resumability (`gui/src/hooks/usePipeline.ts:142-150,532-543`).
- Cancellation ignores IPC errors and has no cancelling/cancelled state (`gui/src/hooks/usePipeline.ts:596-602`).
- The foreground error view only offers **Start a new report** (`gui/src/App.tsx:1015-1033`).
- History already knows how to resume runs (`gui/src/components/HistoryPage.tsx:429-456`).

**Modify**

- Add `cancelling`, `cancelled`, and recoverable `error` states carrying run ID, failed step, persisted checkpoint, partial products, and recovery options.
- Treat user cancellation as neutral, not a failure inferred by matching error text.
- Offer **Resume failed work**, **Retry failed merge**, **Open partial report**, **Inspect preserved outputs**, **Copy redacted diagnostics**, and **Start over** in the current view.
- Disable repeated cancellation and surface cancellation-command failure.

**Done when**

Failure/cancellation fixtures can recover without navigating to History, never lose the run ID, and never show a cancelled run as a generic failure.

### M-21 — Replace synchronous 1,000-node report highlighting

**Priority / effort:** P0 / M  
**Risk:** Large reports can stall the reader, and the complete frontend suite is not deterministic.

**Evidence**

- `gui/src/hooks/useFindBar.ts:3-79,102-121` synchronously creates and later normalizes as many as 1,000 `<mark>` elements on query changes.
- `gui/src/components/ReportViewer.test.tsx:387-397` exercises 1,001 matches. It passed alone in roughly two seconds but exceeded the default five-second test timeout twice under full-suite contention.
- The production build also reports a roughly 553 kB minified `ReportViewer` chunk.

**Modify**

- Debounce input, cancel stale searches, count matches separately from rendered highlights, and chunk DOM work through idle/animation frames.
- Prefer the CSS Custom Highlight API where available, with a bounded fallback.
- Keep current-match navigation responsive even when full highlighting is incomplete.
- Unit-test the search/highlight algorithm directly; retain a bounded integration test and a large-report performance budget.
- Lazy-load heavy syntax/math/source features and inspect the report chunk for avoidable dependencies.

**Done when**

The full frontend suite passes repeatedly under CI worker contention, and a benchmark report with tens of thousands of matches keeps keystroke and navigation latency within the UI budget in Section 8.

### M-22 — Prevent silent user-data loss in Projects and History

**Priority / effort:** P0 / L  
**Risk:** Unsaved project edits disappear on selection, explicit deletion is immediately permanent, and the default byte-retention ceiling can automatically purge older completed work after a run.

**Evidence**

- Selecting a project immediately changes the ID (`gui/src/components/ProjectsPage.tsx:245-260`), and the effect at `gui/src/components/ProjectsPage.tsx:73-78` resets edited name/description from persisted data.
- The fields require explicit save but have no dirty guard (`gui/src/components/ProjectsPage.tsx:284-315`).
- `gui/src/components/HistoryPage.tsx:197-210` permanently deletes a report and artifacts after a browser confirmation.
- Project issue merges are also not represented as reversible history.
- `gui/src-tauri/src/settings.rs:289-298,471-473` defaults the run-byte ceiling to 5 GB even though a zero count limit is described as keeping everything.
- `gui/src-tauri/src/commands/run_entry.rs:61-66` enforces retention after every completed run; `gui/src-tauri/src/commands/run_storage.rs:291-312` reports deletion only in the log.
- `gui/src-tauri/src/runs/retention.rs:99-163,238-252` protects pending/running runs, but does not protect project-linked, annotated, pinned, or manually retained completed work before permanent deletion.

**Modify**

- Autosave serialized project metadata, or guard selection/navigation/quit with **Save / Discard / Cancel**.
- Add Trash with undo, restore, retention duration, and deliberate permanent deletion for reports/projects.
- Default automatic byte/count retention to disabled until the user explicitly chooses a policy in setup or Settings. Show current storage, forecast candidates, affected project/annotation state, and the grace period before activation.
- Protect pinned, project-linked, annotated, incomplete/recoverable, and manually retained runs. Require explicit override before any protected run can become a candidate.
- Move automatic candidates to a recoverable Trash/quarantine first; expose a completion notification and audit record. Reuse and extend the existing preview-token mechanism for race-safe confirmation.
- Make issue-ledger merges reversible and display exactly what will combine.
- Add **Add to project** on completed reports and History; support searchable multi-select and an atomic batch-membership backend command.
- Unify visible lifecycle terms across per-report and project views: **Open / Addressed / Dismissed / Regressed**.

**Done when**

Switch/navigation/quit regression tests preserve or explicitly discard drafts. Default installs never auto-delete completed runs, protected-run fixtures never become ordinary retention candidates, and every deletion remains recoverable through the configured grace period.

### M-23 — Replace browser dialogs and complete accessibility semantics

**Priority / effort:** P1 / XL  
**Risk:** Synchronous alerts interrupt progress/autosave and provide inconsistent focus, keyboard, status, and screen-reader behavior.

**Evidence**

- There are approximately 47 production `alert`/`confirm` calls across App, workflow editing, exports, History, Projects, Settings, and engine management. Representative sites include `gui/src/App.tsx:316-341,665-673,921-929`, `gui/src/components/PipelinePage.tsx:300-420,600-825,948-993`, and `gui/src/components/ExportControls.tsx:39-122`.
- No reduced-motion, forced-colors, or contrast media rules exist in `gui/src/App.css`; global theme transitions apply broadly at `gui/src/App.css:34-40`.
- Progress lacks a concise live status region; several toggle/selection controls do not expose pressed/current state.

**Modify**

- Introduce one accessible modal/toast/status service with focus trapping/restoration and nonblocking success/error messages.
- Use detailed app confirmation for destructive actions and undo where possible.
- Add `prefers-reduced-motion`, forced-colors, and high-contrast behavior; allow a user override for motion.
- Add `aria-live="polite"`, `aria-busy`, labelled controls, pressed/current/selected states, and visible keyboard focus.
- Implement roving keyboard behavior for tablists and test Arrow/Home/End navigation.
- Run automated axe tests plus manual keyboard, VoiceOver/NVDA, zoom, text scaling, and minimum-window checks.

**Done when**

No production path uses native alert/confirm, WCAG 2.2 AA issues are tracked to zero for core journeys, and reduced-motion mode contains no decorative animation or smooth scrolling.

### M-24 — Use progressive disclosure, validation, and durable history in the workflow editor

**Priority / effort:** P1 / L  
**Risk:** The editor exposes implementation details before users can understand the workflow, while invalid save states and destructive changes are hard to recover.

**Evidence**

- Dense tabs expose JSON pointers, regex/globs, fan-out, raw schemas, conditions, dependencies, merge policy, and provider overrides (`gui/src/components/pipeline-editor/EditorPanels.tsx:224-335`; `gui/src/components/pipeline-editor/AdvancedStepOptions.tsx:120-350`).
- Save can be disabled without an adjacent consolidated explanation (`gui/src/components/PipelinePage.tsx:1421-1438`).
- Undo is narrowly scoped to some dependency rewrites (`gui/src/components/PipelinePage.tsx:521-598,1040-1044`).

**Modify**

- Default to a Basic view: purpose, prompt, readable inputs, required outputs, and simple model policy.
- Put selectors, conditions, fan-out, raw schema, and provider overrides under Advanced.
- Add a consolidated validation panel with jump-to-field actions and exact blocking reasons.
- Show a DAG/execution preview, estimated call range, capability requirements, and output products before save/run.
- Add general undo/redo, draft recovery, Cmd/Ctrl+S, migration preview, and compare-to-built-in/reset history.
- Standardize visible terminology on **Workflow** and **Workflow defaults**, reserving `profile` for serialized compatibility.

**Done when**

A novice can customize a prompt and add a review step without seeing raw schema fields, while an expert can reach every existing control and undo structural edits.

### M-25 — Persist batch queues and fix terminal accounting

**Priority / effort:** P1 / L  
**Risk:** App termination loses the queue, and cancelled jobs can leave a completed batch looking unfinished.

**Evidence**

- Batch state is an in-memory vector and cancel flag (`gui/src-tauri/src/commands/lifecycle.rs:210-236`; `gui/src-tauri/src/commands/batch.rs:70-188`).
- The frontend done numerator counts only done + failed, not cancelled (`gui/src/components/BatchPanel.tsx:342-343,539-543`).

**Modify**

- Persist an atomic batch manifest with effective workflow fingerprint, inputs, per-job status, run ID, error, retry count, and timestamps.
- Reconcile running jobs with recovered runs at startup.
- Count every terminal state and show succeeded/failed/cancelled separately.
- Add pending cancellation, Resume remaining, Retry failed, reorder/remove before launch, and resource-aware concurrency bounded by provider/CPU/memory policy.
- Give named-input selectors distinct accessible labels.

**Done when**

Force-quitting mid-batch and reopening reconstructs the queue exactly; completed progress reaches 100% even with cancelled jobs.

### M-26 — Make timeout and retry budgets apply to the whole logical step

**Priority / effort:** P1 / M  
**Risk:** A user-visible “20 minute” timeout can repeat across many attempts and last for hours.

**Evidence**

- Each retry receives the full configured timeout (`gui/src-tauri/src/pipeline/executor.rs:1766-1783,1823-1842`; provider enforcement in `gui/src-tauri/src/pipeline/call.rs:262-312`).
- Orientation independently grants multiple half-timeout attempts (`gui/src-tauri/src/pipeline/orient.rs:169-225`).

**Modify**

- Create one deadline at logical-step start and pass remaining time to cache warmups, calls, retries, and fallbacks.
- Expose total step budget separately from per-attempt network timeout.
- Use bounded exponential backoff with jitter and response-aware retry classification.
- Read a bounded 429 body before retrying so permanent quota exhaustion can fall back or fail immediately (`gui/src-tauri/src/pipeline/api_common.rs:2501-2586`).
- Show retry count, elapsed/remaining budget, and fallback transitions in progress/provenance.

**Done when**

Elapsed time never materially exceeds the declared step budget except for a small cancellation/grace bound, and quota fixtures do not waste rate-limit backoff.

### M-27 — Consolidate exports and separate shareable from forensic data

**Priority / effort:** P1 / L  
**Risk:** The current complete-directory export can include sources, logs, and raw responses, while ordinary output is limited and uses blocking alerts.

**Evidence**

- `gui/src/components/ExportControls.tsx:39-163` exposes competing header buttons and browser alerts.
- `gui/src-tauri/src/commands/export.rs:5-303,655-809` supports copying broad run contents, which may be inappropriate for sharing.

**Modify**

Use one Export menu with explicit contents and sensitivity labels:

- **Shareable report:** rendered report, verified findings, selected evidence/citations, limitations, and redacted provenance.
- **Complete forensic archive:** immutable run artifacts, raw responses, logs, sources, effective configuration, and manifest—clearly labelled sensitive.
- **Custom:** user selects products and source inclusion.

Show progress, final path, checksums, and **Reveal in folder** through a narrow backend command. Label print-dialog behavior accurately if **PDF** is not generated directly.

**Done when**

Snapshot tests enumerate every file in each export mode, secret scanners pass, and the default share action never includes source documents or raw model responses without explicit consent.

### M-28 — Finish high-density reader, console, History, compare, and responsive behavior

**Priority / effort:** P2 / XL  
**Risk:** Professional users can complete a run but encounter friction when navigating large histories, comparing reports, reading at minimum window size, or handling logs.

**Modify**

- **History:** add workflow/status/project/date filters, sort, pagination or virtualization, compact duration/cost, and an overflow menu; keep Open/Resume primary (`gui/src/components/HistoryPage.tsx:141-175,307-478`).
- **Compare:** label Older/Newer reports with title/date/workflow, summarize changes, export the diff, and disclose provider/artifacts before LLM reconciliation (`gui/src/components/ComparePage.tsx:122-152`).
- **Issues:** add text/reviewer/source/status filters, bulk decisions, multiline rationale, unreviewed counts, and export all decisions—not only accepted items (`gui/src/components/IssuesTable.tsx:196-303,443-449`).
- **Issue readiness:** visibly disable annotation controls until persisted decisions load; do not silently drop early edits (`gui/src/components/IssuesTable.tsx:89-186`).
- **Console:** default collapsed after completion, scope it to the active run, replace invalid `<div>`-inside-`<pre>` structure, label search/toggles, and report clipboard errors (`gui/src/components/Console.tsx:262-270,436-668`). Apply the same markup fix to `gui/src/components/ComparePage.tsx:31-51`.
- **Responsive layout:** clamp rails to a viewport fraction, preserve a usable primary content width, auto-collapse secondary panels, and test 1024×700 plus increased text scaling (`gui/src-tauri/tauri.conf.json:17-21`).
- **Reading:** use left alignment by default instead of full justification/hyphenation, reduce padding at narrow widths, and offer text-size/line-height controls (`gui/src/App.css:72-82,141-149`).
- **Sources:** load the full canonical document only when Sources is requested or during cancellable idle time (`gui/src/components/ReportWorkspace.tsx:475-508`).
- **Input selection:** add drag-and-drop, recent sources, clear/replace actions, and a concise interpretation chooser while retaining the system picker (`gui/src/components/PaperSelector.tsx:22-124`).
- **Help:** generate workspace/navigation descriptions from shared product metadata so Help cannot claim three tabs when the implementation can expose Report, Provenance, Issues, and Sources (`gui/src/components/AboutPage.tsx:232-242`; `gui/src/components/ReportWorkspace.tsx:617-622,784-825`).

**Done when**

Large-history, large-report, min-window, keyboard-only, and text-scaling scenarios are included in automated performance/visual coverage and manual release QA.

### M-29 — Replace the global model-discovery lock with keyed single-flight work

**Priority / effort:** P2 / M  
**Risk:** Unrelated providers serialize their first-use catalog discovery, creating avoidable readiness latency; removing synchronization entirely would instead duplicate requests.

**Evidence**

`gui/src-tauri/src/model_catalog/resolution.rs:3-14` uses one global mutex around discovery.

**Modify**

Use keyed single-flight coordination by provider, transport, normalized endpoint, credential revision/fingerprint, and relevant catalog version. Concurrent requests for the same key should share one result; unrelated providers should discover in parallel. Bound and expire keys without storing raw credentials.

**Done when**

Concurrency tests prove one request per identical key, parallel progress for different providers, and correct invalidation after endpoint/credential/model-catalog changes.

---

## 6. ADD before or immediately after release

### A-01 — Add a versioned report-quality benchmark and adversarial corpus

**Priority / effort:** P0 / XL, then continuous  
**Why:** “High quality” must be a measured property of the complete pipeline, not an inference from prompt sophistication or schema success.

**Add**

A repository-owned, versioned corpus using synthetic and permissively licensed documents with:

- seeded major/minor defects and explicit clean controls;
- inconsistent equations, tables, references, claims, citations, and appendices;
- ambiguous issues with acceptable variants and manual-review labels;
- extraction failures, scanned pages, multi-column layouts, figures, tables, and corrupted metadata;
- every supported subject and method family used by the Auto Review router;
- hostile instructions in body text, citations, images/OCR, artifacts, web pages, and local configuration;
- provider refusal, truncation, malformed schema, missing tool, fallback, timeout, and partial-review scenarios.

Each fixture should declare expected routing, required/forbidden findings, evidence spans, severity tolerance, duplicate clusters, citation resolution, validation disposition, and permitted wording variance. Separate evaluation into:

1. deterministic host/schema/renderer tests on every PR;
2. mocked provider-contract tests on every PR;
3. live-provider canaries on a controlled schedule and before model/catalog promotion;
4. blinded human calibration for a sampled release candidate.

Make the statistics operational:

- Score at the document, seeded-defect opportunity, semantic finding cluster, evidence locator, citation, and routing-decision levels.
- Start with at least 200 documents, including at least 50 clean controls and at least 30 major-defect opportunities in every reported defect family; revise upward from a preregistered power analysis.
- Keep at least 25% as a frozen hidden holdout. Prompt/catalog authors must not inspect it; rotate only through a recorded governance decision.
- Run every live candidate configuration at least three independent times on a stratified hidden subset of at least 60 documents, varying order and stochastic seed where the provider permits. Deterministic host tests still run on the full corpus.
- Report 95% confidence intervals. Gate minimum metrics on their lower confidence bound and maximum false-positive metrics on their upper bound; never let a strong aggregate hide a defect-family floor.
- Normalize clean false positives per 50 normalized pages and separately report the share of clean documents receiving any major false positive.
- Score subject routing against expert-approved acceptable-role sets, with a separate exact-primary metric; genuinely ambiguous papers should not be forced into one artificial ground truth.
- Double-annotate a release sample, report agreement, and adjudicate disagreements without leaking holdout labels into development.

Store evaluator version, benchmark version, prompt/catalog hash, provider/model snapshot, sampling plan, and confidence intervals with every result. Never tune against a hidden test set and then report the same set as unbiased evidence.

**Done when**

The Section 8 quality gates are machine-enforced, regressions identify the responsible role/provider/prompt, and no model/catalog update can become default without a comparison report.

### A-02 — Add a single release aggregator, evidence manifest, and packaged E2E gate

**Priority / effort:** P0 / XL  
**Why:** Release integrity requires one authority to prove completeness after all platform work finishes.

**Add**

- Matrix jobs that produce immutable, uniquely named signed packages, checksums, SBOMs, provenance, and smoke records.
- A downstream aggregator that downloads artifacts into an empty workspace and verifies an expected manifest: version, target, architecture, package type, signature identity, hash, size, SBOM, provenance, and test result.
- Clean-machine packaged tests for first launch, readiness flow, document import, a deterministic fixture review, cancellation/recovery, report render, export, restart persistence, and uninstall/upgrade where applicable.
- Cross-platform golden report/print checks that detect clipped equations, split tables/evidence, orphan headings, missing glyphs, broken page numbers, and unreadable output on the packaged WebView/runtime.
- One draft-release creation step after complete verification; a protected human approval to publish.
- OIDC/minimal permissions, pinned actions, protected environments, concurrency controls, and immutable build inputs.
- Release-failure tests: missing platform, duplicate name, wrong signature, altered hash, stale version, smoke failure, and concurrent release attempt.

**Done when**

The aggregator refuses every incomplete/tampered fixture and the published release exactly matches the verified manifest.

### A-03 — Add a resumable first-run readiness wizard

**Priority / effort:** P1 / L  
**Why:** Current readiness presents a blocking dependency modal but does not guide the user through a complete working configuration.

**Evidence**

- `gui/src/App.tsx:721-745` opens dependency state automatically.
- `gui/src/components/DepsCheck.tsx:276-363` mainly offers dismissal; Help holds the useful quick-start content.
- Settings receives a `dependencies` prop but does not use it (`gui/src/components/SettingsPage.tsx:27-63`; call site `gui/src/App.tsx:899-913`).

**Add**

A checklist that can be dismissed and resumed:

1. choose subscription CLI, provider API, or OpenAI-compatible endpoint;
2. authenticate/add a write-only credential;
3. test the connection and discover a compatible model;
4. choose/verify PDF extraction;
5. review privacy and exact egress behavior;
6. run a bundled synthetic sample with no private source data;
7. confirm that report, evidence navigation, history, and export work.

Every failed item should deep-link to the exact setting and return with readiness refreshed. Do not require every optional provider or extractor.

**Done when**

A clean-machine E2E test reaches a verified sample report without external documentation, and dismissal never blocks later access to the checklist.

### A-04 — Add provider connection tests and a readiness matrix

**Priority / effort:** P1 / L  
**Why:** A saved key or installed CLI is not evidence that the selected model/transport can execute the workflow's required contract.

**Add**

Per-provider **Test connection** actions showing:

- authentication mode and, where safely available, account identity;
- endpoint locality/hostname and TLS requirement;
- model discovery result and last-tested timestamp;
- required structured-output, artifact, image, context, and hosted-tool capabilities;
- rate/quota errors versus transient availability;
- selected default plus compatible alternatives;
- actionable remediation without exposing raw provider responses or secrets.

Settings load failures need Retry rather than only **Go back** (`gui/src/components/SettingsPage.tsx:322-333`). Cache readiness with a short lifetime and invalidate on endpoint/key/model change.

**Done when**

Run preparation can explain exactly why a provider/model is compatible, degraded, or blocked before any document leaves the machine.

### A-05 — Add an always-visible Quality panel and report trust summary

**Priority / effort:** P0 / L  
**Why:** Users need to distinguish a clean report from a polished but partial one at a glance.

**Add**

A host-generated Quality surface combining:

- extraction coverage and warnings;
- routing coverage and user overrides;
- reviewer success/quorum and degraded capabilities;
- schema validation, repair, refusal, narration, and truncation events;
- finding/evidence counts by verification state;
- citation resolution and unresolved disagreements;
- canonical renderer parity status;
- effective configuration hash and replay compatibility.

Use a compact title badge (**Complete**, **Complete with limitations**, **Partial**, **Unverified**, **Failed**) and a detailed panel linking to Sources, Provenance, failed steps, preserved responses, and recovery actions. Do not collapse quality into one opaque numeric score.

**Done when**

Users can answer “What did Pipeline inspect, what failed, and which claims are verified?” without opening raw run files.

### A-06 — Add distinct author, editor, and evidence products

**Priority / effort:** P1 / XL  
**Why:** The canonical issue ledger is necessary but insufficient for real academic and professional review workflows.

**Add**

Typed named products that share the same verified finding graph:

- **Issue ledger:** canonical, complete, stable-ID source of truth.
- **Author-facing report/referee letter:** concise overview followed by prioritized actionable issues; optionally includes strengths only when requested by the workflow.
- **Confidential editor/decision memo:** recommendation, confidence, decisive considerations, scope limitations, and conflicts; never included in author exports by default.
- **Evidence appendix:** full locators, excerpts, citations, reviewer/call provenance, and verification state.
- **Revision checklist:** user-selected findings, ownership/status/notes, and acceptance criteria.

Current Auto synthesis deliberately avoids summary/recommendation/praise (`prompts/auto_review/synthesis.md:19` and its validation contract). Keep that strict issue synthesis, then derive optional communication products from the verified ledger with separate schemas and access policies.

**Done when**

Each product declares audience and sensitivity, is reproducible from canonical findings, and cannot silently leak a confidential memo into an author-facing bundle.

### A-07 — Add an editable working copy, redline, and rich exports

**Priority / effort:** P1 / XL  
**Why:** Users need to turn findings into decisions and revisions without destroying the original model output.

**Add**

- An immutable original plus a non-destructive human working copy.
- Editable title/problem/consequence/recommendation, with author, timestamp, and diff for every change.
- Accept/dismiss/address/regress decisions, multiline rationale, ownership, due date, tags, and bulk operations.
- Optional document redline/suggested-change artifacts tied to verified evidence; never silently edit the source.
- Exports for `findings.json`, CSV/XLSX issue ledger, self-contained HTML, deterministic PDF, DOCX, citations/bibliography JSON, effective workflow/provenance JSON, and portable archive.

The original finding ID and source lineage must survive edits. Human text should be visually and structurally distinguishable from model output.

**Done when**

An exported revision checklist can be re-imported without losing IDs, decisions, notes, or provenance, and the original report remains bit-for-bit available.

### A-08 — Add claim-level provenance

**Priority / effort:** P1 / XL  
**Why:** Run-level provenance is not enough to audit a specific claim.

**Add**

An immutable chain:

`finding revision → evidence verification → source fragment/web capture → reviewer response fragment → tool/call record → effective prompt and workflow → input snapshot`

Use content-addressed IDs and explicit many-to-many edges. Preserve response offsets or structured field paths so the UI can show **Why is this here?** without dumping entire raw prompts/responses. Record human edits as new revisions rather than rewriting model lineage.

The existing response journal (`gui/src-tauri/src/pipeline/response_journal.rs:1-6,166-263`) and call records are the right substrate; extend them instead of creating an unrelated logging system.

**Done when**

Every finding in a released report can traverse to a verified source and the exact effective call, and broken provenance makes the finding unverified.

### A-09 — Add a one-click redacted diagnostic bundle

**Priority / effort:** P1 / L  
**Why:** A no-telemetry product needs a safe support path; users should not be asked to attach whole run directories.

**Add**

A previewable bundle containing:

- Pipeline/app/OS/runtime versions and feature flags;
- provider/transport readiness without credentials or account identifiers;
- workflow/effective-plan hashes and schema/catalog versions;
- typed errors, degradations, timing summaries, and scrubbed logs;
- installer/update/signature state and disk-path diagnostics;
- an explicit manifest of included and excluded files.

Exclude source documents, extracted text, prompts with source interpolation, raw responses, evidence quotes, secrets, full custom URLs with credentials/query strings, and user-identifying paths by default. Run deterministic secret/PII/path scrubbers and let the user inspect the archive contents before saving.

**Done when**

Golden tests with planted API keys, emails, home paths, and document phrases find none in the default bundle.

### A-10 — Add an extension compatibility contract and safe tool registry

**Priority / effort:** P1 / XL  
**Why:** Extensibility should not mean arbitrary code execution or provider-dependent hidden behavior.

**Add**

- Independently versioned workflow schema, product schema, prompt/catalog, deterministic transform registry, viewer/export contract, and tool API.
- Preflight compatibility results: **portable**, **supported locally**, **supported with disclosed degradation**, **migration required**, or **incompatible**.
- A host-owned tool registry with typed inputs/outputs, declared data access, deterministic limits, provenance, and per-step authorization.
- Safe first tools: calculator, table/equation consistency checker, citation resolver, reference cross-check, structured dataset summary, and locator verifier.
- Package/profile signing or content hashes, origin/license metadata, migration fixtures, and capability declarations.

Do not add arbitrary shell commands, arbitrary network clients, dynamic native libraries, or unreviewed MCP servers to the portable workflow format. External integrations need a separate permissioned extension boundary.

**Done when**

An imported workflow can be understood and safety-checked without executing it, and every tool invocation is visible in the immutable plan and provenance graph.

### A-11 — Add cost, duration, and hard resource budgets

**Priority / effort:** P1 / XL  
**Why:** “Model work units” do not let users control a long adaptive or batch review.

**Add**

- Historical duration ranges by extraction/profile/provider/step, shown as ranges rather than false precision.
- Estimated call count including orientation, specialists, merge, validation, repairs, and fallback.
- Provider-specific token/price estimate when knowable; explicit **Unknown/subscription-billed** otherwise.
- Per-run and per-batch limits for spend, tokens, wall-clock time, attempts, and adaptive specialist count.
- Hard enforcement at the engine boundary, with **Pause and ask**, **stop partial**, or **fail** policy.
- Actual-versus-estimated summary and attribution by step/provider.

**Done when**

No retry, repair, fallback, or adaptive expansion can exceed a confirmed hard budget, and tests cover budget exhaustion at every stage boundary.

### A-12 — Add portable project/run import and archival

**Priority / effort:** P2 / XL  
**Why:** Pipeline can export broad run data but lacks a complete, validated restore path for moving or preserving work.

**Add**

- A versioned archive manifest with hashes, sensitivity mode, product/effective-schema versions, and compatibility requirements.
- **Shareable archive** and **forensic archive** import paths with schema migration, path sanitization, hash verification, and conflict handling.
- Project archives that preserve run membership, issue-ledger merges, human annotations, and lifecycle history.
- Read-only import for unsupported future versions rather than destructive downgrade.

**Done when**

Round-trip tests move a project between clean installations without changing immutable artifacts, and malformed/path-traversal archives fail closed.

### A-13 — Add a signed, staged update path

**Priority / effort:** P2 / L  
**Why:** A banner that opens a download page leaves version choice, authenticity, rollback, and support state to the user.

**Add**

An opt-in updater or, at minimum, an in-app verified-download flow with signed metadata, channels (stable/beta), release notes, artifact/signature verification, explicit restart, rollback guidance, and no update during an active run. Signed, auditable update metadata may pause new update offers when a release is withdrawn; it must never disable an installed application, block an active run, or function as a remote kill switch. No document telemetry is needed.

**Done when**

Tampered metadata/package fixtures are rejected, active runs are protected, and downgrade/rollback behavior is documented and tested.

### A-14 — Add a quality calibration dashboard and model/prompt promotion process

**Priority / effort:** P1 / L after A-01  
**Why:** Quality will drift as prompts, catalogs, schemas, extractors, and provider models change.

**Add**

A local/release-engineering dashboard showing benchmark changes by:

- workflow and reviewer role;
- subject/method routing family;
- provider/model/transport;
- extraction method and document type;
- defect class, severity, evidence type, and clean control;
- schema repairs, degradations, latency, token use, and cost.

Require a signed promotion record for new default prompts/models/catalogs: baseline, candidate, statistical uncertainty, known regressions, reviewer approval, and rollback target. Alert on drift in live canaries without uploading user documents.

**Done when**

Every default change has a reproducible comparison and rollback decision; “latest model” is never promoted solely because it is newer.

---

## 7. Recommended report architecture

The implementation should converge on one explicit truth pipeline:

```text
immutable inputs
    → verified document bundle
    → section inventory and routing decision
    → independent specialist observations
    → normalized candidate findings
    → evidence resolution and verification
    → deduplication and adjudication
    → validator dispositions
    → canonical findings-v2 graph
    → schema-bound audience products
```

The canonical findings graph—not Markdown—is the source of truth. Renderers and exports are views over that graph. Host-template products should be byte-deterministic. If an author letter or editor memo uses a model for prose, only its allowed facts, finding IDs, ordering constraints, schema, and provenance are deterministic; preserve the generating call and never describe the wording itself as deterministic. Model prose cannot decide whether extraction succeeded, whether a page exists, whether a source URL was actually returned, which reviewers failed, or which provider/model ran; the host must supply those facts.

### Suggested run artifact layout

```text
run/
  manifest.json
  execution-plan.json
  effective-workflow.json
  inputs/                         # immutable source snapshots
  context/document-bundle.json
  context/document.md
  calls/                          # normalized requests/responses/tool events
  products/findings-v2.json       # canonical truth product
  products/validation-ledger.json
  products/evidence-ledger.json
  products/quality.json
  products/report-author.md
  products/report-editor.md       # optional, separately protected
  products/evidence-appendix.html
  decisions/annotations.json      # human layer, revisioned
  logs/events.jsonl               # typed/scrubbed operational events
```

Every mutable human decision should be revisioned outside immutable model/run artifacts. Every renderer should declare which product/schema version it consumed.

---

## 8. Measurable quality and release gates

These thresholds should first be baselined and reviewed by domain experts, then become release-blocking. A lower measured baseline is a reason to improve the system or narrow the claim—not to omit the metric. Unless a row states otherwise, minimum/maximum gates use the lower/upper 95% confidence bound under the A-01 sampling protocol, and each major defect family must satisfy its own approved floor.

### Report truth and evidence

| Metric | Proposed release threshold |
|---|---:|
| Structured-product success | **≥99.5%** of benchmark runs valid after at most one targeted repair |
| Evidence coverage | **100%** of main-report findings have at least one host-verified locator; partial/unverified claims appear only in the manual-verification product |
| Locator resolvability | **100%** resolve to the snapshotted source; **zero** out-of-range pages, nonexistent node/asset IDs, path escapes, or hash mismatches |
| Quote accuracy | **≥98%** exact after allowed normalization; remaining **≤2%** meet the preregistered fuzzy threshold and are labelled `verified_with_normalization` |
| External citation integrity | **100%** have resolvable URL/DOI plus captured metadata; **zero** fabricated sources in benchmark |
| Major-defect recall | **≥90%** on seeded major defects, reported with confidence intervals by defect family |
| Clean-control false positives | **≤0.2** actionable false positives per 50 normalized pages and **zero** major false positives on at least 95% of clean documents |
| Deduplication | **Zero** duplicate defect clusters in canonical output |
| Validator false-positive rejection | **≥95%** of seeded false positives |
| Validator true-positive retention | **≥98%** of verified true positives |
| Disposition completeness | **100%** of candidates have exactly one final disposition |

### Routing and capability integrity

| Metric | Proposed release threshold |
|---|---:|
| Subject routing | **≥95%** within expert-approved acceptable-role sets, with exact-primary accuracy reported separately |
| Method-role routing | **≥90% recall**, **≥85% precision** |
| Required capability mismatch | **100%** fail preflight with remedy |
| Preferred capability degradation | **100%** disclosed before launch and in final limitations |
| Undeclared tool use | **Zero** in adversarial configuration/document tests |
| Required reviewer quorum | **100%** enforced; no complete report below quorum |

### Reproducibility and rendering

| Metric | Proposed release threshold |
|---|---:|
| Effective specification | **100%** of runs store a replay-complete effective graph and hash |
| Finding parity | **100%** same IDs/order across canonical JSON and every shipped full-fidelity surface; new renderers inherit the gate |
| Visual rendering | **100%** approved golden fixtures free of clipping, orphaning, missing glyphs, broken pagination, and unreadable evidence blocks |
| Provenance completeness | **100%** of released findings traverse to source and call record |
| Durable completion | **100%** of Complete states have a valid final manifest and required artifacts |
| Archive round trip | **100%** immutable hashes preserved across supported export/import |

### Performance and UX reliability

Use release-owned minimum-reference hardware for each supported OS and publish the profile. The standard large-report fixture should contain at least 300 pages, 500,000 rendered characters, 250 findings/evidence records, equations/tables, and 10,000 search matches. Record cold start, warm reopen, event-to-paint latency, long tasks, peak/steady memory, and gzip bundle sizes with caches explicitly cold or warm.

| Metric | Proposed release threshold |
|---|---:|
| Complete frontend suite | **Zero flakes** across 30 fresh CI jobs spanning at least three randomized test orders and worker-contention profiles |
| Cold startup to interactive | p95 **<3 s** on each minimum-reference machine; warm reopen p95 **<1.5 s** |
| Report search event-to-paint | p95 **<100 ms** on the standard large-report fixture; no unapproved main-thread task **>50 ms** |
| Current-match navigation | p95 **<100 ms** after search results are available |
| Reader memory | Steady-state RSS **<600 MB** and no unbounded growth across 100 searches/tab switches on the fixture |
| JavaScript delivery | Initial route **≤250 kB gzip** and async report-reader chunk **≤200 kB gzip**, with tracked exceptions requiring performance evidence |
| Crash/restart recovery | **100%** pending/partial fixture runs reconciled without silent loss |
| Retention safety | **Zero** default automatic permanent deletions; **100%** protected-run exclusion and Trash restore in fixtures |
| Accessibility | Zero critical/serious automated violations in core states; manual core journeys pass |

### Release integrity

| Metric | Proposed release threshold |
|---|---:|
| Required CI | Runs and passes on every PR/protected-branch commit |
| Packaged smoke | Passes on every advertised OS/architecture/package |
| Artifact completeness | **100%** expected packages, signatures, hashes, SBOMs, provenance, and attestations verified before draft creation |
| Publication authority | Exactly one protected aggregator/publish path |
| Documentation parity | **100%** objective release/security claims mapped to an enforced control |

---

## 9. Sequenced delivery plan

Calendar estimates are intentionally omitted: elapsed time depends on team size, platform-signing access, provider-test budget, and the volume of expert benchmark annotation. The S/M/L/XL labels are relative implementation effort, not commitments. Sequence work by contract dependency.

### Critical paths

```text
effective run + capabilities:  M-11 + M-12 → A-01 comparisons and promotion evidence
finding truth:                 M-08 → M-09/M-13/M-15 → M-10/M-17/A-05 → A-06/A-07
release integrity:             R-01/R-02/R-03 + M-01/M-02 → A-02 → protected publication
durable recovery:              M-06 → M-20/M-22/M-25
portable extensions:           M-18 → A-10 → A-06/A-07/A-12
```

### Phase 0 — Freeze promises and remove immediate unsafe behavior

**IDs:** R-01, R-02, R-03, R-04, M-02, M-05

- Stop matrix publication/unsigned official release behavior and invert weak-workflow assertions.
- Narrow documentation and report metadata to current truth.
- Decide whether Claude CLI can satisfy isolation; disable the transport for release if it cannot.
- Freeze new built-in prompts/providers until benchmark/effective-run contracts exist.

### Phase 1 — Establish safety, durability, and release foundations

**IDs:** R-05, R-09, M-01, M-03, M-04, M-06, M-07, M-12, M-17, M-20, M-22, A-02

- Make CI continuous and build the single release aggregator.
- Establish backend-owned secrets, immutable egress confirmation, narrow external-open authority, and negotiated tools.
- Require durable completion, dependency quorum, typed limitations, in-place recovery, and safe retention/Trash.
- Remove blanket search while the capability contract becomes authoritative.

### Phase 2 — Make report quality provable

**IDs:** R-06, M-08, M-09, M-10, M-11, M-13, M-14, M-15, M-16, A-01, A-05, A-14

- Land the replay-complete effective specification and benchmark harness early enough to compare every subsequent change.
- Implement findings-v2, semantic evidence/citation verification, validator dispositions, and canonical renderer/visual parity.
- Make routing representative and measurable; bring every built-in profile under the same contract.
- Feed host-generated quality state into the Quality panel and controlled promotion process.

### Phase 3 — Make preparation, failure, and scale humane

**IDs:** M-19, M-21, M-23, M-25, M-26, A-03, A-04, A-11

- Add prepared-document inspection, first-run readiness, and provider connection testing.
- Fix large-report search and complete dialog, motion, keyboard, status, screen-reader, and minimum-window behavior.
- Persist batch queues and enforce total step/run/batch budgets.

### Phase 4 — Complete professional output, workflow, and portability work

**IDs:** R-07, R-08, R-10, M-18, M-24, M-27, M-28, M-29, A-06, A-07, A-08, A-09, A-10, A-12

- Remove dormant/misnamed provider and production-game surface; make generic prompts genuinely generic.
- Finish the declarative workflow language, editor, safe extension registry, and keyed discovery.
- Add audience products, revisioned human working copy, rich/sensitive exports, claim provenance, redacted diagnostics, and portable archives.
- Complete high-density reader, History, comparison, source, input, console, and responsive polish.

### Phase 5 — Release candidate, controlled rollout, and updating

**IDs:** A-13 plus the acceptance evidence for every earlier ID

- Meet every P0 gate and all P1 acceptance criteria; archive the benchmark/calibration result.
- Run clean-machine packaged E2E on every advertised target.
- Conduct manual domain-expert review, accessibility review, and focused security assessment.
- Create a draft through the aggregator, verify it independently, then publish through protected approval.
- Roll out in stages with a documented rollback target. Add signed staged updating only after the base release path is stable.

This matrix accounts for all 53 recommendation IDs. Parallelize within a phase only where the upstream contracts are stable; UI consumers should not precede the schema/evidence/effective-run state they need to display.

---

## 10. Release-candidate acceptance checklist

A release candidate is ready only when all answers are **yes**:

### Trust and privacy

- [ ] Can the webview operate Settings and catalog discovery without ever retrieving saved plaintext secrets?
- [ ] Does the preview name every remote destination and bind confirmation to the exact immutable plan?
- [ ] Are CLI tools/configuration isolated from ambient shell, web, MCP, hooks, and filesystem access?
- [ ] Does every complete report state exactly what was inspected, omitted, degraded, or unverified?

### Report quality

- [ ] Does every released finding have a stable ID, canonical rank, and host-verified locator?
- [ ] Can every external citation be traced to a captured provider call and source object?
- [ ] Does every candidate finding have an explicit validator disposition?
- [ ] Do canonical JSON and every shipped full-fidelity surface preserve the same findings/order and pass the visual fixture suite?
- [ ] Do benchmark results meet the approved recall, false-positive, routing, evidence, and validation gates?

### Durability and recovery

- [ ] Is Complete emitted only after all required run artifacts and the final manifest are durable?
- [ ] Can a user resume failed/cancelled work from the current screen and after app restart?
- [ ] Do required reviewer failures block complete synthesis, while optional failures appear in limitations?
- [ ] Can a saved effective specification reproduce the graph, prompts, inputs, provider/tool plan, and hashes?
- [ ] Are project drafts, deleted reports, batch queues, and human annotations protected from silent loss?
- [ ] Is automatic retention explicitly configured, with protected runs excluded and every candidate recoverable through Trash?

### Usability and accessibility

- [ ] Can a new user configure a compatible provider/extractor and generate the bundled sample without external help?
- [ ] Does the full frontend suite pass repeatedly, including large-report search?
- [ ] Do core workflows pass keyboard, screen-reader, reduced-motion, zoom/text-scaling, and minimum-window review?
- [ ] Are native browser dialogs gone from production?
- [ ] Are export contents and sensitivity clear before saving or sharing?

### Release integrity

- [ ] Are CI checks required on the exact commit being packaged?
- [ ] Do all advertised installers pass clean-machine packaged E2E?
- [ ] Does one aggregator verify the complete signed artifact set, hashes, SBOMs, provenance, and attestations?
- [ ] Can no platform matrix job publish, overwrite, or partially expose a release?
- [ ] Do release, support, privacy, security, and changelog claims match the enforced manifest?

---

## 11. Final recommendation

Pipeline should ship as a product that makes narrower claims and proves them exceptionally well. The highest-return work is not another reviewer role or provider. It is the evidence and release spine: secret-safe configuration, authoritative egress, isolated tools, durable completion, dependency quorum, a strict findings graph, semantic locator verification, exact effective-run capture, renderer parity, and a benchmark that measures whether the review is actually right.

Once those contracts exist, the rest of the roadmap becomes substantially easier. First-run guidance can explain real readiness, the Quality panel can expose real verification state, exports can be reliably audience-specific, workflows can become safely portable, and provider/model changes can be promoted on evidence rather than intuition. That is the path to a useful, robust, extensible Pipeline that can credibly deliver extremely high-quality reports.
