# Workspace simplification audit

**Prepared:** September 7, 2026.
**Scope:** the Workspace route (`WorkspacePage` and its panels), the project
surface, the Research tools studio, the Workspace connection settings, and the
`workbench` Rust backend that serves them. Workflows, the Workflow editor, and
the shared Codex runtime under `agent_runtime/` are out of scope.
**Method:** read the plan documents, the Workspace/studio components, and the
backend modules; counted commands, tables, and lines; traced every write to a
read and every command to a caller. Findings marked *verified* were checked
directly in source; the rest come from the three module-level audits and carry
file references so they can be re-checked.
**No code was changed.**

Two caveats matter for the plan:

1. The whole `workbench/` tree and every `Workspace*.tsx` file are **untracked
   in git**. Nothing below is reversible until the current state is committed.
2. The tree changed while the audit ran. `release/exchange.rs`,
   `release/retention.rs`, and `release/workflow_draft.rs` (PI-11 and PI-12)
   were added and wired into `release.rs` and `commands.rs` between the first
   and second read, taking the command count from 105 to 117. Section 3.1 treats
   that work as in scope for removal; if another session owns it, stop that
   session before starting Phase 1.

---

## 1. Baseline

| Measure | Value |
|---|---|
| Rust production lines under `workbench/` | ~20,700 (17,778 before PI-11/12 landed) |
| Rust test lines under `workbench/` | ~5,000 |
| Frontend Workspace + studio component lines | ~7,300 |
| Frontend client contracts (`workbenchClient`, `studioClient`, `projectClient`) | 979 |
| `workbench_*` Tauri commands | 117 |
| SQLite tables after migration 9 | 39 |
| Tables no UI or service ever reads | 10 |
| Client wrappers with zero component callers | 13 |
| Research inspector tabs | 7 |
| Project surface tabs | 4, one of which opens a further 6-tab studio |
| Distinct status/state vocabularies | 24+ |
| Distinct "receipt"/journal mechanisms | 8 |
| Idempotency implementations (operation id + request hash) | 6 |
| Secret-environment denylists (with two different key sets) | 4 |

For contrast, the studio alone (`research-studio/` + `studioClient.ts` +
`project/studio/`) is about 10,300 lines. The chat client that the studio lives
inside is about 2,100 lines.

## 2. What the Workspace is for

The plan's own "conversation first" principle and the acceptance scenarios
reduce to five things a researcher does repeatedly:

1. Chat with ChatGPT, resume later, keep drafts.
2. Attach a paper, select a passage, ask about it, read the page image.
3. Keep a short list of accepted decisions and abandoned approaches that a
   fresh conversation can see.
4. Run a configured LaTeX, Stata, or Python command with streamed logs, a stop
   button, and a receipt.
5. Let the model edit a task copy of the manuscript and accept the diff.

Everything in Section 3 either serves none of these, serves them with more
ceremony than the benefit justifies, or duplicates something that already does.
Section 5 lists what should stay untouched.

---

## 3. REMOVE

Ordered by confidence and by user-visible impact. "Zero impact" means no
control or behaviour the user can reach today changes.

### 3.1 Dead, unreachable, or write-only code — zero impact

