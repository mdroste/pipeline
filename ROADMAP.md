# Historical Pipeline implementation roadmap

> The 1.1–2.0 numbers in this document are historical development milestones,
> not published package versions. The current package version is recorded in
> `gui/package.json`; see [CHANGELOG.md](CHANGELOG.md) and GitHub Releases for
> what actually shipped.

This plan guided Pipeline's expansion from a referee-report generator with
configurable steps into a general-purpose desktop harness for multi-pass LLM
document work. Size labels are historical estimates (S = days, M = a week or
two, L = several weeks), not current commitments.

---

## Current development-tree status

The deficiencies described in the original baseline are no longer a statement
of current behavior. The tree now has durable run history and logs, usage and
timing records, dependency scheduling, conditions, variables, named inputs,
structured outputs, fan-out, batch operation, resumable re-runs, a
headless CLI, report search, issue annotations, and run comparison.

The app currently ships five built-in profiles across papers, code,
replication, and grants. The engine also supports custom document, folder, and
no-input workflows. Remaining or partial work is identified explicitly below;
an “implemented” milestone means code exists in the development tree, not that
a correspondingly numbered public release was published.

---

## Milestone overview

| Milestone | Theme | Development status |
|---|---|---|
| **1.1** | Runs are first-class | Implemented |
| **1.2** | The generic engine | Implemented |
| **1.3** | Throughput | Implemented |
| **1.4** | Reading and comparing | Implemented |
| **1.5** | Polish and trust | Partial; the remaining ideas are listed below |
| **2.0** | Platform foundations | Schema v2, fan-out, and URL import implemented; a curated template gallery remains external work |

The detailed scopes below preserve the original proposal tense and rationale.
They may describe a pre-implementation “today”; the development-status notes
and the current-tree summary above are authoritative.

---

## 1.1 — Runs are first-class ✅ *implemented*

The single highest-leverage release. Everything needed already exists on disk (`~/.pipeline/runs/{id}/` with `manifest.json`) or in the event stream (`pipeline:usage`, `pipeline:pass`); this release stops throwing that information away.

> **Development status:** implemented. Backend: per-call/run token accumulators
> and a run-log sink in `logging.rs`; `StepOutput` carries
> duration/tokens/model/provider; `output.rs` renders a per-step run-summary
> table; `RunManifest` records status/duration/usage/tags; and `runs.rs`
> provides history, metadata, deletion, disk-usage, and retention operations.
> Frontend history, console filtering/export, elapsed clocks, and retention
> controls are present. Current test results belong in CI, not this roadmap.

### 1.1.1 Run history page (L)

A new top-level page (alongside main / pipeline / settings) listing past runs from `~/.pipeline/runs/`, newest first. Each row: input name, profile, provider, date, duration, token totals, status (done / failed steps / cancelled). Click to reopen the run in the existing `ArtifactExplorer` — the `get_run_manifest` + `read_artifact` plumbing already supports this; today it is only wired to the just-finished run.

- Backend: a `list_runs()` command in `runs.rs` that scans run dirs and returns manifest summaries. Extend `RunManifest` with `status`, `duration_secs`, `usage`, and an optional user-assigned `title` and `tags` (all `#[serde(default)]` so old manifests still load).
- Frontend: `HistoryPage.tsx` with text filter over input name / profile / tags; rename and tag actions (`update_run_meta` command); delete run (confirm dialog, moves to trash rather than `rm -rf` where the OS supports it).
- Retention setting: "keep last N runs / X GB" with a size display and a manual purge button. Runs are big (page PNGs at 150 dpi); people will hit disk pressure before they notice why.
- The legacy `~/.pipeline/history/*.json` reports remain readable but the history page treats `runs/` as the source of truth, matching the architecture note in CLAUDE.md.

### 1.1.2 Per-step timing, tokens, and cost in the report (M)

`StepOutput` gains optional fields: `duration_secs`, `input_tokens`, `output_tokens`, `model`, `provider` (all `#[serde(default)]` — old reports keep loading; mirror in `lib/types.ts`). The executor already knows the wall-clock span of each call and `logging::emit_usage` already has the token counts; route a copy into the output instead of only emitting the event.

