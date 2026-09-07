# Pipeline improvement plan

**Status:** PI-00–PI-11 and the Workflow-authoring half of PI-12 implemented at the user's subsequent requests, with local qualification and remaining release gates below. Concurrent Workspace turns (the other half of PI-12) remain proposed and evidence-dependent; this document itself does not authorize them.  
**Prepared:** September 6, 2026.  
**Scope:** improve Pipeline as an academic research suite, building on the current Workspace and Workflow implementations.

## Implementation update — PI-00–PI-03

The user subsequently requested PI-00, PI-01, PI-02, and PI-03. The implementation
and operational limits are documented in [the project surface guide](docs/workbench/project-surface.md).

| Package | Implemented | Qualification status |
|---|---|---|
| PI-00 | Host-scope review/grants, launch and input identity checks, honest snapshot/output/baseline semantics, Stata cooperative stop, capability inventory, real fixtures and evidence record | Local Git/LaTeX/Python/Stata, native GUI, no-model protocol probe and deterministic checks observed; authenticated, forced-cleanup, packaged and other-platform gates remain |
| PI-01 | Project home, explicit accepted manuscript/baseline, curated notes/history/context, file refresh/ignores/relocation indicators, research tasks | Deterministic and macOS development GUI checks |
| PI-02 | Lazy immutable reader, source/math/PDF-region actions, search/navigation/persisted pin and scroll, conservative anchors, shared read tools, text/table/image inspection | Source/math/selection observed in native GUI; PDF/accessibility breadth and authenticated tool use still require qualification |
| PI-03 | Selected dirty/untracked-file checkpoints, local copies and Git worktrees, source/artifact review, selected-file acceptance, conflict detection, durable recovery and undo | Deterministic interruption/conflict tests and live macOS Git/GUI checks; Windows acceptance disabled, other platforms and packaged abrupt-crash runs pending |

See [the release qualification record](docs/workbench/release-qualification.md)
for evidence and open gates.

## Implementation update — PI-04–PI-09

The user subsequently requested the next six packages (written “P1-04” through
“P1-09” in the request). Project **Research tools** now exposes these optional
Workspace features. Contracts and limits are documented in the
[research studio guide](docs/workbench/research-studio.md).

| Package | Implemented | Qualification status |
|---|---|---|
| PI-04 | Conflict-aware editor/draft recovery, recoverable saves/task edits, source/prose diffs, configured TeX builds, diagnostics, retained PDF comparison, exact-build forward/inverse SyncTeX and explicit page inspection | Real multi-file LaTeX/SyncTeX and macOS development GUI observed; custom scripts, broader PDF/accessibility and other webviews remain unqualified |
| PI-05 | Explicit versioned finding copies, report splitting/numbering, response/disagreement matrix, evidence/change/execution links, flagged Markdown/LaTeX letter export, immutable focused-review coverage preview | Deterministic import/history/link/export checks; authenticated end-to-end review cycles and researcher evaluation remain |
| PI-06 | Two-slot asynchronous queue, per-project write locks, bounded streamed logs, turn/detached ownership, cancellation, restart-unknown receipts and adoption reconciliation, friendly host profiles | Queue/worker/conflict/recovery tests and local GUI build; real oldstata cooperative stop observed; forced cleanup and packaged process death remain |
| PI-07 | Experiment/baseline and specification records, host-bound result import, scalar uncertainty/specification comparisons and explicit conversions, version-2 bounded IRF series | Deterministic provenance/compatibility tests and explicit Python/Stata exporter fixtures; broader toolchains and research evaluation remain |
| PI-08 | Exact numeric passage/table/figure links, manual/proposed confirmation, dependency-specific stale coverage, precision/sign/unit/interval checks, reviewed TeX-value generation | Deterministic rounding, unit, drift and unrelated-input checks; selected bindings only, with unlinked claims visibly untracked |
| PI-09 | Non-destructive BibTeX/key navigation, distinct source versions, exact passage support assessments and question-organized notes, optional read-only Zotero preview/import | Native duplicate-key import and deterministic source checks; official API spike completed, but local Zotero was unavailable and live collection import remains a gate |

Migration 7 extends the existing Workspace store. Catalog version 3 adds bounded
read-only inspection of these records. There are no new provider wire methods,
automatic Workflow runs, or shared cancellation registries. Source discovery
and attachment acquisition remain disabled pending capability qualification.
See the [qualification record](docs/workbench/research-studio-qualification.json)
for observed checks and outstanding release work.

## Implementation update — PI-10

The user subsequently asked to continue the plan, and PI-10 landed as the
**Theory** panel in project Research tools. Contracts and limits are recorded in
the [research studio guide](docs/workbench/research-studio.md).

| Package | Implemented | Qualification status |
|---|---|---|
| PI-10 | Structured theory notes (assumptions, conjectures, propositions, derivations, proof sketches, unresolved steps, counterexamples, rejected approaches) with linked passages and side-by-side proposition/proof opening; typed check receipts distinguishing analytical, symbolic, numerical-verification, numerical-counterexample, heuristic and model-assessment evidence with host-owned scope labels, domain/tolerance/precision and execution links; promotion of derivation prose into a task copy as a reviewable change set with a retained note link; the research-direction template with task conversion; five theory-check recipes; migration 8 and tool-catalog version 4 read-only inspection | Deterministic Rust and frontend tests, full-suite runs and production build on macOS; native GUI walk-through, authenticated model reads and researcher evaluation remain |

Acceptance mapping: a numerical test cannot be labeled a general proof and a
proof sketch retains its unresolved steps (host validation plus evidence labels);
a failed approach is discoverable in a later session with its assumptions and
reason (automatic project context and the records tool); promoting a derivation
changes no assumptions or other files (insertion-only staging with an
assumptions header); ideas are compared in one table and converted into tasks
without any generated novelty or tractability score (schema rejects score
fields).

## Implementation update — PI-11 and PI-12 (authoring half)

The user asked to continue with PI-11 and the following items. Contracts and
limits are recorded in the [project exchange guide](docs/workbench/project-exchange.md).

| Package | Implemented | Qualification status |
|---|---|---|
| PI-11 | Selective `.pwex` project exchange with previewed dependency closure, readable README plus per-object JSON and hashed blobs, default exclusion of conversations/credentials/executable settings/raw inputs with labeled external references and limitations; import into a nonempty store with source namespaces, content fingerprints, deterministic ID remapping, duplicate detection, a separate imported Workspace by default, and reviewable conflicts; retained imported blobs; disk-use report separating evidence from disposable material with previewed, journaled, restorable pruning; migration 9 | Deterministic Rust and frontend tests, full-suite runs and production build on macOS; two-machine round trip, large packages, and native GUI walk-through remain |
| PI-12 (authoring) | Portable Workflow drafts from completed tasks, recipe runs, and theory notes, validated by the normal parser, with unsupported computations listed as prerequisites and import only through the Workflows page | Deterministic tests; native import/launch-preview walk-through and a fixture reproduction remain |
| PI-12 (concurrency) | Not implemented by design: bounded concurrent Workspace turns change the one-active-turn invariant and the plan gates them on demonstrated bottlenecks plus compatibility-record and lifecycle updates | Open; requires its own design and live qualification |

