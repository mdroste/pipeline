# Pipeline product and UI/UX implementation plan

**Prepared:** September 30, 2026, America/Los_Angeles (Claude Fable 5)
**Scope:** The complete desktop product — information architecture, the Workspace/Projects suite, the review workflow designer, the automation (chain) engine's user surface, run monitoring, settings, and the design system. Frontend (`gui/src`), backend command surface (`gui/src-tauri/src`), and product documentation were audited; the running UI was inspected in a browser fixture (see §8).
**Baseline:** Working tree at commit `46b4ae9` ("sep29"), version 0.9.5.
**Relationship to prior documents:** [ASTRA_SEP7_UIUX.md](ASTRA_SEP7_UIUX.md) holds findings UX-01–UX-26; UX-01–UX-10 are implemented, UX-11–UX-26 remain open and are *not* restated here. This plan continues finding numbering at **UX-27** and defines work packages **WP-00–WP-10** (a distinct series from that document's UI-00–UI-17 and from [PIPELINE_CODE_AUDIT_ASTRA_SEP20.md](PIPELINE_CODE_AUDIT_ASTRA_SEP20.md)'s F/R series). A September 9 plan document referenced in earlier sessions is no longer in the repository; nothing here depends on it.
**Convention:** Later implementation passes should append "implementation update" sections to this file rather than starting a new document. File references are `path:line` at this snapshot and will drift.

---

## 1. Assessment

Pipeline's engines are ahead of its interface. The codebase already contains everything the product vision asks for — including the headline request that motivated this plan: *chain LLM prompts, reviews, and replies with conditional logic* ("write a draft → review it → address feedback → review again → address again if needed").

That capability exists **today**, fully implemented, durable, and restart-safe, in the orchestration chain engine (`gui/src-tauri/src/orchestration/definition.rs`): a validated step tree with `If/Else`, `Repeat…until`, `While`, `Parallel`, `ForEach`, nested chains, typed three-valued conditions, JSON-pointer bindings between step outputs, `Input` waits that pause for the user, and calendar/interval triggers. A chain step can run an entire review workflow and branch on its structured result (`complete`, `highPriorityCount` — `orchestration/adapters.rs:426`, `definition.rs:392`). The built-in "Review and revise" automation template *is* the draft→review→revise→re-review loop.

What the user is given to drive all of this:

- **Three hardcoded templates** and a raw **JSON `<textarea>`** ("Edit definition") on the Automations page (`gui/src/components/tasks/Builder.tsx`), whose outline preview labels steps with internal kinds — `workspace`, `snapshot`, `if`, `deliver`.
- In the review designer, a single skip-guard per step whose editor is a **regex input field** ("Run only if… an upstream step's output matches a pattern: `major revision|reject`") behind an Advanced toggle.

So the central product problem is not missing functionality; it is that **four orchestration vocabularies, two "project" concepts, and roughly 45 navigational destinations have accumulated without a unifying model**, and the most valuable engine in the product is the least visible. The redesign should therefore be mostly *consolidation and exposure*, not construction:

1. **One container: the Project.** A project holds conversations, reviews (runs + issue ledger), automations, and the library. The separate "Review collections" concept merges into it.
2. **One way to compose work: the automation outline.** A plain-language, outline-based composer (per the intent already written down in [docs/task-orchestration-design.md](docs/task-orchestration-design.md)) becomes the single authoring surface for multi-step conditional work, with review workflows as callable units inside it. No graph-first editor, no scripting language, no prose-as-scheduler — those remain explicitly declined (development/ROADMAP.md non-goals).
3. **One shell.** A real route model, a flattened navigation rail, one activity surface, and one set of UI primitives.

The intended visual character stays as the Sep 7 audit put it: calm, precise, neutral surfaces, controlled accent, generous reading space, minimal persistent chrome. This plan does not restate those visual rules; it sequences the structural work that makes them enforceable.

### What must not change

These are load-bearing constraints confirmed in code and docs; every work package below respects them:

- Deterministic conditions only — never model prose as control flow (`orchestration/definition.rs` comment; docs/tasks.md:96).
- One active Workspace turn, one active review run, ≤4 concurrent chain actions (`orchestration/mod.rs:289-301`). Raising these is a separately qualified engine change, not a UI decision.
- Compatibility namespaces (`workbench`, `task`/`chain` schema names, `projects/{id}.json`) survive any UI renaming.
- Append-only migrations; portable workflow schema v11 and chain schema v1 stay readable.
- Declined scope stays declined: no cloud service, telemetry, plugin marketplace, WYSIWYG, multiplayer, embedded scripting, mandatory graph home screen.

---

## 2. Findings (UX-27 – UX-40)

Priorities: P1 = blocks the product thesis; P2 = significant friction/incoherence; P3 = cleanup. Evidence cites code at this snapshot and the fixture inspection (§8).