| # | Item | Where | Why |
|---|---|---|---|
| R1 | Selective project exchange (PI-11) | `release/exchange.rs` (2,112 lines), `migrations/009` tables `exchange_imports`, `exchange_conflicts`, 7 commands `workbench_*_project_exchange*` / `*_exchange_conflict*` | Coauthor packages with namespaced ID remapping and conflict resolution are collaboration infrastructure the plan itself lists as deferred. `.pwrx` whole-store export already covers portability. No UI reaches it. *Verified present and wired; no component references it.* |
| R2 | Storage retention and trash (PI-11) | `release/retention.rs` (477 lines), table `storage_trash`, commands `workbench_prune_storage`, `_restore_trash`, `_empty_trash` | A recoverable trash for build caches inside an app-private directory. If disk use matters, one "delete unreferenced blobs" button suffices; see S17. |
| R3 | Workflow drafting from Workspace (PI-12) | `release/workflow_draft.rs` (334 lines), `workbench_draft_workflow` | Crosses the mode boundary the docs spend pages defending, for a feature the plan gates on "demonstrated bottlenecks". |
| R4 | Performance budgets and samples | `release.rs:707-800`, table `performance_samples`, `workbench_record_performance`, `workbench_performance_budgets`, `WorkspaceReleasePanel.tsx:140-148` | Nine engineering SLOs rendered to the researcher with no observed values; nothing ever selects from `performance_samples`. *Verified: only a `COUNT(*)` in archive export reads it.* A release checklist, not a feature. |
| R5 | Research evaluation records | `release.rs:804-901`, table `research_evaluations`, `workbench_record_research_evaluation`, `workbench_list_research_evaluations`, client wrappers | A/B harness for the product's own value with no UI. The 20-case fixture can stay as a test fixture; the table and commands go. |
| R6 | Per-turn config and context snapshot tables | `config_snapshots`, `context_snapshots`, inserts at `research.rs:896-897`, archive special-cases `archive.rs:407,983` | Written on every turn, never read by any command. *Verified: the only read is the new retention module's blob scan.* The harness fingerprint already lives on `session_bindings`. |
| R7 | `session_context_items` table | `migrations/003:83` | Zero references anywhere. |
| R8 | `retained_blobs` table | `archive.rs:1029-1037` | Truncated, rebuilt, and counted during export; nothing consumes it. |
| R9 | `verification_records` and the evidence verification chain | `execution.rs:1224-1283`, `workbench_verify_evidence_results`, `workbench_compare_results` (top-level), `workbench_record_structured_result` (top-level) | No component calls any of the three. Structured results still enter through the studio's Experiments import, which calls the same service internally, so only the top-level commands and the verification table go. |
| R10 | `workbench_link_review_handoff` and `review_handoffs.external_reference` | `release.rs:1100-1110` | Two-way linking that was never wired; the handoff is one-directional in `App.tsx`. |
| R11 | Thirteen unused client wrappers | `workbenchClient.ts`: `clearWorkspaceRoot`, `updateWorkspace`, `sessionSnapshot`, `paperRead`, `proposeEvidence`, `compareResults`, `recordStructuredResult`, `verifyEvidenceResults`, `recordPerformance`, `recordResearchEvaluation`, `listResearchEvaluations`, `linkReviewHandoff`, `reconcileWorkspaceRoots` | Test-only surface. Remove the wrapper and, where no service needs the command, the command. |
| R12 | Dead fields with live validation | `TheoryCheck.recipeRunId` (`theory.rs:340-355`, UI hard-codes `null`); `tolerance`/`precision` on checks (stored, used in no computation); `JobStatus.resource`; `pageInspection`, `synchronization`, `specificationDifferences`, `packageHash`, `duplicateCandidates`, `projectNotesStoredSeparately`, `attachmentAcquisition`; `ResearchTask.expectedOutputs`/`expectedChecks` (never rendered); `Application.checksValid` (never rendered) | Typed, validated, serialized, never displayed or consumed. |
| R13 | `"recipe"` harness module kind and the two `inspector` modules | `workbenchTypes.ts:258`; `research.rs:144-145` | The kind is never emitted; the inspector modules are chips in the Setup tab that gate no tool and add no instruction. |
| R14 | Codex protocol probe as shipped source | `codex/probe.rs` (1,281 production lines), `bin/workbench_probe.rs` | Hand-rolled JSON-RPC client that duplicates `agent_runtime/codex/transport.rs`, `process.rs`, and `toml_basic_string`. Its ~120 lines of real value (request builders and schema assertions) belong in an `#[ignore]` integration test built on the shared transport. |