Acceptance mapping: a coauthor imports a selected package alongside existing
research without overwriting objects (local version kept, conflicts recorded)
and without receiving conversations or credentials (never packaged); reimport
is idempotent (identical objects skipped) and divergent notes or dispositions
become reviewable conflicts; retained imported blobs and packaged copies
survive removal of caches or Workflow runs; a package missing declared inputs
states that limitation in its manifest and README; `.pwrx` semantics are
unchanged.

The central recommendation is to make Pipeline connect research actions that currently require the researcher to carry context between tools: reading a finding, inspecting its evidence, changing a manuscript or computation, checking the result, and recording the disposition. The application should make that chain easy to inspect and resume.

The supplied *Pipeline: Vision for an AI-Assisted Academic Research Workbench* is a useful direction, but too broad to serve as an implementation backlog. Much of its proposed infrastructure already exists in Pipeline. The highest return now comes from qualifying that infrastructure, improving the document interface, making delegated changes reversible, and completing the review-to-revision cycle. A comprehensive editor, universal result parser, provider abstraction, or new research ontology would delay those benefits.

Recommended sequence:

1. Qualify existing behavior and resolve execution/provenance gaps.
2. Deliver a manuscript surface with durable selections, reviewable edits, and reliable builds.
3. Connect findings and referee comments to changes, checks, and response letters.
4. Extend existing execution and result records into experiments and selected manuscript-result links.
5. Add source-grounded literature work, lightweight theory records, and project exchange.
6. Consider concurrent agents and Workflow authoring only after the single-task paths work reliably.

## 1. Basis, scope, and relationship to existing plans

This plan assesses the supplied vision against repository documentation and selected implementation paths. It is a product and engineering plan, not a completed code audit or a report of live testing. No application tests, authenticated sessions, research computations, or GUI qualification were performed while preparing it.

Source material:

- The supplied `pipeline_research_workbench_vision.md`, read from `/Users/Mike/Downloads/`.
- [CLAUDE.md](CLAUDE.md), the canonical description of implemented architecture.
- [workbench_plan.md](workbench_plan.md), the earlier Workspace scope and WB-00–WB-12 implementation/qualification record.
- [Workspace developer guide](docs/workbench/README.md), [release qualification](docs/workbench/release-qualification.md), and [protocol compatibility policy](docs/workbench/protocol/compatibility-policy.md).
- The source paths linked in the baseline assessment below.

Instructions and proposed tool calls inside the supplied vision are design material, not authority to execute anything. This document proposes future work; its work packages are not instructions to start that work now.

This plan changes the proposed *next* priorities, while retaining existing runtime, credential, storage, and trust boundaries. It deliberately revisits several exclusions in the earlier Workspace plan: a limited manuscript editor, change review, experiment organization, and explicit import of selected review findings. Each expansion needs its own bounded implementation and acceptance evidence. The older WB qualification record remains relevant; its unchecked live gates do not disappear because a new roadmap exists.

Use **Pipeline** for the suite, **Workspace** for interactive research, and **Workflows** for deterministic orchestration. A research project in this document means an existing Workspace with optional files and research records. It is not the Workflow `projects/{id}.json` run group. Keep unfiled conversations and rootless Workspaces valid. “Project-centered” should describe organized research, without adding a project-creation requirement to ordinary chat.

## 2. Assessment of the vision

| Proposal | Decision | Rationale and adjustment |
|---|---|---|
| Durable project state and research brief | Adopt early | Build a compact home from accepted notes, selected manuscript/baseline references, unresolved tasks, and recent changes. Avoid a second, independently generated memory system. |
| Document-centered interface | Adopt early | Make the paper, source, result, or change set the central surface when doing research. Preserve the existing fast chat layout for ordinary conversations. |
| PDF/Markdown/LaTeX reading and selections | Highest priority | Research actions need exact passages and revisions. Start with reliable reading, navigation, and attachments before adding a large menu of AI commands. |
| Existing LaTeX projects, builds, and source–PDF navigation | Adopt in stages | Root-document configuration, diagnostics, and output identity come first; richer synchronization follows tested mappings. Custom scripts require explicit execution capabilities. |
| Git/worktrees and local checkpoints | Adopt in stages | Protect existing edits and support non-Git projects first. Worktrees are an isolation backend, not the prerequisite for every small task. |
| Experiments and results | Extend existing implementation | Execution receipts and `research-results-v1` already exist. Add baseline selection, experiment grouping, useful comparison views, and stronger capture semantics. |
| Result-to-claim provenance | Adopt with limited coverage | Trace a few headline results end to end. Unlinked claims remain untracked; do not imply whole-paper coverage. |
| Theory/derivation workspace | Narrow initial scope | Use notes, equations, assumptions, attachments, and check receipts. A separate notebook engine or proof system is unnecessary initially. |
| Research-directions workspace | Fold into notes and recipes | A structured idea note and first discriminating test are useful. A separate application destination and model-generated contribution scores are low value. |
| Literature management | Adopt read-first | Parse BibTeX properly, preserve keys and source versions, then add a tested read-only Zotero adapter. Do not rebuild Zotero. |
| Review finding lifecycle | Move earlier | This is close to Pipeline's established strength and gives immediate value before a complete experiment system exists. |
| Referee-response management | Move earlier | A response matrix grounded in actual changes/checks is a strong standalone benefit for economists. |
| Shared human and agent operations | Adopt as an invariant | Both interfaces should call the same scoped services. Reuse current research tools instead of exposing every conceptual operation as a new tool at once. |
| Provider-neutral Workspace | Defer additional providers | Keep the current Codex integration and narrow service boundaries. A second provider needs evidence of user value and its own qualification effort. Workflows retain their existing provider choices. |
| Harness/context inspector | Improve existing interface | It already exists. Add selection provenance, omission reasons, and more usable controls; avoid building a parallel configuration system. |
| Promote interactive work into Workflows | Defer and narrow | Draft a supported portable definition for user inspection. Conversation history is not an executable graph, and the current Workflow engine does not provide arbitrary research-job nodes. |
| Safe concurrency | Stage after reliable jobs/isolation | First allow independent local jobs without blocking reading/chat. Multiple active Workspace turns require a separate protocol and lifecycle change. |
| Ordinary files and external editors | Treat as foundational | Detect external changes, preserve source files, and maintain exact captured revisions. Users should not have to reorganize their research folders. |
| Project portability and coauthor exchange | Adopt before real-time collaboration | Export selected research objects into a nonempty destination safely. Private conversations and machine configuration stay optional or excluded. |
| Restricted-data computation | Defer execution capability | Scoped indexing and selected summaries do not contain arbitrary model-authored code. A real disclosure boundary is a separate project. |
| Extensions | Defer public extension system | Develop narrow internal adapters first. Publish an extension contract only after several actual implementations establish its shape. |
| Graph visualizations, WYSIWYG, multiplayer, cloud sync, animated agents | Exclude from the near-term plan | These add maintenance and product complexity without completing the central research paths. |

