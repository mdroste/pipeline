# Pipeline: product direction and feature implementation plan

**Prepared:** October 2, 2026.  
**Status:** Recommendation and implementation specification; no application changes authorized by this document.  
**Audience:** An implementing LLM and the researcher reviewing its work.  
**Scope:** The whole Pipeline product: Projects, Conversations, Reviews, Automations, research tools, files, and delivery.

**Reading guide:** [Product judgment](#1-product-judgment) · [Current capabilities and gaps](#22-current-capability-and-remaining-product-gap) · [Target experience](#3-target-researcher-experience) · [Common contracts](#4-product-constraints-and-common-contracts) · [Priorities](#5-priorities-and-delivery-units) · [Detailed work packages](#6-implementation-work-packages) · [Consolidations and deferrals](#7-what-to-consolidate-hide-remove-or-defer) · [Reference implementation](#8-concrete-end-to-end-reference-implementation) · [Delivery sequence](#9-implementation-sequence-and-release-boundaries) · [Test matrix](#10-cross-cutting-implementation-and-test-matrix) · [LLM handoff instructions](#11-instructions-for-the-implementing-llm).

## 1. Product judgment

Pipeline should become the place where a researcher can develop an argument, inspect the evidence behind it, run the next useful investigation, and carry the result into a paper without losing the chain of reasoning. Its strongest possible distinction is **continuity between research decisions, evidence, computation, and writing**.

The present application has unusually extensive foundations. It already has immutable sources, claims and evidence, captured execution, specification families, numerical manuscript bindings, reversible edits, review campaigns, research missions, and autonomous project discovery. Adding another collection of specialized panels would make the product larger without necessarily making research easier.

The next product phase should complete three experiences:

1. **Understand and improve an existing paper.** Open the paper and its project, inspect its central claims, follow a concern to the relevant evidence, make a scoped revision, and establish what the new version has and has not resolved.
2. **Move from analysis to a defensible manuscript.** Run existing research code, compare the complete family of results, select a baseline deliberately, generate publication assets, and maintain exact links from the manuscript back to results and inputs.
3. **Delegate a bounded investigation.** State a question and the decision it should inform, approve an understandable plan, let Pipeline investigate and challenge the findings, and receive an evidence-linked conclusion or an informative negative result.

These experiences should share one Project and a consistent interface while retaining the distinct execution and storage owners documented in [CLAUDE.md](CLAUDE.md). The ambition is substantial; the initial release of this plan should be deliberately narrower than the full roadmap.

### 1.1 The five recommendations that matter most

| Order | Recommendation | Why it changes the product |
|---|---|---|
| 1 | Make project membership explicit across Reviews and Automations; give research objects readable, actionable views. | The researcher can trust that work belongs to the right project and can follow connections without interpreting IDs or JSON. |
| 2 | Put a living argument and evidence view beside the manuscript. | Pipeline becomes useful for assessing the paper's substantive state, beyond generating or reviewing text. |
| 3 | Turn existing computation, results, assets, and bindings into one continuous analysis-to-writing workflow. | A changed dataset or specification can lead to a reviewed, reproducible manuscript update. |
| 4 | Make the assistant propose typed actions with exact inputs and inspectable effects. | Natural language becomes a practical way to operate existing capabilities without becoming an unvalidated scheduler or silently changing research records. |
| 5 | Make autonomous work evidence-first, affordable to try, interruptible, and honest about what was accomplished. | Delegation produces useful research decisions, including failed hypotheses, rather than optimizing for the number of generated papers. |

### 1.2 Preserve these strengths

- The calm neutral visual direction, project/assistant split, shared source/PDF readers, and recent navigation simplification.
- Local ownership of research materials, portable artifacts, explicit provider choice for Reviews, and no required Pipeline account or hosted service.
- Immutable source versions, exact passages, retained negative results, separate model proposals and researcher acceptance, reversible manuscript changes, and bounded execution.
- The ability to use an unfiled conversation, a rootless project, or a one-off Review. Organization should remain helpful rather than compulsory.
- Source-first LaTeX and code editing, compatibility with existing research folders, and the ability to keep using external editors and reference managers.
- Separate Workspace, Review, and Tasks runtime/store/cancellation ownership. A unified product experience does not require a unified mutable executor.

## 2. Review basis and limits

This plan reviews the October 2 working tree, including existing uncommitted work. It is not a review of a pristine release tag. Existing changes must be preserved by the implementing agent.

The review read the repository instructions, architecture map, product README, current feature documents, prior product plan and release audit, and representative frontend and backend owners across the major research workflows. The source checks included project membership and overview derivations, navigation, object rendering, result binding, literature assessments, dataset/sample intake, specification grids, theory records, automation authoring, discovery defaults/validation, captured execution, and the claim/evidence ledger.

The Tauri development build was launched successfully with disposable Workspace and Automation stores. The computer-use adapter could not attach to its unbundled executable. Visual inspection therefore used the repository's existing full-app browser fixture served by that development session: New review, the populated project Overview with companion conversation, and the Automation composer. The fixture uses synthetic native responses; it is not evidence of authenticated or packaged behavior. Opening Result links exposed an incomplete fixture record (`anchor.selection.quote` was unavailable), and the fixture logged missing command stubs. That observation calls for better fixture contracts; it does **not** establish a production result-linking defect. The temporary browser tab and development session were closed after inspection.

No model research run, external literature request, Stata invocation, installer qualification, application feature implementation, or new user-data migration was performed for this plan. No current competitive-market or external-service capability claim is needed for these recommendations. Future connector work must verify its then-current official API and access terms before implementation.

### 2.1 Existing plans are inputs, not unfinished checklists

- [FABLE_SEP30_UIUX_PLAN.md](FABLE_SEP30_UIUX_PLAN.md) records the route model, shared UI kit, flat rail, automation outline editor, project Reviews surface, Activity destination, settings work, and October 1 derived project Overview/repository work. Their absence must not be asserted again. This plan uses **FA-00–FA-17** identifiers to avoid conflicting with its WP/UX numbering.
- [ASTRA_PIPELINE_OCT2.md](ASTRA_PIPELINE_OCT2.md) records release fixes already implemented and qualification still open. Resolve release obligations through that record and the current qualification documents. Do not reimplement fixed findings from its original audit text.
- The [research desk](docs/workbench/research-desk.md), [research studio](docs/workbench/research-studio.md), and [research programs](docs/workbench/research-programs.md) already implement substantial first versions of almost every research building block recommended here.
- [Research missions](docs/research-missions.md) and [self-discovery](docs/self-discovery.md) are implemented capabilities with explicit limitations, not speculative future modes.
- Historical roadmap exclusions are useful context, but the current implementation and explicit user direction take precedence. For example, a real source editor and adaptive research missions now exist.

### 2.2 Current capability and remaining product gap

| Area | Present in the inspected tree | Specific remaining opportunity | Primary evidence |
|---|---|---|---|
| Navigation and visual shell | Four product nouns, six project areas, typed routes, view picker, shared primitives, split desk | Specialized views still form long sibling tab rows; section navigation selects a default tool rather than a substantive section landing experience | [Navigation map](gui/src/lib/workspaceNavigation.ts), [project navigation](gui/src/components/WorkspaceProjectNavigation.tsx), [UI classes](gui/src/ui/classes.ts) |
| Project Overview | Derived draft state, changed evidence, revision round, repository, resume, and recent changes | Connect rows to exact objects and actionable bundles; disclose unavailable checks without turning Overview into a dashboard of warnings | [Overview](gui/src/components/project-surface/ProjectOverview.tsx), [signal loading](gui/src/hooks/useProjectSignals.ts), [derivations](gui/src/lib/projectOverview.ts) |
| Project Reviews | Embedded findings ledgers and folder-scoped runs | Membership uses input-path containment; rootless projects and temporary handoff paths need durable association, and mixed collections need exact scoping | [ProjectReviews](gui/src/components/project-surface/ProjectReviews.tsx) |
| Research objects | Exact revision references, object pane, source tray, project FTS search | Objects often expose field names and nested JSON instead of a researcher-oriented interpretation and next action | [ObjectPane](gui/src/components/research-desk/ObjectPane.tsx), [search owner](gui/src-tauri/src/workbench/search.rs) |
| Claims and impact | Claim/evidence ledger, decisions, dependencies, bounded impact propagation | A connected argument outline with explicit scientific obligations and manuscript coverage | [ledger](gui/src-tauri/src/workbench/research/ledger.rs), [relations](gui/src-tauri/src/workbench/project/relations.rs) |
| Literature | Crossref intake, retained PDFs, BibTeX, local Zotero metadata preview, exact passage assessments and comparison table | Batch intake, work/version identity, usable citation support inspection, selected attachments, collection updates, better intake-to-reading flow | [acquisition](gui/src-tauri/src/workbench/acquisition.rs), [Literature](gui/src/components/research-studio/Literature.tsx) |
| Data | Captured CSV/TSV, dictionaries, declared samples, data policy, FRED/ALFRED vintage capture | Guided adapters for common research formats; executable sample receipts and comparable diagnostics; avoid JSON as the normal metadata interface | [Data](gui/src/components/research-desk/Data.tsx), [data owner](gui/src-tauri/src/workbench/data/mod.rs) |
| Computation | Tested execution profiles, queue, exact captured inputs, execution receipts, scalar/IRF results | Easier setup from existing scripts, fuller environment evidence, stronger result contracts, explainable execution coverage | [execution plans](gui/src-tauri/src/workbench/research/execution_plan.rs), [jobs](gui/src-tauri/src/workbench/research/jobs.rs), [exporters](examples/research-exporters/README.md) |
| Experiments | Bounded complete specification families, exclusions, retained attempts, fixed baselines, comparison and assets | Interactive comparison in one place; typed factors/exclusions; direct proposed table/figure/manuscript updates | [grid UI](gui/src/components/research-programs/Experiments.tsx), [experiment owner](gui/src-tauri/src/workbench/programs/experiments.rs) |
| Theory | Notes, assumptions, symbols, lexical notation candidates, alternative assumptions, numerical/symbolic check records | Proposition-centered work with visible proof obligations and deliberate counterexample search | [theory services](gui/src-tauri/src/workbench/project/studio/theory.rs), [symbol/check UI](gui/src/components/research-programs/Theory.tsx) |
| Writing | CodeMirror, PDF.js, SyncTeX, exact source diffs, partial acceptance, tables/figures, numerical bindings | Manuscript-side evidence inspection, assisted link creation, dependency-aware updates and a coherent final preparation workflow | [file workspace](docs/file-workspace.md), [Bindings](gui/src/components/research-studio/Bindings.tsx), [assets](gui/src-tauri/src/workbench/programs/assets.rs) |
| Revision | Finding exchange, responses, campaigns, exact review handoffs and coverage | One work queue connecting concern, decision, experiment, accepted change, response, and re-review | [review services](gui/src-tauri/src/workbench/project/studio/review.rs), [campaigns](gui/src-tauri/src/workbench/programs/campaigns.rs) |
| Automation | Outline and condition editors, proposal tools, versioned chains, schedules, budgets, journals | Typed research-object pickers, natural-language drafts, dry-run traces, clearer budgets and failure decisions | [OutlineEditor](gui/src/components/tasks/OutlineEditor.tsx), [Builder](gui/src/components/tasks/Builder.tsx), [Tasks contract](docs/tasks.md) |
| Autonomous research | Missions, fresh challenges, supervised/unsupervised portfolios, immutable reviews and exports | Smaller evidence-first defaults, editable briefs, feasibility gates, meaningful scientific evaluation | [defaults](gui/src/lib/discoveryClient.ts), [validation](gui/src-tauri/src/orchestration/discovery/validate.rs), [missions](docs/research-missions.md) |
| Sharing and recovery | `.pwex` selective exchange, coauthor briefs, `.pwrc` replication capsules, `.pwrx` backups | Clear handoff workflows and complete, explicitly scoped project recovery across separate owners | [exchange](docs/workbench/project-exchange.md), [programs](docs/workbench/research-programs.md), [archive owner](gui/src-tauri/src/workbench/release/archive.rs) |

## 3. Target researcher experience

### 3.1 A paper-centered working surface

Retain the existing rail and six project areas. Within a project, the main pane shows the object being worked on: paper, result, dataset, proposition, review concern, or automation. The assistant remains a companion pane. A compact inspector opens on demand for evidence, provenance, or change details. Do not make three full panes mandatory.

The persistent project bar should answer: **which project, which object, which version, and what can I do here?** Status belongs near the object it describes. A long hash, internal record kind, or execution identifier belongs behind an inspect/copy control.

Example of the intended Write surface:

```text
Project / Write / main.tex                     Draft 12 · unsaved changes
────────────────────────────────────────────────────────────────────────
Outline        Manuscript or exact build PDF              Assistant
  Motivation   …a 1 percent change in the tax wedge…      Ask about this
  Model        [selected claim or printed estimate]       passage
  Results
               Evidence: Table 3, baseline B4
               One changed input · Inspect / Prepare update
────────────────────────────────────────────────────────────────────────
On demand: source passage → result → execution → data/sample → assumptions
```

The evidence display is a readable sequence or table by default. An optional graph can answer a particular dependency question, but a large graph must not become the default home screen.

### 3.2 Five complete journeys

| Journey | Researcher starts with | Pipeline guides them through | Durable outcome |
|---|---|---|---|
| Existing-paper revision | Paper/source folder, optional referee report | Confirm paper and bibliography → inspect claims/concerns → plan scoped work → compare changes and evidence → export response | A reviewed manuscript version, response matrix, unresolved concerns, exact supporting artifacts |
| Empirical or quantitative analysis | Existing script and data | Detect toolchain → declare/capture inputs → run baseline → compare variants → generate assets → stage manuscript updates | Results with execution and sample identity, complete attempt history, accepted baseline and linked publication assets |
| Theory development | Proposition and assumptions | Clarify scope → inspect derivation → enumerate obligations → select checks/counterexamples → compare assumptions → stage argument | Versioned proposition, assumptions, gaps, checks, counterexamples, accepted or unresolved interpretation |
| Literature synthesis | Question, PDFs, DOI/BibTeX/Zotero collection | Resolve candidates → retain exact versions → read/annotate → compare mechanisms/methods/results → draft with passage support | A versioned evidence matrix and prose with inspectable source support and missing-access labels |
| Bounded autonomous investigation | Question, evidence boundary, time/action budget | Prepare first discriminating test → investigate → challenge → decide whether to continue → deliver | Evidence-linked answer, negative result, or precise blocker; usable files and an unambiguous next decision |

Every journey must work incrementally. A researcher can stop after importing a paper, after one baseline run, or after creating one theory note. Existing projects must not be forced through a new setup wizard.

### 3.3 Navigation consolidation

The six areas remain stable. Existing deep links and specialized tools remain available while their primary presentation changes.

| Area | Primary experience | Consolidate behind that experience |
|---|---|---|
| Overview | Resume, research brief, a short attention queue, changes since last visit | Setup and low-level diagnostics remain in Project settings; expand the existing Overview rather than replacing it |
| Library | One source list with Reading, Search, and Intake views | Documents, sources, acquisition, bibliography, literature assessments and collections share source identity; local project files remain a distinct scope |
| Analyze | Experiments, Data, Theory; start from the question or a selected object | Results, captured runs, specification grids and result comparisons become views of the same investigation; execution settings are contextual setup |
| Write | Manuscript, Revision, Deliver | Result links and evidence become inspectors; response letters and campaigns become the Revision experience; publication assets and export remain reachable |
| Automate | Project-scoped automations and a guided builder | Ordinary chains, research missions, discovery portfolios and recurring checks retain their owners but receive coherent entry cards and labels |
| Activity | Decisions, evidence changes, execution history | Machine logs and raw receipts are secondary inspection; do not duplicate the suite Activity center's live monitoring |

Library should not silently equate a file in a project folder with an imported citable source. Analyze should not equate a dataset declaration with executed filtering. Write should not equate a built PDF with inspected pages. These distinctions must survive the simpler UI.

## 4. Product constraints and common contracts

### 4.1 Architecture rules for every work package

1. Read [CLAUDE.md](CLAUDE.md), [Workspace ownership](docs/workbench/README.md), and the package's owner documentation before editing. Recheck the current tree; this plan may be implemented after those files change.
2. Keep Workspace, Reviews and the Tasks coordinator independently owned. Cross-mode associations are app-owned references or immutable exchanges. No frontend feature reads private SQLite tables or writable run folders directly.
3. Add migrations after the then-current last migration. Never edit an applied migration, vendor schema, existing archive identity, or old fingerprint merely to fit a new screen.
4. Exact historical references remain exact. A relation to an old result or paper must never silently follow the newest version. "Update to latest" creates an explicit new association and comparison.
5. Persist a side-effect operation identity before dispatch; use optimistic revisions for mutable choices. Reconcile ambiguous submissions or writes rather than replaying them.
6. Host records of source bytes, execution, adoption, and edit application are distinct from model assertions and researcher judgments. Only owning host services can mint execution or accepted-file provenance.
7. Preserve bounded reads, pagination, cancellation, indexing, and lazy feature loading. Ordinary conversation must not initialize all proposed research services.
8. Keep existing concurrency limits until a separately qualified change replaces them. A UI parallel branch is not permission to run additional native Workspace turns.
9. A prepared plan authorizes only its displayed scope. Avoid repeated confirmations inside unchanged authorized scope; reprepare when inputs, destinations, executable identity, network access, or budget authority changes.
10. External materials and model-generated metadata are untrusted source content. They cannot grant execution, change provider/data policy, mark evidence accepted, or instruct a connector to upload additional files.

### 4.2 Use existing identities before adding new ones

Do not invent a parallel knowledge database. Extend the ledger, project records, desk references, and relation projections through their owning services. The following are proposed **app contracts**, not names of interfaces already implemented:

| Contract | Minimum contents | Owning boundary |
|---|---|---|
| `ProjectAssociation` | Workspace project ID; foreign owner/kind/ID; exact input ref/hash when known; association origin; creation operation; revision/tombstone | Workspace project service, resolving foreign objects through Review/Tasks read adapters |
| `ResearchObjectView` | Existing exact reference; readable title/kind; typed presentation payload; allowed actions; source/execution/acceptance/freshness facts; unavailable/truncated state | Owning research service → typed IPC → object renderer registry |
| `ResearchActionProposal` | Kind/version; project; exact inputs; proposed record/file changes; scientific change category; required scope; estimate; precondition hashes/revisions; originating model/user; state | Workspace research/project services; dispatch delegated to existing owners |
| `CoverageAssessment` | Subject exact ref; named checks; checked scope; unresolved scope; evidence refs; assessor/method; timestamp; freshness | Existing scientific owner, projected for UI; never a universal quality score |
| `ExecutionEnvironmentReceipt` | Executable identity; relevant startup/lockfile hashes; observed version info; seeds; declared dependencies; capture/containment limitations | Workspace execution owner |
| `ExportManifest` extension | Exact objects and blobs; audience/purpose; dependency closure; exclusions with reasons; external requirements; manifest hash | Existing archive/exchange/capsule owner according to purpose |

**Mutations:** a request supplies `operationId`, scope, expected revision(s), and exact input identities. Responses return durable outcome references plus conflicts or incomplete coverage. Reusing an operation ID with changed content fails. A duplicate identical request returns the original outcome where the owner supports idempotence.

**Reads:** resolve each object through its owner; return an explicit `available`, `unavailable`, `excluded`, `partial`, or `unsupportedVersion` result. Pagination is bound to query/snapshot identity. Do not display an empty table as a successful "no issues" result when a service failed.

**Action lifecycle:** `proposed → prepared → approved → dispatched → completed | failed | cancelled | outcomeUnknown`. `rejected` and `superseded` are terminal proposal states. Where an existing owner uses different names, use a presentation mapping rather than migrating its runtime state for cosmetic consistency. Completion of a proposed action does not accept its scientific conclusion or its file changes.

### 4.3 Present independent facts, not one green badge

Use compact, expandable labels for the following separate dimensions:

| Dimension | Examples |
|---|---|
| Source access | Full text retained; abstract only; metadata only; unavailable |
| Origin | Researcher statement; model proposal; imported statement; measured output |
| Researcher decision | Proposed; accepted; disputed; rejected; superseded |
| Execution | Not run; running; completed; failed; outcome unknown |
| Freshness | Exact historical version; current against checked dependencies; changed input; incomplete check |
| Scientific support | Quoted passage; model assessment; numerical instance; refuting example; formal checker result; researcher judgment |
| Coverage | Named checks and their scope; omitted checks; inaccessible evidence |

Unknown is a legitimate state. Missing checks must not be interpreted as passed checks. A formal checker result applies to its exact formal statement and assumptions; correspondence to the manuscript remains a separate obligation.

### 4.4 Visual and interaction requirements

- Extend the existing `gui/src/ui/` primitives. Do not introduce a second theme, navigation shell, modal system, or icon family.
- Use generous reading space and restrained chrome. Body text should normally be 14–16 px with comfortable leading; 12 px is for secondary metadata, not the argument or main form instructions. Validate against actual desktop scaling.
- Give each object view one primary next action. Group secondary actions in a labeled menu. Show provenance on demand, with important access/freshness limitations immediately visible.
- Use list/detail and document/inspector patterns. Turn long creation forms into a contextual editor or short progressive setup. Preserve editable expert detail rather than hiding complexity irretrievably.
- Show units, sample, version and baseline in result headers and charts. Default charts should be exportable scientific figures, not decorative activity charts.
- Provide keyboard navigation, visible focus, named status regions, non-color indicators, reduced motion, and usable narrow layouts. Test 1024×700, 1280×800 and a wide desktop; add 200% text/zoom and dark/light checks.
- Do not require drag-and-drop, hover, a graph, or an AI answer to perform an essential action. All operations have conventional controls.
- Preserve selection and scroll when opening evidence, returning from the assistant, or comparing versions. A missing exact reference opens an explanatory state, never a nearby substitute.

## 5. Priorities and delivery units

Sizes describe relative complexity, not promised calendar time: **S** one owner and limited UI; **M** a few coordinated owners; **L** multiple contracts and a complete workflow; **XL** a new deployment boundary. Each L/XL package must be split into independently reviewable slices.

| ID | Work package | Priority | Size | Required predecessors | Main result |
|---|---|---|---|---|---|
| FA-00 | Rebaseline, qualification fixtures, and acceptance scenarios | P0 | M | None | Trustworthy implementation/evaluation baseline |
| FA-01 | Explicit project associations and exact object navigation | P0 | M | FA-00 | Work stays attached to the right project |
| FA-02 | Typed object views and section consolidation | P0 | L | FA-01 | Research objects become usable rather than generic records |
| FA-03 | Guided project intake and capability setup | P1 | M | FA-01/02 | Useful first session with an existing paper or folder |
| FA-04 | Living argument and evidence coverage | P1 | L | FA-01/02 | Central claims, assumptions, evidence and gaps beside the paper |
| FA-05 | Research-aware assistant actions and context | P1 | L | FA-01/02; FA-04 for scientific actions | Natural-language access to inspectable research operations |
| FA-06 | Library intake, versions, and citation support | P1 | L | FA-01/02 | Fast reading and defensible literature synthesis |
| FA-07 | Execution setup, data adapters, and result receipts | P1 | L | FA-02/03 | Existing code produces usable, traceable results |
| FA-08 | Experiment comparison and publication assets | P1 | L | FA-07 | Complete comparison-to-table/figure workflow |
| FA-09 | Connected manuscript and update review | P1 | L | FA-04/05/08; FA-06 for citations | Evidence stays connected as the paper changes |
| FA-10 | Integrated Review and revision workflow | P1 | L | FA-01/04/09 | Concerns lead to justified, inspectable revisions |
| FA-11 | Guided automation, simulation, and resource control | P1 | L | FA-01/02/05 | Powerful automations without raw identifiers or control syntax |
| FA-12 | Evidence-first autonomous investigations and discovery | P2 | L | FA-04/07/11 | Small useful investigations before large portfolios |
| FA-13 | Proposition and counterexample workspace | P2 | L | FA-04/05/07 | Theory work with explicit obligations and test scope |
| FA-14 | Coauthor handoff, delivery, and project recovery | P1 | L | FA-01/09/10 | Useful portable output and trustworthy recovery |
| FA-15 | Search, decision inbox, and research memory | P2 | M | FA-01/02/04 | Find and resume the right work with less context assembly |
| FA-16 | Optional persistent runner and later remote compute | P3 | XL | FA-11/12/14 plus release qualification | Deliberate background execution beyond the desktop process |
| FA-17 | Scientific evaluation, usability, performance, and release gates | P0, continuous | L | Starts with FA-00; gates every release | Evidence that the product improves research work |

P0 means necessary foundation or qualification, P1 means core product value, P2 means an extension after the first complete journeys work, and P3 means explicitly deferred investment. Priority is not permission to implement every package simultaneously.

## 6. Implementation work packages

### FA-00 — Establish the implementation and evaluation baseline

**Purpose.** Prevent another round of duplicate feature construction or attractive screens backed by incomplete sample data. Separate product work from unresolved release qualification.

**Implementation slices**

1. Inspect the current tree and reconcile this plan's inventory with current owners. Record each proposed package as absent, partial, already implemented, or superseded. Preserve the existing dirty tree. Read the latest release-candidate evidence before making any support claim.
2. Create a fixture catalog for: a rootless theory project; an empirical paper with one changed input; a multi-file quantitative model with IRFs; a literature project with metadata-only and full-text sources; a revision with unresolved objections; and an interrupted automation. Use synthetic or appropriately licensed data and deterministic result receipts.
3. Derive fixture DTOs from production contracts. A missing fixture command must produce an explicit unsupported fixture state or fail the fixture test, not return a vaguely shaped success object. Add the missing Result links and event-unsubscription fixture contracts found during this review. Verify against the native boundary before classifying a fixture error as a production bug.
4. Record starting journey measurements: required manual entries, navigation transitions, time to first useful artifact, incorrect completion claims, and lost-context events. These are local test observations; do not add telemetry.
5. Add a small capability record distinguishing implemented, locally tested, native tested, and release-qualified behavior. Do not expose a giant developer checklist in normal product UI; use it to drive specific unavailable controls and support text.

**Owners.** [Existing full-app fixture](gui/e2e/app-full.tsx), [fixture shim](gui/e2e/app-full-shim.ts), typed clients, [qualification policy](docs/workbench/release-qualification.md), and [release qualification tooling](scripts/release/qualify-release.mjs).

**Acceptance.** All six fixtures open through their real routes and provide valid object/detail data. Unsupported commands fail conspicuously in fixture tests. Each release claim names its evidence class. The existing user-data formats and feature behavior are unchanged.

**Delivery boundary.** This is evaluation infrastructure, not a repository-wide type-generation or component rewrite. Introduce generated/validated DTO fixtures for touched owners first.

### FA-01 — Make project membership and object navigation explicit

**Researcher outcome.** A Review or Automation started from a project stays with that project even when its input is an immutable temporary snapshot, the folder moves, or the project has no folder.

**Current gap.** `ProjectReviews.tsx` filters all runs using folder containment and attaches any collection containing such a run. That is useful legacy inference, but it cannot be the authoritative relationship. Starting a Review from that screen currently routes to the general Review page without an exact project/document request.

**Implementation slices**

1. Add a versioned `ProjectAssociation` table under Workspace ownership, with a unique association identity and optimistic update/tombstone semantics. Store foreign owner and ID, exact input reference/hash when known, association origin (`explicit`, `handoff`, `legacySuggestion`) and timestamps. An association does not move a run or acquire a retention pin by itself.
2. Add small read adapters for Review/Tasks metadata and exact navigation. Implement create/list/unlink/resolve associations through the ordinary Workspace command gate. A foreign unavailable object remains an unavailable link. Do not open or transact against another owner's database from the project store.
3. Carry project ID and exact selected artifact through Review preview, launch request, successful run association, and Automation preparation. Capture association intent before asynchronous dispatch so interruption cannot orphan ownership. Reconcile from the run/action receipt without rerunning the work.
4. Keep path-based inference as **suggested membership** for old data. Present a preview to confirm suggested links in bulk. Never automatically claim all nested-folder runs for multiple projects or include an unrelated run through a mixed Review collection. Root reassignment must not move associations.
5. Add exact object location to the route model: owner, object kind/ID, revision, optional passage/page/result component. Route resolution revalidates scope through the owner. Do not encode a host filesystem grant in a URL.
6. Project Reviews lists associated runs and projects the relevant finding subset. Existing Review collection ledgers remain canonical in their owner; importing findings into Workspace remains an explicit immutable exchange. Unlinking a run changes membership only, not the run or research decisions already copied from it.

**Owners.** [ProjectReviews](gui/src/components/project-surface/ProjectReviews.tsx), [project signals](gui/src/hooks/useProjectSignals.ts), [router](gui/src/lib/router.ts), [project commands](gui/src-tauri/src/workbench/commands/project.rs), [Review handoffs](gui/src-tauri/src/workbench/release.rs), [Tasks adapters](gui/src-tauri/src/orchestration/adapters.rs). Proposed new association modules belong beside the project owner, not in a global service locator.

**Acceptance scenarios.** A rootless project owns a Review of an imported PDF; a handoff with a private snapshot path appears correctly; two projects sharing a folder do not steal each other's runs; mixed collections do not leak unrelated findings; deleted/moved artifacts show unavailable links; back/forward opens the same revision; repeated completion events create one association; unlink never deletes an artifact. Import remaps project identities and marks absent foreign objects explicitly.

**Migration/rollback.** Additive migration; no rewriting old run manifests solely for UI membership. Old clients can ignore associations. Keep legacy collections and routes until association-based navigation is proven and exported references remain resolvable.

### FA-02 — Replace generic records with a coherent research-object interface

**Researcher outcome.** Opening a result shows the estimate, uncertainty, units, sample, and baseline; opening a claim shows its statement, support, objections, and next check. Neither requires understanding its JSON representation.

**Implementation slices**

1. Define a typed view registry over existing reference kinds. Start with result, source, claim/evidence, execution, dataset/sample, review finding, and theory note. Each renderer receives an owner-validated `ResearchObjectView`, not an arbitrary JSON bag.
2. Create reusable object-header, exact-reference chip, evidence list, lineage trail, empty/unavailable state, and contextual action components within the existing UI kit. Raw record export/copy stays under Inspect. Unknown newer kinds fall back to a safe read-only view with an explicit compatibility label.
3. Give each section a useful default: Library source list, Analyze recent investigations/results, Write selected manuscript, Automate project work. Use the same object views when entered from search, a manuscript annotation, the Overview, or a model action card. Avoid a second implementation for each entry point.
4. Replace permanent rows of many specialist tabs with a short section switcher, object list, and searchable secondary tools. Preserve old routes as aliases to their new location. Prefer semantic `Library → Sources` to a new hierarchy of backend names.
5. Open evidence in a transient inspector or replace the current pane while remembering the return location. At narrow widths use one focus pane with clear Back and object title. Retain existing draft and pane-size protections.
6. Introduce source-level error boundaries and per-source load states. Extend Overview's current failure-tolerant behavior with a quiet "Some checks unavailable" detail when the user is relying on a coverage summary. Do not fill thin projects with speculative warnings or empty dashboards.

**Owners.** [ObjectPane](gui/src/components/research-desk/ObjectPane.tsx), [WorkspaceProjectSurface](gui/src/components/WorkspaceProjectSurface.tsx), [navigation map](gui/src/lib/workspaceNavigation.ts), [UI kit](gui/src/ui/classes.ts), [desk/search owner](gui/src-tauri/src/workbench/desk.rs), existing domain DTOs.

**Acceptance.** The same exact result renders consistently from five entry points; a provenance click reaches the correct execution and inputs; unavailable evidence does not replace the whole project with an error screen; one keyboard action returns to the originating selection. No raw IDs or JSON are required for the ordinary result/source/claim journeys. Plain conversation still loads neither the PDF reader nor the object registry's heavy views eagerly.

**Removal condition.** Remove a generic form or sibling tab only after every supported operation has an equivalent reachable action and saved deep links have a compatibility route.

### FA-03 — Make the first useful session easy

**Researcher outcome.** The user can bring an existing project and get to a useful paper, reading task, or baseline setup without first learning Pipeline's internal architecture.

**Entry choices.** Offer four concise starts: **Open a research folder**, **Read or review a paper**, **Start from a question**, and **Explore an example**. Retain quick unfiled conversation and one-off Review. Extend the existing starter kits; do not create a separate project-template runtime.

**Implementation slices**

1. Add a resumable intake draft. For a folder, inventory safely and propose likely main TeX/Markdown/PDF, bibliography, scripts, and data dictionaries. Clearly distinguish candidates from selected authority. Do not execute scripts, expand symlinks, scan excluded folders, or treat detected files as permission to transmit them.
2. Present a short confirmation of main document and project purpose, with optional question/target. Multiple TeX roots or several PDFs require selection; a rootless project remains valid. Defer settings unrelated to the next action.
3. Offer capability checks only for the chosen outcome: local reading, Review provider, conversation connection, or a particular computation toolchain. Use existing account/settings services and live capability catalogs. Do not prescribe installation of every provider or the multi-gigabyte parser.
4. Add an offline demonstration project built from FA-00 fixtures with clear synthetic labels and guided tasks. It must work without a model account. A model-assisted continuation is a separate visible action, subject to ordinary provider configuration.
5. Where importing a PDF, show extraction coverage and a small source preview before the user relies on equations/tables. Failed extraction offers explicit alternative selection and retry through the owning policy; never silently changes method.
6. Save intake progress and let the user return later. Finishing intake creates ordinary project settings/records and a next-step draft, not a hidden automation.

**Owners.** [Home](gui/src/components/HomePage.tsx), [project creation](gui/src/components/WorkspaceProjectDialog.tsx), [Project settings](gui/src/components/project-surface/ProjectSettings.tsx), [starter/delivery owner](gui/src-tauri/src/workbench/programs/delivery.rs), existing dependencies and account controls.

**Acceptance.** A signed-out user can inspect a sample paper and its evidence; a researcher opens a multi-file TeX project and selects its main document without a model call; ambiguous roots are not guessed into authority; interrupted intake resumes; no user file is edited; missing toolchains block only the corresponding action. Initial usability target: a researcher unfamiliar with Pipeline reaches a useful local artifact within five minutes, excluding installation/download time.

### FA-04 — Make the argument and its evidence first-class in the experience

**Researcher outcome.** From the manuscript, answer: what are the central claims, what assumptions do they depend on, what evidence supports or contradicts them, and what would change the conclusion?

**Reuse.** The claim/evidence ledger and relation/impact services already provide identities, decisions, and propagation. Extend them; do not add an independent graph database or replace their trust rules.

**Implementation slices**

1. Add an argument outline that groups existing claims by question or manuscript section. Start with manually selected central claims, then offer a model-generated proposal over exact passages. A claim proposal carries its source span and cannot become accepted through extraction alone.
2. Extend claim-version metadata with optional typed scientific fields. Empirical: estimand, population/sample, identifying assumptions and alternative explanations. Quantitative: mechanism, calibration/estimation targets, equilibrium or numerical conditions. Theory: proposition scope and assumptions. Literature/qualitative: interpretive claim, source scope, contrary evidence. Missing fields remain missing; domain templates are optional.
3. Add **research obligations** attached to claims: reproduce an estimate, examine an alternative sample, justify an assumption, check a cited passage, resolve a derivation gap. An obligation names what evidence would address it and can link to an existing action item, check, experiment or Review. It is not another scheduler.
4. Build a readable evidence matrix: claim → supporting/contrary evidence → assumption → current coverage → unresolved obligation. Separate human judgment, source relevance, numerical validity, and freshness. Do not synthesize a paper-wide truth percentage or publication probability.
5. Expose existing impact traversal from a changed source, dataset, assumption or result. Group consequences by manuscript section and propose a minimal recheck set. When graph limits or undeclared dependencies prevent a complete traversal, retain that limitation in the proposed plan and UI.
6. Keep acceptance local and deliberate: accept/reject individual claim or relation proposals, or a clearly displayed homogeneous batch. An accepted claim is the researcher's current position, not a machine-certified fact. Supersession creates a new version and keeps earlier evidence readable.

**Proposed record detail.** A research obligation should reference a claim version, obligation kind, criterion, exact evidence/check refs, origin, workflow state, and a separately recorded resolving judgment. "Execution completed" cannot fill in "criterion satisfied." Store judgment history and reasons for dismissal/deferment.

**Owners.** [ledger](gui/src-tauri/src/workbench/research/ledger.rs), [relations](gui/src-tauri/src/workbench/project/relations.rs), [project documents/anchors](gui/src-tauri/src/workbench/project/documents.rs), [research panel](gui/src/components/WorkspaceResearchPanel.tsx), FA-02 object views, Write inspector.

**Acceptance.** A changed data vintage identifies the connected estimate/table/claim without marking the conclusion false; a metadata-only citation cannot satisfy a passage-support obligation; a rejected model relation does not propagate accepted impact; a numerical example cannot close a general proof obligation; a truncated traversal says incomplete. Existing accepted notes and claims migrate without fabricated scientific metadata.

**First slice.** One selected manuscript claim, one exact supporting result, one assumption, one outstanding check, and one changed-input path. Ship this before automatic whole-paper claim extraction.

### FA-05 — Let the assistant operate the research workbench through inspectable actions

**Researcher outcome.** "Compare these specifications," "check this citation," or "prepare a response to this objection" produces a useful proposal with named objects and next actions, rather than instructions to copy information between forms.

**Current base.** Dynamic research tools, exact context selections, task-chain proposals, accepted notes and context snapshots already exist. The change is a consistent proposal/action contract and better context assembly, not giving the model unrestricted UI or database authority.

**Implementation slices**

1. Add versioned action proposals for a deliberately small catalog: compare results; propose a claim/evidence link; draft a literature assessment; prepare a captured run; prepare a Review; stage a manuscript revision; create an Automation draft. Each proposal validates project scope, exact refs, argument bounds and supported capabilities through its owner.
2. Render an action card in chat with purpose, selected inputs, expected outputs, material side effects, and **Inspect / Prepare / Dismiss**. Preparing resolves current authority and preconditions. Starting work uses the owning command. Do not create a generic `execute_tool(name, arbitraryJSON)` bypass.
3. Add a persistent **Working on** context chip for the active selection or object. Picking it does not send a message. Display explicit versus suggested context and the actual included/excluded/truncated material when asked. Reference budgets remain enforced by the host.
4. Build context packs from accepted project brief, selected objects, relevant decisions, and declared research obligations. FTS candidates are suggestions; excluded data remains excluded. Do not silently include all open files, unrelated conversations, or every source in a project. Reuse the existing source tray and binding fingerprint rules.
5. For long investigations, propose compact handoff notes with exact evidence references and unresolved decisions. Reuse the existing host-generated handoff inventory; model synthesis is a separate proposal. Do not relabel the inventory as a semantic conversation summary or automatically accept a summary as research memory.
6. Add visible scientific edit intent: **wording**, **exposition/structure**, **argument or assumptions**, **analysis/code**. A wording request defaults to preserving equations, claims and evidence. If the model proposes a substantive change, flag it in a separate group with its implications rather than silently including it.

**Authority and recovery.** Cards are durable proposal records, not executable markup in a response. Validate host-issued proposal IDs and re-resolve exact objects on activation. A changed input or permission invalidates a prepared card; an ambiguous dispatch exposes its owning receipt/reconciliation action. Reopening chat cannot execute a card again. Dismissed or superseded proposals remain identifiable in history.

**Owners.** [research tools](gui/src-tauri/src/workbench/research/tools.rs), [catalog](gui/src-tauri/src/workbench/research/catalog.rs), [context tray](gui/src/components/WorkspaceContextTray.tsx), [conversation rendering](gui/src/components/WorkspaceConversationView.tsx), [task proposal owner](gui/src-tauri/src/workbench/tasks.rs), existing per-turn snapshot resolution.

**Acceptance.** A model can propose a comparison using two exact result IDs and the user reaches that comparison without manual ID entry. A response containing forged action JSON cannot invoke an operation. An excluded dataset cannot appear in a context pack. Changed inputs invalidate preparation. A lost acknowledgement does not cause duplicate work. An accepted prose-only revision leaves equations and substantive values intact or visibly flags the deviation for review.

### FA-06 — Turn the Library into a reading and evidence workspace

**Researcher outcome.** Bring a set of sources into the project, understand which versions are present, read them, compare their contributions, and inspect support for a citation without repeating metadata entry.

**Implementation slices**

1. Unify intake into one queue for local PDFs, DOI lists, BibTeX, and existing Zotero metadata import. Support batch preview, duplicate candidates, per-item progress, extraction quality, retry and exclusion. Keep each source's access state and receipt visible. Start with batches of at most 100 items and bounded background processing; retain current file/byte/page limits until separately measured changes are justified.
2. Introduce work-level grouping over existing immutable source versions. Grouping is a user-approved relation, not a replacement identity. DOI match, title/author similarity, and Zotero item identity can propose equivalence; they cannot merge passages or overwrite citation keys. A working paper and published article may share a group while their assessments remain version-specific.
3. Extend the local Zotero adapter to **selected attachment import and explicit refresh**, following then-current official APIs. Preview exactly what will be copied and retain library/item/version identifiers. No direct Zotero-database writes. Start read-only and local; remote credentials, push-back and two-way synchronization are later separate scopes.
4. Improve reading around the existing PDF/text readers: source metadata, outline, highlight/region annotations, source-specific notes, related versions and reading state. Selecting a passage should prefill a literature assessment or supporting/contradicting evidence proposal with exact offsets. Reuse the existing PDF/byte-anchor mechanisms.
5. Upgrade the existing literature comparison table to a question-specific matrix. Editable columns may include question, mechanism, method, population, key result and limitation; every substantive cell can retain a source passage and access scope. Distinguish researcher text from model suggestions. Bulk synthesis creates proposals and cites the captured editions.
6. Add manuscript citation inspection: resolve a literal key, show duplicate/unresolved identities, open the cited edition/passage, and record whether it supports the particular sentence. Metadata correctness, quotation accuracy and substantive support are separate checks. Bibliography cleanup stages a diff through normal manuscript acceptance.
7. Search additions should be adapter-based and justified by the task. Crossref remains useful for metadata; a second scholarly metadata/full-text resolver is a later bounded adapter chosen after verifying coverage and terms. Never describe an abstract as having read the paper; never bypass a publisher's access controls.

**Owners.** [acquisition](gui/src-tauri/src/workbench/acquisition.rs), [sources](gui/src-tauri/src/workbench/research/sources.rs), [literature service](gui/src-tauri/src/workbench/project/studio/literature.rs), [Literature UI](gui/src/components/research-studio/Literature.tsx), Library intake and FA-02 source views.

**Acceptance.** Import a mixed set with duplicates, two versions of one work, a failed PDF and an abstract-only source; every item has the correct identity/access state and can be retried independently. Exact quotations remain tied to their original edition after refresh. Conflicting BibTeX keys never get silently renamed. A synthesized matrix distinguishes evidence from interpretation and can be exported with its source manifest.

**Deliberate exclusions.** No universal literature-coverage claim, unlimited unattended crawler, citation-count quality ranking, or required hosted semantic database. Expand search sources after the reading/support workflow works.

### FA-07 — Make existing research code easy to run and its outputs easy to use

**Researcher outcome.** Select an existing script, understand the execution setup, run it with declared inputs, and obtain structured results that can enter comparisons and writing.

**Implementation slices**

1. Build an execution setup assistant around existing tested profiles and captured plans. Detect candidate entry scripts and known environment files without executing them. Ask for entry point, inputs, working directory and expected outputs using file/object pickers. Preview the actual command and access boundary before authorization/testing.
2. Package the existing Python, R, Julia and Stata result-export examples as versioned, inspectable snippets with minimal examples. Offer to stage an exporter call in a task copy; never rewrite the accepted analysis invisibly. Results are adopted from the exact completed execution, with host-owned provenance as today.
3. Add guided metadata extraction for common formats, initially `.dta` and Parquet through an installed, explicitly selected reader/toolchain. Preserve value labels, missing-value distinctions, units and declared types where available. Use bounded column/row previews. Large files can remain external with a strong immutable reference/hash and a declared or measured dictionary. Do not load them wholesale into the webview or model context.
4. Extend sample declarations with optional **executed sample receipts**: source dataset hashes, evaluated rule/script identity, rows included/excluded by step, missingness decisions, weights, date range and a membership digest where feasible. A prose filter declaration never becomes an executed receipt automatically. Compare declared versus observed sample facts without presuming the declaration is correct.
5. Add `ExecutionEnvironmentReceipt`: observed executable/tool version, selected environment and lockfile hashes, relevant startup-file identity, seeds/parameters and dependency-coverage limits. Avoid reading or retaining secret environment values. A package inventory is explicit optional evidence, not a requirement to install or fully recreate every environment.
6. Propose a versioned extension of the structured-results contract for labeled coefficient vectors, covariance matrices, diagnostics and trajectories. Preserve v1 scalar and v2 IRF readers. Validate dimensions, unique labels, finite-or-missing values, uncertainty metadata, estimand/transformation/sample identity, and covariance consistency. Do not infer causal interpretation or confidence levels from field names.
7. Separate execution modes clearly: existing host execution with declared captures; later optional contained execution where actually supported. The current captured plan explicitly has host access and declared-only dependency coverage. A fresh directory and file hashes are not a sandbox or a complete environment capture.

**Machine-specific requirement.** On this Mac, the current user instruction is `/bin/zsh -lic 'stata …'`, ending Stata with `exit, clear`. Several existing research documents/exporter instructions still refer to `oldstata`; inspect the actual adapter and reconcile that mismatch in a focused compatibility change before qualifying the new setup flow. Do not copy an obsolete clock-changing wrapper into new code or invoke a Stata binary directly. On another machine, confirm its local convention. All Git invocations by an implementing agent follow the machine's explicit outside-sandbox zsh rule.

**Owners.** [execution](gui/src-tauri/src/workbench/research/execution.rs), [captured plans](gui/src-tauri/src/workbench/research/execution_plan.rs), [jobs](gui/src-tauri/src/workbench/research/jobs.rs), [data](gui/src-tauri/src/workbench/data/mod.rs), [structured result consumers](gui/src-tauri/src/workbench/project/studio/results.rs), [exporter examples](examples/research-exporters/README.md), Analyze setup.

**Acceptance.** One real supported toolchain runs the fixture from a captured script and adopted results enter the UI; changing the live script leaves the historical run reproducible from its capture; package/startup changes invalidate applicable grants; failed jobs cannot adopt stale preexisting outputs. Test `.dta` extended missing values and labels, Parquet nullable columns, a sample mismatch, covariance dimension failure and an external-data dictionary. Unsupported tools remain explicit, with no auto-installation.

**First slice.** One Python script and the current machine's Stata path, scalar/vector results, and one measured sample receipt. R/Julia/native format adapters ship only after their real-tool fixtures pass; the UI must not equate a provided snippet with qualification.

### FA-08 — Build an experiment workspace around complete comparisons

**Researcher outcome.** See what changed between specifications and what the change means for a stated research question, then produce a table or figure from the selected results without assembling it in a separate form.

**Implementation slices**

1. Combine experiment definition, captured runs, specification family, comparison and generated assets in one investigation view. Keep existing immutable experiments and family records; use relations rather than duplicating executions into a new store.
2. Replace JSON-like factor values and exclusions in the normal path with typed controls: numeric sequence/list, category, boolean, date/vintage or sample reference; explicit exclusion conditions and reasons. Preview the complete Cartesian expansion and exclusions before preparation. Keep an advanced serialized view for expert portability.
3. Add an aligned result table with sticky specification labels, baseline choice, estimates/uncertainty, sample size, fit/convergence diagnostics, units and execution outcome. All attempted, excluded, failed and missing rows remain accessible. Filters must report hidden rows; default ordering follows declared scientific comparisons, not favorable significance.
4. Offer coefficient/IRF overlays, sensitivity plots and specification curves from compatible result contracts. Chart selection opens exact specification, sample, execution and source data references. No interpolation, unit conversion, or uncertainty construction happens without an explicit recorded rule.
5. Baseline selection is an explicit versioned decision with a reason. Running a better-fitting or more significant specification never changes the baseline. Record whether a family was planned before its results or added afterwards; do not claim formal preregistration merely because a local timestamp exists.
6. A selected comparison creates a publication table/figure specification through the existing assets service. Preview formatting, rounding, labels, sample notes and source coverage; retain unrounded source values. Regeneration produces a new asset version and a staged manuscript change, not an overwrite.
7. Add targeted rerun suggestions from FA-04 impact. Reuse only exact validated inputs and applicable execution semantics. Skip a computation only when a documented equivalence rule covers scripts, dependencies, parameters, environment and stochastic policy; otherwise show a candidate reuse choice rather than silently caching a scientific result.

**Owners.** [Experiments UI](gui/src/components/research-programs/Experiments.tsx), [experiment family owner](gui/src-tauri/src/workbench/programs/experiments.rs), [Studio results](gui/src-tauri/src/workbench/project/studio/results.rs), [assets](gui/src-tauri/src/workbench/programs/assets.rs), [plot adapter](gui/src-tauri/src/workbench/programs/plot.py).

**Acceptance.** A family with success, timeout, failure and exclusion is visible and exportable in full. Incompatible units/estimands cannot be silently overlaid. Missing intervals stay absent. Switching the baseline creates a decision and leaves earlier figures unchanged. A table caption links to the exact comparison. Figure exports pass visual inspection at print scale, including grayscale, labels, legends and missing observations.

**Scientific limit.** A specification curve describes the declared set; it does not cure specification search or justify a preferred estimator. Diagnostics and uncertainty remain distinct from economic interpretation.

### FA-09 — Connect the manuscript to the research that supports it

**Researcher outcome.** Select a sentence, equation, table or figure and inspect the evidence behind it. When an input changes, prepare a coherent update and review its scientific and textual consequences together.

**Implementation slices**

1. Add a manuscript evidence inspector using the existing source/PDF selection and exact anchor machinery. It should show linked claims, numerical values, citations, assets, assumptions, Review concerns and coverage for the selected passage. Source/PDF synchronization continues to use exact build receipts and SyncTeX where available.
2. Make numeric and citation linking contextual. Selecting a printed number offers candidate exact results with compatible units, component and rounding; selecting a citation offers the resolved source edition and recorded support. Candidates are proposals. Ambiguous repeated values, transformed quantities and macro expansion require explicit resolution.
3. Extend the current single-value macro and asset staging flows into a **manuscript update proposal**: changed dependency, new computation/result, proposed number/table/figure edits, affected claims, possible prose implications, and required checks. The proposal freezes all source and destination hashes.
4. Present source diff, readable prose comparison, scientific change summary, and exact old/new PDF where available. Distinguish actual value/assumption changes from rewording. The summary is advisory; the exact file diff remains authoritative.
5. Apply selected changes through the existing reversible application journal. Partial acceptance invalidates checks against the unaccepted combination, as today. Rebuild and recheck the accepted combination. On external edits, stop with a conflict comparison rather than overwriting or rebasing by guesswork.
6. Add a manuscript preparation view using existing build diagnostics, asset dependencies, citations, unresolved claims and review coverage. Show named checks with dates/versions and export limitations. Provide actions to fix each specific problem; do not invent a universal "publication ready" certification.
7. Add writing instructions scoped to this project/deliverable and edit intent, with the current agent-profile system as the source of instructions. Preserve the user's academic voice; make author style a configurable instruction layer, not a hardcoded house style or silent prompt rewrite.

**Owners.** [file workspace](gui/src/components/file-workspace/FileWorkspace.tsx), [source editor](gui/src/components/file-workspace/SourceEditor.tsx), [Bindings](gui/src/components/research-studio/Bindings.tsx), [manuscript service](gui/src-tauri/src/workbench/project/studio/manuscript.rs), [project tasks/acceptance](gui/src-tauri/src/workbench/project/tasks.rs), assets and result owners.

**Acceptance.** Change one calibration parameter in a fixture, rerun the relevant capture, and prepare a new table plus two printed values and a caveated prose proposal. Verify exact old/new inputs, preserved unrounded values, conflicting external edits, partial acceptance, build failure and undo. A failed new build never presents the old PDF as new. Untouched citations/claims retain exact historical references. A rootless manuscript can inspect evidence and export proposals without acquiring folder-write authority.

**First slice.** Trace and update one reported estimate and one generated table. Do not begin with a WYSIWYG editor, automatic rewriting of every number, or a whole-document semantic diff engine.

### FA-10 — Turn Reviews into a complete revision workflow

**Researcher outcome.** A concern becomes a decision, a concrete investigation or edit, and a response tied to evidence. The user can disagree with a reviewer without disguising disagreement as resolution.

**Implementation slices**

1. Introduce a Review brief in the launch preview: purpose, target artifact/version, audience, selected scope, known concerns, available supporting artifacts and resource limits. Keep the existing automatic specialist routing and deterministic workflow execution. Advanced users still edit the Workflow directly.
2. Present an extraction/coverage preview for the exact input: pages/sections available, equations/tables that require inspection, inaccessible attachments, and the chosen extraction policy. Enable a user to proceed with disclosed limited scope where the existing engine supports it; do not turn extraction failure into silently valid input.
3. Build a revision queue from the existing canonical finding exchange, response records and campaign rounds. A row shows the concern, exact evidence/anchor, researcher disposition, next action, and verification status. Related findings can be grouped through recorded links; preserve independent finding identities and disagreement.
4. Add a **Revision work item** view connecting concern → decision → proposed task/check → captured result → accepted change → response paragraph → re-review. It is a presentation/composition of existing records. Keep Workflow issue annotations and Workspace scientific decisions independent.
5. Supply action proposals from FA-05: locate the passage; challenge the premise; prepare a robustness test; draft a scoped edit; draft a reply from completed work. A model may propose a response but cannot assert an experiment ran because its text says so.
6. Re-review only the selected changed passages when requested, carrying declared dependencies and an explicit coverage statement. Offer a full Review when changes affect central claims, assumptions, identification or broad manuscript structure. Absence from a later scoped report cannot close an earlier concern.
7. Display reviewer disagreement and validation failures. Merge duplicate concerns conservatively and retain rejected/unsupported reviewer comments. Reuse the existing calibration-from-judgments proposal as an optional reviewable prompt change; never silently train the reviewer to stop raising inconvenient substantive issues.

**State distinction.** `open`, `working`, `awaitingEvidence`, `readyForJudgment`, `addressed`, `disputed`, `deferred`, and `dismissed` may be useful presentation labels. Map to existing states or deliberately version additions; do not conflate them with run completion. `addressed` requires the researcher's judgment plus visible evidence/check status, not an automatic model verdict.

**Owners.** [ReportWorkspace](gui/src/components/ReportWorkspace.tsx), [IssuesTable](gui/src/components/IssuesTable.tsx), [project issue ledger](gui/src/components/ProjectIssueLedgerPanel.tsx), [responses UI](gui/src/components/research-studio/Responses.tsx), [review bridge](gui/src-tauri/src/workbench/project/studio/review.rs), [campaigns](gui/src-tauri/src/workbench/programs/campaigns.rs), Review preview/extraction owners.

**Acceptance.** Import the same finding exchange twice without duplicating work; handle a changed finding version without losing the researcher's decision; retain a disputed premise; export a response letter with exact manuscript/results references. A reply claiming added analysis is flagged when its execution is absent or stale. A scoped re-review cannot close an unexamined concern. Review provider failure remains distinct from scientific criticism.

### FA-11 — Make powerful automation understandable before it runs

**Researcher outcome.** Explain what should happen, inspect a comprehensible plan, see its possible branches and limits, and know why work stopped or is waiting.

**Current base.** A real outline/condition editor exists. Do not replace it with another graph builder. Complete the research-object selection, input types, budget presentation and runtime explanation around it.

**Implementation slices**

1. Add **Describe the automation** as an optional authoring route using the existing proposal catalog and tools. A model proposes the existing declarative chain format; the host validates it and renders the ordinary outline. The user can edit the outline without another model turn. Natural language never becomes executable control flow.
2. Replace normal-path raw check/plan identifiers, JSON named inputs, and manually typed bindings with project-scoped selectors and typed input forms. Include captured checks in the add-step catalog only when a valid exact plan is available. Keep raw JSON import/export under Advanced and round-trip it losslessly.
3. Ship a small curated recipe set as ordinary versioned definitions: review an existing draft; prepare a revision with a stopping limit; rerun selected changed results; maintain a literature query; investigate one claim; and build a submission bundle. Each recipe names required capabilities and its evidence/output contract. Do not add another recipe execution engine.
4. Add a deterministic **Preview paths** mode: resolve known inputs, list reads/writes/provider uses, show each branch as reachable/unreachable/unknown, describe waits, and calculate conservative action/fan-out/retry bounds. Unknown conditions remain unknown. This mode performs no model, compute, network-acquisition or file-write action.
5. Provide a bounded synthetic test-input trace for conditions and loops, clearly labeled simulation. Validate missing fields/types, output availability, stop rules and failure cases. Keep logs separate from real run receipts.
6. Improve resource control in existing owners: named maximum actions, active time, elapsed deadline, review/retry limits, and optional monetary limit only where enforceable accounting exists. Reserve allowances before dispatch and count child work consistently. Subscription "API-equivalent cost" remains an estimate, not a bill or spend ceiling. Unknown usage is unknown, not zero.
7. Add typed optional-step failure policy only as an explicit versioned engine extension after UI need is demonstrated: halt for attention by default; skip with recorded reason or use a declared fallback for selected recoverable failures. Scope denial, budget exhaustion, uncertain side effects and invalid evidence cannot be bypassed with a generic continue-on-error option.
8. Improve monitoring: current purpose, exact child owner, what it is waiting for, last durable output, next decision and remaining budget. Explain that a "side by side" plan may queue behind the current single-Workspace-turn limit. Pause/Stop retain existing owner-specific semantics.

**Prepared identity.** The prepared plan fingerprint includes definition version, selected references, capabilities, profile versions and limits. A changed plan requires new preparation. Reusing a prepared scope must not silently select the newest Workflow profile or manuscript.

**Owners.** [Builder](gui/src/components/tasks/Builder.tsx), [OutlineEditor](gui/src/components/tasks/OutlineEditor.tsx), [ConditionEditor](gui/src/components/tasks/ConditionEditor.tsx), [task client](gui/src/lib/taskClient.ts), [definition](gui/src-tauri/src/orchestration/definition.rs), coordinator validation/state/adapters, existing Review usage accounting.

**Acceptance.** A user creates a review/revise/wait workflow without JSON or IDs; a synthetic missing-output case reaches attention; a stop rule is never satisfied by malformed prose; nested work and retries honor reservations; a schedule with a changed source requires scope review; pause/stop/restart preserve once-only adoption. A preview makes zero model/tool calls and cannot mint an execution receipt.

**Dependency caution.** Natural-language plan drafting may ship before hard monetary caps. The UI must not imply that a displayed estimate is an enforced cap. Hard caps require conservative reservation and reconciliation at each provider call boundary, including repairs/retries, in the owning runtime.

### FA-12 — Develop autonomous research around evidence and decisions

**Researcher outcome.** Delegate a useful investigation with a modest starting commitment, obtain evidence before extensive drafting, and expand successful work deliberately.

**Current base and concrete change.** Discovery defaults currently request 75 candidates, a shortlist of 10, five papers and up to 500 managed actions; native validation accepts 50–100 candidates. Missions already retain goals, investigations, challenges and negative results. Preserve that machinery, but make a small supervised investigation the default discovery experience. This is an intentional product change, not a claim that the existing system lacks limits or independent review.

**Implementation slices**

1. Add plain entry choices under Automate: **Investigate a question**, **Explore possible projects**, and **Develop selected projects**. Keep existing modes accessible under those choices and preserve their stored compatibility names. New users should not need to interpret "Full self-discovery" before trying a bounded task.
2. Introduce versioned presets for new definitions. Proposed starting preset: five candidate directions, shortlist at most three, develop one only after selection, two investigation rounds, and an explicit small action budget. Larger portfolio settings remain expert options. Because the native minimum is currently 50, change the definition version/validator and fixtures deliberately; never just lower a frontend slider while leaving the backend contract unchanged. Old running/prepared definitions retain their recorded behavior.
3. Add a feasibility checkpoint before development: exact evidence already available; data/source access still required; tools/license needs; first discriminating test; cost/time bound; reasons the project may fail. Acquisition/computation scope is part of the prepared proposal and is not inferred from an ambitious topic prompt.
4. Make shortlisted briefs editable before selection. Changes create a new brief/version and require reassessment of affected feasibility and scope. Bind selection to the reviewed shortlist hash, as the current system does. A timeout does not select a project for the researcher.
5. Extend the existing research-contract/evidence-dossier approach so investigation can finish as a counterexample, failed replication, inconclusive estimate, narrowed conjecture, or inaccessible-evidence report. Draft a full manuscript only when the evidence supports that deliverable and it was requested. Negative results remain first-class outputs.
6. Keep the existing independent challenge and frozen manuscript review. Add challenge strategies chosen by the claim type: alternative explanation, boundary case, sample/specification change, contradictory source, numerical stability or dimensional consistency. Similar-model agreement is not independent empirical confirmation.
7. Stop or replan when required evidence is unavailable, investigations repeat without information gain, outputs do not change, budget is exhausted, or scientific criteria remain unmet. A proposed larger experiment or new external access becomes a specific decision, not an invisible expansion.
8. Deliver a readable brief with conclusion, strongest evidence, strongest remaining objection, tested domain, failed attempts, exact artifacts and a recommended next decision. Optionally adopt selected outputs into the project through FA-01/04/09 proposals. Mission completion still does not accept manuscript edits.

**Owners.** [missions](gui/src-tauri/src/orchestration/missions), [discovery](gui/src-tauri/src/orchestration/discovery), [Workspace mission adapter](gui/src-tauri/src/workbench/missions.rs), [discovery adapter](gui/src-tauri/src/workbench/discovery.rs), [discovery UI](gui/src/components/self-discovery), [mission UI](gui/src/components/research-missions).

**Acceptance.** Compare the small supervised preset and current portfolio mode on the same benchmark questions. Require retained negative results, exact evidence links, no fabricated access, no unsupported central claim labeled established, and correct stop/recovery behavior. A brief edit invalidates the prior selection identity. A new tool or data source cannot run under the old grant. The useful outcome may be one well-supported rejection of a hypothesis, not a paper.

**Rollout.** Experimental opt-in until FA-17 establishes research usefulness. Keep large unsupervised portfolios available to experienced users with explicit limits and qualification status; do not market generated paper count as a quality metric.

### FA-13 — Give theory research a proposition-centered workspace

**Researcher outcome.** Develop a proposition with its assumptions, derivation, unresolved steps and checks in one place; compare an alternative assumption without confusing a numerical example with a proof.

**Implementation slices**

1. Compose existing theory notes, symbols, assumption branches and checks into a proposition view. Show statement and domain, assumptions, dependency propositions, derivation, open obligations, attempted approaches, counterexamples and manuscript locations.
2. Add typed proof-obligation records referencing exact theory-note revisions. Examples: existence, uniqueness, regularity, sign, boundary case, limit interchange or dimensional consistency. The user can enter them directly; a model can only propose them. Keep derivation prose as ordinary editable source/note content.
3. Add actions to prepare a symbolic identity check, bounded numerical search, comparative-statics experiment or parameter-boundary check using FA-07 captured execution. A proposed script remains reviewable and receives no host authority through its mathematical description.
4. For a counterexample search, record tested domain, sampling/search method, parameter bounds, seeds, tolerances, solver/convergence diagnostics and witness artifact. "No counterexample found" remains a statement about the tested search. A failing numerical solve is not itself a refutation.
5. Improve notation collision suggestions by pairing the existing lexical candidates with user-confirmed scope/definitions. Show the two conflicting usages in context. Macro expansion and mathematical equivalence remain limitations unless an explicitly qualified parser/checker supports them.
6. Support alternative assumptions as a comparison of the original proposition, changed assumptions, affected proof obligations and resulting checks. Do not automatically inherit acceptance. Stage selected derivation/manuscript changes through FA-09.
7. An optional formal-checker adapter is later work. Its record must include exact formal source, imports/tool version, assumptions/axioms and result; the prose-to-formal correspondence stays separately reviewed. No general "proof verified" label based on model agreement or numerical evidence.

**Owners.** [Studio theory](gui/src-tauri/src/workbench/project/studio/theory.rs), [program theory](gui/src-tauri/src/workbench/programs/theory.rs), [Theory UI](gui/src/components/research-studio/Theory.tsx), [symbols/checks UI](gui/src/components/research-programs/Theory.tsx), FA-04 obligations and FA-07 execution.

**Acceptance.** A plausible false comparative-static claim produces a retained counterexample with a checked domain and witness; a failed optimizer remains inconclusive; changing a regularity assumption marks dependent obligations for review without overwriting the original; a numerical instance cannot close a universal proof obligation. Exported notes retain assumptions and unresolved gaps alongside polished exposition.

### FA-14 — Make delivery, coauthor handoff, and recovery understandable

**Researcher outcome.** Prepare the right material for a coauthor, a journal, a replication exercise, or a new machine, and understand exactly what is included.

**Three product actions.** Present **Send a research snapshot**, **Prepare a replication package**, and **Back up my research**. These are distinct purposes over existing `.pwex`, `.pwrc`, `.pwrx` owners and any deliberately versioned outer package. Do not replace their different import policies with one generic ZIP importer.

**Implementation slices**

1. Build a deliverable preview over existing manuscript/assets/deliverable services. Select audience and main artifact, then show included sources/assets, response letter, evidence appendix, reproduction instructions and exclusions. Mark draft or incomplete checks plainly. A submission bundle is local preparation; uploading to a journal is not an automatic next step.
2. Add coauthor review tasks tied to exact manuscript/claim/result versions: question, requested decision, comment and selected change. Export a human-readable brief plus selected existing exchange objects. Coauthors can review without running an agent or granting access to the exporting machine.
3. Improve round-trip import around the existing ancestor/conflict support. Preview base/local/incoming only when a real base exists; preserve author/source attribution; keep unresolved comments tied to their original revision; propose remapping to a new passage rather than guessing. Imported executable settings remain inert.
4. Add selectable evidence appendices and replication manifests to the deliverable builder. Use the existing capsule numeric tolerances and exact plan binding; distinguish executed-unchecked, selected values matched, mismatch and incomplete. Include all specified failures/exclusions where they affect interpretation, not only the best result.
5. Offer a **project recovery preview** that lists Workspace records/blobs, conversation working files, external project folders, Review runs, Automation definitions/history, and excluded credentials. Current `.pwrx` intentionally excludes several of these; the new flow must not imply it already covers them.
6. If complete project backup is implemented, add a versioned outer manifest that orchestrates separately owned exports. Quiesce relevant writers through owner gates, capture association/manifest identities, record component completion, and publish the package atomically only after hashes/closure are verified. No cross-store transaction or raw SQLite-file copying. If a consistent component cannot be obtained, return an incomplete preview rather than a success-labelled partial backup.
7. Restore into staging/appropriate destinations through each owner's importer. Validate all components and space requirements before publishing references; journal partial progress and offer recovery. Existing `.pwrx` still restores into an empty store; `.pwex` still has selective conflict semantics. Restored schedules, runtime bindings and execution grants stay inactive. External paths require explicit remap/detach, never foreign machine authority.
8. Provide optional Word-oriented manuscript/response export and slide-outline export only after TeX/Markdown/PDF bundles are solid. Start with one-way conversion and explicit unsupported constructs; preserve original sources. Do not promise round-trip Word/Overleaf synchronization or silently flatten equations/citations.

**Owners.** [delivery](gui/src-tauri/src/workbench/programs/delivery.rs), [capsules](gui/src-tauri/src/workbench/programs/capsule.rs), [exchange](gui/src-tauri/src/workbench/release/exchange.rs), [archives](gui/src-tauri/src/workbench/release/archive.rs), [retention](gui/src-tauri/src/workbench/release/retention.rs), Review run owner, coordinator export/import adapters and Write delivery UI.

**Acceptance.** A coauthor package is readable without Pipeline; round-trip comments preserve exact source versions and actual conflict ancestors; omitted proprietary data has usable instructions and hashes without raw rows. Restore a complete test project into another disposable location and open its paper, linked result, Review and inactive Automation. A missing/corrupt component prevents a complete-success claim. Test older packages, size limits, symlinks, interrupted publication, remapped roots and disk-full failure. Recovery never reruns work.

**First slice.** Deliverable preview and coauthor response round trip. Whole-project recovery is a separate storage milestone with its own destructive/crash qualification; do not hold basic manuscript export hostage to that larger change.

### FA-15 — Help the researcher find, decide, and resume

**Researcher outcome.** Find a source passage, result, prior objection or research decision quickly, and understand what needs attention without checking many panels.

**Implementation slices**

1. Extend the existing command/view picker into a scope-aware palette with **Go to**, **Find**, and **Do** modes. Search the existing FTS index first, with exact citation/key/path lookup and object-kind filters. Return readable hits and exact revision chips. A navigation command and a model action must remain visibly different.
2. Add optional cross-project search through owner-bounded queries, off by default for model context. Show project scope on every result; opening a hit does not authorize copying it into the active conversation. Exclusion/deletion policy invalidates projections and any cached snippets.
3. Introduce a project decision inbox projected from existing note/evidence proposals, response judgments, changed dependencies, and automation questions. Group duplicate causes while preserving owner IDs. Acknowledge, dismiss, resolve, pause and stop remain separate actions. Acknowledging a stale result does not make it current.
4. Improve suite Activity into Running, Needs attention and Finished views with clear ownership and project filters. Reuse its current adapters. Avoid duplicating history stores; retain bounded pagination and event/focus-driven refresh. Hidden panels should not poll all research stores indefinitely.
5. Keep **Since you were last here** grounded in the existing derivations. Add links to the grouped causes and exact changed objects. If a service could not check current state, offer retry and show its last successful timestamp where relevant rather than implying a clean project.
6. Offer accepted decision/method reuse through an explicit preview: original question, scope, source evidence, limitations and reason for reuse. Keep failed approaches searchable. Reusing a method does not import its old authority, acceptance of new conclusions or executable grants.
7. Local semantic retrieval is an optional later enhancement only after exact/FTS search is evaluated. If implemented, it must be opt-in, rebuildable, versioned by embedding model, scoped by current policy, and deletable. Default operation must work without a remote embedding service. Similarity is retrieval, not evidence of support.

**Owners.** [search](gui/src-tauri/src/workbench/search.rs), [tool picker](gui/src/components/WorkspaceToolPicker.tsx), [Activity](gui/src/components/ResearchActivity.tsx), [Overview](gui/src/components/project-surface/ProjectOverview.tsx), mission method records, existing attention and notification owners.

**Acceptance.** Search finds an exact equation-adjacent passage, old negative result and citation key with correct versions; excluded data disappears from search/context after invalidation; a cross-project hit cannot silently enter another project's assistant context. Repeated monitor events produce one pending decision per owner occurrence. An acknowledged warning can recur for a genuinely new change. Focus/keyboard and back navigation preserve the originating view.

### FA-16 — Extend background execution only through a qualified owner

**Researcher outcome.** Eventually allow long research jobs to continue when the desktop UI is closed, and optionally submit expensive computation to a controlled remote environment.

**Priority and boundary.** Defer this until the local journeys and recovery model work. The existing app-open scheduler and optional tray keepalive are valuable; label them accurately now. Do not detach a child process and call it a durable service.

**Stage A: persistent local runner**

1. Define one per-user coordinator owner with authenticated local IPC, owner leases, protocol/version negotiation, upgrade/drain behavior, startup/shutdown policy and explicit installation/removal controls. Move coordinator ownership deliberately; never let desktop and service independently dispatch the same action.
2. Keep Workspace and Review execution adapters separate inside that ownership design. Serialize current native limits, preserve per-action process trees, store gates and permission identities, and avoid concurrent unsupported account homes.
3. Require explicit background availability/credential policy. Tokens remain in the existing account owner; no credentials in queue definitions or exports. Locked-session and login/logout behavior must be tested on each supported OS.
4. Reconcile sleep, wake, restart, service crash and missed schedules. Wall-clock deadlines, active execution budget and scheduled occurrence identity remain distinct. OS wake support is a separate opt-in capability, not implied by service installation.
5. The UI shows runner connection, queued/running work, last heartbeat, evidence receipt and exact stop authority. Loss of IPC means unknown/connecting, not finished or automatically retried.

**Stage B: remote computation adapter**

Start with one explicitly supported execution mechanism, such as a configured SSH/batch-cluster target, selected after actual researcher demand. Send only a reviewed capture with declared data policy; retain remote job identity, environment receipt, artifact hashes and transfer journal. Reconcile disconnections without double submission. Local cancellation requests remote cancellation and waits for a confirmed or unknown outcome. Licensed tools, restricted data, resource/account limits and remote deletion are explicit scope. A remote job is not a remotely shared mutable Workspace store.

**Owners.** [Tasks coordinator](gui/src-tauri/src/orchestration), existing native process/account primitives, [jobs](gui/src-tauri/src/workbench/research/jobs.rs), execution adapters and packaging/service tooling introduced only for this milestone.

**Acceptance.** Fault-inject before and after submission acknowledgement, receipt publication and reconnect; prove one dispatch and one adoption. Exercise quit, reboot, user logout, upgrade, exhausted budget, network partition and stop on macOS/Windows/Linux where claimed. Do not broaden advertised support until actual installer/service tests pass. No promise of concurrent native research teams is part of Stage A.

### FA-17 — Evaluate research usefulness as rigorously as runtime correctness

**Purpose.** Make "powerful and easy to use" a testable product claim. More features, more tokens, more papers and more passing unit tests are not sufficient evidence.

**Implementation slices**

1. Build a small maintained benchmark from FA-00 fixtures plus consented real projects. Include known positive, negative, ambiguous and inaccessible-evidence cases. Store expected facts, allowed uncertainty, common misleading conclusions and exact source/result references. Keep task evaluation separate from prompt authoring to reduce overfitting.
2. Test the five journeys with researchers in at least three modes: empirical/quantitative, theory, and literature/qualitative work. Begin with 5–8 participants across those modes for formative usability; treat that as issue discovery, not a statistically representative study. Include someone unfamiliar with the product.
3. Compare against the present Pipeline workflow on the same tasks. Measure researcher time, manual data entry, context switches, evidence-trace success, rejected/incorrect suggestions, task completion and recoverability. Record model versions, prompts, input snapshots and repeated trials for nondeterministic comparisons.
4. Use blind expert judgments for substantive quality where feasible. Reviewers should inspect exact evidence and criteria, not generated confidence scores or author identities. Report disagreement and uncertainty. Autonomous work needs separate scores for usefulness of the question, feasibility, execution, evidence, conclusion and limitations.
5. Add adversarial scientific fixtures: attractive unsupported novelty claim; wrong source edition; abstract/full-text confusion; post-treatment control; sample attrition; mislabeled units; incompatible uncertainty; non-converged model; false theorem with a boundary counterexample; missing referee-requested analysis. Test appropriate uncertainty, not only successful happy paths.
6. Add fault-oriented native cases at persistence and authority boundaries, plus realistic large-project performance and accessible UI checks. Keep deterministic tests, no-model protocol probes, real-tool runs, authenticated model tests and packaged-platform tests separately reported.
7. Record dated evidence in the owning topic/qualification document. Update short maps when owners move. Keep `AGENTS.md` and `CLAUDE.md` within their combined size budget; this plan must not become a second canonical architecture reference.

**Initial acceptance targets, to refine after baseline measurement**

| Measure | Proposed release target | Interpretation |
|---|---|---|
| First local value | At least 80% of formative participants reach a useful paper/example/result preview within 5 minutes, excluding installs | Navigation/setup quality, not model speed |
| Core workflow friction | At least 30% fewer manual field entries or cross-tool transfers in revision and analysis-to-writing tasks than the baseline | Count meaningful actions; do not hide necessary scientific decisions |
| Evidence traceability | Every displayed result/claim support link in the fixtures resolves to the exact retained object or explicit unavailable state | No silent latest-version substitution |
| Scientific overclaim | Zero false host-certified completion/support/proof states in the adversarial fixtures | Model errors may occur, but cannot be laundered into host facts |
| Data preservation | No lost accepted artifact, draft, decision or history in the defined crash/recovery matrix | Exact scenarios must be documented; not a universal guarantee |
| Interaction responsiveness | Warm project/object navigation p95 under 300 ms for metadata; first useful cold pane under 1 s on the benchmark machine | Proposed budgets; measure IPC and paint, not only SQL |
| Stop feedback | Visible cancellation request within 200 ms; actual settlement reported separately | Process termination duration depends on toolchain/cleanup |
| Large-project behavior | Benchmark 1,000 sources, 10,000 research records and 1,000 executions with bounded pagination and no full-store UI load | Synthetic scale target, not permission to remove current limits |
| Background resource use | No hidden-view polling storms; no repeated full-history scans for one attention item | Measure actual idle CPU/memory before setting platform thresholds |
| Portability | Export/restore and selected replication checks succeed on each supported platform in the declared matrix | Captured bytes alone do not qualify independent replication |

These are targets, not achieved measurements. Performance tradeoffs must not remove expected-hash checks, bounded reads, source integrity or ownership controls. A slower correct operation should show progress and be cancellable where its owner supports it.

## 7. What to consolidate, hide, remove, or defer

Removal should reduce the researcher's conceptual burden without deleting their work or reducing expert access. The following is a product recommendation, not permission to purge records or break saved formats.

| Item | Decision | Replacement or condition |
|---|---|---|
| Folder-path matching as authoritative project membership | Replace | FA-01 explicit association; retain path inference only as a migration suggestion |
| Unrelated Review collections inside a project | Remove from the primary project experience | Keep legacy collection management reachable from Review history; show only associated runs/findings in a project |
| Many permanent specialized project tabs | Consolidate | Six stable sections, short section modes, contextual inspectors and searchable advanced destinations |
| Generic record JSON as the normal research-object view | Replace for supported kinds | FA-02 typed readers; retain raw inspect/export and safe unknown-kind fallback |
| Manual cross-screen ID entry | Remove from ordinary flows | Exact-reference pickers, selection actions and FA-05 proposal cards |
| JSON sample metadata / exclusion rules as the default authoring experience | Hide under Advanced | File-based adapters and typed forms; preserve portable import/export |
| Duplicate creation forms for sources, findings, results and manuscript links | Consolidate after behavior parity | One owning editor per object type, callable from several entry points |
| Separate recipe galleries scattered across research tools | Consolidate presentation | One searchable outcome catalog with capability requirements; retain existing underlying recipe/workflow/chain formats |
| "Full self-discovery" as the easiest first autonomous action | Demote | Small supervised investigation or proposal exploration, preserving advanced portfolio mode |
| Paper counts and model-generated quality scores as success measures | Do not emphasize | Evidence produced, uncertainty reduced, useful decisions and transparent scientific limitations |
| Always-visible technical provenance prose | Shorten and contextualize | Compact access/freshness facts with detailed receipts one action away; material limitations stay visible |
| Silent omission of failed coverage sources when making a readiness claim | Change | Explicit partial/unavailable assessment, while preserving the existing responsive Overview and quiet thin projects |
| Repeated scope confirmations inside an unchanged authorized plan | Remove where redundant | A readable preparation step, durable exact authorization and re-review only for material scope changes |
| Existing source editor, workflow designer and JSON portability | Keep | Essential expert escape hatches; simplify entry, not capability |
| Existing lightweight one-off Review and unfiled chat | Keep | Projects are optional organization, not a prerequisite for every useful task |
| Small harmless easter eggs | Leave alone | Not a meaningful product priority compared with research workflows |
| Another top-level "Research OS", graph home, Kanban system or productivity analytics suite | Do not add | Extend current project sections and object views; avoid a second shell or progress theater |
| Real-time multiplayer/cloud synchronization | Defer | Make explicit coauthor snapshots/conflicts excellent first; current local ownership is a strength |
| Mandatory managed environments or containers for all execution | Do not require | Work with installed academic tools; introduce optional qualified containment separately |
| Broad provider abstraction for all persistent Workspace conversations | Defer | Reviews already offer provider diversity; new persistent backends require full context/tool/approval/recovery qualification |
| Plugin marketplace, unrestricted autonomous crawling, general scripting in workflow conditions | Do not add | Curated adapters/definitions with host-validated scope and declarative control |
| Native concurrent agent teams | Defer | Existing concurrency is load-bearing; qualify it separately after serial work is useful and dependable |
| Fully automatic journal submission, public posting or Git publication | Out of scope | Prepare artifacts and proposed changes; external publication remains a separate user-requested action |

### 7.1 Documentation and vocabulary cleanup

As each package lands, update the current topic document and its owner links. Do not rewrite dated historical evidence to imply current qualification. Current documentation still contains old navigation labels and the `oldstata` convention in some feature guides; reconcile those with the implemented UI and the user's active machine rule. Keep Project, Conversation, Review and Automation as the main product nouns. Words such as claim, sample, proposition and result are research objects; `workbench`, chain, harness and mission remain implementation terms unless needed in an expert editor.

## 8. Concrete end-to-end reference implementation

Use this example to keep the first release coherent. It is a synthetic acceptance scenario, not a substantive recommendation about any actual paper.

**Starting state:** A paper reports an employment coefficient of −0.042. It links to an adopted baseline result, one data vintage, a declared sample and a captured script. A referee requests a different clustering choice. A second source is abstract-only. One prior robustness attempt failed and is retained.

1. The researcher opens the project. Overview shows the outstanding concern and the exact current manuscript; the associated Review is found through project identity, not its path.
2. Opening the concern shows its passage, researcher disposition, available baseline and the next research obligation. The accepted conclusion is not automatically changed.
3. The researcher asks, "Prepare the requested comparison and show how it would affect Table 4." The assistant proposes a comparison using exact baseline and sample references. Missing requirements appear in the card.
4. Preparation shows the captured script/parameters, permitted computation, output contract and bounded work. The researcher starts that scope once. No additional prompt is required for each already-authorized child computation.
5. The existing job owner runs the prepared capture. Failure retains a receipt and stops the dependent manuscript update. Success adopts host-bound outputs. A claimed result in model prose cannot replace the receipt.
6. Analyze shows baseline and alternative with uncertainty, sample and specification differences, plus the earlier failed attempt. A new baseline requires an explicit reason. The assistant can discuss the comparison without choosing the favorable result automatically.
7. A generated table retains its exact source values and proposed caption. The manuscript update contains the table, linked printed values, and separately flagged prose implications. The abstract-only citation remains limited access.
8. The researcher accepts selected edits. Expected hashes prevent overwriting an external edit; partial acceptance triggers checks against the accepted combination. The new build PDF belongs to those exact inputs.
9. The response draft names the actual completed comparison and accepted manuscript passage. Its scientific adequacy remains a researcher decision. Focused re-review states its limited coverage; it cannot close unrelated findings.
10. Deliver exports the paper, response, table sources and selected evidence manifest. A new machine can inspect everything without model access. A replication package separately states which data and environment inputs are present or omitted.

**Minimal vertical slice boundaries:** FA-01 membership → FA-02 result/finding views → FA-04 one obligation → FA-05 comparison proposal → FA-07 one real toolchain → FA-08 two-result comparison/one table → FA-09 one update bundle → FA-10 one response → FA-14 readable export. Each stage must leave an independently useful artifact and be testable without implementing every option in its package.

## 9. Implementation sequence and release boundaries

### Stage 0 — Establish facts and preserve the current tree

Complete FA-00 and open the current release-qualification record. Write a short package status ledger in the implementation branch's topic docs. Record baseline measurements and unresolved native/platform limits. No redesign or schema migration until the selected slice's owner and acceptance scenario are known.

**Exit:** Valid fixtures, readable current capability matrix, no duplicate proposals for implemented features, and one chosen vertical slice. Unresolved release blockers remain blockers for publication; they need not prevent local feature development that respects existing restrictions.

### Stage 1 — Project identity and a consistent object experience

Implement FA-01, then the result/source/claim/finding subset of FA-02, followed by the smallest FA-03 intake. Build exact route compatibility and unavailable states before more automatic linking.

**Exit demonstration:** Open a rootless project, import a paper, associate a Review, inspect a source/result object and return to the same passage. Move or detach a project root without losing associations. No model call is required for this demonstration.

### Stage 2 — The connected paper and analysis release

Implement the reference scenario in §8 through narrow slices of FA-04/05/07/08/09/10 and the basic FA-14 deliverable preview. FA-06 starts with contextual source passage/citation support, not every connector. FA-11 starts with object pickers and plan preview, not a new failure-policy engine.

**Exit demonstration:** A real supported computation changes one result; the resulting table, manuscript update and response can be inspected, accepted, rebuilt, re-reviewed and exported with exact provenance. At least one failure/conflict path is demonstrated. The same path is usable through conventional controls without natural-language authoring.

**Release scope:** This is the first substantial product release recommended by the plan. It does not require a daemon, remote cluster, semantic embeddings, real-time collaboration, universal data support or high-volume discovery.

### Stage 3 — Broader research work and lower friction

Complete FA-06 batch/version intake, more FA-07 qualified adapters/result types, FA-08 comparison families, FA-10 revision queue, FA-11 simulations/resource explanation, FA-13 proposition workspace, and FA-15 search/decisions. Add FA-12 small supervised investigations after evidence and execution paths are stable.

**Exit demonstration:** Literature, empirical/quantitative and theory users each complete their journey with exact evidence and explicit limitations. Compare against Stage 0 baseline. Scientific evaluation can recommend narrowing a template rather than shipping every feature.

### Stage 4 — Portability and dependable unattended use

Complete FA-14 cross-owner recovery with crash/restore qualification, then evaluate whether the demand justifies FA-16 Stage A. Stage B remote computation requires its own product and deployment decision. Expand autonomous portfolio presets only when FA-17 demonstrates value over smaller investigations.

**Exit demonstration:** A restored project is complete for the chosen backup contract; an installed runner, if shipped, owns execution uniquely and survives the defined lifecycle matrix. Claims remain platform-specific where evidence is platform-specific.

### 9.1 Dependency summary

```text
FA-00 ──► FA-01 ──► FA-02 ──► FA-03 ──► FA-07 ──► FA-08
                         │        │                    │
                         ├──► FA-04 ──► FA-05          │
                         ├──► FA-06       │            │
                         │                └──► FA-11    │
                         └──── FA-04/05/06/08 ──► FA-09 ──► FA-10
                                             FA-09/10 ──► FA-14
                         FA-04/07/11 ──► FA-12
                         FA-04/05/07 ──► FA-13
                         FA-01/02/04 ──► FA-15
                         FA-11/12/14 + qualification ──► FA-16
FA-17 begins with FA-00 and gates every stage.
```

This is sequencing, not a request for automatic parallel agents. If work is delegated later, assign separate owner modules and integrate only after their contracts agree. Do not let several agents independently create migrations or competing object/action schemas.

### 9.2 Suggested first implementation tasks

Use these as bounded tasks rather than asking an LLM to "implement the plan."

1. **Contract and fixture baseline:** reproduce the fixture gaps, define valid exact-object fixtures, and record current journey measurements. No product redesign.
2. **Project associations:** schema, bounded commands, immutable handoff association and legacy preview. Include rootless/path-move/crash cases.
3. **Exact navigation:** route payloads, resolver and back/forward tests. Preserve draft guards and old links.
4. **Result and source readers:** two typed FA-02 views with shared provenance and unavailable states; replace only those generic renderers.
5. **One manuscript obligation:** select a passage, link a result and one open check, and show declared impact.
6. **One research action card:** prepare a comparison from exact selected results, with idempotent proposal lifecycle and changed-input handling.
7. **Execution setup:** one supported real toolchain and sample receipt, using existing captures/adoption.
8. **Comparison-to-table:** two compatible results, explicit baseline, full attempt history and one versioned table asset.
9. **Manuscript update:** exact diff, partial acceptance, build, conflict and undo.
10. **Revision/export:** connect one concern to its completed comparison and accepted passage, then export an honest response bundle.

Each task should be reviewable before the next starts. Broad refactoring, dependency upgrades or unrelated cleanup belong in separate changes unless strictly necessary for that task's acceptance scenario.

## 10. Cross-cutting implementation and test matrix

### 10.1 Persistence, compatibility and failure rules

| Change | Required handling |
|---|---|
| New project/object records | Append migration; defaults or explicit absence for old data; expected revisions; workspace-scope validation; bounded lists |
| New exact associations | Preserve owner/kind/ID/revision; idempotent create; tombstone unlink; no authority from path/name resemblance |
| New model action | Strict schema; validated refs; durable proposal; precondition fingerprint; owner dispatch; no replay on unknown outcome |
| Result schema extension | Versioned reader/exporter; fixtures for old/new versions; no invented uncertainty, units, sample or environment facts |
| New source grouping | Group relation over immutable editions; manual conflict resolution; no retroactive reattribution of quotes |
| New automation semantics | Versioned definition, validator, interpreter, budget and UI together; old prepared/running definitions keep their scope |
| Archive/export change | Versioned manifest and bounded closure; byte/hash verification; old readers handled explicitly; inert imports; owner-specific policy |
| Derived index/view | Rebuildable projection; generation/query identity; invalidation on exclusion/deletion/policy change; authoritative exact reads |
| Background process | One owner; durable leases; authenticated IPC; crash/upgrade/reconnect tests; no detached-process workaround |
| Navigation/UI consolidation | Legacy route aliases; unsaved draft guard; stale response suppression; focus/selection restoration; unavailable-object path |

For any new side-effect path, test interruption immediately before dispatch, after dispatch but before acknowledgement, after receipt persistence and before adoption, and after adoption but before UI refresh. The acceptable outcome is a single durable result or explicit uncertainty, never a duplicate run presented as recovery.

### 10.2 Scientific and usability acceptance fixtures

| Fixture | Required observation | Must not happen |
|---|---|---|
| F-A: Rootless literature project | Papers, notes and associated Review work without a folder or execution toolchain | Inventing a project root or requiring a Git repo |
| F-B: Changed dataset vintage | Exact historical result remains readable; dependent current work is marked for recheck | Claim automatically marked false or old result overwritten |
| F-C: Changed sample membership | Declared and executed sample facts are distinguishable; comparison flags the difference | Unexecuted prose rules labeled applied |
| F-D: Incompatible estimates | Estimand/unit/transformation mismatch blocks automatic comparison or requires a recorded valid transformation | Numeric agreement or significance used to hide mismatch |
| F-E: Citation edition mismatch | Original quote reopens old edition; related new edition requires its own support check | Source grouping transfers evidence silently |
| F-F: Abstract-only source | Restricted access remains visible in synthesis and manuscript inspection | Full-text support badge from metadata |
| F-G: False theorem | Retained witness or explicit untested obligations; search scope shown | "Proof verified" from a numerical search or model consensus |
| F-H: Partial manuscript acceptance | Accepted combination is rebuilt/rechecked; original proposal and undo survive | Old validation attached to a different combination |
| F-I: Scoped Review | Exact examined material and omitted scope shown; old unrelated concern remains open | Absence from the new report interpreted as resolved |
| F-J: Automation acknowledgement loss | Owner reconciliation or explicit attention; one adopted result | Automatic blind resubmission |
| F-K: Backup with missing component | Incomplete package/preview, precise omission and recoverable state | Complete-success message with missing linked Review/files |
| F-L: Large project | Search/list/impact bounded with visible incomplete coverage and responsive navigation | Unbounded whole-store fetch or clean status from a timed-out scan |
| F-M: Local-store/read failure | Unavailable check shown where relied on; drafts retained; specific Retry | Empty state misrepresented as no problems or whole-app crash |
| F-N: Unfavorable experiment | Failed, excluded and negative results remain inspectable and exported as appropriate | Selection of favorable outcomes silently changes baseline |

### 10.3 Appropriate engineering checks

Follow the then-current commands in [CLAUDE.md](CLAUDE.md), [CONTRIBUTING.md](CONTRIBUTING.md) and [RELEASING.md](RELEASING.md). At this review, the normal checks include:

```sh
# Repository root
node scripts/check-markdown-links.mjs
node scripts/source-size-report.mjs --check

# gui/
npm test
npm run build
npm run format:check
npm run test:release

# gui/src-tauri/
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
```

Run focused owner tests during each slice, then the full relevant gates at a milestone. For protocol, authority or process changes, read the [compatibility policy](docs/workbench/protocol/compatibility-policy.md) and run the required no-model probes before authenticated qualification. Use `npm run tauri dev` with disposable stores for native GUI checks on this machine; never open the installed `/Applications/` version for development review.

Use real toolchains and visually inspect exported tables/figures/PDFs for the capabilities actually claimed. Mocked figures, synthetic research responses and browser screenshots cannot qualify real computation, scientific correctness, native WebKit behavior or another platform. Do not run paid model evaluation or export private research to a new service under the guise of a local test.

## 11. Instructions for the implementing LLM

### 11.1 Before editing

1. Read the active user request and repository/machine instructions. This document is a specification; it is not authorization to implement every package, publish artifacts externally, install a background service or spend an unbounded model budget.
2. Inspect current repository status using the required Git invocation. Preserve unrelated changes. Determine which package/slice the user actually requested.
3. Read that package's existing owner and tests, then verify whether its proposed capability has already landed. Update the implementation status instead of duplicating it.
4. Write a short concrete slice contract: entry action, data/authority changes, output, failure states, compatibility and acceptance scenario. Use the default decisions below unless later evidence or user direction supersedes them.
5. Choose the smallest additive schema/API change. New file names in this plan are conceptual unless linked to an existing file. Locate current owners before creating modules.

### 11.2 While implementing

- Keep production ownership explicit. A new screen composes existing services; it does not become a scheduler or provenance authority.
- Implement host validation and persistence before wiring a model action or UI success state. Never substitute a frontend type assertion for native validation.
- Use exact fixture contracts and real representative states: empty, loaded, stale, partial, excluded, conflicting, failed, cancelled and unknown.
- Preserve current behavior for older saved data. New required scientific metadata can be absent in legacy records; do not backfill it with model guesses.
- Keep helpers cohesive and respect source-size budgets. Split a growing owner by responsibility instead of adding a global utility or giant controller return object.
- Do not add exhaustive mirror tests for presentation-only changes. Spend test effort on scientific distinctions, scope crossings, serialization compatibility, asynchronous races, once-only effects and loss prevention.
- Inspect the result in the development GUI and exercise keyboard/back navigation. Screenshot fixtures supplement native checks; label which was used.
- Do not call a feature done because its form renders or its happy-path test passes. Complete the stated user journey and failure case.

### 11.3 Completion report for each slice

Report: implemented behavior; exact owners changed; old behavior/data preserved; tests and visual/native evidence; remaining limitations; and the next dependency. Update the owning topic document with a dated record. Mark only the completed slice, not the entire large work package, as done.

If a limitation prevents a safe complete implementation, leave a specific unavailable state and preserve drafts/artifacts. Do not fabricate data, suppress errors, silently lower a check, or broaden execution authority to satisfy the demonstration.

### 11.4 Default product decisions

| Question | Recommended default |
|---|---|
| Should everything require a project? | No. Keep one-off Review and unfiled Conversation, with explicit later association. |
| Should every project start with a paper? | No. Questions, theory notes, data work and literature projects are valid. |
| Should the living argument replace the manuscript? | No. It is a linked inspection and planning view over the research, with the source manuscript authoritative for writing. |
| Should an AI accept evidence or manuscript changes? | It may propose and complete authorized operations; researcher acceptance remains separately recorded by the owning UI/service contract. |
| Should code run just because it was discovered in the folder? | No. Detection proposes setup; execution follows existing exact preparation/authorization/testing. |
| Should source refresh rewrite old citations? | No. New edition, new association and explicit comparison. |
| Should a completed Review imply the paper is correct? | No. Completion, coverage, findings and scientific judgment are separate. |
| Should a failed background check disappear? | No. Keep quiet ordinary refresh, but show unavailable coverage when the user is relying on it. |
| Should smaller discovery replace existing saved large portfolios? | No. Apply new versioned defaults to new definitions; preserve old recorded behavior. |
| Should the UI unify runtimes? | No. Unify names, object views and explicit associations; preserve runtime ownership. |
| Should a new release depend on every package here? | No. Ship the connected-paper vertical slice first; use evaluation to select extensions. |
| Should the product force a particular cloud, editor, reference manager or language? | No. Work with local research files and qualified optional adapters. |

## 12. Review and document validation record

This file is the requested planning deliverable. The application was not changed to implement its recommendations. Existing working-tree modifications and earlier remediation records remain separate.

The review established current feature ownership and sampled concrete implementation paths rather than treating historical plans as current defects. Visual observations are limited to the stated synthetic browser fixture; native attachment, authenticated research quality, packaged behavior and cross-platform usability were not qualified. The Tauri development build launched, but that is only a development-build observation.

Validation performed for this planning deliverable: `node scripts/check-markdown-links.mjs FEATURE_ASTRA_PLAN.md` passed. Repository status before and after the review showed this plan as the only additional changed/untracked path from this task; the pre-existing changes were left in place. No application tests were rerun solely for the Markdown addition. The successful Tauri development compilation and the limited fixture observations are recorded in §2, separately from release qualification.

Keep all recommendations marked unimplemented until subsequent authorized work produces evidence. Revalidate owner links and package status when implementation begins. The product should be judged by whether a researcher can complete and inspect the connected journeys, not by how many sections of this roadmap have corresponding screens.