Estimated removal: about 5,500 Rust lines, 300 TypeScript lines, 25 commands,
9 tables.

### 3.2 Speculative studio features — small visible impact

| # | Item | Where | Why |
|---|---|---|---|
| R15 | Zotero local-API preview and import | `literature.rs:342-494`, `Literature.tsx:268-396`, 2 commands | Never succeeded on the qualification machine; imports metadata only; Zotero exports BibTeX, which the panel already reads. |
| R16 | SyncTeX forward/inverse search | `manuscript.rs:351-542`, `Manuscript.tsx:632-707`, `workbench_build_sync` | Spawns an external `synctex` binary with process-group tracking so the user can type page X/Y coordinates into three number boxes. Every TeX editor the user already owns does this. |
| R17 | "Record inspection of build page N" | `manuscript.rs:557-593`, `Manuscript.tsx:708-724` | Writes `pageInspection: researcher_recorded`; nothing reads it. |
| R18 | Impulse-response series (`research-results-v2`) | `results.rs:111-146, 365-416`, `Experiments.tsx:682-748`, `series` record kind, `workbench_compare_series` | One "explicitly illustrative" fixture produces them; the section is invisible until you run that fixture. |
| R19 | Unit-conversion recording in result comparison | `results.rs:309-325`, `Experiments.tsx:508-537` | A positive-affine conversion with a mandatory rationale, to compare two of your own regression outputs. Keep the blocker ("units differ"); drop the conversion form. |
| R20 | Specification metadata as hand-written JSON | `results.rs:11-24, 68-75`, `Experiments.tsx:619-681` | Asks the researcher to author `{"controls":{"value":[…],"origin":"declared","source":"…"}}` in a textarea. Replace with two text fields (specification id, sample id) on the experiment record. |
| R21 | Findings exchange **file** format in Responses | `review.rs:54-80`, `Responses.tsx:262-280, 319-349`, `workbench_finding_package_preview` | Package hashing, 4 MiB JSON import/export, and reimport semantics exist so the Workspace can consume a file the same app produced. Keep the direct "choose a completed Workflow run" import (R21 does not remove `workbench_workflow_finding_preview`). Ungate focused re-review from the bridge checkbox (`Responses.tsx:655`). |
| R22 | Theory derivation promotion | `theory.rs:462-596`, `Theory.tsx:585-649` | 135 lines to paste a paragraph into a `.tex` file wrapped in comment markers. The researcher pastes the paragraph. |
| R23 | Theory check-scope taxonomy and evidence counters | `theory.rs:234-313, 598-683` | Six scope values and seven counters; the UI renders one string (`label`). Keep the rule "numerical checks cannot claim generality" as a single boolean and one label. |
| R24 | `origin: proposed` on theory notes, checks, and bindings | `Theory.tsx:380-391`, `theory.rs:222-226`, `results.rs:455` | No tool in the catalog can propose any of these. Three selects, three validation branches, zero producers. |
| R25 | `unresolved_step` as a theory-note *kind* | `theory.rs:10-19` | Unresolved steps are already a list field on every note, and only the field participates in the `supported` rule and the context text. |
| R26 | Git worktree isolation backend | `tasks.rs:230`, `WorkspaceProjectSurface.tsx:113` | Requires host Git through the zsh launch policy, hooks disabled, a `codex/` branch, and `--no-checkout`. The local-copy backend already gives an isolated task root. Git provenance was optional in the plan. |
| R27 | `.pwrx` restore prompt flow | `WorkspaceReleasePanel.tsx:83-87` | `window.prompt` asking for hand-edited JSON root remappings, then a page reload. Either build a real remap form (S13) or hide restore behind Settings until one exists. Export stays. |

Estimated removal: about 1,800 Rust lines, 900 TypeScript lines, 6 commands.

### 3.3 Chat-surface clutter — visible, deliberate

