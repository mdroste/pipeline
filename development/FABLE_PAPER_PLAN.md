# Auto Paper Review — improvement plan

Written 2026-08-12. Goal: make **Auto Paper Review** a detail-oriented academic
auditing tool whose final report reads like a careful referee's, with every
finding anchored, verified, and annotatable. Items are ordered by expected
impact (P0 highest). Each item states the change, the reason, and the concrete
edits with file paths so any coding agent can execute it independently.

## Baseline (what exists today)

- Saved profile is a four-step skeleton built in `gui/src-tauri/src/auto_review.rs::steps()`:
  `auto_contribution` (WebSearch), `auto_consistency`, `auto_exposition` (Parallel)
  and `auto_synthesis` (Sequential).
- One combined orientation/classification call (`prompts/auto_review/orientation.md`,
  schema in `auto_review.rs::orientation_schema()`, contract `auto-review-v2`)
  inventories the paper and selects 1–2 subject + 1–4 method specialist IDs from
  the host-owned catalog (239 subjects in `auto_review/subjects.rs`, 62 methods in
  `auto_review/methods.rs`). `materialize_config()` inserts the selected steps
  before synthesis and wires their reports into synthesis.
- Specialist prompts = role focus + shared contract
  (`prompts/auto_review/subjects/_review_contract.md`,
  `prompts/auto_review/methods/_review_contract.md`). Comment ceilings: core 6,
  specialists 7, synthesis 12.
- Context: every Parallel step gets `primary{text,structure,visuals,source}` +
  `survey` (`pipeline_config/builtins.rs::configure_artifact_flow`, called with
  `primary_readers = &[]`). **`auto_synthesis` therefore never sees the paper** —
  only the survey and upstream reports.
- There is **no verification step** (the Full profile's `validate_feedback` has no
  Auto counterpart) and synthesis emits plain markdown, so Auto runs never get
  the Issues table, per-issue accept/reject annotations, evidence deep-links
  into Sources, or `draft_calibration`.

## Cross-cutting requirements (apply to every item that touches them)

- **Profile migration.** `write_builtin_if_missing` never rewrites an existing
  `~/.pipeline/profiles/auto-review.json`. Any change to the saved skeleton or
  orientation schema needs a one-time migration in
  `gui/src-tauri/src/pipeline_config/migrations.rs` that upgrades *semantically
  untouched* stock Auto profiles and leaves user-edited ones alone (precedent:
  the legacy catalog migration and `auto_review/legacy.rs`).
- **Contract versioning.** Schema-shape changes (items 8, 16) bump
  `AUTO_REVIEW_CONTRACT` to `auto-review-v3`; materialization and rerun must
  keep recognizing `auto-review-v2` (and v1) orientations exactly as
  `legacy.rs` does today, so saved runs stay resumable and rerunnable.
- **Tests.** Skeleton-shape changes update
  `saved_profile_is_a_four_step_skeleton`,
  `materialization_inserts_only_selected_specialists_before_synthesis`,
  `every_discipline_fallback_materializes_and_the_run_stays_under_ten_steps`,
  and `assert_report_output_fields` in `auto_review.rs`. Contract-text changes
  update the required-string assertions there. Run
  `cargo test --locked --all-targets` and `npm test`.
- **Docs and UI copy.** Keep `docs/auto-review-catalog.md`, the CLAUDE.md Auto
  section, and the workflow-editor virtual-slot copy (step-count ranges,
  "four-step skeleton") in sync with skeleton changes.
- **Prompt-size safety.** Host-injected prompt additions must go through the
  same length guard as `apply_agent_count_instruction`
  (`safety::MAX_EXPANDED_PROMPT_BYTES`).

---

## P0 — Verification chain and ground truth

The single biggest gap: nothing between the specialists and the final report
ever re-checks a claim against the manuscript. False positives are the main
credibility risk of an LLM referee, and detail-orientation is worthless if the
details are wrong.

### 1. Add an enabled-by-default verification step `auto_verify` (impact: high, effort: M)

- **Change.** New Sequential step after `auto_synthesis`, prompt
  `prompts/auto_review/verify.md` adapted from `prompts/validate_feedback.md`
  (keep its false-positive taxonomy: "missing" items that exist in appendices,
  contradictions from partial reading, misquotes, alternative valid
  derivations, extraction artifacts, visual re-inspection of disputed
  tables/figures). It verifies every consolidated comment against the paper,
  repairs salvageable details, drops unsupported ones, and never adds issues.
