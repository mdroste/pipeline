# Research programs — NF-07 through NF-14

Implemented September 7, 2026 as the bounded first slices in [SEP7_ASTRA_NEWFEATURES.md](../../notes/SEP7_ASTRA_NEWFEATURES.md). These features build on the [research desk](research-desk.md), the [research studio](research-studio.md), and captured execution. This guide describes implementation and development checks, not a packaged-release qualification.

## Navigation

Open a Workspace project and choose **Open project**. The new destinations are available without signing in. Rootless projects support notes, kits, symbols, deliverables and coauthor briefs. Computation and accepted-file checks require an attached project folder.

| Feature | Destination | Implemented behavior |
|---|---|---|
| NF-07 | Analyze → Specification grids | Review a complete bounded family, explicitly start serial captured executions, inspect every attempt and exclusion, export receipts |
| NF-08 | Write → Publication assets | Generate tables and configured Python figures from exact numeric results; export or stage them for ordinary acceptance |
| NF-09 | Analyze → Symbols & assumptions | Scoped notation, lexical candidates, alternative-assumption branches, captured executable check records |
| NF-10 | Automate → Revision automations | Immutable rounds, response evidence and current-file checks, researcher judgment, ordinary scoped Review preview |
| NF-11 | Conversation → Next message; rail → Activity | Persist an editable next draft, explicitly queue/run/reorder/cancel, inspect runtime owners, create a context branch |
| NF-12 | Write → Coauthors & replication | Coauthor briefs and `.pwex` conflicts; separate inert `.pwrc` replication materials and numeric verification |
| NF-13 | Write → Deliverables & kits | Seven small starter kits, ordered Markdown/TeX assembly, asset bundle export, additive deliverable roles |
| NF-14 | Automate → Scheduled checks | Explicit app-open local checks and opted-in Crossref refresh, quiet baselines, deduplicated attention and backoff |

Native Workspace turns, captured jobs, task-coordinator runs and Review Workflows retain separate ownership. Research programs call those existing services; they do not create another orchestration mode or broaden execution grants.

## NF-07: complete specification families

Select an NF-06 captured base plan. Declare the research question, 1–8 factors, scalar values, exclusions with reasons, interpretation and total run/time budgets. The expansion retains the declared ordering, every excluded specification and every attempted outcome. Limits are 32 values per factor and 64 total specifications. Budget rejection occurs before derived plans are written.

The immutable family contains exact child plans, parameters, source inputs, expected outputs and a content fingerprint. Each child runs from a new private input copy. The family uses a serial producer over the existing bounded local job service, preserving its project write lock, ownership and cancellation. Starting the reviewed family authorizes its exact children; it does not authorize a changed family. Pause stops advancement; cancellation also targets its active owned job. Failures and timeouts retain full receipts alongside successes.

A process restart pauses a running family until an explicit resume. Lost/unknown execution outcomes produce attention rather than replay. Operation identities bind an ordinal to one queued execution. Export includes all attempts, exclusions and execution receipts. Import an adopted results JSON through the existing result-validation service. The grid does not infer significance, rank specifications by favorable results, or change a baseline.

## NF-08: publication assets

A table or figure specification references exact immutable structured results or adopted IRF series. Scalar entries must agree on estimand, units and transformation; IRFs also check variable, shock normalization and horizon units. Different samples require a written comparison rationale. Series values are checked against their retained adopted artifact, not merely a provenance label.

Tables produce Markdown, TeX, CSV and an unrounded JSON source record. Display precision never changes the retained values. Standard errors and confidence intervals come from the result contract; missing uncertainty remains missing. Manual scalar overrides require a reason, retain the original estimate, clear incompatible uncertainty and visibly label the row as manual. Tables are limited to 40 entries; figures to 12 series/coefficients and 2,000 IRF horizons.