| # | Item | Where | Why |
|---|---|---|---|
| R28 | "Voice input" composer menu item | `WorkspaceComposerMenu.tsx:32-39` | Its entire content is a sentence saying voice is unavailable and a button that focuses the textarea. |
| R29 | Disabled "Native web search" checkbox | `WorkspaceResearchPanel.tsx:128`, `research.rs:800-820` | A control that cannot be turned on. Mention the limitation once in Help. |
| R30 | "Instruction source and resolved context" details block | `WorkspaceResearchPanel.tsx:132` | Raw developer-instruction dump, `valueSources` JSON, and a truncated fingerprint. Debug output. Keep "effective preset: X" as one line. |
| R31 | Note revision history (latest 50) details block | `WorkspaceProjectSurface.tsx:111` | Nobody audits their own note edits; the change ledger remains for recovery. |
| R32 | `literature_review` preset | `research.rs:168` | Module-identical to `research_assistant`; differs by one sentence of prompt. Fold the sentence into the research preset's editable instructions. |
| R33 | Recipes as a separate subsystem | `release.rs:51-700`, tables `recipe_definitions`, `recipe_runs`, 7 commands, `WorkspaceRecipesPanel.tsx` | Nine prompt templates plus a checklist whose "checks" are self-attested checkboxes stored as `recorded`/`not_recorded`, kept in `useState` so they vanish when the panel closes. The host verifies nothing. See S3 for what replaces them. |

---

## 4. SIMPLIFY

### 4.1 Structure and navigation

| # | Item | Now | Proposed |
|---|---|---|---|
| S1 | Research inspector tabs | Setup, Documents, Recipes, Memory, Evidence, Results, Release | **Setup, Documents, Notes, Runs.** Recipes fold into Setup (S3), Evidence folds into Notes (S9), Release moves to Settings (S13). |
| S2 | Project surface tabs | Project home, Documents, Changes, Research tools (6 more tabs inside) | **Home, Documents, Changes, Manuscript, Responses, Runs.** Experiments and Result links merge into Runs; Literature and Theory become note kinds (S9, S10). |
| S3 | Presets, instruction packs, modules, recipes | 6 presets × 9 modules × 9 recipes with a module registry, capability flags, and a recipe-run lifecycle | One concept: a **preset** = editable instruction text + three switches (paper tools, propose notes, run commands). Ship `plain`, `research`, `research + execution`. The nine recipe prompts and the three instruction packs become entries in a "Insert instructions" picker on the preset editor. No recipe tables, no completion cards, no run lifecycle. |
| S4 | Document import entry points | Three (composer +, Research Documents tab, Project home) with three different default roles | One import action, one default role, reused by all three places. |
| S5 | Note surfaces | Memory tab and Project home Notes, same table, different affordances | One notes component with kind, accept/reject, pin-to-brief, and exclude-from-context. |
| S6 | Execution profile editors | Three writers: Research Results tab (JSON argv textarea), Experiments (friendly form), Manuscript build settings (synthesizes a profile) | One profile form (the Experiments one) used by Runs and by Manuscript build settings. Delete the JSON-argv variant. |
| S7 | Jobs views | Studio Jobs tray and the Research Results tab both list executions with Stop and the Stata cleanup alert | One Runs panel with the Jobs tray. |
| S8 | Harness context budget and access controls | Budget select (16/64/256/512 KiB), access mode, command network, successor-thread warning, diagnostics, unavailable-module chips, workspace-default save/reset | Keep access mode and command network. Fix the budget at 64 KiB with an advanced override in Settings. Keep the successor-thread warning. Drop module chips. |

### 4.2 Data model