- **How.**
  - `auto_review.rs::steps()`: append `auto_verify` (Sequential, no tools).
  - `pipeline_config/builtins.rs::auto_review_profile()`: call
    `configure_artifact_flow(steps, "document", &["auto_verify"])` so the step
    receives `primary{text,structure,visuals,source}` for evidence checking.
  - After `configure_artifact_flow`, prune `auto_verify.context.include` to
    `Primary`, `Survey`, and `Step("auto_synthesis", report)` — the generic
    flow would otherwise also hand it every raw parallel report, which bloats
    context and lets it re-litigate pre-consolidation text.
  - `materialize_config()` needs no change (specialists are inserted before the
    *first* Sequential step, which remains `auto_synthesis`).
- **Done when.** A stock Auto run has core + specialists + synthesis + verify;
  the verify step demonstrably drops a seeded false comment (see item 6);
  migration upgrades untouched stock profiles.
- **Status (2026-08-12): implemented.** Step id `auto_validate`, label
  "Validate Feedback", prompt `prompts/auto_review/validate.md`; consolidation
  renamed "Consolidate Feedback" with its ceiling raised to forty comments;
  `WebSearch` enabled by default on every skeleton and materialized specialist
  step. Migration `.builtin-catalog-v14` upgrades untouched stock v2 skeletons
  (preserving a configured adaptive-agent count); the prior synthesis text is
  frozen as `prompts/auto_review/synthesis_v1.md` so the v1/v11 fingerprints
  in `auto_review/legacy.rs` stay historically exact. Eval-based confirmation
  (item 6) still pending.

### 2. Deterministic quote verification feeding `auto_verify` (impact: high, effort: M)

- **Change.** After the parallel wave completes, Rust extracts every
  `**In the paper:**` quotation from step reports and checks it against
  `context/document.md` with normalized matching (collapse whitespace, strip
  markdown/ligatures/smart quotes, then windowed similarity). Write
  `artifacts/quote-check.json`: per quote `{step_id, comment, quote, matched,
  similarity, nearest_page}`.
- **Why.** Fabricated or drifted quotes are the most mechanical hallucination
  class and the cheapest to catch deterministically; the verify model then
  starts from evidence instead of re-deriving it.
- **How.** New module `gui/src-tauri/src/pipeline/quote_check.rs`; invoke from
  the executor between the last parallel wave and `auto_verify`; stage the JSON
  into `auto_verify`'s resolved artifact view (same staging seam
  `resolve_artifact_context` uses for synthesized reports) and append a short
  host-owned paragraph to the verify prompt: unmatched quotes are advisory
  flags, not automatic deletions (specialists legitimately paraphrase). Unit
  tests with seeded verbatim/paraphrase/fabricated quotes.

### 3. New core step: Numerical & Statistical Reporting Audit (impact: high, effort: M)

- **Change.** Fourth parallel core step `auto_numbers`, prompt
  `prompts/auto_review/core/numbers.md`. Remit — the checks a careful referee
  does with a pencil, currently diluted inside consistency's 6-comment cap:
  - abstract/introduction numbers vs. text vs. tables;
  - coefficient / SE / t / significance-star agreement (recompute t = coef/SE);
  - N consistent across columns, panels, and described sample restrictions;
  - percentages and shares that should sum; means inside plausible ranges;
  - units and scaling (log points vs. percent, per-1000 vs. per-100k, nominal
    vs. real, per-capita vs. total) between text claims and table units;
  - effect-size translations ("0.3 SD = X units") recomputed.
  Same output contract as other cores, cap 7.
- **How.** Add to `auto_review.rs::steps()` before synthesis. Gate with
  `run_if: SurveyPath { pointer: "/tables_figures/0", exists: true }` so pure
  theory papers skip it (skips record placeholders; synthesis omits them). The
  skeleton test asserting `run_if.is_none()` on all steps changes accordingly.
  Simultaneously edit `prompts/auto_review/core/consistency.md` to defer
  arithmetic recomputation to this step and focus on claim-vs-claim agreement,
  notation drift, and scope creep — otherwise the two steps double-report.