- A "Run summary" table at the bottom of the rendered report (`output.rs`): step × agent × duration × tokens × estimated cost.
- Cost estimation: a small static price table per known model ID (same place the model aliases live, `api_anthropic.rs` etc.), with a user-editable override map in settings for models we don't know. CLI-subscription runs show tokens without a dollar figure — printing a price for a flat-rate plan would be wrong. Label estimates clearly as estimates.
- `PipelineProgress.tsx` shows elapsed time per pass and for the whole run (the frontend already receives `pipeline:pass` running/done transitions; it just needs to timestamp them).

### 1.1.3 Persisted logs (M)

Write every `pipeline:log` line to `runs/{id}/logs/run.log` (plain text, one line per entry, with timestamp, session id, label, level). Tap the stream at the emit site in `logging.rs` so all providers are covered without touching each wrapper.

- Register the log file in the manifest (group "context") so it is browsable from the artifact explorer and survives with the run.
- "Save console to file…" button in the console header for the live view.
- Since logs may quote source text, note in the help page that run dirs contain the document — relevant for people reviewing confidential drafts.

### 1.1.4 Console upgrades (M)

The console (`App.tsx:502` region + `usePipeline.ts`) gets:

- Full-text search box with match count and next/prev, operating on the visible (session-filtered) set.
- Level filter chips: all / warnings / errors. The level field already exists on every line.
- Timestamps (added at emit time in 1.1.3, displayed on hover or via a toggle to keep lines compact).
- Per-line copy on hover; "copy from here down" for sharing an error tail.
- A "jump to first error" affordance when a run fails — today the user scrolls through up to 10,000 lines.
- Auto-scroll pause when the user scrolls up (currently new lines always yank the view down), with a "resume following" pill.

---

## 1.2 — The generic engine ✅ *implemented*

This is the generalization release. The goal: a profile should be able to describe most linear-ish document workflows — not just "N parallel readers then a synthesizer" — without the engine learning anything about papers. Paper-ness stays in profile data, as now.

> **Development status:** implemented. The wave loop is a dependency scheduler
> (`resolve_dependencies` plus ready-set execution in `executor.rs`);
> `StepConfig` includes artifact inputs, conditions, and output schemas;
> profiles include variables and named inputs; and validation rejects invalid
> dependency graphs. The editor exposes these controls and skipped steps have
> a distinct progress state. DAG visualization and richer prompt-token linting
> remain possible follow-ups.

### 1.2.1 Step dependencies (L)

Add `inputs: Vec<String>` to `StepConfig` (default empty). Semantics:

- Empty `inputs` — exactly today's behavior: parallel steps join the adjacent wave; sequential steps wait for everything prior. Every existing profile runs unchanged.
- Non-empty `inputs` — the step waits for exactly those steps and its `{prior_outputs}` / `{step:<id>}` placeholders resolve against them.

The executor's wave grouping (`executor.rs:209`) becomes a topological schedule: compute ready-sets from the dependency graph, run everything ready in parallel under the existing `max_workers` semaphore. Adjacency-derived waves are just the degenerate case, so this is a refactor of wave computation rather than a second engine. Reject cycles at save time in `pipeline_config.rs` validation (there is already ID-uniqueness validation to hang this on).

- `WaveDiagram.tsx` upgrades from a linear wave strip to a small DAG rendering. Keep it read-only and simple — boxes and arrows, no interactive graph editor. The step list stays the editing surface.
- `{step:<id>}` already exists for sequential prompts (`executor.rs:593`); with dependencies it becomes available to any step, and lint in `pipelineHelpers.ts` should flag references to steps not in `inputs`.

### 1.2.2 Conditional steps (M)

Add an optional `run_if` to `StepConfig`, deliberately minimal — two deterministic checks evaluated in Rust, not by an LLM:

- `{ "output_matches": { "step": "<id>", "pattern": "<regex>" } }` — run only if a prior step's text matches.
- `{ "survey_path": "<json-pointer>", "equals"/"exists": … }` — run only against the orientation JSON (e.g. only run the empirical step if `/paper_type` is `"empirical"`).

Skipped steps record a `StepOutput`-adjacent skip entry so downstream `{step:<id>}` resolves to an explicit "(skipped: condition not met)" rather than silently missing, and the progress view shows "skipped" as a distinct pass state.

This unlocks the most-requested shape of flexibility ("only run X when the survey says Y") without turning profiles into programs. Anything fancier belongs in prompts.