| # | Item | Now | Proposed |
|---|---|---|---|
| S9 | Claims, claim versions, evidence links, freshness | Four tables, 4-state claim workflow, 5-state assessment, 3-state freshness, propose/accept/confirm flows, staleness propagation | A **note with an optional locator** (paper revision + page or byte span, or execution id) and one `confirmed_by_user` flag. Staleness becomes a computed warning when the located revision or execution is no longer the accepted one. Model proposals stay proposals. This removes `claims`, `claim_versions`, `evidence_links`, and the Evidence tab. |
| S10 | Theory notes, literature notes, research tasks, research directions | Four parallel note-like records in `project_records` with 8 + 4 + 5 + 4 statuses | Keep them as **kinds of note** on the one notes table: `theory`, `literature`, `direction`, `task`, each with a small typed detail JSON. Statuses collapse to `open`, `done`, `rejected`, with `abandoned` reason kept for theory (its context injection is the best feature in the studio). |
| S11 | Numeric result bindings | 8 fields per bound number (result, anchor, role, component, printed text, precision, units, origin) plus a confirmation checkbox | Keep result, anchor, component, printed text. Derive precision from the printed text. Make "Generate TeX value macro" the primary action so numbers stop drifting instead of being audited after the fact. |
| S12 | Referee response record | Six prose fields (intended response, draft, rationale, disputed premise, counterargument, resolving check) plus category, severity, disposition | Draft, optional rationale, category, disposition. Validate severity or drop it (it is unvalidated today: `review.rs:259-274`). |
| S13 | Archive restore | Prompt-driven JSON remap | Move export/restore to Settings → Workspace. Restore shows one row per archived root with a folder picker or "detach". |
| S14 | `project_records` CHECK constraint on `kind` | Rebuilt the table in migrations 7 and 8 to widen a list that typed services already validate | Drop the CHECK; validate in Rust only. |
| S15 | Status vocabularies | 24+ enums, several near-duplicates (task status vs response disposition; note state vs claim state; four ways to say pass/fail/unknown) | One `record_state` (`proposed`, `accepted`, `rejected`, `retired`), one `outcome` (`passed`, `failed`, `unknown`), and the execution lifecycle. Everything else is a display label. |
| S16 | Provenance labels | Eight free-string channels plus constant strings such as `declared_only`, `unverified_live_files`, `host_adopted_output` written into JSON on every record | One `origin` enum (`user`, `model`, `host`) and a per-execution `coverage` string. Constant strings belong in docs, not rows. |
| S17 | Disk retention | Trash journal with restore | "Delete cache and unreferenced blobs" with a size preview; blobs referenced by any note, artifact, or execution are kept. |

### 4.3 Backend plumbing

| # | Item | Now | Proposed |
|---|---|---|---|
| S18 | Idempotency | Six implementations of operation-id + request-hash (`change_log`, `execution_jobs`, `project_operations` twice with identical SQL, `recipe_runs`, `review_handoffs`) | One `operations` table and one `run_idempotent(op_id, request, f)` helper in `commands.rs`. |
| S19 | Execution finalization journal | `execution_jobs.finalization_json` duplicates every terminal column of `research_executions`, written back to back (`execution.rs:936-937`); `reconcile_job` re-hashes blobs to replay it | Adopt outputs to blobs first, then write the receipt in one transaction. Keep the "outcome unknown after restart" marking. The journal, `workbench_reconcile_job`, and the Reconcile button go. Lower confidence than S18: check the crash matrix in `release-qualification.md` first. |
| S20 | Secret-environment denylist | Four copies with two key sets (`probe.rs:25`, `agent_runtime/codex/process.rs:10`, `execution.rs:144`, `execution.rs:447`) | One shared constant. Do this regardless of everything else; it is a security inconsistency. |
| S21 | Manuscript working-copy save | Each save creates a task, a checkpoint, and an application (`manuscript.rs:82-107`), and every open task is injected into model context (`project.rs:270-275`). Twenty saves of `main.tex` become twenty "Open task: Edit main.tex" lines in every turn. *Verified.* | Working-copy saves go through the application journal without a task record. Only user-created tasks exist. |
| S22 | Migrations | Nine, with two full rebuilds of `project_records` and one of every table (migration 2) | Workspace is unreleased. Squash to a single `001_schema.sql` at version 1 with the reduced table set; on startup, an existing store with a higher-numbered legacy version is backed up to `backups/` and a fresh store is created, with a one-time notice. Keep `.pwrx` export working on the *old* build so a user can carry data across if they want. If squashing is unacceptable, add migration 010 that drops the dead tables and leave the chain. |
| S23 | Command surface | 117 commands, three client modules | Target ~60. Collapse per-record CRUD into `workbench_records_list/get/mutate` with a typed `kind`, as `studio_mutate` already does. |