- **Done when.** A fixture with a wrong star (t = 1.2, three stars) is caught by
  `auto_numbers` and survives verification.

### 4. Structured issues output — light up the Issues table for Auto (impact: high, effort: M)

- **Change.** Make the *verified* end of the chain emit the issues-shaped JSON
  the app already understands (`prompts/editor_synthesis_issues.md` shape:
  `{issues: [{id, title, severity, section, body, evidence[]}]}` with evidence
  `{page, node_id, asset_id, artifact_path, quote, description}`).
  Recommended shape: `auto_synthesis` keeps consolidating in markdown;
  `auto_verify` outputs surviving comments as issues JSON with an attached
  `output_schema` (subset-schema validator retries on mismatch). The final
  report then renders through the existing issues machinery.
- **Why.** This one change unlocks four shipped features for Auto runs that
  currently only Full-with-issues gets: the Issues table, per-issue
  accept/reject/note annotations, evidence deep-links into the Sources view,
  and `draft_calibration`'s rejected-issue learning loop.
- **How.** New prompt `prompts/auto_review/verify_issues.md` (merge item 1's
  verification protocol with the issues JSON output rules); build the schema in
  Rust next to `orientation_schema()`; add `source_step` to each issue (see
  item 33). Extend `lib/issues.ts` only if the extra field needs surfacing.
  If evals (item 6) show verification quality degrades when combined with JSON
  emission, split into `auto_verify` (markdown) + `auto_issues` (JSON) — keep
  both variants behind the eval before choosing.

### 5. Enforce anchors end-to-end (impact: high, effort: S)

- **Change.** Every comment must carry a page **and** a structural anchor
  (section / equation / table / theorem), at both the specialist and the
  consolidated level.
- **How.** In both `_review_contract.md` files, tighten the `**Location:**`
  line: "Give a page number and the specific section, equation, table, figure,
  or theorem. A comment with no page anchor will be discarded." In
  `prompts/auto_review/synthesis.md`, extend the required per-comment format to
  end with a `**Location:**` line preserved from the strongest underlying
  report (today the format only mandates the bold title, so anchors routinely
  get summarized away). Update `assert_report_output_fields` and the synthesis
  assertions in `auto_review.rs` tests.

### 6. Evaluation harness with seeded-error papers (impact: high, effort: L)

- **Change.** A fixtures corpus + scorer so every prompt change in this plan is
  measured, not vibed. `development/eval/` containing:
  - 6–10 short synthetic papers (LaTeX, compiled to PDF) across paper types
    (proof-based theory, DiD, IV, RCT, structural, descriptive) with 8–15
    seeded defects each (wrong star, N mismatch, mis-stated theorem hypothesis,
    parallel-trends violation discussed nowhere, misattributed citation,
    dangling cross-reference, unit error) and clean control sections;
  - per-paper `expected_findings.yaml` (`{id, page, keywords[], severity}`);
  - a runner script that drives the dev-only `pipeline-cli run` per paper and a
    scorer matching report comments to expected findings by page ± 1 and
    keyword hit → precision / recall / false-positive counts per step.
- **Why.** Items 3, 5, 9, 11, 17–24 all edit prompts; without a fixed target
  set, regressions are invisible. This also measures the verify step's
  false-positive kill rate directly.
- **How.** Pure dev tooling, no app changes; runs from a source build. Document
  the workflow at the top of `development/eval/README.md`.

---

## P1 — Router quality and report shape

### 7. Inject each specialist's selection reason into its prompt (impact: high, effort: S)

- **Change.** `materialize_config()` appends a host-composed section to each
  materialized specialist step:
  `## Routing context\n\nYou were selected for this paper because: {reason}.
  Prioritize that material first.` using the already-validated
  `selection_notes` reason for that ID.