### 1.2.3 Run-time variables (M)

Profiles gain `variables: Vec<VarSpec>` — `{ key, label, kind: text|choice|file, default, choices? }`. Prompts reference them as `{var:key}`. When a profile has variables, the Run button opens a small form (pre-filled with defaults) before dispatch; values are recorded in the run manifest.

This turns profiles into reusable templates: "Review for journal: {var:journal}", "Rubric: {var:rubric_file}", "Reviewer persona: {var:persona}". Substitution happens in the executor next to the existing placeholder pass; `PromptEditor.tsx` chips learn the new tokens; lint flags unknown keys.

### 1.2.4 Multiple named inputs (L)

Today a run has exactly one input path. Add optional `extra_inputs: Vec<InputSlot>` to `ExtractionConfig` — `{ key, label, mode: document|folder, required }`. The main input keeps its current role and placeholders; extra inputs are extracted through the same cascade (`extract.rs`), cached by their own hash, and exposed as `{input:key}` path placeholders.

This is what several obvious workflows have been blocked on:

- **Revision response check**: paper + response letter + prior report → "verify each claimed change was actually made."
- **Code + paper**: the replication audit currently gets only the folder; giving it the paper PDF as a named input makes code–paper consistency checks first-class rather than prompt-hacked.
- **Rubric-based review**: document + rubric file.

UI: `PaperSelector.tsx` grows a slot list when the active profile declares extra inputs. Also fix the baseline UX while in there: drag-and-drop onto the selector and a recent-inputs list (S each).

### 1.2.5 Structured step outputs (M)

Optional `output_schema` (JSON Schema, stored as raw `Value`) on `StepConfig`. When set, the step's report file must parse as JSON and validate; on failure, retry with the validation error appended — exactly the loop `orient.rs` already runs for the survey, extracted into a shared helper.

Structured outputs are what make 1.2.2 conditionals and 1.4.2's issue table reliable, and they make merge/synthesis prompts dramatically more consistent ("here are three JSON arrays of issues" instead of three essays). Steps without a schema behave exactly as today.

---

## 1.3 — Throughput ✅ *implemented*

> **Development status:** implemented.
>
> **Batch queue:** `start_batch`/`get_batch_status`/`cancel_batch`/`list_input_files` commands run a list of inputs sequentially through the pipeline under the shared run guard, publishing `batch:progress`/`batch:done` events; `BatchPanel.tsx` (nav entry) stages files/folders, shows live per-job status, and links each finished job to its run in History.
>
> **Resume / partial re-run:** every run writes a structured `report.json`; `RunManifest` gained `parent_run_id`; the executor accepts a `preloaded` map (a preloaded step reuses the parent output instead of executing) and exposes `dependents_of`; a `rerun_run` command reuses the parent's cached extraction + orientation — "Re-run failed" and "Re-run" buttons on the History page, using the *active* profile so editing a prompt and re-running is cheap.
>
> **Headless CLI:** the pipeline's `AppHandle` dependency was abstracted behind
> an `emit::Events` trait / `EventBus`. A second binary, `pipeline-cli` (`run` /
> `batch` / `profiles`), reuses the engine without a window or WebView.
>
> **Per-pass cancel:** a `PASS_KEY` task-local lets `register_child_pid` attribute each subprocess to its pass; `cancel_pass` kills just that pass's processes and marks it so it won't retry (other passes continue). A ✕ button appears on each running pass in the progress view.
>
> **Resume boundary:** re-run requires a sufficiently recent run with the
> structured report and captured inputs needed by the active profile. Older
> run formats remain readable but may not be eligible for re-execution.

### 1.3.1 Batch queue (L)

Replace the boolean `PIPELINE_RUNNING` guard in `commands.rs` with a run queue: an ordered list of (input, profile, variables) jobs executed strictly one at a time (parallelism stays *inside* a run, where `max_workers` governs it — two concurrent runs would double-dip subscription rate limits and confuse the console).

- UI: "Add to queue" next to Run; a queue panel with per-job status, reorder, remove; "queue this folder" — select a directory of PDFs, one job each.
- Each job produces a normal run in history (1.1); a batch summary lists jobs with status, duration, tokens, and a link to each report.
- Failures don't stop the queue; the summary flags them.