Figures use the versioned `programs/plot.py` Matplotlib adapter. Choose an installed Python profile with Matplotlib, inspect the generated script and input JSON, then review, authorize and test its captured plan. Nothing installs a Python environment automatically. The successful exact execution supplies SVG, PDF, PNG and a version receipt. Missing observations remain gaps; no interval is fabricated. The launcher preserves a selected virtual environment's invocation path and fingerprints `pyvenv.cfg` in addition to the executable. This identifies configuration changes; it does not fingerprint every installed package.

Export files individually, or stage a selected artifact and its deliverable attachments into an existing isolated/review checkpoint. Staging checks expected destination hashes and confined relative paths, refuses executable destinations, captures changes and records exact inclusion links. Accepted files change only through the existing checkpoint application flow. A new asset/draft creates a new immutable version; existing drafts are preserved. Dependency projections connect results → assets → deliverables/inclusions for impact review.

## NF-09: notation and assumption checks

Symbols retain notation, aliases, scope, definition, domain, units and exact sources. Collision candidates require overlapping scopes or global notation with different definitions. Superseded definitions remain available. Lexical TeX extraction is bounded and explicitly leaves macro expansion, local scope and mathematical interpretation to the researcher.

An alternative-assumption branch copies the selected theory-note revision into a new open note with explicit alternative assumptions and a question. It preserves the original argument and clears acceptance/promotions. An executable check records its expression, boundary conditions, exact current theory reference, captured plan, parameters, seed, declared engine and completed execution. Existing theory-check validation remains authoritative: numerical examples cannot establish a general proof. Superseded note revisions cannot receive a new current check silently.

## NF-10: revision campaigns

Each immutable round references one manuscript revision, selected response revisions, required output references, change summary, re-review scope and an optional earlier round. Each response keeps six distinct observations: task done, accepted files still current, execution completed, required outputs current, link checks pass, and the researcher's substantive judgment. Claiming an added analysis without linked evidence produces flags even if the researcher marked the comment addressed.

Current-file checks compare applied output hashes with accepted working files. Required objects are read at their exact versions and checked for later revisions/supersession. The old round remains readable. A scoped re-review requires current response revisions and anchors from the round's exact manuscript; it uses the existing immutable Review handoff preview. A declared broad change recommends full manuscript Review. No status asserts that the response is scientifically adequate.

## NF-11: explicit queue and context branches

The next-message editor remains editable during an active turn. Its local recovery draft is separate from the ordinary composer. **Queue next message** stores a one-turn request, model/effort, exact selected context, preceding turn, harness/root identity and operation fingerprint in the Workspace database. It does not send. Up to 20 pending entries are allowed. Active entries appear before terminal history and remain visible on every 100-entry history page. Only unsubmitted entries can be reordered or cancelled.

**Run next** validates the binding and ordinary draft state, checks the existing single-turn admission rule, and durably marks dispatch before entering the ordinary host submission path. A lost acknowledgement is reconciled to a receipt or explicit attention. Neither reconnect, startup, listing nor elapsed time submits a queued request. An uncertain request cannot be resubmitted by the queue. Inspect its owning conversation before creating a replacement. After context/history changes, **Review current context** previews the saved message and model/effort, the current conversation/project/folder, research settings, and old/current selected sources. **Use reviewed context** refreshes only an unchanged queued request and rejects a stale preview. It preserves the message and operation identity without sending; **Run next** remains an explicit separate action. Cancelled, running, uncertain, and completed requests cannot be refreshed into a new submission.

The Activity drawer lists Workspace turns, local jobs, task-coordinator runs and active/interrupted Review runs through owner read adapters. It opens the exact owning conversation, task, job surface or Review history. Native Stop retains the exact thread/turn identity. Workspace attention is derived from a set of unresolved request IDs, distinguishing numeric and string IDs, so resolving one request does not clear another.