- **Why.** The reasons are paper-specific ("the staggered rollout in Table 2
  identifies the main effect") and currently buried in survey JSON the
  specialist must rediscover. This is the cheapest targeting improvement
  available.
- **How.** Plain text only, passed through `safety::strip_span_tags`, length
  guarded (cross-cutting note). Tests: a materialized step's prompt contains
  its note reason; a hostile reason with markup/instructions is neutralized to
  text.

### 8. Claims register in orientation; delete the advisory `paper_forms`/`methods` arrays (impact: high, effort: M)

- **Change.** Orientation gains a required `claims` array: the 2–6 claims the
  paper actually stakes, `{id: "C1", statement, kind:
  "theoretical|causal|descriptive|quantitative|methodological", location,
  evidence: "what the paper offers in support"}`. Both review contracts add a
  required `**Affects:**` line naming claim IDs (or `general`). Synthesis
  orders by affected-claim centrality. Simultaneously **delete**
  `review_plan.paper_forms` and `review_plan.methods` from the template and
  schema — free-text taxonomy that duplicates the specialist selection, looks
  like an enum but isn't, and spends orientation tokens on nothing downstream.
- **Why.** This is the connective tissue that turns scattered comments into an
  audit of the paper's actual argument: every finding says which conclusion it
  threatens, and the synthesis can say "C1 survives; C2 does not."
- **How.** Edit `prompts/auto_review/orientation.md`, `orientation_schema()`
  (contract bump to v3 per cross-cutting), both `_review_contract.md` files,
  `synthesis.md`; keep v2 orientations rerunnable (claims section absent →
  specialists fall back to `stated_contribution`). Update contract tests.

### 9. Define severity precisely; add a confidence line (impact: med-high, effort: S)

- **Change.** One canonical block in both `_review_contract.md` files:
  - *Critical* — a central claim is invalid as stated;
  - *Major* — a central claim is materially weakened or its scope must shrink;
  - *Moderate* — a correctable defect that would mislead a careful reader.
  Add a required `**Confidence:**` line: `demonstrated` (shown by
  derivation/quotation), `strongly indicated` (grounded inference), or
  `needs author response` (cannot be resolved from the manuscript).
- **Why.** Specialists currently self-calibrate "critical/major/moderate" with
  no definitions, so synthesis ordering mixes scales; the confidence axis is
  what real referees use to separate "this is wrong" from "please explain",
  and it gives `auto_verify` a triage signal (verify `demonstrated` claims
  hardest).
- **How.** Contract edits + `assert_report_output_fields` gains the new
  required strings + synthesis instructed to preserve the confidence line.

### 10. Scale the synthesis ceiling with the number of reports (impact: med-high, effort: S)

- **Change.** Replace the fixed "at most twelve comments" with a host-injected
  ceiling: `10 + 2 × (specialists beyond four)`, capped at 18. Inject at
  materialization exactly like `apply_agent_count_instruction` (prepend an
  instruction; do not edit the saved prompt).
- **Why.** A 9-report run and a 5-report run both funneling into 12 comments
  silently discards verified detail on exactly the papers that need more scrutiny.
- **How.** Small function in `auto_review.rs` invoked from
  `materialize_config()`; adjust `synthesis.md` wording ("Retain at most twelve"
  → "Retain at most the configured ceiling; default twelve when none given").
- **Status (2026-08-12): largely superseded.** The ceiling is now a flat forty
  ("Retain up to forty comments total") with a validation step downstream.
  Revisit report-count scaling only if evals show forty binds or floods.

### 11. `## Minor items` section in the final report (impact: med-high, effort: S)

- **Change.** `synthesis.md` (and the verify/issues prompts) gain a trailing
  `## Minor items` section: one-line, page-anchored bullets for verified small
  defects (mislabeled panel, wrong equation reference, notation typo) that
  don't merit numbered comments and don't count against the ceiling.
- **Why.** Real referee reports end with exactly this list; today those items
  are either dropped by the cap or inflated into numbered comments. This is
  where "detail-oriented" becomes visible to the author.
- **How.** Prompt edits; both contracts allow specialists an equivalent bounded
  trailing list (max 5 bullets) so the material reaches synthesis; for item 4's
  JSON these become `severity: "low"` issues.

### 12. "Not auditable from the manuscript" disclosure (impact: med-high, effort: S)

- **Change.** Both contracts: after the numbered comments, an optional bounded
  section `**Not auditable from the manuscript:**` (≤3 bullets) naming central
  claims that cannot be verified without data, code, or absent supplements.
  Synthesis aggregates these into a closing `## Review coverage and
  limitations` section that also records extraction-quality caveats from the
  survey.
- **Why.** Today the contracts instruct specialists to *omit* anything they
  can't evidence — which silently hides the paper's unauditable core when the
  data isn't public. An auditing tool must report the boundary of the audit.
  This is a disclosure, not praise, so it doesn't violate the no-praise rule.
- **How.** Contract + synthesis edits; keep it out of the comment caps; tests
  for the new required strings.

### 13. Citation-attribution audit inside `auto_contribution` (impact: medium, effort: S)

- **Change.** Extend `prompts/auto_review/core/contribution.md` with an
  attribution-audit paragraph: for the 3–5 references that carry the
  positioning ("X showed...", "unlike Y..."), verify via WebSearch that the
  cited paper actually claims what is attributed (misattribution, wrong year,
  superseded working-paper versions with materially different results); flag
  cited-but-missing / listed-but-uncited entries when a bibliography is
  present. Require DOI/stable URL, already contract policy.
- **Why.** Misattributed citations are among the most common referee catches
  and currently nobody owns them; novelty checking alone doesn't surface them.
- **How.** Prompt-only. If eval shows cap pressure (contribution's 6 slots fill
  up), split a dedicated `auto_citations` step with WebSearch as a follow-up.

### 14. Extraction-quality preflight gate (impact: medium, effort: S)

- **Change.** After orientation validates, if `extraction_quality_notes`
  reports garbled equations/tables **and** the plan selected math/number-heavy
  roles (`formal_proofs`, `structural_estimation`, any causal role, or
  `auto_numbers` will run), emit a console warning + run quality note
  recommending re-running with LLM or Full Parser extraction. Never block.
- **Why.** A proof audit over garbled math wastes the run's most expensive
  step and produces confident nonsense; today the notes ride along silently.
- **How.** Small check in the orchestration site right after
  `materialize_config()`; emit through `EventBus`; unit test with seeded notes.

### 15. Deterministic cross-reference checker (impact: medium, effort: M)

- **Change.** Rust pass over `context/document.md`: extract
  `Table/Figure/Section/Appendix/Theorem/Proposition/Lemma/Equation N`
  mentions, verify each target exists in the DocumentBundle nodes / orientation
  inventory, and write `artifacts/reference-check.json` (dangling references,
  duplicated numbering, tables never discussed in text). Stage it into
  `auto_consistency` and `auto_verify` views with a one-paragraph prompt note.
- **Why.** Grounds the consistency pass in an exhaustive machine sweep (LLMs
  sample; regex doesn't) and frees its comment budget for semantic conflicts.
- **How.** New module beside `quote_check.rs` (item 2 builds the staging seam);
  conservative patterns per numbering style (`3`, `A.2`, `IV`); tests on
  fixture text. Advisory-only: the LLM confirms before reporting.

### 16. Optional `focus` locations per selection note (impact: medium, effort: M)

- **Change.** `selection_notes` entries gain an optional `sections` array
  (section numbers/titles from the survey's own `sections`). Materialization
  forwards them into item 7's routing-context block: "Your material is
  concentrated in: §4, Appendix B."
- **Why.** On long papers, specialist attention is the binding constraint;
  the router already knows where each role's evidence lives at selection time.
- **How.** Schema (optional, backward compatible; part of the v3 bump with
  item 8), orientation prompt, `validate_review_plan_object` tolerates absence,
  materialization formatting, tests.

---

## P2 — Depth upgrades to specific prompts

Each of these is a focused edit to one or a few prompt files; validate against
item 6's fixtures. All keep the existing contracts and ceilings unless stated.

### 17. `methods/formal_proofs.md`: verification-ledger protocol (impact: medium, effort: S)

Require an internal per-result ledger (statement → verified / gap found /
unchecked, with the failing step for gaps), explicit checking of every invoked
external theorem's hypotheses, boundary and degenerate cases, quantifier order,
and measurability/regularity conditions at limit operations. Results left
unchecked from context limits go in the item-12 disclosure list rather than
being silently skipped.

### 18. Causal family: estimand-first protocol (impact: medium, effort: S)

Prepend one shared paragraph to `causal_identification.md`,
`difference_in_differences.md`, `instrumental_variables.md`,
`regression_discontinuity.md`, `matching_weighting.md`,
`synthetic_control.md`: (1) state the estimand and for whom; (2) state the
identifying assumption in counterfactual terms; (3) name the single most
plausible violation for *this* setting; (4) check the paper's own diagnostics
before demanding new ones. Comparable, concrete causal reviews across designs.

### 19. `methods/structural_estimation.md`: identification diagnostics (impact: medium, effort: S)

Add: which moments/variation move which parameters (sensitivity-style
reasoning); whether counterfactuals are driven by estimated parameters or by
functional-form/calibration choices; in-sample vs. out-of-sample fit; whether
reported uncertainty propagates first-stage and calibration error into
counterfactuals.

### 20. `methods/randomized_experiment.md` + `methods/clinical_study.md`: registration and attrition (impact: medium, effort: S)

Add pre-registration/analysis-plan adherence checks (compare reported outcomes
and subgroups against the registry/PAP when the paper cites one; flag outcome
switching), differential-attrition arithmetic (rates by arm, bounds when
material), multiple-outcome multiplicity, and randomization-inference
appropriateness for small/clustered designs.

### 21. `subjects/economics.md`: magnitude plausibility (impact: medium, effort: S)

Extend the lens with numeric plausibility duties: compare headline
elasticities, multipliers, discount rates, and welfare numbers against
literature ranges; translate coefficients into economic units and check the
translation; recompute one headline counterfactual back-of-envelope when the
paper provides the inputs. (Primary audience is economists; this lens earns
extra depth.)

### 22. `methods/measurement_data.md`: units and construction audit (impact: medium, effort: S)

Add: unit/scaling consistency (per-capita vs. total, nominal vs. real, log
points vs. percent); index construction (base year, weights, rebasing);
winsorization/trimming/imputation disclosure and sensitivity; variable
definitions that drift between text, notes, and appendix.

### 23. Sharpen `core/exposition.md`; cap 6 → 5 (impact: low-med, effort: S)

Add one aggregate exhibit-self-containment sweep (every main table/figure:
variables defined, units, sample stated, SE convention in notes — reported as
*one* comment when systemic), and reduce the ceiling to 5. Exposition findings
are the least valuable per token in an auditing tool; the freed budget flows to
verification-heavy steps.

### 24. Let synthesis use agreement privately (impact: low-med, effort: S)

`synthesis.md` currently bans mentioning agreement counts, which also
discourages *using* them. Add: "When independent reports identify the same
defect from different evidence, treat that as corroboration when ordering;
never mention the process or counts in the output."

### 25. New method role: `external_validity` (impact: low-med, effort: S)

Transportability of estimates: site/sample selection into the study, general
equilibrium and scale-up effects, compliance and dosage differences, Hawthorne
and piloting artifacts, and whether policy conclusions respect the estimated
population. No current role owns this referee staple. Add the prompt, a
`methods.rs` entry with a tight routing exclusion ("do not select merely
because the paper generalizes in its conclusion section"), catalog-count test
update (62 → 63), and a `docs/auto-review-catalog.md` line.

### 26. Slim the orientation inventory (impact: low-med, effort: S)

The survey is part of every step's (cached) prefix, so its size taxes the whole
run. In `orientation.md`: bound `notation` to symbols used in central results
(not "every explicitly defined symbol"), bound `sections` to depth 2, and cap
`tables_figures` summaries to one sentence each. Keep `formal_results`
exhaustive — specialists depend on it.

### 27. Orientation sanity validators (impact: low-med, effort: S)

Rust-side advisory checks after schema validation: section page ranges
monotonic and within `page_count`; `formal_results`/`tables_figures` pages
inside the document; sections covering < 40% of pages triggers a
survey-quality note. Record as run quality notes (never hard failures — the
survey is model output and the paper may genuinely be odd). Small function
beside `validate_review_plan`, unit tests.

---

## P3 — Features and UX

### 28. Effort escalation for reasoning-heavy roles (impact: medium, effort: M)

Add a `reasoning: heavy` flag to `MethodSpec` for `formal_proofs`,
`structural_estimation`, `causal_identification`, `bayesian_inference`,
`optimization_control`; materialization sets those steps' effort to the high
tier via `effort_overrides` when the resolved provider/transport supports
effort control, otherwise leaves defaults. Provenance already records effective
effort. Disclose in RunPreview.

### 29. Optional second-opinion on the primary method role (impact: medium, effort: M)

Profile setting (schema-embedded like `x-pipeline-adaptive-agent-count`):
run the first-listed method specialist with two agents; the existing
multi-agent merge machinery produces the combined view, and disagreement is
itself signal for `auto_verify`. Off by default; RunPreview already surfaces
the extra provider calls.

### 30. "Why these reviewers" card in the report workspace (impact: medium, effort: M)

Render `review_plan` as a first-class card in ReportWorkspace's Provenance tab:
specialist labels (via `auto_review::catalog()`), the reason for each, and
`routing_uncertainty`; link each role into `AutoReviewCatalogDialog`.
Frontend-only (`gui/src/components/ReportWorkspace.tsx` + a small presenter);
falls back gracefully for non-Auto runs and legacy plans.

### 31. Optional plan-confirmation gate (impact: medium, effort: L)

Setting "Confirm reviewer plan before continuing" (default off): after
orientation validates, emit the plan and await approve/cancel (v1: no editing)
before materialization, with a timeout that proceeds automatically so headless
and batch runs never hang. Wire through `EventBus` + a small command;
batch/CLI ignore the gate. Editing the plan in the dialog (add/remove within
allowlist + bounds, re-validated in Rust) is a v2 follow-up.

### 32. Revision mode: prior-issue ledger injection (impact: medium, effort: L)

When the selected input matches a Project run lineage (stable input path, as
the Projects page already tracks), offer "Review as revision": stage the
project ledger's accepted issues as a context artifact for `auto_synthesis` /
`auto_verify` with instructions to mark each prior issue resolved / partially
addressed / unresolved in a dedicated report section, and not to re-raise
issues the user rejected. Uses the existing ledger (`projects/ledger.rs`) and
the item-2 staging seam.

### 33. Per-specialist calibration (impact: low-med, effort: S)

Add `source_step` to each issue in item 4's JSON (the step id whose finding
survived). Extend `draft_calibration` to group rejected issues by
`source_step`, so the drafted addendum can say "the DiD reviewer over-flags
pretrend noise" instead of only patching the synthesis prompt. Depends on
item 4.

### 34. Referee-letter export (impact: low-med, effort: M)

New export in `ExportControls`: render **accepted** issues (annotations) into a
formal referee-letter markdown — scope-of-review paragraph (from item 12's
coverage section), major comments, minor comments, each with anchors —
clipboard/file. Pure frontend over existing annotations + issues data. This is
the artifact an economist actually pastes into their review.

### 35. Deterministic metadata fill (impact: low, effort: S)

After orientation validates, overwrite `metadata.page_count` (and
`has_appendix` when detectable from bundle nodes) from the DocumentBundle
instead of trusting model output, and stop asking the model for `page_count`
in the template. Downstream prompts and item 27's validators then rely on true
values. Small change where orientation is accepted; tests.

### 36. Raise the adaptive-agent maximum to 8 (impact: low, effort: S)

For genuinely sprawling papers (structural + experiment + ML + software), 4
method slots can bind. Raise `MAX_ADAPTIVE_AGENTS` to 8 and the method-ID
schema `maxItems` to 6 while keeping the router's "smallest covering set"
default and the exact-count setting's range; verify run-budget and step-count
tests (max run becomes 12 steps + verify — still far under limits). Do last:
only worthwhile after items 1–10 make each added specialist trustworthy.

---

## Considered and rejected

- **Deleting `auto_exposition`.** Its remit (evaluation-blocking presentation
  failures) is distinct and cheap; item 23 shrinks rather than removes it.
- **Merging orientation and routing into two separate calls.** One call keeps
  the survey/rerun contract simple; items 26–27 address the quality risk at
  lower cost.
- **Deleting `prompts/auto_review/fields/` + `orientation_v1.md`.** Still load-
  bearing for v1 rerun/migration via `auto_review/legacy.rs`.
- **A Rust table parser recomputing statistics natively.** Extraction noise
  makes deterministic table parsing brittle; item 3 (LLM recomputation) plus
  item 2 (quote grounding) gets most of the value at a fraction of the cost.

## Suggested execution order

1. Item 6 (eval harness) first — it measures everything else.
2. Items 1, 2, 5, 9 (verification chain + anchors + severity/confidence).
3. Items 3, 10, 11, 12 (numbers audit + report shape).
4. Item 4, then 33 (structured issues, calibration).
5. Items 7, 8, 13–16 (router + orientation contract v3, one migration).
6. P2 prompt depth as eval results direct; P3 features last.