For a referee with five papers, or an instructor with thirty submissions and a rubric profile (1.2.3 + 1.2.4), this is the difference between a tool and a workflow.

### 1.3.2 Resume and partial re-run (L)

Every step already writes its output under `runs/{id}/artifacts/steps/`. Record enough in the manifest to treat those as checkpoints:

- "Re-run failed steps" on a run with failures: completed step outputs load from disk; only failed ones execute; the synthesis re-runs downstream of anything re-executed.
- "Re-run from step N" for prompt iteration: tweak the consolidation prompt, re-run just the sequential tail against cached parallel outputs. This makes prompt engineering roughly free instead of costing a full pipeline of tokens each iteration — probably the single biggest quality-of-life win for anyone customizing profiles.
- Implementation: executor accepts a set of pre-supplied `StepOutput`s and skips execution for those IDs (the dependency graph from 1.2.1 says what is downstream and must re-run). Re-runs write a new run dir that references the parent run id in its manifest, keeping runs immutable.

### 1.3.3 Headless CLI (M)

A second binary target in the existing crate (`src-tauri` already holds all the logic; the Tauri `AppHandle` is only used for event emission, which becomes a stdout/JSON-lines sink behind a small trait):

```
pipeline run --profile deep-review --input paper.pdf --var journal="AER" --out report.md
pipeline batch --profile grading --input-dir ./submissions/
pipeline profiles list
```

This enables cron jobs, CI hooks (run the code-review profile on a repo), and scripting — and it is the cheap path to "applied to many tasks," because tasks that don't fit a GUI (nightly re-review of a working draft) fit a one-line cron entry. Exit codes reflect failed steps.

### 1.3.4 Smaller items

- **Per-pass cancel** (M): kill one step's child PID (already tracked per call) and mark that pass failed without aborting the wave. Global cancel stays.
---

## 1.4 — Reading and comparing ✅ *implemented*

The report side has had less love than the run side. This release is about what happens *after* the tokens are spent.

> **Development status:** implemented.
>
> **In-report search:** a reusable `useFindBar` hook (testable `applyHighlights`/`clearHighlights` DOM helpers) drives a Cmd/Ctrl-F find bar in `ReportViewer` with highlight-all, next/prev, and a match counter.
>
> **Structured issue table:** `lib/issues.ts` (`parseIssues`/`extractJson`/`detectReportIssues`, mirroring the Rust JSON extraction) detects when a step emitted an issues-shaped JSON list; a Report/Issues toggle in the report view renders `IssuesTable` — sortable by severity, severity-filterable, rows expand to the body. A compiled-in `editor_synthesis_issues` prompt and a one-click "Use issues schema" preset in the step editor make it easy to enable; profiles without it are unaffected.
>
> **Annotations:** each issue row carries accept/reject/done + a note, debounce-saved to `runs/{id}/annotations.json` via `get_annotations`/`save_annotations` (never touching the report artifact), with an "Export accepted" action that writes the accepted subset to markdown.
>
> **Run-vs-run comparison:** `get_run_report` loads a run's structured `report.json`; a Compare mode in the History page picks two runs and opens `ComparePage`, which shows a per-step line diff (tested `lib/diff.ts` LCS) and an on-demand `reconcile_runs` LLM pass (generalized `reconcile.rs`, older run auto-detected as "prior").
>
> **Learn from my judgments:** `draft_calibration` aggregates the issues
> rejected across a profile's annotated runs and has the LLM draft a short
> addendum for the synthesis step; the editor shows it for approval before
> applying it.
>
> **Follow-up:** annotations currently attach to structured issues (the Issues view); attaching them to the prose report's numbered `#N.` cards is a small future addition.

### 1.4.1 In-report search (S)

A find bar in `ReportViewer.tsx` (Cmd/Ctrl-F, highlight all, next/prev). Also applies inside the artifact explorer's text/code viewers. Long reports are unnavigable without this; it is embarrassing to fix this after shipping KaTeX rendering, so fix it first in the release.

### 1.4.2 Structured issue table (M)

When the synthesis step uses an `output_schema` (1.2.5) with the shape `issues: [{ id, title, severity, section, body }]`, the report page adds an "Issues" view: a sortable, severity-filterable table alongside the prose report, each row expanding to the full comment and deep-linking into the rendered report. Ship an updated `editor_synthesis` prompt + schema as the default for the built-in paper profiles so it works out of the box; profiles without the schema just don't get the table.