**Context branch** explicitly creates a new conversation from selected retained research objects. It does not copy chat history, root-write authority or an execution binding. This is the plan's fallback when native forking is unavailable. Native `thread/fork`, `turn/steer`, and concurrent native turns remain disabled pending the separate protocol/lifecycle qualification in the original plan. The one-active-turn invariant is unchanged.

## NF-12: two distinct sharing formats

### Coauthor review

Create a readable brief with questions, change summary, excluded material/unresolved dependencies, selected exact references and revision-specific comments/decisions. The brief becomes an ordinary handoff note eligible for the existing selective `.pwex` export. Select Notes and the desired referenced research objects in that preview. A package is an explicit snapshot, not synchronization.

Accepted imports now retain the actual common ancestor by source namespace and object identity. Conflicts show base/local/incoming only when that base exists; otherwise the UI states that it is comparing two versions. Taking imported content advances the retained base; keeping local content does not invent or overwrite an ancestor. Existing remapping, immutable evidence and explicit conflict resolution remain in force.

### Replication capsule

`.pwrc` is an ordinary ZIP with an independently versioned `research-capsule-v1` manifest, readable README and hashed blobs. It is separate from `.pwex` and whole-store `.pwrx`. Preview 1–16 ordered command plans, declared scripts/inputs, availability instructions, parameters, seed, timeouts, toolchain description, portable environment requirements, lockfile names, expected artifacts and optional JSON-pointer numeric tolerances.

Included non-script data requires the project's package-data permission. Omitted inputs require acquisition/license instructions and retain hashes. Environment values containing machine paths are excluded and replaced with recipient-local variable requirements. The manifest explains that inherited host settings and installed packages are not fully captured. Exporting-machine executable paths, credentials and grants are excluded. Plans with nonportable absolute arguments must be adapted before export. Archive validation rejects traversal, symlinks, duplicate entries, undeclared blobs and hash mismatches; bounds are 64 MiB total and 8 MiB per entry.

Import retains inert material only. Bind each ordered plan to a configured recipient command toolchain, supply required external captures by exact hash, and satisfy declared environment settings. Missing inputs/settings return **incomplete** before creating an executable profile. Review and explicitly authorize/test the new captured plan. Partial preparation can recover its unchanged committed profile; a changed operation cannot reuse it. Binding grants no authority.

Verification compares only a completed execution of that exact binding. Missing artifacts or nonnumeric selected values are incomplete; differences beyond `absoluteTolerance + relativeTolerance × |expected|` are mismatches. A successful run without numeric expectations is **executed_unchecked**, not a numeric pass. A matched report means the selected values agreed, not scientific validity. Package provisioning, licensed adapters, transitive dependencies and independent-machine qualification remain explicit limitations.

## NF-13: useful projects before manuscripts

Seven versioned kits cover literature synthesis, empirical research, quantitative models, theory, grants, seminars and teaching. Installation previews editable instructions, one outline note and two ordinary tasks. It makes no model call, attaches no folder, installs no software and grants no execution. Customized projects are preserved; another install creates ordinary new records.

Deliverable assembly orders the researcher's prose, exact source excerpts and retained publication assets into Markdown and TeX drafts. It retains template version and dependency references. Regeneration creates a new draft for ordinary comparison and acceptance. Export the draft with every relative asset file in one ZIP, or stage into a checkpoint. TeX figure inclusions require the usual `graphicx` package in the receiving document; generated TeX is a document fragment. DOCX/PPTX adapters are not included in this slice.

Additive roles (`main`, `grant`, `seminar`, `teaching`, `memo`, `supporting`) make a manuscript optional while preserving `manuscriptRevisionId`. External-file opening, inventory refresh and incoming-change comparison continue through Edits & acceptance.

## NF-14: app-open checks

Create an explicit check for an accepted file, tracked dataset lineage, local results, an execution receipt or a saved Crossref query. Intervals range from one minute to 30 days. Remote queries disclose the provider/query and require both project acquisition permission and per-check network consent. They perform bounded metadata refresh outside the database gate. They never execute arbitrary commands or automatically import papers.