---

## 5. KEEP as-is

- The chat client: sidebar, transcript window, outline, copy, export, archive.
- Paper import with immutable revisions, search, page render, and the
  `paper_read` / `paper_search` / `paper_page` / `anchor_read` tools.
- The document reader and its selection actions (Ask, Add note, Create task).
- Anchors and `map_anchor` across revisions.
- Notes with accept/reject and pin-to-brief; abandoned-approach context
  injection (`theory.rs:685-739`).
- Host-execution authorization: preview, fingerprint over the resolved
  executable and zsh startup files, re-verification at spawn.
- The Stata policy (`/bin/zsh -lic` + `oldstata`, direct binaries banned) and
  the LaTeX shell-escape ban.
- The two-slot job queue, streamed logs, Stop, turn-vs-detached ownership,
  inputs-changed-while-queued abort, unchanged-preexisting-output check.
- Task copies with capture, per-file accept, undo, and recovery journal
  (local-copy backend only).
- Manuscript editor with hash-checked saves and configured TeX builds, retained
  PDF/log per build, two-up PDF comparison.
- Referee report import, paragraph split, response matrix, letter export,
  direct import of findings from a completed Workflow run, focused re-review.
- `.pwrx` export.
- The supervisor, transport, turn permit, and epoch-scoped approvals.

---

## 6. Plan

Each phase ends with the full check set:

```bash
cd gui && npm test -- --run && npm run build
cd src-tauri && cargo fmt --all -- --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked --all-targets
```

### Phase 0 — Freeze (half a day)

1. Stop any other session editing `workbench/` or `release/`.
2. Commit the current untracked tree on a branch (`workspace-baseline`) so
   every later deletion is a reviewable diff. Nothing in this plan is
   reversible until this is done.
3. Record the baseline numbers from Section 1 in the PR description.
4. Decide S22 (squash vs. migration 010). The rest of the plan assumes squash.
5. If any local research store holds data worth keeping, export a `.pwrx`
   with the current build now.

### Phase 1 — Delete dead code (1–2 days, zero user-visible change)

Order matters only where noted; each bullet is one commit.

1. **PI-11/PI-12 removal (R1–R3).** Delete `release/exchange.rs`,
   `release/retention.rs`, `release/workflow_draft.rs`; remove the three
   `mod`/`pub use` lines in `release.rs:1112-1121`; remove the 12 commands from
   `commands.rs:609-679` and their `lib.rs` registrations; delete migration 009.
   Grep `gui/src` for any wrapper that appeared with them.
2. **Performance and evaluation (R4, R5).** Delete `release.rs:707-901`, the
   two `WorkspaceReleasePanel.tsx` sections, five commands, four client
   wrappers, and the `research_evaluations` / `performance_samples` counts in
   `archive.rs:587`. Move `docs/workbench/research-evaluation-v1.json` under
   `tests/fixtures/` if a test still reads it; otherwise delete it.
3. **Snapshot tables (R6–R8).** Remove the inserts at `research.rs:896-897`
   and the archive special-cases; drop `config_snapshots`, `context_snapshots`,
   `session_context_items`, `retained_blobs` from the schema. Confirm
   `session_bindings.harness_fingerprint` still carries what the inspector
   shows.
4. **Verification chain (R9, R10).** Delete `verify_evidence_results`,
   top-level `compare_results` and `record_structured_result` commands (keep
   the `research::` service functions the studio calls), `verification_records`,
   `link_review_handoff`, and the `external_reference` columns.