### 1.4.3 Run-vs-run comparison (L)

Pick any two runs of the same input (or a parent/re-run pair from 1.3.2) in the history page:

- **Deterministic layer**: side-by-side per-step artifact diff (text diff of `steps/*.md`) in the artifact explorer.
- **LLM layer**: the existing `reconcile.rs` prompt (addressed / remaining / new) generalized to compare any two reports, not just latest-by-hash. With structured issues (1.4.2), reconciliation gets issue IDs to anchor on instead of prose similarity, which is currently its weakest point.

### 1.4.4 Annotations and the feedback loop (M)

Per-issue status on any report: accept / reject / done, plus a free-text note, stored as `runs/{id}/annotations.json` (never mutating the report artifact).

- Export respects annotations: "export accepted issues only" produces the letter-ready subset.
- The creative part: **"Learn from my judgments."** An opt-in button on the profile aggregates rejected issues across annotated runs and drafts a calibration addendum for the relevant step prompts ("this reviewer does not consider X worth flagging; do not report issues of type Y"). Shown as a diff to the prompt, applied only on user approval. This closes the loop that makes a review tool feel like *your* reviewer rather than a generic one, and it needs nothing but data the annotations already capture.

---

## 1.5 — Polish and trust

Smaller items, batched. Roughly in order of value:

1. **Provider "Test" buttons** (S) — one cheap call per configured provider from Settings; reports round-trip time and the resolved model. Kills the "is my key/CLI even working" class of support question.
2. **Model pickers** (S, implemented) — populate a dropdown from each provider's models endpoint where one exists (Anthropic/OpenAI/Google/Ollama `/models`), with free-text still allowed. Also addresses the CLAUDE.md constraint that hardcoded model IDs age: surface what the account actually has.
3. **Fallback provider order** (M) — optional ordered list; when a step exhausts retries on provider A, try provider B. Off by default; recorded per-step in the run summary (1.1.2) so it is never silent.
4. **Keyboard shortcuts** (S) — Run, Cancel, Save profile, console toggle, find. Displayed in a `?` cheat-sheet overlay.
5. **Step list quality-of-life** (S each) — per-row enable/disable toggle (the field exists; the toggle doesn't), duplicate step, "reset step to built-in default", visible enabled/disabled state.
6. **Prompt editor: diff against default** (S) — user overrides in `~/.pipeline/prompts/` and profile-level prompt edits show a diff vs. the compiled-in default, so upgrades that improve stock prompts are visible instead of silently shadowed.
7. **Settings export includes a "reset all" and a redacted diagnostics bundle** (S) — settings + profiles + deps report + recent log tail, keys stripped, for bug reports.

---

## 2.0 — Platform foundations *(core implemented)*

2.0 is less about new machinery than about declaring the generalized model stable and building the library on top of it.

> **Development status:** schema v2, fan-out, generalized validation, and URL
> import are implemented. The curated gallery itself has not been created.
>
> **Fan-out (map) steps:** `StepConfig.for_each { glob, max }` runs a step once per file matching a glob under the input, binding `{item}` in the prompt; outputs are keyed `step_id/item` and combine through the existing merge/`{step:id}` machinery. A dependency-free glob matcher (`pipeline/glob.rs`, tested: `*`, `?`, `**/`) resolves against the input folder, capped at `max`. Editor field in the step's advanced options.
>
> **Profile schema v2:** `CURRENT_SCHEMA_VERSION = 2`; exports carry `schema_version`, and import rejects profiles from a newer app version. v1/unversioned profiles still load (every field is `#[serde(default)]`).
>
> **Generalized-profile validation:** Revision-response, rubric-grading, and thesis-review workflows originally exercised extra inputs, variables, structured output, and fan-out. All three were later retired from the default catalog while the underlying engine capabilities remain available to custom profiles.
>
> **Template sharing:** `import_profile_from_url` fetches a shared profile JSON over HTTPS (size-capped, schema-checked, redirect- and private-network-restricted) and imports it; a "From URL…" button in the profile controls. The *curated gallery repo* itself is external content (a GitHub repo of template JSONs) not created here — import-from-URL is the mechanism it would use.

### 2.0.1 Profile schema v2 (M)

Add an explicit `schema_version` to profile JSON; v2 is the 1.2 feature set (dependencies, conditions, variables, input slots, output schemas) documented as a stable, public format. v1 profiles migrate on load, as the referee/post-step format does today. This is what makes sharing safe.

### 2.0.2 Template gallery (M)

"New from template" backed by a curated repo of profile JSONs (fetched like `updates.rs` fetches releases — plain GitHub, no accounts, no telemetry). Import-from-URL for sharing profiles in a lab or a syllabus. The existing envelope import already validates and de-duplicates IDs; the gallery is mostly UI plus a signed-manifest check.

### 2.0.3 Fan-out (map) steps (L)

The last big engine feature: `for_each: { glob: "chapters/*.tex", max: 20 }` on a step runs its prompt once per matching file in a folder input, as one wave under the semaphore, producing `StepOutput`s keyed `step_id/item`. The merge machinery (which already combines multi-agent outputs under composite keys) combines per-item outputs the same way. Guard rails: hard cap on fan-out size, mandatory pre-run estimate (1.5.4) showing the multiplied cost.

This makes whole categories of work natural: per-chapter thesis review, per-exhibit replication checks, per-module code audit, grading a folder where each file is a submission *within one run* (complementing the batch queue, which is one run per file).

### 2.0.4 New built-in profiles

Each exercises the generalized engine and ships as data, not code:

- **Revision response check** — inputs: revised paper + response letter + prior report (1.2.4); verifies each claimed change; structured verdict table (1.2.5).
- **Rubric grading** — document + rubric file + variables (course, weight scheme); batch- and fan-out-friendly.
- **Literature positioning scan** — WebSearch-heavy profile: given an abstract or draft, map claimed novelty against recent work.
- **Thesis / long-document review** — fan-out per chapter (2.0.3), then a cross-chapter consistency synthesis.
- **Dataset documentation audit** — folder input; checks codebooks against data files, flags undocumented variables (structured output).

---

## Cross-cutting engineering (ongoing, every release)

- **Type sync**: `lib/types.ts` mirrors the serde models by hand; the surface grows every release above. Adopt `ts-rs` (or similar) to generate the TS types from the Rust structs in a build step, and delete the hand-maintained mirror. Do this in 1.1 *before* the model changes start landing. (S)
- **Serde discipline**: every new field on persisted types (`RunManifest`, `StepOutput`, `StepConfig`, `Settings`) is `#[serde(default)]` and round-trips old files; add a backend test that loads fixture files from each prior release.
- **Executor tests**: 1.2's scheduler and 1.3's resume logic are exactly the kind of deterministic Rust this project keeps out of LLM hands — cover them with real unit tests (dependency cycles, skip propagation, checkpoint reuse), not manual runs.
- **Event schema**: the five `pipeline:*` events become load-bearing for history, logs, and the CLI sink. Freeze their shapes in one Rust module with types, emitted through the trait introduced for the CLI (1.3.3).
- **Docs**: the Help page and README gain a "profile cookbook" as generalization features land; CLAUDE.md's step-model section is the source of truth and must track 1.2 exactly.

## Non-goals

Stated to keep the scope honest:

- **No Pipeline cloud service, accounts, or telemetry.** Local artifacts stay
  under `~/.pipeline/`, while remote-provider runs transmit the explicitly
  allowed inputs described in `PRIVACY.md`.
- **No auto-updater.** Public Windows releases are Authenticode-signed and
  timestamped by the protected release workflow.
- **No embedded scripting language for profiles.** Conditions stay declarative (regex / JSON-pointer); anything smarter belongs in prompts or in the CLI + a real script.
- **No general agent loops.** Steps remain single isolated LLM calls with bounded tools. The pipeline's value is that its structure is deterministic and inspectable; that is the line.

## Sequencing rationale

1.1 first because it is pure win with no model risk and every later release depends on runs being durable (batch summaries, comparisons, annotations all live on runs). 1.2 second because dependencies/variables/inputs/schemas are the enabling layer for nearly everything in 1.3–2.0, and shipping it early gives the new model time to harden before it is declared stable in 2.0. Throughput (1.3) before reading (1.4) because batch + resume changes how much people run, which multiplies the value of the comparison and annotation tools that follow. 2.0 is deliberately thin on engine work: stabilize, document, and ship the library.