The first successful observation establishes a quiet baseline. Each changed observation can create an attention item, including a return to a state seen earlier. The captured check revision identifies the occurrence: retrying its result stays quiet and cannot advance the failure count twice. Unchanged observations stay quiet. Three consecutive failures create attention with exponential backoff; recovery allows a later failure episode to create new attention. Sleep/missed intervals coalesce into one due observation. Each check can be paused/resumed with revision checks; results captured before that control change are discarded. Attention can be acknowledged separately. Notifications are optional; attention remains inspectable.

The worker runs while Pipeline's process is open, using bounded due batches. The task coordinator's existing optional tray keepalive may keep that process open after closing a window. Quit, restart and reboot are not covered by a new daemon. Whole-store restore disables checks, clears family execution authorization and puts uncertain follow-ups/jobs into attention. No restored schedule automatically executes research.

Stage B's authenticated local service, durable leases, upgrade negotiation and packaged cross-platform lifecycle remain separate work. No process is detached as a substitute.

## Implementation and validation

`gui/src-tauri/src/workbench/programs/` contains typed services and a narrow host command facade. `workbench_program`, `workbench_followups` and `workbench_research_activity` are registered Tauri commands. All production store access passes through the existing blocking store gate; execution and network waits occur outside it. Migration 013 appends immutable record kinds, mutable queue/check/family tables, attention, exchange ancestors and rebuildable search projections. Private execution/capsule/monitor bodies remain excluded from curated search.

`gui/src/components/research-programs/` provides lazy project surfaces; the ordinary Workspace page retains transport and lifecycle ownership. `programClient.ts` owns app-facing DTOs. Tests cover complete grids including real Python failure/timeout cases, exact-source assets, manual-value handling, kit idempotency, draft preservation, safe checkpoint staging, ambiguous queue outcomes, monitor deduplication/backoff, malformed capsules and two-store reproduction.

The opt-in `qualified_scientific_figures_and_tex_table` test runs the captured Matplotlib adapter and compiles TeX with shell escape disabled. Its September 7 development run used a disposable Python environment with Matplotlib 3.10.8 and the installed TeX toolchain; PNG/IRF/table renders were visually inspected. This verifies that fixture on this Mac. Authenticated native submissions, live remote metadata monitoring, licensed Stata/R/Julia figure variants, independent-machine replication and packaged/cross-platform lifecycle have separate qualification requirements in [release-qualification.md](release-qualification.md).

Final development validation, September 7, 2026:

- `npm run build` passed; Vitest passed 557 tests across 72 files in the shared checkout.
- `cargo test --locked --all-targets` passed 869 library tests and 14 additional target tests. Ten opt-in qualification tests remained ignored; the scientific figure/TeX qualification fixture was run separately and passed.
- The all-targets Clippy check passed at the earlier validation snapshot; the latest production-library Clippy check and Git whitespace check passed. A subsequent concurrent storage refactor blocked a fresh all-targets check (`src/storage.rs` references an unfinished test module, and a `Store` test fixture needs its new `codex_home` field). These are separate shared-checkout changes; the 883-test result above records the completed run before those edits.
- `gui/e2e/research-programs-native-check.mjs` passed against Tauri development mode with disposable Workspace/task stores: seven project destinations, rootless kit installation, deliverable generation, notation save, scheduled-check pause, explicit queue without model transport, and owner activity navigation. The local WebDriver adapter required a standard DOM change event for its select control; ordinary buttons, form inputs, host IPC and rendered screens were exercised. No authenticated model turn or remote query was sent.
- Final coefficient and IRF PNGs and the compiled TeX table were visually inspected. The table compiled without overfull-box warnings.

The run also exposed and fixed three integration defects: loss of Python virtual-environment identity through executable canonicalization, artifact deduplication returning an uninserted ID, and an effect resetting the first harness-tab click. These fixes preserve the existing host execution and preset-selection contracts.