5. **Unused wrappers and dead fields (R11–R13).** Remove the 13 client
   wrappers and their commands where nothing else calls them. Strip the fields
   in R12 from structs, validators, and TS types. Delete the two inspector
   modules and the `"recipe"` kind.
6. **Denylist (S20).** Create `workbench/env.rs::SECRET_ENVIRONMENT_KEYS` with
   the union of both key sets; replace the four copies.
7. **Probe (R14).** Move `probe.rs` request builders and schema assertions to
   `src-tauri/tests/workbench_probe.rs` as an `#[ignore]` test built on
   `agent_runtime::codex::transport`; delete `codex/probe.rs` and
   `bin/workbench_probe.rs`; update the three doc references and CLAUDE.md.
8. **Idempotency (S18).** Add `operations` table and helper; migrate the six
   call sites; drop `project_operations`, `recipe_runs.operation_id`,
   `review_handoffs.operation_id`, and the `change_log` request-hash reads.
9. **Schema squash (S22).** Write `001_schema.sql` with the surviving tables;
   set `CURRENT_SCHEMA_VERSION = 1`; add the legacy-store backup path in
   `store.rs::migrate`; delete migrations 002–009 and the `project_records`
   CHECK (S14). Update `store/tests.rs` fixtures.

Exit check: 117 → ~85 commands; 39 → ~26 tables; Workspace behaves identically
in a manual walk-through of the five core actions in Section 2.

### Phase 2 — Studio cuts (2–3 days)

1. **Literature (R15).** Delete `literature.rs:342-494`, `Literature.tsx:268-396`,
   `workbench_zotero_preview/import`, and the Zotero paragraph in
   `research-studio.md`.
2. **Manuscript (R16, R17, S21).** Delete `manuscript.rs:351-593` except the
   two-up reader; remove `workbench_build_sync`; remove the SyncTeX and
   page-inspection JSX. Change `save_text` so a working-copy save with no
   checkpoint writes through `tasks::apply`'s journal directly and creates no
   task. Add a regression test: ten saves of one file leave zero open tasks
   and no "Open task" lines in `project_context`.
3. **Experiments (R18–R20).** Delete series validation, `compare_series`, the
   IRF section, the conversion form, and the specification JSON editor; add
   `specification_id` / `sample_id` text fields to the experiment record.
4. **Responses (R21, S12).** Remove the file-package import/export and its
   4 MiB check; keep run-based finding import; ungate focused re-review; reduce
   the response record to draft, rationale, category, disposition; validate or
   drop severity.
5. **Theory (R22–R25).** Delete promotion, `check_scope`, `TheoryEvidence`
   counters, the `origin` select, and the `unresolved_step` kind. Replace
   `check_scope` with `fn claims_generality(method, outcome) -> bool` and one
   label string.
6. **Tasks (R26).** Remove the Git worktree backend from `tasks.rs` and the
   backend select; the local-copy path is unchanged.
7. **Profile editors and job views (S6, S7).** Extract the Experiments profile
   form into `research-studio/ProfileForm.tsx`; use it from the Research Runs
   tab and Manuscript build settings; delete the JSON-argv form and the
   duplicate execution list in `WorkspaceResearchPanel.tsx`.

Exit check: `research-studio/` from ~5,150 to ~3,000 lines;
`project/studio/` from ~4,500 to ~2,800; the manuscript-save regression test
passes.

### Phase 3 — Chat surface consolidation (2 days)

1. **Presets replace recipes (S3, R32, R33).** Delete `release.rs:51-700`,
   `WorkspaceRecipesPanel.tsx`, `recipe_definitions`, `recipe_runs`, and seven
   commands. Reduce `harness_presets()` to `plain`, `research`,
   `research_execution`; move the nine recipe prompts and three instruction
   packs into `prompts/workspace/*.md` and expose them through an
   "Insert instructions" picker in the preset editor. The per-turn instruction
   assembly in `resolve_harness` keeps working with the preset text alone.