Important additions to the vision are explicit baseline promotion, conflict-safe external editing, reproducibility coverage, application-owned job recovery, diagnostic usability, storage retention, and evaluation against the researcher's existing workflow. These affect whether the proposed features can be trusted in daily use.

## 3. What already exists, and where the gaps are

“Present” below means implementation is visible in the inspected repository. It does not imply authenticated, packaged, or cross-platform qualification.

| Area | Present foundation | Improvement needed |
|---|---|---|
| Workspace identity and persistence | Independent Workspaces, optional roots, unfiled conversations, optimistic revisions, schema-5 SQLite, migrations and backups | Project home, selected baseline, object-linked research tasks; preserve current ownership |
| Interactive runtime | Isolated Codex supervisor, durable turns, streaming, approvals, recovery, global turn serialization | Close live qualification gates; preserve ordinary-chat responsiveness |
| Harness and memory | Versioned presets, accepted/proposed notes, pinned context, immutable turn setup, source diagnostics | Research brief as a view over selected records; clearer inclusion/exclusion and proposal review |
| Documents | Immutable imports, bounded source-tree capture, search, exact revision/byte/page reads, PDF page artifacts | Comfortable reader, persistent annotations, source editing, root-document model, robust cross-revision anchors |
| Workflow document structure | Richer `DocumentBundle`, nodes, math/table representations, page/figure assets | Reuse neutral document services through explicit interfaces; do not assume Workspace imports already have all Workflow extraction features |
| Execution | Tested profiles, receipts, timeouts/cancellation, output capture, LaTeX/Stata validation | Verified execution containment, async job ownership, streaming logs, input consistency, machine-specific profile handling |
| Numeric results | `research-results-v1`, stored results, deterministic scalar comparisons and verification receipts | Usable registration/comparison UI, specification records, signed differences, units/conversions, series support later |
| Evidence | Claims, links, model/human/check distinctions, freshness, retained blobs | Dependency-specific invalidation, baseline semantics, tracked coverage and navigable manuscript links |
| Review | Canonical structured findings, per-run annotations, durable cross-run Workflow issue ledger, report comparison | Explicit transfer of selected findings into Workspace tasks, revision evidence, response matrix |
| Cross-mode handoff | Explicit immutable paper staging into normal Workflow launch preview | A separately specified return package for selected findings; no shared writable state |
| Literature | Local source import, bibliographic fields, access/version provenance and duplicate candidates | Actual BibTeX entry parsing/key mapping, citation navigation, source support workflow, read-only library connection |
| Portability | Validated `.pwrx` whole-store export/restore with credential exclusions | Per-project selection, previewed redaction, import into nonempty store, stable merge identities |
| Research evaluation | Checked-in 20-case fixture, comparison variants, performance records | Execute and score real cases, add interaction/retention/conflict fixtures, publish measured qualification status |

Primary implementation references: [Workspace research](gui/src-tauri/src/workbench/research.rs), [execution and result services](gui/src-tauri/src/workbench/research/execution.rs), [command boundary](gui/src-tauri/src/workbench/commands.rs), [research inspector](gui/src/components/WorkspaceResearchPanel.tsx), [migrations](gui/src-tauri/src/workbench/migrations/003_research_harness.sql), [Workflow issue ledger](gui/src-tauri/src/projects/ledger.rs), and [archive service](gui/src-tauri/src/workbench/release/archive.rs).

Several concrete observations should shape the first work package:

1. `run_profile_process` launches a host `Command` directly. Process ownership, a valid working directory, and a passed profile test do not establish a filesystem/network sandbox. Qualify the actual launch route and connect it to the advertised permission model before expanding agent execution.
2. The input manifest hashes declared inputs before execution. Its completeness flag currently describes successfully hashing those declarations; it does not establish that all dependencies were captured or remained unchanged throughout a run.
3. A newer completed execution for a profile currently stales evidence pointing to older executions of that profile. An alternative specification should be able to coexist with an accepted baseline. Replacing a baseline and creating another run need different semantics.
4. The scalar comparator allows a supplied rationale to make otherwise incompatible results comparable. A rationale is useful for comparing different specifications, but cannot substitute for a unit conversion or repair incompatible quantities.
5. The research panel exposes execution arguments as JSON and many records as compact lists. The main gap is often the usable research interaction above the service, not a missing backend table.
6. Accepting a `.bib` file as a local source is not a full bibliography integration: the current import path stores supplied metadata and file content rather than parsing a library into independently addressable entries.

These observations motivate planned changes; they are not claims that fixes have been made.

## 4. Target research experience

### 4.1 Open and resume a project

Opening an existing folder should require only a name and, when applicable, selection of the manuscript root. If several root documents are plausible, show candidates and let the researcher choose. Do not automatically run a build script, recursively import every dataset, or create a Workflow project.

The project home should show:

- Selected manuscript revision and whether working files differ from it.
- Accepted computational baseline, which may be absent.
- Research question, key assumptions/conventions, and next task from selected notes.
- Open findings and comments awaiting action.
- Changed dependencies and claims requiring attention, with coverage limits.
- Recent completed work, failed jobs, and unresolved conflicts.

Keep this as a compact navigational view. Avoid a dashboard of empty counters. Signed-out users can read saved documents, notes, results, and history; locally configured human actions should not depend on a model session.

### 4.2 Read, revise, and inspect a manuscript

Use a project navigator, a large central work surface, an optional contextual pane, and a collapsed job drawer. The central surface can display a manuscript, source paper, result comparison, code file, finding, or change set. Start with two saved arrangements, Reading and Revision; more layouts should follow observed need.

The initial selection menu needs only Ask, Add note, Create task, and Propose revision. Specialized actions such as Check derivation or Trace result appear when the selected object has the required context. An unsupported action should explain the missing source/result, rather than inventing a link.

A selection attachment carries an immutable revision, locator, selected content, and extraction limitations. Navigating elsewhere while an agent works must not change that attachment or steal focus.

The first editor should support focused Markdown and LaTeX text edits, search, diagnostics, and external-editor handoff. It does not need a full IDE. DOCX can remain an import/reading format initially; native Word editing and faithful round-trip export are separate work.

### 4.3 Address a review finding or referee comment

Select a finding, open its source passage, record the planned response, and create a bounded task. The task produces a proposed change set and check receipts. The researcher reviews the changes, accepts the desired subset, rebuilds, and records the final disposition. The response matrix can then draft a statement grounded in that actual record.

“Addressed” is the researcher's disposition. A successful build or an agreeing model is a separately recorded check. A focused re-review is optional and has visible coverage; a full independent review remains available.

### 4.4 Investigate a result

Select a linked coefficient or table entry, open the accepted baseline result and its provenance, create an alternative experiment, run a configured analysis, and compare quantities and specification differences. The alternative does not replace the baseline automatically. A subsequent manuscript change must be reviewed against the result version it actually uses.

The first demonstration should support manual linking of the headline result. Automatic discovery can propose a link, but should not be required for the path to work.