| ID | Priority | Finding | Direction |
|---|---|---|---|
| UX-27 | P1 | **Two orchestration languages, two editors, both hostile to the core journey.** Workflow steps express conditions as a single regex/JSON-pointer skip-guard edited in a raw pattern field behind an Advanced toggle (`pipeline-editor/AdvancedStepOptions.tsx`; `pipeline_config.rs:270`). Chains have full control flow but are authored via three fixed templates plus a JSON textarea with internal kind labels (`tasks/Builder.tsx`; `lib/taskClient.ts` DSL). The draft→review→revise loop cannot be *composed*, only instantiated from one template. | Build the outline composer (WP-04) on the chain schema; present workflow conditions through the same condition-builder vocabulary (WP-05). |
| UX-28 | P1 | **"Project" means two unrelated user-facing things.** Workspace research projects (`project-index`, `workspace` pages) and "Review collections" (`projects` page, `lib/types.ts:586`) are disjoint stores both labeled project; the fixture shows two same-named "Minimum wage revision" objects with different contents. The step editor's "Publish findings to Projects" bridges them one-way, deepening the confusion. Backend holds four project-shaped stores (`projects.rs:25`, `workspaces_v2`, `project_records`, `desk_records`). | Merge review collections into research projects at the UI level (WP-03); keep on-disk stores compatible. |
| UX-29 | P1 | **The product's best capability is invisible and exits its context.** Automations live on a page reached through a collapsed "Tools" disclosure; conversation task cards navigate *away* from the workspace (`WorkspaceTaskCards.tsx`); an automation cannot be created from the place where its inputs live (a conversation, a finished review, a project). The "Research" tab nests missions and self-discovery two levels deep. | Per-project Automate section + "continue with an automation" entry points from conversations and review reports (WP-04, WP-07). |
| UX-30 | P1 | **No route model.** Navigation is one `useState` in a 1,113-line hook returning ~109 values (`hooks/useAppController.ts`), ~20 localStorage keys, and 6 `window.dispatchEvent` channels (`pipeline:open-project`, `pipeline-context-add`, …). Startup restores only 4 of 12 pages (`lib/appPreferences.ts:144-154`) — confirmed live: deep-linking any tool page falls back to Home. Nothing is linkable; back/forward doesn't exist; UX-21 is unfixable in this architecture. | Introduce an in-app router as the first structural change (WP-00). |
| UX-31 | P2 | **Information architecture and naming drift.** The review suite — the product's origin — sits behind a "Tools ▸ Show advanced tools" disclosure. The same object is "New review" (rail), "New run" (page title), "Review report" (submit button), "Report history" vs "Review history" (page vs rail), "Workflow" vs "Review designer" vs "profile" (dropdown/rail/storage). Settings says "Reviews"; the designer says "Workflow". | One vocabulary table, enforced in copy review (WP-02); flatten the rail. |
| UX-32 | P2 | **Workspace destination sprawl.** 31 destinations in 6 sections (`lib/workspaceNavigation.ts`) rendered as ~28 permanently-mounted retained views (`WorkspaceProjectSurface.tsx`, 970 lines); `WorkspaceResearchPanel` (1,155 lines) is mounted 8 different ways with `allowedTabs` arrays of length 1 — evidence the taxonomy was retrofitted. The ⌘K picker is the only practical navigation; the section model has no landing pages. | Collapse to 6 section landing pages + true subpages; palette becomes accelerator (WP-07). |
| UX-33 | P2 | **Designer dead-ends and buried power.** Opening the designer shows "Select a step to edit" beside an empty pane (verified in fixture; UX-16's finding still open). The New run screen is a narrow form beside a static "01/02/03" explainer that duplicates it (`App.tsx:483-524`). Conditions, fan-out, dependency quorum, and output schemas — the engine's differentiators — hide behind Basic/Advanced and a Schemas tab. `WorkflowPanel`'s 291-line read-only step list duplicates the navigator with an unreachable `editorOpen` branch (`RunSetupPanel.tsx:101`). | Designer opens on overview; integrated run setup; promote conditions/fan-out into the primary step editor with plain language (WP-05). |
| UX-34 | P2 | **Automation page is a form, not a composer.** Template pickers with fixed fields; outline preview prints internal kinds right-aligned (`workspace`, `snapshot`, `if`, `review`, `deliver` — verified in fixture); editing means raw JSON with a "Named inputs {}" field; validation errors surface as strings. The documented outline-editor UX (docs/task-orchestration-design.md §"Authoring") was never built. | WP-04 in full. |
| UX-35 | P2 | **Run monitoring is fragmented across five surfaces.** `PipelineProgress` (stage list), `Console` (bottom dock appearing only when logs exist), `BatchPanel` (separate page), task `Detail` receipts, workbench job log — plus an "Activity" overlay that routes elsewhere. No single "what is Pipeline doing right now / what finished while I was away" surface, despite the rail's badge dots. | One Activity center with live run/automation/job rows and history (WP-06). |
| UX-36 | P2 | **Design-system fragmentation.** Four incompatible `button` style constants (`project-surface/shared.tsx:20`, `research-studio/shared.tsx:7`, `research-desk/shared.ts:9`, `research-programs/shared.tsx:30`), two dark neutral scales (`dark:bg-gray-900` vs `dark:bg-neutral-900`/`#101010`), ~15 empty-state variants in three voices, two spinner implementations plus ~8 text spinners, three unrelated table styles, three toggle implementations, two modal mechanisms, three private SVG icon sets redrawing the same paths (`NavRail.tsx:63-202`, `HomePage.tsx:12-118`, `WorkspaceIcon.tsx`), and 3,748 lines of page-scoped CSS across 10 files. | Primitive kit + tokens, migrate opportunistically (WP-01). |
| UX-37 | P2 | **Settings has outgrown its taxonomy.** 7 sections + 4 legacy alias redirects, a 442-line search index as a crutch, `sectionForTarget()` hardcoding 4 items that live outside their declared section (`settings/navigation.ts:85-96`), DOM-walking focus helper, and naming that matches neither the rail nor the workspace ("Connections" vs "Assistant connection settings"). | Restructure around the Project/Reviews/Automations model; kill aliases after a release (WP-08). |
| UX-38 | P3 | **Vestigial surfaces and dead config.** `use_orientation` renders as a permanent "Required" badge (`lib/types.ts:184`, `EditorPanels.tsx:155-170`); the idle "01/02/03" explainer; the `Context` inspector is prose plus three redirect buttons (`WorkspacePage.tsx:912-957`); `WorkspaceProjectSurface`'s alternate header chrome is unreachable in-app (`:544-614`); `SplitView` lost to `WorkspaceDesk` (2 importers); `pipeline.tasks.enabled` is a stale feature gate (`TasksPage.tsx:101`); legacy plan-matching branches render in `PipelineProgress.tsx:400-420`. | Itemized removals (WP-09). The Flappy Bird easter egg (Konami code in Help) is harmless — keep it. |
| UX-39 | P3 | **Type and API surface drift risk.** 256 distinct frontend `invoke` commands against 298 registered handlers; `lib/types.ts` (888 lines) and `workbenchTypes.ts` (859) are hand-maintained mirrors of Rust serde shapes; ROADMAP's `ts-rs` generation item was never done. This audit's fixture broke on exactly this class of drift. | Adopt generated types for new/touched commands; consolidate command registration per feature (WP-10). |
| UX-40 | P3 | **Reading and comparing results is run-centric, not document-centric.** History is a filter table over runs; Compare is only reachable by selecting two runs there; the issue ledger — the closest thing to "state of my paper" — is buried inside Review collections. A researcher's question is "what's outstanding on draft v7," not "which run ids exist." | Project-level Reviews section leads with the document/ledger view; run list becomes secondary (WP-03, WP-06). |

Non-finding worth recording: the September 20 audit's nine fixes (F01–F09) are implemented but qualification-gated; nothing in this plan re-claims them, and WP-10 keeps their gates.

---

## 3. Target product model

Five decisions, stated once and used by every work package:

**D1 — The Project is the only container.** A project holds: **Conversations**, **Library** (papers, sources, files, notes), **Reviews** (runs of review workflows against project documents, plus the issue ledger), **Automations** (chains scoped to the project), and **Activity**. "Review collections" disappear as a user concept; their data (run grouping + ledger) becomes the project's Reviews section. Quick, project-less reviews remain possible and land in an automatic "Unfiled reviews" area with a one-click "move into project."

**D2 — Three verbs inside a project: Ask, Review, Automate.** *Ask* = a conversation turn. *Review* = run a review workflow (the deterministic DAG engine, today's "profile"). *Automate* = compose Ask/Review/Save/Wait/Deliver steps over time with conditions and repetition (the chain engine). The Home page already gestures at exactly this triad ("Work with the assistant / Review a paper / Automate research") — the rest of the app should follow it.

**D3 — Outline-first composer with plain language.** The automation composer is an indented outline (the documented intent), never a mandatory graph. Every chain `Action` kind gets one user-facing name and one sentence pattern:

| Engine kind | User-facing step | Sentence pattern in outline |
|---|---|---|
| `workspace` | Ask the assistant | "Ask: ⟨prompt title⟩" |
| `review` | Run a review | "Review ⟨input⟩ with ⟨workflow⟩" |
| `snapshot` | Save a version | "Save ⟨file⟩ as a version" |
| `check` / `capturedCheck` | Run a check | "Run check ⟨name⟩" |
| `literatureLookup` | Look up literature | "Search literature for ⟨query⟩" |
| `deliver` | Deliver a result | "Deliver ⟨what⟩" |
| `input` | Wait for me | "Pause and ask me: ⟨question⟩" |
| `delay` / `until` | Wait | "Wait ⟨duration / until date⟩" |
| `if` | Only if… | "If ⟨condition⟩ → … otherwise → …" |
| `repeat` / `while` | Repeat | "Repeat up to ⟨n⟩ times until ⟨condition⟩" |
| `parallel` | Side by side | "Do these side by side" |
| `forEach` | For each item | "For each ⟨list⟩ → …" |
| `chain` | Group | named sub-outline |

Conditions are built from typed pickers (source step → field → comparison → value) rendered back as a sentence ("the review found no high-priority issues"). Regex stays available under an "advanced pattern" affordance only.

**D4 — One shell.** Persistent left rail: **Home · Projects · Reviews · Automations** + bottom **Activity · Help · Settings**. No "Tools" disclosure. "Reviews" is the review suite (new review, history, designer, gallery as tabs/subroutes of one destination). Every destination has a route; startup restores any of them; back/forward works. Workspace remains project-scoped (entered through a project), not a rail item.

**D5 — One interaction foundation.** A `ui/` primitive kit (Button, IconButton, Input, Select, Card, Tabs [wrapping `useTabList`], Toggle, Table, EmptyState, Spinner, Badge, Dialog [via `DialogService` only]) with design tokens (one neutral scale, one accent, spacing/radius/type ramp) replaces per-directory `shared.tsx` constants and page CSS incrementally. New/touched surfaces must use it; untouched surfaces migrate opportunistically.

---

## 4. Work packages

Sizes: S ≈ ≤2 days, M ≈ ≤1 week, L ≈ 1–3 weeks of focused agent work, before qualification. Dependencies are listed; WP-00 and WP-01 unblock everything else.

### WP-00 — Route model and shell state (L) — *prerequisite*

Addresses UX-30, UX-21 (prior audit).

1. Introduce an in-memory router with typed routes for every destination: the 12 `AppPage` values, workspace section/subpage (`/project/:id/:section/:sub?`), settings section, designer selection, history filters, report tabs, automation detail. Hash-based URLs so routes are copyable in dev; no filesystem/deep-OS integration required.
2. Replace the 6 CustomEvent channels with typed navigation calls; keep the events firing one release for compatibility, then remove.
3. Decompose `useAppController` (1,113 lines, ~109 returned values) into route-scoped controllers; the shell keeps only session-wide state (theme, deps, updates, notices).
4. `startupPage()` restores the full last route (validated, degrading to the nearest ancestor: unknown project → Projects index).
5. Keep `confirmLeaveCurrentPage` semantics as a router guard (save handlers and destructive confirms preserved).

**Acceptance:** every destination reachable by route; app relaunch restores designer/tasks/history/settings locations (kills the UX-21 limitation); back/forward traverses project subpages; all existing navigation tests pass rewritten against routes; no `window.dispatchEvent` navigation remains in `src/`.

### WP-01 — Design tokens and primitive kit (M) — *prerequisite for visual work*

Addresses UX-36; builds on the good primitives that exist (`ResizeHandle`, `SplitView`, `useTabList`, `SidebarPanel`, `DialogService`).

1. `src/ui/` package: tokens (single neutral scale — settle on `neutral`; accent; spacing/radius/type) + the primitives in D5, each with light/dark and keyboard/ARIA contracts, replacing the four `shared.tsx` button/input/card constants.
2. One `EmptyState` (icon slot, one-line message, optional action) and one `Spinner/Busy` with action-specific label; migrate the ~15 empty states and ~10 loading strings as their screens are touched.
3. One icon module consolidating the three private SVG sets (NavRail, HomePage, WorkspaceIcon).
4. Consolidate modal usage on `DialogService`; `useModalDialog` and self-portaling dialogs (`WorkspaceToolPicker`, `AddStepDialog`) migrate.
5. Lint guard: forbid new `dark:bg-gray-*` (wrong scale) and raw `<button className="rounded…">` in touched files (soft check in CI like the existing size-policy script).

**Acceptance:** kit used by every WP-02+ surface; Workspace/designer/settings render both themes from one scale on touched screens; axe-style checks pass on primitives; no new page-scoped CSS files.

### WP-02 — Shell, rail, and vocabulary (M) — depends on WP-00

Addresses UX-31; finishes the D4 shell.

1. Rail becomes Home · Projects · Reviews · Automations · (recents) · Activity · Help · Settings. "Tools" disclosure removed. "Review designer / history / collections / New review" collapse into the **Reviews** destination as subroutes (Run, History, Designer, Gallery tabs); "Automations" is top-level.
2. Vocabulary table applied across all copy: **review workflow** (was profile/workflow), **review** (a run), **automation** (a chain run), **project** (the only container), **version** (snapshot). Rail, page titles, buttons, settings sections, and empty states use identical terms — the "New review/New run/Review report/Report history" drift ends.
3. Home keeps its triad but the three cards route into the new destinations; "Tools and history" card list goes away (redundant with rail).
4. Badge semantics unified: one attention dot vocabulary (running = accent, needs-you = amber) fed by the Activity store (WP-06), announced politely to screen readers.

**Acceptance:** no orphan pages (gallery, batch, compare reachable via visible routes); a copy audit finds each concept named identically in rail/page/buttons/settings; persisted IDs, stores, and command names unchanged.

### WP-03 — Project unification (L) — depends on WP-00; pairs with WP-07

Addresses UX-28, UX-40.

1. Project detail gains a **Reviews** section: ledger-first view (open/regressed/addressed/dismissed counts, issue list with occurrence history — today's `ProjectIssueLedgerPanel`), then runs grouped by document lineage (`sameInputLineage`), then compare. "Start a review" here pre-fills the project's manuscript.
2. The standalone "Review collections" page is removed from navigation. Existing collections surface as project Review sections: a collection whose runs match a workspace project's documents is offered for linking; unmatched ones appear under **Unfiled reviews** (same component, no container pretense). On-disk `projects/{id}.json` and ledgers are untouched — this is presentation-level adoption, with an explicit link table persisted in workspace project records.
3. Review completion flow: "Open report · Add to project · Start an automation from this result" — replacing the publish-findings checkbox mystery (the checkbox remains in the designer as "Track findings in the project ledger" with a plain explanation).
4. History page becomes the Reviews→History subroute; its collection filter becomes a project filter.
5. New-project creation absorbs the review-collection creation path (one dialog, shared modal focus handling per UX-01).

**Acceptance:** a user can go from a finished review to the project ledger in one click; both prior "project" lists appear in exactly one place each; no data migration required to open old collections; naming collision from UX-28 is impossible to reproduce in the UI.

### WP-04 — Automation composer (L) — the centerpiece; depends on WP-00/01/02

Addresses UX-27, UX-29, UX-34. Exposes the existing chain engine; **no engine changes** beyond §6's small additions.

1. **Outline editor** replacing the template-or-JSON dichotomy: add/indent/reorder steps from a palette of the D3 vocabulary; every row renders as a sentence with inline editable slots (prompt, workflow picker, file picker, condition, iteration cap). Keyboard-first (arrows + Enter/Tab), drag secondary. The existing three templates become starting outlines in a template strip, fully editable afterward.
2. **Condition builder**: source picker (a prior step's structured output — e.g. a review's `highPriorityCount` / `complete`; a named input; a file's existence) + comparison + value, composable with All/Any/Not rows; rendered sentence preview; "Unknown → pauses for your attention" stated inline (three-valued logic surfaced honestly).
3. **Bindings without JSON pointers**: where a step consumes a prior output, a picker lists the prior steps' documented output fields (the `reviewOutput` contract, snapshot paths, input keys); the `{{output:step#/pointer}}` string remains visible in an inspectable "exact reference" row for trust, not as the editing surface.
4. **Scope & preview** panel (exists today as `task_prepare`) rendered as the outline with resolved names — kept as the pre-start confirmation; limits (max actions, timeout, deadline) become visible fields with the engine defaults.
5. **Entry points**: "Automate this" from a conversation (pre-binds the session), from a review report ("re-run when the manuscript changes" / "address-and-re-review loop" starting outlines), and from the project Automate section. Task cards inside the workspace open automation detail in-place (split or drawer), not by leaving the workspace (kills UX-29's context exit).
6. **Detail view**: the same outline, live — per-step receipts, current step highlighted, "waiting for you" input steps answerable inline, plain-language failure rows with the manual Retry the engine already supports. The raw JSON stays available read-only under "Definition (advanced)" with copy/import — power users and the CLI keep full fidelity.
7. Saved chains: "Save as template" on any outline (content-addressed saved_chains already exist); a project's Automate section lists its templates first. Import/export unchanged.
8. Missions and Self-discovery remain their own guided builders but move under Automations → Research as siblings, one level up from today.

**Acceptance:** the motivating scenario — *draft from a prompt → auto review → address feedback → review again → address again if needed → deliver* — is composable from an empty outline in under two minutes without seeing JSON, a regex, or an internal kind name; the produced definition round-trips through `task_validate_chain` unchanged; every chain the old UI could express remains expressible (JSON import still works); outline renders correctly for all 16 action kinds including 8-level nesting.

### WP-05 — Review designer and run setup (M) — depends on WP-01/02

Addresses UX-33 and the designer half of UX-27; carries prior findings UX-16/17 to done.

1. Designer opens on the **Overview** (wave diagram + workflow summary + validation state), not "Select a step to edit"; the diagram gets the full right pane when no step is selected.
2. **New review** becomes an integrated screen in Reviews: input drop zone, workflow picker with step summary, agent row, variables inline (no modal chain), and a live plan preview replacing the static 01/02/03 explainer; `RunPreview` becomes the confirm state of the same screen. `WorkflowPanel`'s duplicate read-only list is deleted in favor of the designer's navigator rendered read-only.
3. Step editor: "When it runs" absorbs conditions and fan-out in Basic mode using the D3 condition-builder component ("Run only if the referee report recommends major revision" — sentence, not regex; regex under Advanced pattern). Output schema moves from a separate Schemas tab to a per-step "Output contract" disclosure; the Schemas tab remains as the cross-step overview.
4. The Basic/Advanced toggle stops hiding capability categories; it only gates raw-value affordances (regex, JSON schema source, model policy keys).
5. Gallery templates open *in* the designer as drafts (today they import-then-hunt); "Duplicate/Rename/Delete" become a kebab menu with confirm via DialogService.
6. Remove the dead `use_orientation` badge row; orientation stays visible as a fixed pipeline stage in Overview.

**Acceptance:** opening the designer never shows an instruction-only pane; a conditional escalation step is authorable without typing regex; New review fits 1024×700 with no dead half-screen; UX-16/17 acceptance rows in ASTRA_SEP7_UIUX.md can be marked implemented by reference to this WP.

### WP-06 — Activity center and run monitoring (M) — depends on WP-00/02

Addresses UX-35, half of UX-40.

1. One **Activity** destination (replacing the overlay): live rows for review runs, automations (with current outline step), batch jobs, workbench jobs, missions/discovery — each with status, elapsed, and needs-you flags; history below, merged and filterable. Existing event streams and polling invalidation (`tasks:changed`, `pipeline:stage`, coalesced re-reads) feed one store; no new backend events required.
2. `PipelineProgress` remains the in-context run view (inside Reviews → run detail); `Console` docks inside it as a "Log" disclosure rather than a page-level bottom dock that appears only when logs exist.
3. `BatchPanel` becomes Reviews → run detail for a batch (same chrome, per-job rows); the separate `batch` page route is kept but renders within Reviews.
4. Rail badges and the workspace attention dot read from the same store; notification preferences already in Settings apply per category.

**Acceptance:** one place answers "what is running / what needs me / what finished"; no surface exists that can only be discovered by a log appearing; run detail shows progress + log + report tabs in one location; badge counts match Activity rows.

### WP-07 — Workspace consolidation (L) — depends on WP-00/01; pairs with WP-03

Addresses UX-32 and the workspace half of UX-29.

1. The 31 destinations collapse into the 6 sections as real **landing pages** (Overview, Library, Analyze, Write, Automate, Activity) — each landing page summarizes and links its subpages (Library: documents/files/papers/search/inbox/notes; Analyze: results/experiments/data/execution/theory; Write: manuscript/responses/edits/assets/delivery/sharing). Subpages are routes (WP-00), lazily mounted, not 28 retained views; retained-view memory stays only for the heavy editors (manuscript, files).
2. `WorkspaceResearchPanel`'s eight single-tab reuses are split into their owning subpages; the panel remains only as the assistant-settings inspector (its `setup` tab) — the 1,155-line component dissolves along module seams that already exist (`research-panel/useResearchPanelController.ts`).
3. The **Automate** section hosts project-scoped automations (WP-04 list + composer) and scheduled checks; action items stay distinct ("Action items are notes-with-intent; Automations run" — one sentence, shown once).
4. The `Context` inspector stub is removed; the context tray gets an "expand" affordance showing exactly what the model will see (`contextPreview` exists server-side) — the honesty surface researchers actually need.
5. ⌘K picker remains over everything (sections, subpages, papers, conversations) and gains automation/review verbs ("Review manuscript…", "New automation…").
6. Keep the desk split behavior (`WorkspaceDesk`) as-is; fold `SplitView` and `WorkspaceDesk` into one splitter implementation during this pass (both already use `ResizeHandle`).

**Acceptance:** every subpage reachable in ≤2 clicks from the project without the palette; mounted-component count on project open drops from ~28 to ≤6 (measurable in the fixture); no `allowedTabs={[single]}` usage remains; conversation → automation → back preserves draft and scroll (existing draft-persistence tests extended).

### WP-08 — Settings restructure (S–M) — depends on WP-02 vocabulary

Addresses UX-37.

1. Sections follow the product model: General · Connections & models · Reviews · Automations · Projects & storage · Notifications · Advanced. Items relocate so `sectionForTarget()`'s four exceptions disappear; the function reduces to the declared map.
2. Search stays (it's good) but stops being the only way to find relocated items; the four legacy alias routes redirect for one release, then are removed with their deep-link sources updated (`DepsCheck`, provider banner, workspace gear).
3. The seven scattered connection/storage components (`ChatgptConnection`, `WorkflowCodexConnection`, `WorkspaceConnectionSettings`, `AgentDefaultsControl`, `StorageSettings`, `WorkspaceResearchDataSettings`, `WorkspaceStorageRetention`) render inside Settings sections only; in-context gears link to the relevant section route (WP-00 makes that addressable).
4. Appearance gains the density control the workspace-ui plan deferred, applied via tokens (WP-01).

**Acceptance:** every setting findable by browsing in ≤2 levels; `settings/navigation.ts` has no exception table; deep links from all in-app sources land on the focused control via routes, not DOM walking.

### WP-09 — Removals (S)

Addresses UX-38. Explicit list; each removal lands with a test proving the surrounding flow still works.

| Item | Action |
|---|---|
| `use_orientation` editor row + type comment ritual | Remove from UI; keep field in schema for compatibility |
| `main` idle "01/02/03" explainer (`App.tsx:483-524`) | Deleted by WP-05's integrated New review |
| `WorkflowPanel` read-only list + unreachable `editorOpen` branch | Delete component (291 lines) in WP-05 |
| `Context` inspector stub (`WorkspacePage.tsx:912-957`) | Replaced in WP-07 by real context preview |
| `WorkspaceProjectSurface` alternate `!navigationInSidebar` chrome (`:544-614`) | Delete; fixture updated |
| `SplitView` as a separate implementation | Merge with `WorkspaceDesk` splitter (WP-07) |
| `pipeline.tasks.enabled` stale gate (`TasksPage.tsx:101`, `useWorkspacePageController.ts:78`) | Remove flag; task cards always available |
| Legacy plan matching in `PipelineProgress.tsx:400-420` | Remove after confirming no pre-1.1 runs render through it (History already normalizes) |
| Settings legacy alias routes | Remove one release after WP-08 |
| `FlappyBirdGame` | **Keep.** It is reachable only by Konami code, self-contained, and beloved by exactly the audience that finds it. |

Not removed (explicitly): legacy Codex CLI transport (documented migration path), retired built-in profiles in `.retired-builtins/`, history JSON readers, `antigravity` provider id — all compatibility surface.

### WP-10 — Qualification, tooling, and drift control (M, continuous)

Addresses UX-39, UX-26 (prior), and protects the Sep 20 gates.

1. **Commit the full-app browser fixture** added during this audit (`gui/e2e/app-full.html`, `app-full.tsx`, `app-full-shim.ts`): fake `__TAURI_INTERNALS__` with a command map + logged misses, `?page=&theme=` params. Extend its sample data as WPs land; wire the route matrix into vitest the way `projects.tsx`/`workspace-layout.tsx` fixtures already are, so every route renders without IPC in CI. (The previous session's equivalent fixture was lost precisely because it was never committed.)
2. Visual matrix per WP: 1440×860 and 1024×700, light+dark, on the fixture; screenshots attached to the implementation update appended to this file.
3. Type drift: introduce `ts-rs` (or equivalent) generation for every command a WP touches; new commands may not add hand-written mirror types. Target: `types.ts`/`workbenchTypes.ts` shrink monotonically.
4. Keep release gates intact: nothing in WP-00…WP-09 claims packaged/Windows/real-account qualification; the Sep 20 outstanding-qualification list transfers unchanged.
5. `scripts/check-markdown-links.mjs` and the source-size policy stay green; large files touched by WPs (WorkspacePage, ReportWorkspace, useWorkspacePageController, HistoryPage) leave smaller than they arrived.

**Acceptance:** CI renders all routes headlessly; a deleted/renamed command fails the frontend build, not runtime; this document carries a validation record per landed WP.

---

## 5. New functionality summary (what a user gains)

- Compose conditional, iterative research automations in plain language — including the draft→review→revise→re-review loop — from any conversation, review, or project, without JSON or regex (WP-04).
- Start automations and reviews from where their inputs live; answer an automation's questions inline in the workspace (WP-04/07).
- One project containing everything about a paper: conversations, library, reviews with a ledger-first "state of the draft" view, automations, activity (WP-03/07).
- Linkable, restorable locations everywhere; back/forward (WP-00).
- One activity surface answering "what's running, what needs me, what finished" (WP-06).
- A designer that teaches the engine's real capabilities instead of hiding them (WP-05).

## 6. Engine asks (small, deliberately deferred until the composer proves need)

These are *not* prerequisites; the composer ships on the engine as-is. Listed so they aren't reinvented ad hoc:

1. Saved-chain **reference** (by content hash) from a `chain` step, so templates compose without inlining (today: inline `Box<Chain>` only, `orchestration/store.rs:711`).
2. An `onFailure` branch or per-step `continueOnAttention` flag, so a failed optional step doesn't halt the whole automation (today: manual Retry from attention only, `orchestration/state.rs:216`).
3. Comparison operators beyond `Equals/LessThan/Exists` (at minimum greater-than and contains) in chain conditions.
4. Revisit the one-active-`review`/`workspace` admission cap before advertising `Parallel` for research branches (`orchestration/mod.rs:299`) — a measured, separately qualified change per the PI-12 note.

## 7. Sequencing

- **Phase 1 (foundations):** WP-00 → WP-01 → WP-02. The app looks similar but every location is a route, primitives exist, the rail is flat. No data changes.
- **Phase 2 (the thesis):** WP-04 with WP-05 in parallel (different code areas); then WP-03 + WP-06.
- **Phase 3 (consolidation):** WP-07 → WP-08 → WP-09 sweep. WP-10 runs throughout.

Rough total: 8–12 weeks of focused implementation at recent repo velocity, excluding release qualification. Each WP lands behind the existing test gates and appends its implementation update + visual record to this file.

## 8. Method and validation record (this audit)

- Full frontend and backend architecture mapped (three parallel audits over `gui/src`, `gui/src-tauri/src`, and `docs/`+`notes/`+`development/`); key claims verified directly in source.
- Live UI inspected in the browser against the user's running Vite dev server via a **new full-app fixture** written for this audit: [gui/e2e/app-full.html](gui/e2e/app-full.html), [gui/e2e/app-full.tsx](gui/e2e/app-full.tsx), [gui/e2e/app-full-shim.ts](gui/e2e/app-full-shim.ts) (fake `__TAURI_INTERNALS__`, realistic minimum-wage-paper sample data, `?page=` and `?theme=` params, console-logged stub misses). Inspected at 1440×860, light and dark: Home, Projects index, project workspace (split view, tool picker, composer), Review designer (Basic/Advanced, step editor, Execution rules, Overview diagram), New run, Report history, Review collections + ledger, Automations (templates, outline preview, JSON editor). Settings renders in the native app but its loader needs an additional stub the fixture doesn't provide yet (`SettingsExperience` envelope) — noted for WP-10.
- Verified live: the run_if regex editor; the "Select a step to edit" dead-end; the internal-kind outline labels; the two same-named "project" objects; startup restoring only 4 pages; "New review/New run/Report history" naming drift; dep-status "Check failed"→"System ready" driven by `get_execution_plan.readiness`.
- The Tauri dev app builds and launches cleanly against an already-running Vite server with `npx tauri dev --config '{"build":{"beforeDevCommand":""}}'` (one duplicate-instance caution: the packaged coordinator takes an exclusive owner lock).

---

*Implementation updates append below this line.*

## Implementation update — September 30, 2026 (Claude Fable 5)

All P1 and P2 work packages (WP-00–WP-08, plus the WP-09 items embedded in them) are implemented in the working tree. Validation: the full frontend suite passes (**103 files / 737 tests**, ~15 s), TypeScript and the production build pass, Prettier passes, and every surface below was verified live in the browser fixture at 1440×860 in light and dark themes. `check:source-size` and `check:links` cannot run in the implementing environment (no system `rg`); all touched files were verified under the 1,200-line/50 KB policy manually — `WorkspacePage.tsx` (1,102→1,058) and `useAppController.ts` (1,113→1,090) both shrank.

| WP | Status | What landed |
|---|---|---|
| WP-00 | **Done** | `lib/router.ts`: typed `AppRoute` union for all 13 destinations, guarded `navigate`/unguarded `apply`, subscriber revisions, hash serialization (`#/reviews/history?run=…`) with webview back/forward via `hashchange`, and full-route persistence (`pipeline.ui.route`, legacy `pipeline.ui.page` fallback kept). `startupPage` → `startupRoute` restores **any** destination (closes UX-21/UX-30). All six `window.dispatchEvent` channels are gone: navigation events became router routes (the workspace controller subscribes for in-place project/session switches); `open-file`/`context-add`/`workspace-destination` became the typed `lib/appEvents.ts` bus. `useAppController` derives settings/help/history/tasks params from the route; a synchronous route-commit subscription does storage prep before render. Tests: `lib/router.test.ts` (serialize/parse round-trips, guard, revisions, persistence, hash back-navigation). |
| WP-01 | **Done** | `src/ui/` kit: `classes.ts` (canonical button/primary/danger/link/input/card/panel/muted/notice vocabulary on the single `gray`-as-neutral scale), `Button`, `EmptyState`, `Spinner` (always labeled), `Badge` (running/attention/ok/error vocabulary), `icons.tsx` (the one icon set; NavRail and Home now draw from it). The four divergent `shared` style modules (project-surface, research-desk, research-studio, research-programs) now re-export the kit, so every feature area renders one button/input/card. Finding correction: the Tailwind config already aliases `gray`→`neutral` and an adaptive accent already exists — UX-36's "two dark scales" were visually identical; the fix was vocabulary, not retheming. |
| WP-02 | **Done** | Flat rail: Home · Projects · **Automations** (top-level) · Recent projects · **Reviews** group (New review, Review history, Review designer) · Activity · Help · Settings. The "Tools ▸ Show advanced tools" disclosure is deleted. Vocabulary pass: "New run"→"New review", "Review report"→"Start review", "Report history"→"Review history" everywhere (pages, settings, tests); history is rail-active for batch and current-run states. Home's redundant "Tools and history" card list removed. Badges unified on `ui/Badge`. |
| WP-03 | **Done (presentation-level)** | New project destination **Reviews & findings** (`project-surface/ProjectReviews.tsx`, in the Write section): ledger-first (reuses `ProjectIssueLedgerPanel`), then the project's runs, with "Start a review". Scoping is deterministic and migration-free: runs whose `input_path` lies under the project's folder; collections containing such runs render their ledger in place, others appear as "Other review collections". "Review collections" left the rail; it remains reachable from Review history ("Collections") and from the project Reviews view. Step-editor checkbox renamed to "Track findings in the project ledger" with a plain explanation. Tests: `ProjectReviews.test.tsx` (root-scoping, unfiled handling, no-folder state). |
| WP-04 | **Done (composer core + entry points)** | The centerpiece. `tasks/language.ts`: one user-facing name and sentence per chain step kind (`workspace`→"Ask the assistant", `snapshot`→"Save a version", `if`→"Only if…", …), recursive condition/binding describers, documented output fields (the `reviewOutput` contract) for pickers. `tasks/ConditionEditor.tsx`: typed condition builder (source picker over prior steps' documented outputs, is-true/false/equals/below/exists, and/or rows, negation) with a live sentence preview and the three-valued-logic note; structures beyond the picker round-trip untouched with a JSON-edit pointer. `tasks/OutlineEditor.tsx`: structural outline editing — add (grouped palette: Do/Wait/Logic), reorder, delete, nest (if/else, repeat-until, while, parallel branches, for-each, groups), per-kind inline editors, binding pickers with "exact reference" fallback and first-available fallbacks. `Builder.tsx` rebuilt: template strip (+ **Start empty**) generates editable outlines; quick fields regenerate while pristine and hand over on the first structural edit; "Definition (advanced)" is a read-only JSON view with copy/import/saved-templates/**Save as template** and limit fields; `task_validate_chain` still gates prepare. The read-only `Outline` (builder preview and run detail) now renders sentences — internal kind ids no longer appear anywhere. Entry points: "Automate follow-up" on every finished report (prefills the review-and-revise template with the run's input via the new `automatePath` route param). Acceptance test passes: the draft→review→revise→re-review loop is **composed from an empty outline** in `OutlineEditor.test.tsx` and the prepared chain round-trips validation with the expected structure. |
| WP-05 | **Done** | Designer opens on the workflow overview (full-pane `WaveDiagram`, selection opens the stage editor) — "Select a step to edit" is gone (closes UX-16). New review screen: slim `WorkflowPanel` (profile picker + adaptive-agents note only; the read-only step list and unreachable `editorOpen` branch are deleted) beside a live `RunPlanPreview` (workflow name, input status, wave diagram; any stage opens the designer) replacing the static 01/02/03 explainer. Step editor: **"When it runs"** is a Basic-mode tab; run conditions get a sentence preview, step *labels* instead of ids, and a **"contains the text"** mode that writes an escaped pattern — the escalation condition is authorable without regex (regex remains one select away). Dead `use_orientation` badge row removed. Gallery install banner gained "Open in the designer". |
| WP-06 | **Done (v1)** | Activity is a routed destination (`/activity`): live turns/jobs/pending-requests/automations/reviews with per-row controls, "Nothing is running right now", and recently finished reviews; rail badge vocabulary unified; leave-guards apply to it like any destination. The Console log dock renders only on the run view instead of every page. (The overlay variant remains for embedding; the rail no longer opens it.) |
| WP-07 | **Done (targeted)** | The `Context` inspector stub is now `WorkspaceContextInspector`: the real honesty surface — enabled/unavailable tools with reasons and the assembled `contextPreview` with truncation notice, from `effectiveHarness`. A section subnav above the project surface puts every destination in the current section one click away (suppressed on Overview, which already links everything); the ⌘K palette gained action verbs ("Start a review…", "New automation…"). The stale `pipeline.tasks.enabled` gate is deleted — automations are always available from conversations. Finding corrections: `RetainedWorkspaceView` already lazy-mounts (the "28 views mounted at open" reading was wrong — retention is post-visit, by design), and the "unreachable alternate header" is load-bearing harness chrome for direct-rendered surfaces (tests/fixtures) — kept, labeled as such. |
| WP-08 | **Done (targeted)** | `sectionForTarget`'s per-target exception table is deleted — every caller (search index, jump links, deep links) declares its real section; deep links into settings are now router routes (`#/settings/extraction?target=paddleocr-local-engine`). The density control already ships (finding correction: it exists in Preferences). Legacy alias sections intentionally retained for this release per plan. |
| WP-09 | Partial (embedded) | Done via the WPs above: idle explainer, WorkflowPanel duplicate list + `editorOpen`, `use_orientation` row, Context stub, tasks-enabled flag. Kept deliberately: standalone surface header (harness chrome), legacy plan matching in PipelineProgress (needs its own verification), settings aliases (one-release policy), Flappy Bird. |
| WP-10 | Ongoing | Fixture extended for every new surface (`activity` route, research-activity/task-session stubs); all composer/router/reviews behavior locked in new unit tests. `ts-rs` adoption and CI fixture-matrix remain open. |

**Remaining work (honest list):** dissolving `WorkspaceResearchPanel` along its module seams (its single-tab reuse already renders without tab chrome, so this is architecture debt, not UX debt); merging `SplitView` into `WorkspaceDesk`; batch view relocation under Reviews; task-card automation detail opening in-place inside the workspace (today it navigates to Automations); saved-chain references, `onFailure` branches, richer comparisons, and the parallel admission cap (§6 engine asks); settings alias removal next release; `ts-rs` type generation. One pre-existing intermittent test flake was observed twice across ~10 full-suite runs (settings suite timing); it reproduces on `main` inputs unrelated to this change set and is not introduced here.

**Visual record (browser fixture, 1440×860):** flat rail + decluttered Home (light); designer opening on the wave-diagram overview; New review with live plan preview and slim workflow panel; Automations builder with the four-template strip and the sentence-rendered outline editor (review-and-revise template: "Repeat up to 3 times until the review finished…", "Only if the result of 'Run the Review profile' exists", per-row move/remove, per-level "+ Add step…"); Activity destination in dark mode. The `?page=` fixture parameter now deep-links every destination through the persisted route.

## Implementation update — October 1, 2026 (Claude Opus 5.5)

Two findings raised after the September 30 pass, both implemented in the working tree. Detail and data boundaries are in [docs/workbench/projects-home.md](docs/workbench/projects-home.md).

| ID | Sev | Finding | Resolution |
|---|---|---|---|
| UX-41 | P1 | **The project overview showed only what the researcher typed.** Conversations, self-added notes, and action items; the one derived signal was a stale-claims count. The fixture project rendered the same conversation twice plus three empty states, while the issue ledger, binding coverage, change impact, build receipts, response records, and scheduled-check attention stayed buried in their destinations. | Overview rebuilt around derived state (WP-11). |
| UX-42 | P2 | **A project folder's Git repository was invisible.** Git was used only as an edit-checkpoint backend; coauthor commits, uncommitted work, and unpushed commits never reached the project. | Local repository status plus an explicit remote check (WP-12). |

| WP | Status | What landed |
|---|---|---|
| WP-11 | **Done** | `lib/projectOverview.ts` (pure derivations: `draftBlock`, `syncBlock`, `roundBlock`, `sinceBlock`, `leftOff`, visit tracking, `isThinProject`) and `hooks/useProjectSignals.ts` (independent, failure-tolerant lazy loads; reload on return, on request, and at most every two minutes). `ProjectOverview.tsx` renders blocks of single-action rows and hides empty ones: State of the draft, Out of sync, Revision round, Since you were last here, Needs your decision, Other conversations. The resume card became "Where you left off" (last request, unsent draft, runs, next-step notes). "Draft a brief from the paper" opens an unsent assistant draft. Backend: `ProjectHome.workingCopyChanged` (structured count beside the prose status) and one optional `targetDate`/`targetLabel` on home settings, validated and default-compatible with older records. Conversation-only projects get a reduced page and, via `WorkspaceDesk`'s unsaved `conversationFirstRequest` default, open on the chat with the brief under the title; an explicitly chosen layout always wins. |
| WP-12 | **Done (read-only v1)** | `workbench/project/repository.rs`: `inspect` (branch, upstream, ahead/behind as of the last fetch, uncommitted/untracked/conflict counts, 30 recent commits touching the folder, incoming commits) and `fetch` (remote-tracking refs only, credential prompts disabled, 30 s limit), reusing the hardened `tasks::git` runner and running outside the database workers. Remote URLs are reduced to host/owner/name in the backend; only `github.com` yields links. Commands `workbench_repository_status` and `workbench_repository_fetch`; a Repository block on the overview, commits in "Since you were last here", and a Repository card in Project settings. `PRIVACY.md` lists the fetch as a user-initiated network request. |

**Finding correction.** The September 30 analysis assumed `workbench_session_handoff` could supply a conversation summary. It writes a host-generated inventory (proposed notes, open tasks, runs since the session began) as a draft record and is a mutation, so the overview does not call it; "Where you left off" derives the same facts read-only and adds the last request and unsent draft.

**Deliberately not built.** Kanban, progress bars, activity or word-count charts, a literature feed on the overview, and deadline tracking beyond the single target date. For the repository: commit, push, merge, and pull-request creation (outward-facing, need an explicit product decision), and pull requests, issues, and CI status (need GitHub API authentication: `gh` CLI or a stored token).

**Validation.** Frontend: 105 files / 773 tests, TypeScript, production build, Prettier. Native: `workbench::project` 52 passed / 6 ignored, including a clone-and-fetch round trip against a local bare remote; the full Rust suite was not rerun. Fixture (`?project=rich|thin|bare`) inspected at 1440×860 light and dark and 1024×700. Not exercised: the native window, a fetch against github.com, and the derived sources on a large real project (binding coverage and change impact are time- and byte-bounded in the backend, but their cost there is unmeasured).

**Next finding ID: UX-43. Next work package: WP-13.**