2. **Tabs (S1, S8).** Reduce the inspector to Setup, Documents, Notes, Runs.
   Remove the budget select, module chips, and the debug details block (R30).
   Keep access mode, command network, and the successor warning.
3. **Composer (R28, R29).** Remove the voice item and the disabled web-search
   checkbox; add one sentence to Help.
4. **Imports and notes (S4, S5, R31).** One `importDocument()` helper with one
   default role; one `NotesPanel` used by the inspector and project home;
   remove the revision-history block.
5. **Release panel (S13, R27).** Move export/restore into Settings → Workspace;
   build the per-root remap form; delete `WorkspaceReleasePanel.tsx`.
6. **Project surface (S2).** Fold Experiments and Result links into Runs;
   render Literature and Theory notes through the notes panel with their typed
   detail forms.

Exit check: Research inspector has four tabs; project surface has six; no
`window.prompt` or `window.confirm` remains in Workspace components.

### Phase 4 — Data-model collapse (3–4 days, the only phase with migration risk)

1. **Notes absorb claims and evidence (S9).** Add `locator_json` and
   `confirmed_by_user` to `research_notes`; port `claim_propose` and
   `evidence_propose` tools to `note_propose` with a locator; compute staleness
   at read time from the accepted revision and latest execution; delete
   `claims`, `claim_versions`, `evidence_links`, `research_ledger`, and the
   Evidence tab. Update the `research_assistant` instructions so the model
   proposes located notes instead of claims.
2. **Notes absorb studio record kinds (S10, S15, S16).** Move theory,
   literature, direction, and task records onto the notes table with typed
   detail JSON; map their statuses onto `record_state`; keep `abandoned_reason`
   for theory and the direction→task conversion. Replace free-string origins
   with the `origin` enum.
3. **Bindings (S11).** Reduce the form; make macro generation primary.
4. **Finalization journal (S19).** Only after re-reading the crash matrix; if
   the journal exists to survive a crash between blob adoption and the receipt
   write, keep it and drop only the Reconcile UI.
5. **Command surface (S23).** Collapse remaining per-kind CRUD into the
   `records_*` trio.

Exit check: tables ~18; commands ~60; status enums 3; the ten-step acceptance
scenario in `workbench_plan.md` §12 still passes minus the evidence-specific
steps, which are re-expressed as located notes.

### Phase 5 — Documentation (half a day)

1. Rewrite the Workspace section of `CLAUDE.md` to describe the reduced
   system; delete the WB-05 through WB-12 and PI-04 through PI-12 narrative.
2. Replace `workbench_plan.md` and `PIPELINE_IMPROVEMENT.md` with one short
   `docs/workbench/README.md` (current behaviour) and one `docs/workbench/
   deferred.md` (what was removed and why, with this file's item numbers).
3. Update `release-qualification.md` to remove gates for deleted features.

---

## 7. Expected result

| Measure | Before | After (estimate) |
|---|---|---|
| Rust production lines under `workbench/` | ~20,700 | ~10,000–11,000 |
| Frontend Workspace + studio lines | ~7,300 | ~3,500 |
| Commands | 117 | ~60 |
| Tables | 39 | ~18 |
| Inspector tabs | 7 | 4 |
| Project/studio tabs | 4 + 6 | 6 |
| Status vocabularies | 24+ | 3 |
| Migrations | 9 | 1 |

The user-visible losses are Zotero, SyncTeX, impulse-response comparison,
findings exchange files, theory promotion, Git worktrees, recipes as a lifecycle,
and the claims/evidence ledger as a separate object. Everything in Section 2
survives, most of it with fewer clicks.

## 8. Two things to fix even if nothing else is adopted

1. S21: manuscript saves mint open tasks that pollute every model turn's
   context. *Verified.*
2. S20: four secret-environment denylists with two different key sets.