## 5. Data and behavior decisions

### 5.1 Extend a small relational model

Use existing IDs, immutable blobs, optimistic revisions, operation IDs, and change history. Add records only when the owning work package uses them. Do not introduce a graph database or a second research store.

| Record or extension | Purpose and minimum contract |
|---|---|
| Workspace research settings | Selected manuscript and accepted baseline references; curated brief sections referencing notes; optional saved layout |
| Document anchor/annotation | Original immutable revision, source span/node/page geometry, selected quote, contextual text, annotation body, proposed successor mappings |
| Research task | Objective, origin/anchor/finding, scope, parent task if needed, status, originating session, expected outputs/checks, resulting change set |
| Checkpoint/change set | Captured base identities, task scope, pre-task file manifest, proposed file/artifact revisions, checks, acceptance journal and conflict state |
| Build configuration/receipt | Root document, cwd, locally resolved execution profile, dependency capture, expected PDF, diagnostics, source–PDF mapping identity |
| Experiment | Research question, baseline reference, intended change, configuration/specification revision, execution attempts, result references, interpretation note |
| Specification/sample record | Explicit descriptions and hashes for estimator/model, sample, weights/inference or calibration/solver settings; missing fields remain missing |
| Result binding | Result revision and quantity key, table/figure location, manuscript anchor, relation and origin of link |
| Review import/response item | Immutable source finding/comment, run or document revision, interpretation, planned action, linked task/check/change, draft response, disposition |
| Project exchange manifest | Schema/version, source namespace, selected object revisions, dependency closure, redactions/exclusions, blobs, remapping requirements |

Use note kinds and linked attachments for early theory objects, abandoned approaches, and research ideas. Split these into additional tables only when their querying or lifecycle requires it.

### 5.2 Separate working state, accepted state, and history

- Working files are mutable. Captured revisions, executions, and verification receipts are historical evidence.
- A selected manuscript/baseline reference changes through an explicit action. “Most recent execution” is a convenience sort, not a scientific choice.
- An alternative experiment leaves the accepted baseline intact. Failed and abandoned attempts remain available with their reasons.
- Model-generated decisions, metadata, and links remain proposals until accepted through the applicable UI operation.
- A note's acceptance does not turn its substantive claim into an established result.

### 5.3 Anchor and freshness semantics

An annotation always opens its original revision. A file hash plus source span or a PDF hash plus page/geometry is the minimum reliable locator. Add quote/context, TeX labels, and bundle nodes where available. Labels and repeated quotes can be ambiguous; they are mapping aids, not universal identities.

For a newer revision, classify a proposed mapping as exact, candidate, ambiguous, or missing. Only mechanically established mappings can be applied automatically. Keep the original anchor and record the mapping method. Require review of uncertain mappings, especially repeated numbers and equations.

Freshness should distinguish at least current against recorded dependencies, stale, unknown coverage, and unavailable. An unchanged hash establishes identity, not correctness. A claim may require reconsideration after new contradictory evidence even when its original dependencies are unchanged; record that substantive challenge separately from mechanical staleness.

Propagation follows recorded dependency edges and selected baselines. Show the reason and a concrete action, such as rebuild a table or inspect a changed sentence. Do not stale every result merely because another run exists, and do not mark untracked sections current.

### 5.4 Reversible changes and external editing

For substantial delegated edits, default to a task-specific copy or Git worktree with a recorded base. Include the researcher's relevant uncommitted and selected untracked files; starting from `HEAD` alone is insufficient. Large datasets can remain explicitly declared, read-only dependencies with identity and availability limitations. Never make shared source files writable through hard-linked task copies.

Review supports whole-task and selected-file acceptance first. Hunk acceptance follows once dependency and validation behavior is reliable. A selection that combines code, tables, and prose from different alternatives must show that prior checks no longer cover the assembled result. Run the relevant checks again before recording it as checked.

Applying changes needs a preflight comparison with current destination hashes, per-resource locking, staged writes, and a recoverable acceptance journal. A multi-file operation cannot be assumed atomic merely because each file write is atomic. Recover or roll back an interrupted application without losing unrelated edits. “Reject task” discards proposed changes; it does not reset the user's working tree.

Use bounded file watching and a manual refresh fallback. Coalesce change bursts, exclude build/cache directories, and compare identities again before writes and artifact adoption. External editors cannot be forced to honor Pipeline's locks. Detected races become conflicts or uncertain input consistency, not silent overwrites.

### 5.5 Execution and reproducibility

All builds and computations use the same app-owned job services whether initiated by a person or agent. Ordinary execution should use an explicitly enforced sandbox; a required host launcher is a separate locally authorized capability. A passed test run confirms observed behavior, not blanket authorization for changed scripts or settings.

Record the launcher/argv, working directory, profile revision, permission scope, relevant non-secret environment and tool identities, declared/discovered inputs, seed where supplied, outputs, diagnostics, and capture limitations. Distinguish declaration completeness, dependency coverage, and snapshot consistency. Prefer execution from an immutable task snapshot; otherwise compare inputs before and after and report uncertainty. A matching before/after hash alone cannot prove that a file was never changed temporarily during execution.

Outputs need evidence that they were produced/adopted for this attempt. An old PDF or result file already in the output directory must not make a failed or no-op run pass. Clean task-specific output directories are preferable where supported. Byte differences alone are not a universal success check because a legitimate rerun may produce identical bytes.

For this Mac, retain the required `/bin/zsh -lic` invocation of `oldstata` for every Stata operation, including probes. Allow `exit, clear` and wrapper cleanup to complete; forced cancellation exposes cleanup uncertainty. Every Git invocation on this machine uses zsh outside the sandbox under scoped authorization. These are local launch policies, not universal rules for other operating systems.

### 5.6 Preserve suite boundaries

Workspace retains its isolated runtime, credentials, store, and process ownership. Workflows retain their own launch preview, provider selection, scheduler, artifacts, and cancellation. Share neutral document/rendering/result utilities through narrow interfaces; do not pass Workflow mutable state into Workspace.

The existing paper handoff remains explicit and immutable. A proposed return bridge should export a versioned package of user-selected findings and source references, then import a copy into Workspace. It must work without reading the other store directly, retain origin IDs/hashes, be idempotent, and survive deletion of the original run to the extent selected evidence was copied. Workspace dispositions do not silently rewrite Workflow annotations. Bridge failure cannot rerun a review.

Recipes remain optional instructions and checks. They are not Workflows and do not gain scheduling authority. Native Workspace web search remains unavailable until the checked-in compatibility policy and live gates qualify an enforceable control. Nothing in this plan asserts new upstream API support.

## 6. Work packages and acceptance criteria

All packages below are **planned**. Dependencies indicate engineering order, not permission to begin implementation. Size estimates are relative: S is one narrow feature, M spans a few services/views, and L requires several independently reviewable changes. They are not calendar commitments.

### PI-00 — Qualify and reconcile the existing foundation

**Priority:** prerequisite. **Size:** M, with external qualification dependencies. **Depends on:** none.

Deliverables:

- An evidence matrix separating source implementation, deterministic checks, authenticated behavior, local research tools, packaged behavior, and supported platforms.
- Reconciliation of actual execution launches with the advertised permissions, including profile tests, model-initiated calls, changed launch scripts, and native command paths.
- Correct definitions for input coverage, snapshot consistency, output production, and accepted baseline versus latest run.
- A small set of real project fixtures and initial interaction/performance measurements. Use representative multi-file TeX, empirical, and quantitative-theory examples; include a non-Git project.
- A short capability inventory in Help/diagnostics, with actionable missing-tool and permission errors. Reuse existing qualification records instead of creating a competing release checklist.

Acceptance:

- A configured build and computation have recorded outcomes and enforced or explicitly authorized scopes; failed/ambiguous work is never presented as successful.
- Stata wrapper cancellation and cleanup behavior are observed on the supported local configuration.
- Signed-out reading and ordinary chat remain usable independently of research qualification.
- Unsupported platform/capability combinations are visible. No release claim rests solely on simulator evidence.

Implementation home: existing `workbench/codex/`, `research/execution.rs`, command boundary, and release qualification/evaluation records. Do not rewrite the runtime to solve UI gaps.

### PI-01 — Project home, accepted baselines, and file identity

**Priority:** first release slice. **Size:** M. **Depends on:** PI-00 definitions and storage review.

Deliverables:

- Project home assembled from existing records, with manuscript/baseline selection and a compact brief.
- Accepted notes for decisions, assumptions, notation, next steps, and rejected approaches, with source links and explicit revisions.
- File inventory/refresh for selected project scope, ignore rules, root relocation, and working-file versus captured-revision indicators.
- Task records with an objective, source anchor or finding, intended outputs/checks, and status. Do not force every chat message to become a task.
- A bounded context preview showing included records, selection provenance, excluded/stale material, and truncation reasons.

Acceptance:

- A fresh conversation recovers the selected research question, baseline, accepted decisions, and next task without importing rejected proposals as accepted context.
- Registering/relocating a project preserves object IDs and history. Missing files remain explicit.
- Creating an alternative run does not change the accepted baseline.
- An unfiled conversation starts without any of these services loading.

### PI-02 — Document reader and durable selection actions

**Priority:** first release slice. **Size:** L. **Depends on:** PI-01 identity contracts.

Deliverables:

- A reusable lazy document surface for PDF pages, rendered Markdown/math, and text/LaTeX/code, plus table and image inspection.
- Page/section navigation, search, retained scroll position, keyboard navigation, and opening two selected objects side by side.
- Persistent annotations and selection attachments. Start with source spans and PDF page regions; add equation/table/figure labels as available.
- Contextual actions and exact-revision evidence navigation shared by the UI and agent tools.
- A bounded renderer evaluation covering text selection, scanned PDFs, large papers, mathematics, accessibility, and supported Tauri webviews. Choose the implementation from those results; do not select a new viewer library solely from a feature list.

Acceptance:

- Select text, close the project, reopen the annotation, and retrieve the original revision and passage.
- Repeated equations or moved paragraphs produce candidate/ambiguous mappings rather than a false exact match.
- Scanned/lossy text shows extraction limits and still allows page-region inspection.
- Reading and navigating require no model calls. Streaming never steals selection or focus.

Reuse `ReportViewer`, math helpers, artifact readers, and neutral `DocumentBundle` utilities without importing `ReportWorkspace` lifecycle state.

### PI-03 — Checkpoints, isolated tasks, and change review

**Priority:** first release slice. **Size:** L. **Depends on:** PI-00 execution policy, PI-01 identity/tasks; PI-02 for readable review.

Deliverables:

- Task checkpoints covering selected current files, relevant dirty/untracked state, scope, instruction, and baseline identities.
- Non-Git task copies and a Git worktree backend through locally configured launch policy. Detect unsupported repository features or large-file dependencies explicitly.
- File/source diffs, artifact previews, check summaries, and acceptance/rejection of a whole task or selected files.
- Conflict detection, recoverable multi-file apply, and task-level undo based on recorded changes.
- Clear separation between task status, file acceptance, and research disposition.

Acceptance:

- A task starts from a project with staged, unstaged, and selected untracked work; unrelated work survives acceptance, rejection, and interruption.
- An external edit made during the task prevents a silent overwrite.
- A crash during applying multiple files leaves a recoverable journal and identifiable partial state.
- A non-Git project has equivalent basic protection. Unsupported file types/large inputs have visible limits.
- No automatic push, destructive reset, or unrelated branch switch is part of accepting a task.

Add hunk acceptance and comparison of two alternatives after these cases pass. Partial acceptance invalidates checks whose inputs no longer match.

### PI-04 — Manuscript editing, builds, and source–PDF navigation

**Priority:** first release slice. **Size:** L. **Depends on:** PI-02 and PI-03; PI-00 execution qualification.

Deliverables:

- A limited Markdown/LaTeX editor with explicit save state, external-editor handoff, and conflict-aware refresh.
- Build configuration for project/root document, working directory, expected PDF, timeout, diagnostics, and a locally resolved launcher.
- Forms for common build profiles instead of mandatory argv JSON. Retain an advanced editor.
- Streamed logs, clickable source diagnostics, bibliography/reference warnings, immutable build receipt/PDF, and retained reader position.
- Forward/inverse synchronization when the exact build produced usable mapping data. Use source location/page fallback when it did not.
- Source diff and affected-page comparison. Add readable prose diff as an aid, while retaining source inspection for equations, labels, and citations.

Acceptance:

- An existing paper with included sections, generated tables/figures, bibliography, and appendix builds from its declared root without restructuring.
- A failed build cannot display an older PDF as the new output. Warnings, successful compilation, and rendered-page inspection remain distinct outcomes.
- A proposed agent edit is compiled and relevant pages can be inspected before acceptance; accepting a changed subset triggers appropriate revalidation.
- Custom build scripts do not run on project open. Imported build configuration remains inert until locally resolved.

Support common tested engine profiles first. An existing `build.sh` is a trusted command profile with declared capabilities, not a shortcut around the LaTeX policy. Retain shell-escape restrictions unless a separate, explicitly scoped design qualifies another mode.

### PI-05 — Findings, revision tasks, and referee responses

**Priority:** first major research payoff. **Size:** L. **Depends on:** PI-02–PI-04; PI-01 task records.

Deliverables:

- Versioned export/import of selected canonical Workflow findings, with an explicit preview and immutable source references. Extend the existing handoff boundary deliberately.
- Local import of referee/editor reports. Preserve original text and locators when proposing a split into individual comments; let the researcher correct interpretation and numbering.
- A response matrix linking each comment to its intended response, task, manuscript revision, computation/check, draft response, and disposition.
- Finding categories that distinguish an observed error, unresolved objection, missing robustness, omitted source, unclear exposition, and optional extension. Preserve severity separately.
- Review disagreement records retaining the disputed premise, evidence, counterargument, and possible resolving check.
- Focused re-review of changed passages and declared dependencies, with a visible coverage manifest and the normal Workflow preview.
- Markdown/LaTeX response-letter export with stable comment numbering and manuscript references; DOCX export can follow if demand warrants it.

Acceptance:

- A finding imported twice creates one logical source occurrence, without losing later user decisions.
- A claim that an appendix lacks a derivation can be investigated against that appendix and rejected with evidence.
- A response saying an analysis was added links to a completed execution and the relevant accepted manuscript change. Unsupported draft statements are flagged before export.
- A researcher can defer or reject a comment without pretending a scientific objection was resolved.
- A reappearing or remapped finding retains history. Reviewer agreement alone never closes it.
- Disabling the bridge leaves both orchestration modes functional and their stores independent.

Do not replace the existing Workflow issue ledger. Reuse its semantics where appropriate; Workspace tracks the imported copy and its revision tasks.

### PI-06 — Durable local jobs and usable execution profiles

**Priority:** computational foundation. **Size:** L. **Depends on:** PI-00, PI-01, PI-03. Can follow the minimal build path in PI-04.

Deliverables:

- Split durable job bookkeeping from the long-running process wait. Database worker/gate capacity must not be held for the duration of a computation.
- Typed start/status/log/cancel operations, incremental bounded logs, duration/resource information when available, and a job drawer.
- Explicit job states covering queued, running, cancelling, completed, failed, cancelled, timed out, and unknown outcome; output adoption/finalization is recoverable.
- A local resource queue, task-specific write roots, per-resource locks, and conflict checks against managed edits.
- Friendly configured profiles with input/output selection and human-readable validation. Add small exporter examples for toolchains actually qualified.
- Two explicit ownership modes: turn-owned jobs stop with the turn; deliberately detached local jobs continue while the application remains open. Do not silently change the meaning of Stop.

Acceptance:

- A long computation does not block another project's reading, transcript persistence, or job cancellation.
- Duplicate tool delivery does not rerun the same side effect. A crash at launch/adoption exposes an unknown/reconcilable outcome rather than fabricating exactly-once execution.
- Timeouts and cancellation terminate owned descendants and preserve required launcher cleanup status.
- App exit has a clear stop policy; no daemon, machine-sleep survival, or automatic restart is promised in this package.
- Workflow cancellation remains independent. Conflicting filesystem writes are serialized or rejected even when execution owners differ.

### PI-07 — Experiments and meaningful result comparison

**Priority:** computational research value. **Size:** M–L. **Depends on:** PI-06 and PI-01 baseline semantics.

Deliverables:

- Experiment records grouping a question, baseline, intended change, executions, exported results, and an editable interpretation note.
- Specification/sample/calibration records with declared versus inferred fields visibly distinguished.
- A result registry UI using existing immutable execution/output identities. Preserve the `research-results-v1` reader; version any expanded contract.
- Deterministic comparisons of estimates, uncertainty, sample size, and specification differences; signed and absolute changes, and relative changes only with meaningful nonzero denominators.
- Explicit quantity compatibility and conversion rules. Comparing alternative specifications is valid with a stated rationale; incompatible units need an actual recorded conversion. An unavailable value is never zero.
- One bounded macro result extension, such as an impulse-response series with horizon, units, shock normalization, and variable identity. Add welfare/decomposition views only with comparable semantics and fixtures.

Acceptance:

- A baseline and alternative show their numerical changes alongside sample, weights, controls/inference, or calibration differences.
- A new alternative does not invalidate the baseline merely by existing.
- Numeric exports cannot assign themselves host-confirmed execution provenance. The host binds them to adopted outputs.
- Nonfinite values, missing results, differing scales, and incompatible series axes have explicit outcomes.
- Solver convergence, numerical agreement, and economic interpretation remain separate checks.

Do not infer authoritative coefficients/specifications from arbitrary logs. Offer proposed extraction where helpful, with source spans and review; prefer explicit exports for repeatable checks. General dataset inspection should use bounded local summaries over authorized data, without assuming this makes restricted microdata usable.

### PI-08 — Trace headline results into the manuscript

**Priority:** distinctive integrated capability. **Size:** M–L. **Depends on:** PI-02, PI-04, PI-07; PI-05 for review integration.

Deliverables:

- Manual and proposed links from result quantities to table cells/figures and manuscript passages, retaining origin and exact revisions.
- Dependency-specific refresh and actionable stale-item lists.
- Deterministic checks for declared numeric bindings, including rounding, signs, units, and intervals. Match the reported precision rather than comparing every printed digit to raw floating-point equality.
- Optional generation of a small selected table or TeX value macro file from structured results, reviewed through the existing change-set service.
- A coverage view showing which selected headline claims are linked, current, stale, unknown, or unavailable. Begin with lists and drill-downs, not a graph visualization.

Acceptance:

- A linked coefficient opens the exact output, execution, specification, and captured dependencies.
- A changed upstream input identifies the dependent result/table/claim; unrelated results remain unaffected.
- Updating a table without revising prose exposes the discrepancy, including a changed sign or reported unit.
- An ambiguous automatically proposed table-cell link cannot become authoritative silently.
- A historical claim still opens its historical evidence after the accepted baseline changes.

### PI-09 — Literature and citation support

**Priority:** useful parallel product area after the reader. **Size:** M–L. **Depends on:** PI-02 and source identity; independent of PI-06–PI-08.

Deliverables:

- Parse BibTeX entries into stable source records while retaining the original file, citation keys, and unknown fields. Detect duplicate keys and versions without destructive merging.
- Citation-key navigation from manuscript to metadata, an available source version, and selected supporting passages.
- A source-support checklist distinguishing identity, source access, passage relevance, assessment method, and freshness.
- Literature notes/comparison tables organized by research question, with each substantive entry linked to a passage or explicitly labeled unsupported.
- Read-only Zotero connection after a short official-interface/permissions qualification spike. Start with a selected collection and an import preview; preserve upstream item/version identities and keep project notes separate.
- Optional source acquisition/discovery only through qualified capabilities, with query history, access limitations, download validation, and clear source-version distinctions.

Acceptance:

- A citation that exists but does not support the manuscript claim is represented accurately.
- Metadata-only or abstract-only access is not mislabeled as full-text support.
- Working paper and published versions retain distinct passages/locators and a relationship between versions.
- Reimport preserves keys and project notes. No library write-back or citation-key renaming occurs implicitly.
- Local bibliography/source work remains useful when web search, Zotero, or authentication is unavailable.

Do not assert that a specific external API or attachment capability is available until checked during the integration spike. Search results alone cannot establish novelty or exhaustive coverage.

### PI-10 — Lightweight theory and research development

**Priority:** after manuscript/task foundations. **Size:** M. **Depends on:** PI-01–PI-04; PI-06/PI-07 for executable checks.

Deliverables:

- Structured notes for assumptions, conjectures, derivations, proof sketches, unresolved steps, counterexamples, and rejected approaches.
- Linked equations/source passages and side-by-side proposition/proof inspection through the document surface.
- Configurable recipes for limiting cases, dimensional checks, accounting identities, comparative statics, and numerical counterexamples.
- Check metadata distinguishing analytical argument, symbolic identity, numerical verification, numerical counterexample, heuristic, and model assessment. Numerical checks include domain, tolerance, and precision where known.
- Promotion of selected derivation prose into the manuscript as a reviewable change set, preserving its working-note link.
- A research-direction note template: question, mechanism, closest known work, minimal model/data, first discriminating test, likely failure mode, and next action.

Acceptance:

- A numerical test cannot be labeled a general proof, and a proof sketch retains unresolved steps.
- A failed approach is discoverable in a later session with its assumptions and reason for rejection.
- Promoting a derivation does not silently change assumptions or the economic model.
- Ideas can be compared and converted into tasks without model-generated novelty or tractability scores becoming decision rules.

No bespoke notebook runtime, formal proof assistant integration, or automatic theorem database is required.

### PI-11 — Project exchange, retention, and coauthor handoff

**Priority:** portability before collaboration infrastructure. **Size:** L. **Depends on:** PI-01 identity, PI-03 history, PI-05 response records; incorporate later object types as they land.

Deliverables:

- Export one project's selected notes, findings, response matrix, changes, result receipts, and evidence with a previewed dependency closure.
- Default exclusion of credentials, native runtime state, private conversations, executable machine settings, and unselected/raw datasets. Clearly label excluded evidence and external references.
- Human-readable Markdown/JSON plus ordinary document/artifact files. Project exchange should remain interpretable without Pipeline.
- Import into a nonempty store using source namespaces, revision hashes, ID remapping, duplicate detection, and explicit conflict handling. Default to a separate imported project when merging is ambiguous.
- Root remapping and machine-local execution profile resolution; imported settings stay inert.
- Disk-use and retention controls separating disposable build/cache material from evidence-linked immutable blobs, with deletion previews and recoverable backups.

Acceptance:

- A coauthor imports a selected package alongside existing research without overwriting objects or receiving private conversations/credentials.
- Reimport is idempotent; divergent notes/dispositions generate a reviewable conflict rather than last-write-wins replacement.
- Removing a cache or Workflow run does not remove evidence deliberately retained in an exchanged project.
- A package missing a large external dataset states that limitation and does not claim full reproducibility.
- Whole-store `.pwrx` backup/restore keeps its existing semantics and remains independent of selective project exchange.

Visibility labels govern what Pipeline exports; they are not filesystem access controls or a confidential-data security boundary. Real-time editing, accounts, and cloud synchronization remain deferred.

### PI-12 — Conditional concurrency and Workflow authoring

**Priority:** later, evidence-dependent. **Size:** L. **Depends on:** PI-03, PI-06, and reliable end-to-end usage of PI-05/PI-08.

Deliverables, only if demonstrated bottlenecks justify them:

- Bounded concurrent Workspace turns in independent task roots, with per-turn permissions, tool catalogs, quotas, approvals, cancellation, and recovery.
- Explicit serialization for shared accepted state and source files. Resource arbitration can be shared at a neutral layer while each mode retains process/cancellation ownership.
- Draft Workflow authoring from selected successful research steps, with explicit inputs, dependencies, artifact contracts, checks, and unsupported operations.
- Validation through the existing portable schema/editor and ordinary launch preview. No automatic installation or execution from a transcript.

Acceptance:

- Stopping one task cannot cancel another, late events cannot release another task's permit, and overlapping writes cannot race.
- A literature task and isolated computation can proceed together without sharing unintended context or mutable files.
- Generated Workflows validate against the installed engine and reproduce the selected supported process on a fixture.
- Unsupported computation steps remain explicit prerequisites. If deterministic command nodes are needed, they receive a separate Workflow-engine design; prompt text cannot masquerade as a job scheduler.

This package intentionally changes the present one-active-Workspace-turn invariant. It must update the compatibility record, storage/event contracts, and live qualification matrix before the restriction is lifted. It does not authorize a generic provider framework or autonomous background research.

## 7. Release slices and dependency order

| Slice | Packages | User-visible outcome | Release gate |
|---|---|---|---|
| A — Usable manuscript work | PI-00–PI-04 | Open an existing paper, read/select, delegate a bounded edit, inspect changes, compile, and resume | Existing dirty files survive; exact revisions/outputs remain inspectable; live local tool path passes |
| B — Review through revision | A + PI-05 | Findings/referee comments become tasks, checked changes, and a grounded response letter | A complete response cycle with preserved disagreement and no unsupported completion claims |
| C — Traceable computational revision | A + PI-06–PI-08, integrated with B | Baseline → alternative → comparison → manuscript update → focused review | The coefficient demonstration below passes with exact evidence and partial-acceptance checks |
| D — Broader research continuity | PI-09–PI-11 | Source-grounded literature, theory notes, and selective coauthor exchange | Version fidelity, note/check distinctions, and safe import into a nonempty store |
| E — Optional scaling | PI-12 | Isolated concurrent tasks and supported Workflow drafts | Qualified lifecycle/permission contracts and demonstrated value over serialized work |

PI-09 can proceed after the reader without waiting for the experiment system. PI-11 can start with a small selected-object package after PI-05 and extend to later types. Keep one integrated acceptance scenario working as scope grows rather than opening every package at once.

If capacity is constrained, complete A and B before expanding the later packages. Within A, retain the execution/identity foundations, reader, basic selected-file change review, and common builds. Defer hunk acceptance, sophisticated prose/PDF diffs, extra saved layouts, and custom build variants before cutting protection for existing work.

Do not assign a fixed release date from this document. The largest uncertainty is the combination of real execution boundaries, dirty-project isolation, cross-platform document interactions, and crash-safe acceptance. Estimate those after PI-00 fixtures and narrow prototypes provide evidence.

## 8. End-to-end acceptance scenarios

### Scenario A: A manuscript revision without computation

Open a multi-file theory paper with existing uncommitted edits. Select a proposition, attach the proof from the appendix, and request an expository revision preserving the model. Review the proposed source changes, accept selected files, compile, inspect affected pages, and reopen the project after restart.

Pass conditions: unrelated edits survive; original and new selections remain resolvable; a rejected substantive change is not applied; build/output identity is correct; the next session can recover the accepted decision.

### Scenario B: A referee-response cycle

Import an editor letter and two referee reports. Correct the proposed comment breakdown, dismiss one objection using an existing appendix, revise one ambiguous explanation, and record one genuinely unresolved robustness request. Export the response letter and selected supporting record.

Pass conditions: quotations/comment numbers remain faithful; disagreement is preserved; unresolved work stays unresolved; every statement that a revision/check was completed links to the actual evidence. The exported letter is readable without internal database IDs in its prose.

### Scenario C: The defining coefficient investigation

1. Open a paper with source, bibliography, code, an authorized dataset, and existing table/result exports.
2. Select a manually linked headline coefficient and inspect its accepted baseline.
3. Create an isolated task to add state-specific trends, carrying relevant dirty source files into its starting state.
4. Run the configured analysis and register the resulting exports.
5. Compare coefficient, uncertainty, sample size, and specification differences.
6. Record an interpretation without automatically promoting the alternative baseline.
7. Propose and review changes to the table and manuscript discussion.
8. Compile and inspect affected pages against the accepted selection of files.
9. Run an explicitly launched focused review, including affected claims and relevant appendix material.
10. Record remaining findings and preserve the chain for a fresh session/coauthor package.

Pass conditions: every traced object has a revision and origin; calculations are deterministic where applicable; unsupported interpretation stays visible; partial acceptance cannot retain mismatched check status; historical baseline evidence remains available.

### Scenario D: Quantitative theory and a misleading numerical check

Compare two calibrations of a small macro model, with recorded solver tolerances, seeds, and impulse-response normalization. Check a limiting case and link a welfare statement to its generating result. Include a numerical example that passes while a proposed general proposition is still unproved.

Pass conditions: horizons/units are aligned, nonconvergence is explicit, welfare concepts are comparable, and numerical success is never promoted into an analytical proof.

### Scenario E: Failure, conflict, and portability

While a task runs, edit a target file externally, interrupt a build, remove an external input, and exercise a crash during multi-file acceptance in an isolated fixture. Export selected project objects and import them into a nonempty store under remapped paths.

Pass conditions: no silent overwrite/replay, stale outputs do not pass, uncertainty survives recovery, retained evidence remains readable, and private/native state does not cross the export boundary.

## 9. Verification and measures of value

Extend the existing research evaluation contract rather than inventing a success score based on feature count. Measure the same tasks with comparable files and model settings in Pipeline's research surface, plain Workspace, and the researcher's ordinary tools. Record operator familiarity and setup time so the comparison is interpretable.

| Measure | What to record |
|---|---|
| Research correctness | Researcher assessment of the finding, derivation, comparison, or response; unsupported claims and substantive errors |
| Traceability | Whether each selected claim/change opens the promised exact source/result/check; ambiguous and unavailable cases |
| Task completion | Time and interventions from initial finding/question to accepted checked artifact |
| Revision safety | Unrelated edits preserved, conflict behavior, partial-acceptance validity, recovery after interrupted apply |
| False alarms | Unnecessary stale flags, incorrect anchor mappings, duplicate findings, and repeated rejected suggestions |
| Context quality | Relevant accepted decisions included, rejected proposals excluded, omissions and stale material identified |
| Resource use | Model/tool calls, tokens where available, elapsed time, local CPU/memory, artifact growth, and unknown accounting values |
| Interaction quality | Cold/warm document open, search/selection latency, scroll stability, keyboard access, cancellation, and chat regression |

Correctness gates should include zero silent overwrites, false successful executions, credential exports, or falsely exact anchor mappings in the release fixture set. These are fixture requirements, not a statistical claim that failures are impossible. Set performance thresholds from recorded reference hardware and representative documents before accepting each feature. Preserve the existing lazy module boundary, bounded transcript, and coalesced streaming behavior.

Verification layers:

1. **Deterministic contracts:** schemas/migrations, identifiers, locators, unit conversions, formatting tolerances, duplicate operation handling, archive integrity, and conflict/apply recovery.
2. **Integration fixtures:** ordinary multi-file projects, dirty Git and non-Git work, external file changes, missing dependencies, old outputs, partial acceptance, and Workflow bridge independence.
3. **Live research tools:** configured TeX and required `oldstata` launchers, actual diagnostics and result exports; no paid account or license required for the deterministic suite.
4. **Authenticated runtime:** real tool invocation, questions/approvals, interrupted turns, scope enforcement, quota/failure handling, and resumed sessions.
5. **Platform/package qualification:** required process, permission, viewer, and recovery behavior on each claimed supported platform. On this computer, interactive GUI inspection uses the Tauri development application as required by repository instructions. Package qualification must use an approved isolated test environment consistent with that restriction.
6. **Research evaluation:** a researcher scores judgment-heavy outcomes and compares the complete paths, including time spent correcting agent mistakes.

Use the repository's pinned toolchains and current commands in [CLAUDE.md](CLAUDE.md): focused checks during implementation, then the applicable frontend build/tests, Rust format/clippy/tests, and release checks before handing off a completed feature. Document-only planning does not require running those application suites. Record the exact runtime/tool/platform and distinguish a code check from a live gate.

## 10. Main risks and deliberate limits

| Risk | Planned response |
|---|---|
| The suite grows into several incomplete applications | Release complete manuscript/revision paths first; reuse notes, tasks, and existing services instead of creating a destination for every concept |
| Document anchors look precise but point to the wrong revision | Preserve original immutable locators; make remapping confidence and ambiguity explicit |
| Provenance records overstate reproducibility | Separate declared dependencies, captured coverage, snapshot consistency, and actual successful reruns |
| “Newest” silently becomes “accepted” | Explicit baseline/manuscript promotion and retained alternatives |
| Task isolation loses dirty files or permits external-write races | Snapshot relevant working state; preflight destination hashes; conflict-aware, journaled apply |
| Sandboxed labels conceal host execution | Qualify actual launch paths and scopes; distinct host capabilities; no fallback that relaxes boundaries |
| Review integration couples the orchestration modes | Immutable, versioned, user-selected transfer packages; independent credentials, lifecycle, and writable stores |
| Agent-generated response letters exaggerate completed work | Link completion statements to accepted revisions and named checks; researcher reviews final text |
| Literature convenience erases source distinctions | Preserve citation keys, bibliographic identity, version identity, and access/support states separately |
| Storage grows without bound or deletes evidence | Bounded capture, declared large-file references, retention/reference tracking, size previews and deletion controls |
| Cross-platform promises exceed observed behavior | Capability-specific qualification; visible unsupported states; no inference from one successful Mac run |
| A restricted-data label is mistaken for containment | Keep restricted execution out of scope until an independently enforced disclosure boundary is designed and tested |

Continue to defer remote/HPC execution, cloud synchronization, real-time coauthor editing, a public extension marketplace, generalized semantic graphs, universal log parsing, automatic literature-novelty certification, and unrestricted autonomous research. Revisit each only when a completed research path produces a concrete unmet need.

## 11. Implementation handoff when work is requested

The next implementation request should focus on closing the remaining PI-00–PI-09 release gates and on PI-10+ only after the researcher has reviewed the current integrated slice. Each task should name its package, describe the behavior being changed, state existing versus proposed contracts, and identify acceptance evidence before editing code.

For each completed increment:

- Preserve unrelated work and existing persisted formats.
- Add append-only migrations only for records actually used by that increment.
- Keep UI and agent actions on the same scoped service implementation.
- Record tests, live qualification, limitations, and unresolved decisions separately.
- Update `CLAUDE.md` and the relevant topic/qualification documents to describe what actually landed.
- Update this roadmap's status only after its acceptance conditions are met.

The product decision is to prioritize a dependable path from a research objection or selected passage to an inspected, checked revision. Once that path works comfortably, experiments, literature, theory records, and coauthor exchange can extend the same underlying objects without requiring another orchestration system.
