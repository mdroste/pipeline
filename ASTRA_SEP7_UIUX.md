# Pipeline UI/UX review and implementation plan

**Prepared:** September 7, 2026, America/Los_Angeles  
**Scope:** Current working tree on codex/nf07-nf14; desktop React/Tauri presentation, interaction, frontend state, and the service boundaries that affect the user experience.  
**Original deliverable:** Implementation plan. The subsequent UX-01 through UX-10 implementation is recorded below; the review findings describe the audited baseline.  
**Design objective:** A coherent, restrained research application with accessible controls, readable content, predictable navigation, and configurable, resizable work surfaces.

## P1 implementation update

UX-01 through UX-05 are implemented. UX-06 through UX-10 are recorded in the next update; UX-11 onward and the wider modular-layout program remain planned work.

| Finding | Implemented behavior |
|---|---|
| UX-01 | New project uses shared modal focus handling, inert background controls, a unique title/description association, and return focus. Busy creation cannot be dismissed accidentally. |
| UX-02 | Reader-specific heading, issue and footnote identities; contents/body fragment navigation stays inside its reader. Canonical source anchors remain available to file navigation. |
| UX-03 | One serialized save path handles autosave, Save details, collection changes and route leaving. Failed writes preserve the draft; edits during a write are saved before leaving. Collection dirty state participates in native close protection. |
| UX-04 | Container-aware Project/Assistant/Show both controls; optional navigation/inspectors collapse when space is insufficient. Hidden panes retain state, requested widths survive temporary fitting, and the composer remains inside the minimum supported window. |
| UX-05 | Semantic light/dark contents colors, visible keyboard focus and an active-location state replace low-contrast heading/issue labels. |

Regression coverage includes modal Tab/Shift+Tab and busy dismissal, two copies of one Markdown document with footnotes, failed and overlapping saves, app navigation/close protection, and narrow/wide desk transitions with retained drafts. Browser fixtures reproduced correct modal return focus and right-reader-only scrolling (left 0 px, right 834 px). At 1024 × 700, the composer and Send control remained in view without page overflow. Light secondary contents text is now #525252; dark text is #a3a3a3, with distinct tested focus/active colors.

Validation: the full frontend suite passed (81 files, 611 tests); the focused project-width regression suite passed (7 tests). The production frontend build and final TypeScript check passed. The existing lazy PDF chunk-size warning remains. Contents contrast is approximately 7.49:1 for #525252 on #fafafa and 7.11:1 for #a3a3a3 on #171717.

These are frontend implementation and synthetic-browser checks. Native assistive-technology, packaged, authenticated and cross-platform qualification remain separate release gates. The original automated-provider review limitation below does not affect the local implementation.

## UX-06 through UX-10 implementation update

Implemented September 7, 2026. The original findings below remain the audit baseline.

| Finding | Implemented behavior |
|---|---|
| UX-06 | Shared `SplitView` replaces equal-fraction file/file, source/preview and PDF comparison grids. Saved ratios and Automatic/Side by side/Stacked preferences, independent pane focus, horizontal/vertical resizing, local minimum sizes and retained mounted panes. Automatic splits stack below 960 px; insufficient space focuses the last-used pane. Each Markdown reader collapses its outline below 760 px and exposes a local Contents control with reduced reading padding. |
| UX-07 | Shared `ResizeHandle` supports mouse/touch/pen Pointer Events and capture, rAF DOM previews, one commit on release, Escape/cancel/lost-capture/blur/unmount cleanup, pane association, contextual bounds, and preserved requested widths. Arrow/Shift/Home/End/reset remain; Enter opens numeric sizing with steppers, validation, reset, inert background and return focus. |
| UX-08 | Shared `useTabList` now owns ReportWorkspace and Tasks keyboard behavior and unique tab/panel relationships. Reports retain automatic activation; Tasks moves focus with arrows/Home/End and activates with Enter/Space because it loads native data. Missions participates in the same panel contract, including lazy loading. |
| UX-09 | File names are actual tab buttons with one roving tab stop and scoped panels. Separately named close buttons contain no nested tab controls. Delete closes the focused file, moves focus to a neighboring tab (or Quick open), and retains unsaved buffers/recovery drafts. Late reads cannot undo a newer selection; Back/Forward reopens the corresponding tab. |
| UX-10 | The rail groups New run, Current run, History, Run collections and Designer under Workflows. Workspace retains research projects, with Action items for local research tasks. Tasks and its composer entry identify durable task chains; missions retain their own destination. Collection forms, filters, ledger, help, settings and task launch copy use the same terms. Persisted names/IDs, native commands, records, credentials and execution ownership are unchanged. |

Implementation entry points: [SplitView.tsx](gui/src/components/SplitView.tsx), [ResizeHandle.tsx](gui/src/components/ResizeHandle.tsx), [useTabList.ts](gui/src/hooks/useTabList.ts), [FileWorkspace.tsx](gui/src/components/file-workspace/FileWorkspace.tsx), [ReportViewer.tsx](gui/src/components/ReportViewer.tsx), [TasksPage.tsx](gui/src/components/TasksPage.tsx), and [NavRail.tsx](gui/src/components/NavRail.tsx).

Regression coverage exercises capture/release and unrelated pointers, cancellation paths, commit frequency, numeric validation/return focus, contextual sibling bounds, persisted sizing/orientation, small-container focus and draft retention, local outlines, Tasks manual activation, file closing/reopening and delayed-read races. Existing report conditional-tab and keyboard tests now verify scoped IDs.

Validation: the full frontend suite passed **82 files / 631 tests**; after the final PDF comparison migration, the focused split/PDF/file suite passed **3 files / 12 tests** and the production build (including TypeScript) passed. The follow-up navigation/help label suite passed **2 files / 17 tests**. The existing 711 kB lazy PDF chunk warning remains. Actual React browser fixtures were checked at **1400 × 800**, **1024 × 700**, and in dark mode at **1280 × 720**. Numeric resizing produced 800/404 px file panes at the wide size with an outline only in the wide reader. Automatic stacking and source/preview panes retained usable widths at the minimum size; the source/preview check had no page-level horizontal overflow. The sizing dialog exposed only its own controls while open and returned focus on Apply. Reloading the fixture retained the split proportion.

These are local frontend and synthetic-browser checks. Native touch/pen hardware, screen readers, authenticated execution, packaged builds and cross-platform release qualification were not performed by this change. No model execution was needed to exercise these layout and navigation controls.

## 1. Assessment and decisions

Pipeline has substantially better foundations than its current presentation suggests. It already has persistent conversations, bounded transcript rendering, lazy research modules, a shared sidebar shell, keyboard-operable resize handles, report reading controls, an accessible report-tab implementation, guarded file saves, and explicit execution previews. These should become the basis of one consistent interface.

The central problem is composition. Features have accumulated as independently designed screens and panels. Each screen makes its own decisions about spacing, navigation, terminology, persistence, and scrolling. The result requires researchers to understand implementation structure before they can use the research tools comfortably. Adding more configuration switches to individual components will compound that problem unless a common layout and interaction model comes first.

The implementation should make five decisions explicit:

1. **One product shell, with clear destinations.** Workspace, Workflows, and Tasks remain distinct product destinations. Activity is a shared inspection surface. Workflow run collections must be visibly distinguished from Workspace research projects.
2. **One constrained layout system.** Panels have identities, supported positions, minimum usable sizes, visibility rules, and persistent user preferences. The shell computes a layout that fits the available space. Users can resize, hide, restore, move supported panels, and save named layouts.
3. **One interaction foundation.** Dialogs, tabs, menus, forms, splitters, notices, tables, and save states have shared behavior. Accessibility is part of their contract.
4. **Research objects lead the interface.** A paper, result, task, or revision opens a useful work surface with contextual actions. Raw records, hashes, command arguments, and schemas are available through details where needed.
5. **Migrate through complete user journeys.** Fix current defects first, then migrate the shell and one representative desk before rolling the system through every feature family.

The intended visual character is calm and precise: neutral surfaces, a controlled accent, consistent typography, generous reading space, compact but legible navigation, and minimal persistent chrome. Configuration should help a researcher arrange their work; ordinary use should require no layout setup.

## 2. Evidence, coverage, and limits

### Review method

- Inspected the application shell, major frontend routes, layout helpers, styling, dialogs, readers, task surfaces, settings, persistence code, representative research modules, and associated tests. Read the architectural and ownership documentation and traced relevant frontend-to-service contracts.
- Inventoried 170 production frontend TypeScript/TSX/CSS files, approximately 46,388 lines. The inventory is a scope measure, not a claim that every line received equal scrutiny.
- Created a fixed review snapshot containing 259 frontend and documentation files. A subsequent hash check found no changes to those snapshotted files during the audit.
- Ran the frontend suite and production frontend build.
- Reviewed actual React components in isolated browser fixtures using synthetic research data and mocked native commands. The fixtures could not dispatch model turns, access research files, change native settings, or run tools.
- Inspected the default 1400 × 800 desktop layout and the configured minimum 1024 × 700 layout, including dark and light surfaces, keyboard interaction, rendered accessibility trees, and DOM dimensions.
- Consulted primary W3C accessibility guidance and Apple layout guidance. Specific requirements and references appear below.

### Observed visual and interaction coverage

| Surface | Evidence obtained |
|---|---|
| New report | Default shell, setup column, input controls, workflow summary, first-use privacy notice, empty-state hierarchy |
| Workspace conversation | Conversation navigation, transcript, composer, model toolbar, context tray, follow-up disclosure |
| Workspace project | Overview, nested navigation, empty project setup, assistant composition at 1400 × 800 and 1024 × 700 |
| New project dialog | Initial focus, Shift+Tab escape into background content, Escape dismissal |
| Tasks | Empty state, task builder, task outline, tabs, arrow-key behavior |
| Settings | General and Review & workflows, light/dark styling, categories, model controls, existing appearance options |
| Workflow editor | Basic mode, profile actions, step navigator, empty selection, prompt editor |
| Report workspace | Reading, contents, typography controls, provenance, successful arrow-key tab navigation |
| File workspace | Markdown preview, file tabs, toolbars, duplicated split reader, duplicate IDs, wrong-pane anchor navigation |
| Other surfaces | Source review of history, run collections, issues, artifacts, console, PDF components, research tools, missions, execution/recovery contracts, and relevant tests |

The running native process was verified as the Tauri development version started by npm run tauri dev. The computer-use tool could not attach to its unbundled executable. The installed application was not opened. Direct browser access to the development frontend lacks Tauri IPC; that expected environment failure was excluded from product findings. The isolated fixtures provide frontend evidence, **not native, authenticated, packaged, screen-reader, or cross-platform qualification**. PDF rendering, real file dialogs, real execution, and end-to-end provenance exchanges still need the native checks specified in section 10.

### Engineering baseline

| Check | Result |
|---|---|
| Frontend suite: npm test | 79 files passed, 1 file failed; 592 tests passed, 1 failed, 593 total |
| Failing case | WorkspaceContextTray: “persists an exact passage and its role without replacing it with the current document”; timed out finding “Source revision-one” |
| Isolated rerun of WorkspaceContextTray.test.tsx | All 4 tests passed |
| npm run build | Passed TypeScript checking and Vite production build |
| Build diagnostic | PdfReader chunk approximately 711.29 kB minified / 217.93 kB gzip; Vite emitted its >500 kB chunk warning |
| Rust/native qualification | Not run as part of this plan-only review |

The isolated test pass leaves the full-suite failure unresolved. Investigate timing, effect readiness, and suite interaction before calling it a product regression or simply increasing the timeout. The PDF chunk is an investigation target; its size alone does not establish a performance defect, particularly because the reader is lazy-loaded.

### Pipeline-generated review preparation

A portable four-step workflow was generated from the current CLI template: three parallel responsibilities—layout/architecture, accessibility/interaction, and product journeys—followed by sequential synthesis. The current defaults selected Codex for parallel work. Merge was disabled. The CLI reported a work bound of four step units, zero merge calls, and twelve provider attempts; its plan also included the required orientation stage. The legacy use_orientation field is normalized by Pipeline and must not be treated as an effective disable switch.

Validation succeeded with schema version 11 and fingerprint:

~~~text
sha256:0c2f3ac9ed439399b4d2a7e626d76b83dde74973e4397c72ddc7a513e853f88e
~~~

The sandboxed preflight returned ready: false because the Workflow ChatGPT connection reported “Operation not permitted (os error 1).” Automatic approval review rejected the outside-sandbox preflight over possible transmission of private repository material. Specific permission was requested. No automated provider review was run, and no findings in this document are attributed to one. The local review and implementation plan were completed independently.

Review working files are under /private/tmp/astra-sep7-uiux-review/ and /private/tmp/astra-uiux-fixture/. These are temporary audit aids, not installed workflows or release artifacts. The durable deliverable is this document.

## 3. Prioritized findings

**Priority:** P1 = material accessibility, navigation, or work-preservation defect; P2 = significant coherence/usability problem; P3 = refinement. **Evidence:** V = reproduced in frontend fixture; C = directly established by source; R = risk requiring targeted reproduction. Source identifiers resolve to concrete files and line anchors in section 12.

| ID | Priority / evidence | Finding and user impact | Required change |
|---|---|---|---|
| UX-01 | P1 · V/C | New project declares a modal but does not contain focus. Shift+Tab from Project name focused the Workspace sidebar separator behind it. Escape left focus there. [S01] | Adopt the shared modal primitive; inert background, contained keyboard focus, explicit dismissal policy, and reliable return focus. |
| UX-02 | P1 · V/C | Split Markdown readers generate identical heading IDs and global fragment links. Clicking Technical details in the right reader scrolled the left reader from 0 to 834 px while the right stayed at 0. [S02, S03] | Namespace rendered IDs per reader instance and route fragment navigation through the owning reader. Keep canonical source fragments separately. |
| UX-03 | P1 · C | Workflow collection selection proceeds after saveProject catches a failed write. The next selection resets the edit fields, so an unsuccessful rename/description save can be lost. The app-level guard tracks only workflow/settings dirty state. [S04, S05] | Return an explicit save result, retain the draft after failure, block dependent navigation, and register collection editing with the shared draft guard. |
| UX-04 | P1 · V/C | The default project layout allocates 176 px to primary navigation and 288 px to Workspace navigation before splitting the remaining desk. In the fixture the assistant was about 368 px wide, with a roughly 287 px composer. At 1024 × 700 the two 480 px minimum-height panes place the composer below the viewport. [S06, S07] | Use container-aware fit constraints; collapse optional navigation/inspectors, offer a focused pane, and keep essential composer/approval actions reachable. |
| UX-05 | P1 · V/C | Light-theme report contents links render at 13 px in rgb(163,163,163). That is approximately 2.52:1 on white and 2.42:1 on #fafafa, below the 4.5:1 normal-text target. [S08] | Replace low-contrast informative text with tested semantic text tokens; test hover, focus, selected, and default states independently. |
| UX-06 | P2 · V/C | File splitting is fixed at equal grid fractions. Each nested report also reserves its own 240 px contents pane at a viewport-wide breakpoint, leaving two very narrow reading columns. There is no divider for the file split. [S03, S09] | Make reader splits resizable and container-aware; collapse each local outline according to the reader’s width. |
| UX-07 | P2 · C | ResizeHandle already supports arrows, Shift, Home/End, and reset, but mouse events are its only pointer path. It lacks pane association, drag cancellation, and a click-only means to choose arbitrary widths. Its fixed pixel bounds do not account for sibling panels. [S10] | Extend a shared SplitView/Splitter contract with pointer events, capture cleanup, aria-controls, contextual bounds, and a numeric/stepper alternative. |
| UX-08 | P2 · V/C | Tasks exposes tab roles but ArrowRight does not move focus or selection. All tabs remain in the normal Tab sequence and lack aria-controls. ReportWorkspace already implements the stronger pattern. [S11, S12] | Extract and reuse the report tab behavior; select automatic/manual activation according to load latency. |
| UX-09 | P2 · V/C | File tabs place opening and closing buttons inside a div with role=tab. There is no roving focus or panel association, and the accessibility tree conflates the filename and close action. [S03] | Use a coherent tab-strip pattern with unique pane IDs, one active tab stop, keyboard closing, and separately named close actions. |
| UX-10 | P2 · V/C | Projects in the primary rail means Workflow run groups; Project inside Workspace means a research workspace. Tasks appears globally and inside the project; Review alternates with report/workflow wording. [S13, S14] | Distinguish research projects, run collections, project action items, task chains, and missions through location and language. Preserve persisted names and runtime ownership. |
| UX-11 | P2 · V/C | A project has five primary sections and twenty secondary destinations; several have another internal tab row. The project selector, project card, project header, and composer repeat its identity while its header title truncates. [S06] | Keep one persistent project identity, a grouped contextual navigator, and one local view selector. Add search and pinning for frequently used destinations. |
| UX-12 | P2 · C | Layout preferences are scattered across independent localStorage keys, DeskLayout, file layouts, and component state. DeskLayout lacks schema versioning; the panel-width hook does not catch storage failures. [S15, S16] | Introduce versioned presentation preferences, defensive reads/writes, migration, bounded storage, and “Reset this layout.” |
| UX-13 | P2 · V/C | The project assistant is always present; its width is changed through a range inside Desk layout rather than a visible divider. Several sidebars resize but cannot collapse. [S06, S07] | Give every optional pane Show/Hide, Focus/Restore, supported Move, width controls, and a visible route back to hidden content. |
| UX-14 | P2 · V/C | Task screens use a separate green/ivory palette, larger headings, and generous page padding; chat, settings, file tools, and Workflow editing use other scales and chrome. [S17, S18, S19] | Introduce semantic tokens and a small component system; migrate all screen families to a single visual grammar. |
| UX-15 | P2 · V/C | Appearance settings expose theme, while report size, width, and line-spacing controls are local state and disappear on remount. There is no common UI density or text-scale preference. [S02, S18] | Add shared appearance preferences and separate UI, prose, and code typography settings; retain the existing report controls as contextual entry points. |
| UX-16 | P2 · V/C | New report concentrates actionable setup in a narrow column beside a mostly empty instructional area. Opening Workflows initially shows “Select a step to edit” instead of a useful workflow summary. [S05, S20] | Give idle setup an integrated form and meaningful preview; open the designer on Overview or the user’s last valid selection. |
| UX-17 | P2 · V/C | Opening the task builder leaves an empty-state illustration competing with the active form. The outline prints internal kinds such as workspace, snapshot, if, and deliver. [S11] | Expand the builder when the list is empty; show a compact summary of steps in ordinary language and keep the detailed graph available. |
| UX-18 | P2 · C | Generic research-object rendering enumerates raw record keys and serializes nested values. Some publication flows ask the user to supply the current file hash. [S21, S22] | Add typed object summaries and native-backed destination/conflict pickers; retain raw records and identities under Technical details. |
| UX-19 | P2 · C/R | Approval cards and errors are rendered in the scrolling transcript, while the activity drawer has separate polling and focus behavior. A researcher reading earlier messages may miss a new blocking request. [S23, S24] | Add a persistent, countable attention summary outside transcript history and one drawer contract; verify with a live pending-request fixture. |
| UX-20 | P2 · C/R | loadSessions applies whichever asynchronous response completes, without checking the still-selected project. Rapid changes with delayed responses can show an obsolete list. [S25] | Scope list requests to project/session identity and generation; add delayed out-of-order response tests. |
| UX-21 | P2 · C | The top-level remembered page is reduced to Workspace or main, so Tasks, History, designer, and Settings locations are not restored. Feature-specific state has inconsistent return behavior. [S05] | Persist typed destinations, selected objects, filters, and pane scroll anchors where useful; validate restored IDs and degrade to useful empty states. |
| UX-22 | P2 · C | ResearchTablePreview renders every cell as td. It cannot express a header row or headerless data explicitly and reports only a 200-row cap. [S26] | Add an explicit header interpretation, associated headers or generated column labels, visible row/column counts, and a source-preserving full-data action. |
| UX-23 | P2 · C/R | Large route controllers combine IPC, state machines, dialog state, navigation, and rendering. Settings is 1,923 lines, PipelinePage 1,723, ArtifactExplorer 1,596, and App 1,165 in this snapshot. [S05, S18, S20, S27] | Extract feature controllers and presentation components at stable boundaries. Size is evidence of maintenance concentration, not by itself a defect. |
| UX-24 | P2 · C/R | Streaming updates render the conversation view, including its message mapping and Markdown tree, every flush. The 200-message window bounds the work, but does not establish smooth typing or streaming under heavy content. [S23, S25] | Profile before tuning; isolate memoized completed messages and the active streaming message, while preserving the existing stream coalescing and transcript bound. |
| UX-25 | P2 · C/R | Research modules use inconsistent local busy/error strings and generic “Saving…” status for actions that may load, build, import, or execute. [S06, S21, S22] | Use typed async states and action-specific progress, cancellation, retry, and diagnostic details. Never label an uncertain execution as safe to rerun. |
| UX-26 | P2 · test | The baseline suite has a non-reproduced aggregate failure, and current coverage does not establish the visual, screen-reader, container-resize, or native interaction gates needed for this redesign. | Stabilize the baseline and add a representative visual/accessibility/native qualification matrix. |

UX-03, UX-20, UX-24, and UX-25 must be verified with targeted failure/performance fixtures before a broad refactor is used as their remedy. No security incident or incorrect scientific result was established by this review.

## 4. Product structure and researcher journeys

### Navigation model

Retain Pipeline as the suite name. Keep the workbench implementation namespace and existing Workflow/project/task schemas. Presentation changes should not imply a new orchestration mode or shared credentials.

| Shell destination | Local navigation | Expected first screen |
|---|---|---|
| Workspace | Project switcher; Conversations; Overview; Library; Analyses; Writing; project action items | Last usable conversation/object, or a simple conversation start |
| Workflows | New run; History; Run collections; Templates; Designer | Last workflow destination, otherwise New run |
| Tasks | Active; Scheduled; History; Missions | Work needing attention, then active tasks; clear empty action |
| Activity button | Grouped attention, active work, recent completion | The selected runtime/object with a direct next action |
| Settings / Help | Consistent bottom-of-navigation position | Last category; direct search and contextual deep links |

The current Projects destination becomes **Run collections** beneath Workflows. Existing project records continue to be stored and addressed exactly as before. A research project can show an explicit link to selected review results; that link does not merge its identity with a run collection.

Use the following vocabulary consistently:

| Object | User-facing description |
|---|---|
| Workflow | A reusable sequence of steps |
| Run | One execution of a workflow |
| Report | A run’s principal written output |
| Finding | An issue or conclusion with evidence and user status |
| Research project | A paper, files, conversations, and research records |
| Project action item | A recorded piece of research work; may be linked to an edit or automated task |
| Task chain | A prepared sequence that can wait, resume, or run on a schedule |
| Mission | A bounded adaptive research assignment |
| Harness / recipe | Research setup / guided research instructions in ordinary entry points; retain exact advanced labels where the user edits them |

Avoid exposing all twenty research destinations as equivalent global navigation. Group them under the existing five project sections. Within a section, provide a short contextual list with favorites and a search field. Keep advanced capabilities discoverable through an “All tools” view and contextual links. A missing prerequisite should lead to its setup action and return the user to the original task.

### Journey requirements

| Journey | Intended interaction and acceptance outcome |
|---|---|
| Ask a question | Open Workspace, type, send. A project is optional. Signed-out state offers connection setup while keeping the draft. No research configuration is required. |
| Start a research project | Supply a name; optionally choose paper/folder. After creation, show the next useful action. Partial import failures preserve the created project and identify the failed step. |
| Read and ask | Open a paper, select an exact passage, choose Ask, and compose beside it. The context tray identifies title, page/lines, version, and role. Navigation preserves the selection. |
| Write and compare | Open source, edit, preview, compare, run checks, and explicitly accept changes. Save/conflict state remains visible. Changing the layout cannot discard a buffer. |
| Run a review | Choose workflow and input, inspect readiness and exact execution preview, start, monitor, then read findings with source links. Recovering an interrupted run remains an explicit choice. |
| Inspect a finding | Select a row, see its source and recommendation in a detail pane, update status with optional rationale, return to the same filtered list. |
| Schedule follow-up work | Select a task template and conversation, set timing, inspect the pinned scope/budget and next occurrences, then start. Display timezone and the existing application-lifecycle limitations. |
| Resolve attention | A visible summary announces a pending approval, conflict, failed save, or interrupted action. Open the responsible object, take the scoped action, and return to the previous reading position. |
| Resume later | Restore the last valid destination, layout, selected objects, draft, and useful scroll position. Missing sources receive a recoverable unavailable state. |

## 5. Modular, configurable layout architecture

### Components and ownership

Introduce a presentation layer under proposed directories such as gui/src/ui/, gui/src/layout/, and gui/src/navigation/. Feature components continue to call their existing typed clients. The layout layer receives identifiers, view state, and callbacks; it does not call raw App Server methods, own execution state, or obtain file permissions.

| Building block | Contract |
|---|---|
| AppShell | Global navigation, route outlet, command entry, activity summary, title-bar safe area |
| WorkSurface | A feature-owned workspace with one primary canvas and optional local navigation, assistant, inspector, and bottom area |
| PanelRegistry | Finite set of compiled-in panel definitions: identity, label, allowed locations, size limits, persistence scope, availability explanation, lazy loader |
| PanelFrame | Shared header, title, contextual toolbar, focus target, body scroll region, collapse/focus/move menu |
| SplitView / Splitter | Horizontal/vertical geometry, pointer/keyboard resizing, bounds, focus, cancellation, and committed size events |
| LayoutController | Requested layout, effective fitted layout, preset/custom state, focused panel, fit priorities, and restoration |
| LayoutMenu | Presets, Show panels, supported placement, saved layouts, Reset this layout, Restore default layout |
| PreferencesStore | Validated, versioned presentation preferences with safe fallback and migrations |
| DraftRegistry | Feature-owned unsaved work and flush/leave decisions, independent of layout preferences |
| ActivityStore adapter | Read-only presentation of existing runtime summaries and explicit owner-routed actions |

Extend SidebarPanel and ResizeHandle into this system rather than replacing their successful interaction behavior wholesale. Use the console’s existing horizontal-resize behavior as a second migration case. Reuse ReportWorkspace’s tab keyboard implementation as the starting point for Tabs.

The registry is deliberately finite. Panel configuration selects compiled-in views; it does not import executable code, create a plugin runtime, or enable assistant tools. A panel can be visible while its feature is unavailable, in which case it explains the prerequisite. Hiding a research panel must not silently remove a tool or change a conversation’s harness.

### Panel menu and interaction contract

Every optional pane should offer:

- **Hide / Show:** reachable through both the pane menu and Layout menu; never permanently strand its content.
- **Focus this pane / Restore layout:** temporarily give the pane the work area, preserve the previous arrangement, and restore focus to a meaningful control.
- **Move to supported location:** right/bottom/tab group where the feature supports it. Offer explicit menu commands for all drag operations.
- **Resize:** visible divider, pointer drag, keyboard increments, numeric or stepper input, and reset.
- **Pin visibility:** a preference that keeps the pane visible when it fits; if space is insufficient, expose it as a temporary sheet rather than making the primary canvas unusable.

During a drag, update geometry at animation-frame cadence. Commit persistence at drag end. Handle pointercancel, lost capture, Escape, window blur, and component unmount. Restore the previous cursor/selection styles rather than assuming their original values were empty. A cancelled drag restores its starting geometry. Announce the committed size without announcing every pointer event.

Keep keyboard directions spatially consistent for left- and right-edged panes. Expose a named separator, orientation, value, min/max, pane association, and a useful value description. Enter may collapse/restore a collapsible pane; Home/End use its allowed bounds. Provide pane cycling with F6/Shift+F6 where the native platform permits it, and retain menu access when shortcuts conflict. The [W3C splitter pattern](https://www.w3.org/WAI/ARIA/apg/patterns/windowsplitter/) is informative guidance and itself notes incomplete example review; native assistive-technology testing remains necessary.

Keyboard resizing does not replace a click-only alternative. Each panel’s Size control must support clicking/tapping to set or increment its size without dragging. This follows the separate requirement explained in [W3C dragging-movement guidance](https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements.html).

### Fit algorithm

Use the actual work-surface container width and height, including text scale, rather than the whole window width. A reader inside a split can be narrow even on a wide monitor. Container queries handle local presentation; a ResizeObserver-backed controller can resolve panel constraints.

1. Measure the available rectangle after title bar, global navigation, and persistent status areas.
2. Clamp requested panel sizes to their own valid ranges.
3. Reserve a usable primary canvas and the currently focused/attention-bearing control area.
4. If the requested composition does not fit, collapse optional inspectors and secondary navigation in an explicit priority order.
5. If the remaining split still does not fit, show a single primary pane with a visible switcher or temporary sheet for the assistant/comparison.
6. Restore the requested arrangement as space returns. Do not overwrite the saved wide-window layout with temporary narrow-window adjustments.

Initial design targets, to be tuned through the representative fixtures:

| Region | Suggested comfortable range | Narrow-space behavior |
|---|---|---|
| Primary navigation | 160–224 px expanded; 48–56 px compact | Compact rail, then a labeled navigation button when needed |
| Context navigation | 220–320 px | Collapsible list/sheet; selection remains visible in the header |
| Reading/editing canvas | Prefer at least 480 px of usable content area | Focused single canvas; local outline becomes a popover/sheet |
| Assistant | 360–480 px preferred | Explicit assistant tab/sheet; composer kept within its usable viewport |
| Inspector | 280–400 px | Overlay/sheet, or replace another optional pane |
| Bottom activity/log pane | 160–320 px preferred | Collapsed status strip with direct Open logs action |

These are usability constraints, not WCAG thresholds. A geometry rule must account for the whole composition, including dividers and padding. At 1024 × 700, retaining two full navigation columns plus an assistant is not an acceptable default. Keep the active canvas and its essential actions visible. Large tables, code, and PDF pages may scroll within their own labeled region.

### Presets and user preferences

| Preset | Default visible work surfaces |
|---|---|
| Conversation | Context navigation + conversation; research inspector closed |
| Read | Paper/Markdown reader + optional assistant; outline adapts to reader width |
| Write | Source/editor + preview; assistant available through one toggle; changes/checks inspector on demand |
| Compare | Two resizable objects with explicit identities; independent or opt-in synchronized scrolling |
| Review results | Findings/report + selected source/detail pane; provenance one action away |
| Monitor | Task/run list + progress/detail; logs collapsed until requested or relevant |

Users can save, rename, duplicate, and delete a custom presentation preset, restore a built-in preset, and reset one panel. Built-in presets remain recoverable. A clear Custom indicator appears after a layout change. Keep preference scope visible: application appearance, mode default, or this project/run. Support comfortable/compact density, UI text scale, prose size/line spacing/reading measure, and code size. Advanced toolbar pinning and table column preferences follow after the foundational panel system.

### Persistence model and migration

Store semantic presentation state, using a versioned envelope. The following is a proposed shape, not an existing API:

~~~typescript
type LayoutPreferencesV1 = {
  schemaVersion: 1;
  scope: { kind: 'mode' | 'workspace' | 'run'; id: string };
  presetId: string;
  panels: Array<{
    id: string;
    placement: 'left' | 'center' | 'right' | 'bottom';
    visible: boolean;
    preferredSize: { unit: 'px' | 'fraction'; value: number };
    order: number;
  }>;
  focusedPanel: string | null;
};
~~~

Keep selected research references and navigation state in separately typed records that preserve exact object/version identity. Keep file buffers, conversation drafts, execution grants, and provider configuration outside this envelope. A shared preference service can expose subscriptions and reducer actions without requiring a new state-management dependency.

Migrate existing panel-width keys, pipeline.desk.<workspaceId>, pipeline.files.layout.<adapterId>, project pinned revisions, and relevant view preferences. Validate numeric finiteness, units, panel IDs, allowed placements, maximum entries, scope ownership, and object-reference lengths. Handle denied storage, malformed JSON, old versions, and newer versions with safe defaults and a recoverable notice. Bound and prune stale presentation records without deleting research data.

A migration must be idempotent and additive. Read legacy values once, preserve a rollback copy, write the new version only after successful validation, and avoid interpreting stale layout preferences as authority to read a file or initialize a runtime. Layout reset affects presentation only. Cross-window changes need a defined last-write/revision policy if multiple windows become supported.

## 6. Design system and accessibility specification

### Visual foundations

Define semantic tokens for canvas, surface, raised surface, selected surface, border, strong border, primary text, secondary text, interactive text, focus, accent, success, warning, and danger. Supply independently tested light and dark values. Keep status meaning available through a label/icon as well as color. The current Tailwind neutral palette mapping and reduced-motion/forced-colors support are useful foundations; preserve them while removing screen-specific semantic palettes.

| Foundation | Initial specification |
|---|---|
| Typography | System UI font by default; 14 px interface body, 12–13 px metadata, 16 px reading text. Avoid routine informative text at 10–11 px. Choose and bundle a custom font only if it is actually part of the product. |
| Type scale | A small set of title, section, body, label, and metadata roles; one page-header hierarchy across routes |
| Reading | Approx. 65–85 characters per line where the subject permits; adjustable prose size and line spacing; math/table overflow stays local |
| Spacing | 4 px base scale; consistent 8/12/16/24/32 px steps; denser metadata with generous reading space |
| Controls | Comfortable height around 36 px; compact variant around 30–32 px; larger touch/coarse-pointer treatment where applicable |
| Corners | Small set of 4/8/12 px roles; pill shapes reserved for appropriate chips/toggles |
| Elevation | Borders establish ordinary hierarchy; restrained shadows for menus, dialogs, and temporary overlays |
| Motion | Short functional transitions; no layout animation during continuous resizing; honor system reduced-motion preferences |
| Icons | One stroke/size family, explicit accessible names, tooltips on keyboard focus, stable placement, text for unfamiliar actions |
| Chrome | One page title, one contextual toolbar, one clear primary action for the current stage; overflow menus for low-frequency actions |

Minimal presentation must remain legible. Use at least 4.5:1 contrast for normal text and 3:1 for qualifying large text, with the applicable exceptions assessed rather than assumed. Test interaction boundaries and focus indicators as well. See [WCAG 2.2](https://www.w3.org/TR/WCAG22/) and the [W3C text-contrast technique](https://www.w3.org/WAI/WCAG22/Techniques/general/G18.html).

The 24 × 24 CSS pixel target-size minimum has spacing and other exceptions; it is not a universal 44 px desktop requirement. Adopt a comfortable product target larger than the minimum for common controls, and evaluate narrow splitters through their equivalent accessible controls and target-spacing design. See [W3C target-size guidance](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html).

### Shared UI primitives

Build or extract Button/IconButton, TextField/TextArea, Select/Combobox, Checkbox/Switch/RadioGroup, FormField, Tabs, Menu, Tooltip, Dialog, Sheet, Disclosure, PageHeader, PanelToolbar, EmptyState, InlineNotice, SaveIndicator, StatusBadge, and a semantic table foundation. Each primitive needs a small documented API, controlled-state support, accessible names/descriptions, disabled/busy/error states, theme tokens, and a few meaningful interaction tests.

Use native elements when their interaction is appropriate. A searchable combobox is justified for long project/model/source lists; a short select need not become a custom widget. Evaluate a maintained accessibility primitive dependency only after the first two migrations reveal a concrete gap. Avoid introducing separate competing libraries for dialogs, menus, tabs, and docking.

### Keyboard, focus, and accessible reading

- Modal dialogs contain focus, hide/inert the background, expose title and description, and return focus to the invoker or a sensible fallback. Nested dialogs obey one ownership stack. Async work disables only the necessary actions; dismissal policy is explicit.
- Docked inspectors are nonmodal and part of pane traversal. Temporary narrow-screen sheets use consistent modal behavior. Closing or collapsing a focused pane moves focus to its reopen control.
- Tabs use one active tab stop, appropriate arrow/Home/End behavior, IDs and panel associations. Manual activation is appropriate when changing a tab triggers substantial loading. Navigation destinations remain navigation, not gratuitous tab widgets. Follow the [W3C tabs pattern](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/).
- Menus expose expanded state, close on Escape, support keyboard movement, and return focus. Tooltips are supplementary; they cannot be the only place that explains a blocked action.
- Provide skip-to-content and pane navigation. Scope shortcuts to the active feature; do not intercept text editing, IME composition, native file shortcuts, or screen-reader commands indiscriminately.
- Namespace DOM IDs for every reader, tab group, dialog, and form. Two renderers of the same source must remain independently navigable. Preserve canonical document anchors in the file/navigation adapter rather than relying on global page fragments.
- Announce saved/failed/completed/needs-attention transitions with concise status regions. Do not announce every token in a stream. Give users a direct route to the latest response and pending requests while preserving their reading position.
- Retain meaningful source-table headers and math accessibility output. A data preview must explicitly distinguish a header row from data; do not silently reinterpret scientific values as labels.
- Verify 200% text enlargement and narrow logical viewport reflow. Where code, data tables, or figures require two dimensions, contain that scrolling locally and keep surrounding controls operable. This applies the distinctions in [W3C reflow guidance](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html).

Apple’s [split-view guidance](https://developer.apple.com/design/human-interface-guidelines/split-views) and [layout guidance](https://developer.apple.com/design/human-interface-guidelines/layout) support adapting adjacent panes and hiding tertiary columns as space decreases. Use native title-bar spacing and macOS/Windows/Linux conventions through a small platform adapter rather than scattered offsets.

## 7. Feature-specific implementation requirements

### Workspace, project desk, and research tools

Extract WorkspaceSidebar, ConversationController, ConversationPane, Composer, PendingRequests, and ProjectDesk from the current route. Keep event reconciliation, serialized draft/model writes, the 50 ms stream flush, and the 200-message transcript window intact. Use memoized completed messages and isolate streaming work after profiling.

Replace the permanent project/assistant arrangement with the WorkSurface contract. Show the project name once at the desk level; use breadcrumbs for the active object. Keep the project switcher searchable and the conversation list filterable. A research inspector can be hidden without closing a conversation or altering its harness. Existing research setup, recipes, sources, and execution tools stay lazy.

Replace the nested horizontal navigation hierarchy with grouped destinations and a concise local toolbar. The empty project overview should prioritize Add paper, Attach folder, and Continue conversation; secondary metadata forms should not dominate before any research exists. Notes and action items become reusable list/detail surfaces with explicit save state, meaningful empty states, and preserved drafts.

The context tray should show a human title, role label, version/page/line summary, and remove action. Map data_dictionary, prior_draft, and referee_report to ordinary labels while retaining their wire values. Opening a source reveals the exact retained identity. Failed source attachment remains visible with Retry or Remove; silently substituting the newest source is prohibited.

Provide specialized object renderers for papers, notes, decisions, datasets/samples, experiment plans/results, publication assets, symbols/checks, revision campaigns, and deliverables. Share metadata and evidence components, but preserve each object’s domain semantics. Technical details remain available; normal workflows should not require entering a record ID or copying a file hash manually.

### File workspace, PDF, and comparison

Create one FileTabStrip and one ReaderToolbar. Distinguish “Split view” for two files from “Source and preview” for one file. Make both arrangements resizable; support horizontal/vertical placement where usable. Give each pane an explicit label and independent view state. Preserve buffers and cursor/scroll position when moving between source, preview, comparison, and focused mode.

Scope every outline, find operation, fragment, and shortcut to its reader. Local outline collapse follows the pane width. Reading preferences persist consistently across report and document renderers, with an explicit per-document override only when necessary. Share rendering policy where appropriate without collapsing Workflow read-only adapters into Workspace write access.

The PDF toolbar needs coherent page/total, fit/zoom, search, outline/thumbnails, selection mode, and available export/open actions. Persist view state by document identity. A loading or failed page keeps navigation and retry reachable. Test the existing locally bundled worker/font/CMap/codec path in the native app; do not infer it from the browser Markdown fixture.

Comparison shows both source identities and whether scrolling is linked. Every copy, apply, or accept action names its target and uses the existing expected-hash contract. Replace manual hash entry with a native-backed current-file identity preview; a changed file produces a conflict view with inspect/reload choices. Keep read-only status, preview truncation, missing assets, and unsaved draft restoration visible.

### Workflows, run setup, and reports

New run should expose input, workflow, and relevant provider/readiness choices in the main task area. Use a succinct workflow summary and progressive Advanced controls. The existing exact execution preview remains the final launch step; retain input identities, providers, tool access, work bounds, and failure implications.

Designer opens on a useful overview or previous valid step. Keep Basic/Advanced, Steps/Overview/Schemas, undo/redo, import/export, and schema validation. Use shared navigator rows, tabs, field grouping, and toolbar placement. Advanced controls should explain context dependencies and output roles with examples. Reorder operations have menu/button equivalents. A graph is an alternate view over the canonical workflow, not a new execution model.

During execution, show stage, elapsed time, completed/failed/skipped work, and the next user action. A compact activity/log pane opens when useful, keeps scroll position, and distinguishes run cancellation from cancelling one pass. Preserve checkpointed outputs and honest partial-report states. “Resume failed work” and “Run again” must remain distinguishable.

ReportWorkspace is a strong starting point. Preserve its report/provenance/issues/sources separation and keyboard semantics. Standardize the header, reading controls, and evidence detail pane. Findings need user-configurable columns, sort/filter, selection, status/rationale, saved views, and keyboard-accessible detail inspection. Keep annotations separate from immutable generated findings. Use semantic tables and bounded rendering appropriate to the data volume.

### Tasks, missions, and activity

Keep the durable coordinator authoritative. Present task creation as a sequence of template, inputs/conversation, timing, and reviewed scope. The preview clearly shows the next action, limits, wait conditions, and output. The template picker should expose one selection rather than appearing as unrelated toggle buttons.

Show ordinary step labels first, with advanced control-flow details on demand. Collapse the empty task-list area when the builder is the only useful content. Make list/detail resizable. Preserve a partly authored definition across navigation and offer discard explicitly. Missions retain remit, criteria, budget, proposed work, challenge assessment, and separate completion/acceptance semantics.

Activity uses one presentation component for pending requests, interrupted work, active jobs, and completed results, grouped by owning runtime. Provide direct Open/Resolve actions and visible counts. Replace unconditional parallel polling with event-driven refresh and a bounded fallback where supported. The drawer must have a defined modal/nonmodal behavior, focus restoration, partial-load errors, and an empty state. It must never route an action to a different runtime merely because its status label matches.

### Settings, history, run collections, and help

Add searchable settings with results linked to the owning section and temporary highlighting. General includes theme, density, text, layout defaults/reset, and keyboard reference. Provider panels expose connection state and test/retry actions with separate Workspace and Workflow scopes. Keep the existing serialized autosave and dirty guard; standardize saving, saved, failed, and retry states across the app.

History and run collections share list/table/detail patterns, filtering, search, empty states, and loading behavior. Preserve existing recoverable deletion/trash behavior and keep permanent deletion explicit. Collection edits must survive failed saves and route changes. Add named views and column settings only after the foundational table component is stable.

Help opens the relevant topic from a blocked or unfamiliar control, supports search, and states prerequisites in ordinary language. Technical details can include a copyable diagnostic identifier/path. Keep update/install actions native and explicit. The GUI should explain the selected extraction mode’s actual behavior without overstating scientific fidelity or release qualification.

## 8. Implementation work packages

Each package should be independently reviewable. “Done” requires the user behavior and acceptance evidence listed, not only a component extraction. S = roughly 1–2 engineering days, M = 3–5, L = 6–10, subject to native platform findings; these are planning estimates rather than commitments.

| Package | Scope / primary ownership | Dependencies | Size | Acceptance evidence |
|---|---|---|---|---|
| UI-00 | Record the current fixture/test baseline; stabilize context-tray aggregate failure; create audit fixtures with no native side effects | None | S | Reproducible baseline report; the failure is explained or retained as an explicit blocker |
| UI-01 | Fix New project focus, failing collection navigation, duplicate reader anchors, and low-contrast contents links | UI-00 | M | Targeted tests and keyboard/reader reproductions pass; no source identity or save behavior regresses |
| UI-02 | Semantic tokens, type/spacing scales, shared control variants, visual catalog | UI-00 | M | Light/dark/focus/disabled/error samples approved; token contrast checks pass |
| UI-03 | Shared Dialog/Sheet/Tabs/Menu/FormField/Notice/SaveIndicator; migrate Tasks/file tabs and representative overlays | UI-01, UI-02 | L | Modal/roving-focus/return-focus tests; keyboard-only representative journey passes |
| UI-04 | Versioned presentation store and legacy preference migration; separate DraftRegistry | UI-00 | M | Corrupt/denied/newer storage tests; round-trip migration; no research data or grant mutation |
| UI-05 | SplitView/Splitter, PanelFrame, layout constraints, effective/requested layout distinction | UI-02, UI-04 | L | Pointer/keyboard/click-only resize, cancellation, reflow, and viewport restoration tests |
| UI-06 | Shell destination model, contextual navigation, Run collections labeling, typed route restoration | UI-03, UI-04 | M | Every legacy entry still resolves; back/return behavior preserves selected objects and drafts |
| UI-07 | Workspace conversation composition and project WorkSurface; add show/hide/focus controls | UI-05, UI-06 | L | 1400 × 800 and 1024 × 700 journeys; composer and attention controls remain reachable |
| UI-08 | Presets, custom layouts, density/text settings, per-scope controls, reset/restore | UI-07 | M | Restart persistence, schema migration, narrow/wide restoration, keyboard operation |
| UI-09 | Shared readers/file tabs/toolbars, resizable comparison, pane-local anchors/find, persisted reading preferences | UI-03, UI-05 | L | Same-document dual-pane test; edit/preview/save conflict; Markdown/math/PDF native matrix |
| UI-10 | Integrated New run, designer overview, reusable step/field panels, progress/activity docking | UI-05, UI-06 | L | Input → preview → run → partial/complete report fixture journey; launch semantics unchanged |
| UI-11 | Findings/history/collection table and detail foundation; saved views and column configuration | UI-03, UI-05, UI-06 | L | Filtering/selection/evidence/annotation persistence; bounded large-list rendering; header semantics |
| UI-12 | Research object summaries and consistent prerequisite/setup flows across project tools | UI-07, UI-09 | L | Read → select → ask; experiment result → evidence; edit → compare → accept journeys |
| UI-13 | Task/mission list-detail and builder redesign, timing language, resumable authoring draft | UI-03, UI-05, UI-06 | L | Prepare-before-start, input wait, timezone preview, pause/stop/recovery fixtures |
| UI-14 | Shared attention/activity presentation and event-driven refresh, visible runtime scope | UI-07, UI-10, UI-13 | M | Pending requests found while reading earlier text; no cross-owner Stop; focus/partial-error checks |
| UI-15 | Settings search, help links, common autosave/errors, remaining typography/token migration | UI-02, UI-03, UI-08 | M | Search-to-setting, failed-save retry, separate connection scopes, no inaccessible labels |
| UI-16 | Performance profiling and targeted controller/render extraction | UI-07, UI-09, UI-10, UI-13 | M–L | Measured typing/streaming/resize budgets, stable subscriptions, lazy-loading boundaries |
| UI-17 | Native accessibility, visual regression, usability sessions, migration/rollback rehearsal | All preceding release-scope packages | L | Section 10 gates satisfied and remaining platform limitations recorded |

Suggested rollout:

1. **Stabilize:** UI-00 and UI-01. Fix the observed defects before changing navigation.
2. **Establish the foundation:** UI-02 through UI-06. Demonstrate a resizable two-pane surface, modal, tab strip, and migrated preference record.
3. **Prove the desk:** UI-07 through UI-09. Ship the conversation/read/write experience behind a reversible rollout setting.
4. **Complete the suite:** UI-10 through UI-15. Migrate feature families using the established contracts.
5. **Qualify:** UI-16 and UI-17, with performance and accessibility checks occurring throughout.

A two-engineer team with regular design/accessibility review should budget approximately 8–12 weeks for the full program, including research-tool migration and native qualification. Re-estimate after UI-07. An earlier usable milestone is the defect fixes plus the coherent Workspace desk; it should not be presented as completion of the suite-wide redesign.

## 9. State, migration, and release safeguards

### Preserve authoritative ownership

- Workspace and Workflows keep independent runtime state, credentials, cancellation, stores, and lifecycle. The task coordinator continues to invoke them through their owning adapters.
- Layout configuration cannot grant filesystem access, enable research execution, accept manuscript edits, or launch a task. Exact-source context and immutable handoffs keep their current identities.
- Do not rename serialized workbench identifiers, persisted projects, or workflow formats as part of the presentation migration.
- Preserve the scoped file readers, expected-hash saves, read-only run adapters, bounded data loads, and explicit preview/acceptance transitions documented for the project.

### Make persistence behavior explicit

Use a shared dirty-work protocol with feature-owned persistence. Navigation asks an editor to flush; success permits the transition, a failed save keeps its draft, and the user can explicitly discard where appropriate. Collapse/focus/move actions normally preserve the editor instance or its durable buffer and should not produce a confirmation dialog.

Model optimistic and server-confirmed state separately. Every save can be idle, dirty, saving, saved, or failed; revisions and stale responses are handled by the feature controller. Concurrent saves are serialized or revision-checked. A stale response cannot overwrite a newer project selection or draft. A save indicator refers to the object actually being edited.

Retain the existing explicit treatment of ambiguous model submissions and interrupted host actions. A common Retry button must not replay uncertain work automatically. Show “Inspect recorded result” or the owning reconciliation action where that is the correct recovery path.

### Incremental adoption

Keep legacy presentation adapters until each route has migrated and its behavior is checked. A rollout switch may select the new layout renderer while using the same domain clients and records. Avoid dual writable domain stores or duplicate event subscriptions. Do not keep every heavy route mounted to preserve state; preserve appropriate presentation/draft state while suspending hidden feature work.

Make preference migrations reversible without requiring a research-store rollback. Preserve unknown future preference versions for later recovery. Provide “Reset this view” and a safe default layout after a renderer error; the global navigation remains available. Route-level ErrorBoundary coverage should isolate failures while retaining pending work.

Update CLAUDE.md and the relevant file-workspace, research-desk, tasks, and project-surface maps when behavior lands. Qualification records should distinguish deterministic tests, synthetic browser checks, native development checks, authenticated work, packaged builds, and platform coverage.

## 10. Acceptance and validation plan

### Automated checks

| Area | Required regression scenarios |
|---|---|
| Modal foundation | Initial focus, forward/reverse wrap, Escape, return focus, nested overlays, busy states, hidden/disabled controls |
| Layout | Valid/corrupt/denied storage; NaN/out-of-range sizes; missing panel IDs; migration twice; narrow→wide restore; hide/focus/restore; horizontal/vertical resizing; pointercancel and unmount |
| Navigation/drafts | Rapid project switches with reversed response order; failed collection save; route leave during debounce; restore after restart; missing object/source |
| Tabs/menus | Roving focus, activation, Home/End, panel associations, close action naming, focus fallback after deletion |
| Readers | Two instances of the same Markdown; identical headings in different files; right-pane anchors; pane-scoped find; unsaved buffer in both panes; read-only source; external-file conflict |
| Workflow journeys | Concrete input readiness; exact launch preview; cancelled/skipped/failed/partial output; source-evidence navigation; no accidental rerun |
| Tasks | Draft/prepared/running/waiting/attention/unknown/finished; timezone/DST examples; scope changes; explicit reconciliation; action owner routing |
| Research objects | Exact version/passage retained through layout changes; unavailable/truncated source; unaccepted proposal; missing prerequisite; stage/compare/accept boundary |
| Accessibility | Representative axe-style semantic checks, label/description associations, contrast token checks, focus visibility and keyboard tests; no reliance on an automated score as full qualification |

Keep existing relevant unit and integration tests. Add tests that exercise user outcomes, timing, and ownership boundaries rather than snapshotting every implementation detail. A test fixture must display a clear fixture identity and mock native writes/model calls. Browser fixtures are complementary to Tauri tests.

### Visual and native matrix

Check 1024 × 700, 1280 × 800, 1400 × 800, and 1920 × 1080; light/dark/system themes; comfortable/compact density; 100/125/150/200% text scale; reduced motion; and supported high-contrast settings. Include long project/file/model names, empty data, loading, errors, large tables, long transcripts, multiple readers, and an attention request while another pane has focus.

Use reference screenshots for complete states and important responsive transitions. Compare meaningful geometry, truncation, focus visibility, and content hierarchy, with manual review of intentional changes. Do not “fix” a failure by indiscriminately increasing pixel tolerances.

Native testing must use the Tauri development app on this machine. Verify title-bar dragging versus controls, native file pickers, keyboard modifiers, editor shortcuts, menu/tooltip placement, focus after dialogs, scrollbars, resize capture across window boundaries, PDF assets, and close/reopen behavior. Run VoiceOver on macOS; verify Windows with NVDA/WebView2 and Linux with the supported WebKitGTK/assistive-technology environment before claiming support there. Browser automation does not establish these outcomes.

### Performance targets

Measure on a named representative machine and production frontend build. Establish a baseline first. Treat the following as proposed budgets:

- Continuous panel resizing should avoid long main-thread tasks over 50 ms and keep visible geometry within roughly one frame of pointer movement in the representative heavy fixture.
- Ordinary typing should remain responsive, with p95 input-to-paint below 100 ms while a response streams and a reader is open.
- Local tab/panel activation should present useful feedback immediately; cached lightweight switches should target under 150 ms.
- Preserve the existing maximum 200 mounted transcript messages and lazy PDF/research loading. Do not silently replace bounded data reads with eager loading for a smoother-looking initial screen.
- Layout persistence should commit at gesture end/debounced boundaries; it should not serialize full research objects or write localStorage on every pointer move.
- Profile PDF chunk loading and memory before changing bundling. Record warm/cold reader-open timings and long-session memory/subscription behavior.

These targets are implementation goals, not measurements achieved during this audit.

### Usability review

Recruit approximately five representative researchers for a formative round, including a first-time user, a heavy keyboard user, and someone who benefits from enlarged text. Use their ordinary research vocabulary. Ask them to start a project, read and ask about a passage, compare two sources, configure a review, recover a failed save, and locate a pending task request.

Record task completion, wrong destinations, assistance required, missed state changes, and recovery success. Initial acceptance targets: at least four of five complete the core tasks without instruction; all can find a hidden pane and reset a layout; no participant mistakes a Workflow collection for a research project or believes a layout action authorizes execution. Use results to refine navigation and defaults. This is formative evidence, not a statistically representative usability claim.

## 11. Definition of done

The redesign is complete when:

- Every major route uses the same shell, panel, typography, control, and status conventions.
- A researcher can resize, hide, restore, focus, and configure supported panels with pointer and keyboard, and can save/reset layouts without losing work.
- The minimum supported desktop window retains a usable primary surface and reachable essential actions; narrow nested readers adapt independently.
- Workspace projects, Workflow runs/collections, task chains, missions, and research action items are distinguishable without reading architecture documentation.
- The observed modal, wrong-pane anchor, failed-save navigation, tab, and contents-contrast defects are fixed with regression coverage.
- Appearance and layout preferences survive restart and migration, while research identities, credentials, permissions, and execution semantics remain authoritative in their existing owners.
- Core read/ask/write/review/schedule/recover journeys pass the relevant automated, visual, keyboard, and native checks.
- The baseline aggregate test failure is resolved or explicitly tracked; performance targets are measured; platform qualification is accurately recorded.

## 12. Source index

Line anchors refer to the audited working tree and may move during implementation. Proposed new modules in this plan are design targets; existing-file links below are evidence anchors.

| ID | Source and relevance |
|---|---|
| S01 | [WorkspaceProjectDialog.tsx:26](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspaceProjectDialog.tsx:26) — Escape-only effect; modal markup at line 64 |
| S02 | [ReportViewer.tsx:448](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/ReportViewer.tsx:448) — reading state, outline, heading IDs and fragment links |
| S03 | [FileWorkspace.tsx:551](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/file-workspace/FileWorkspace.tsx:551) — split control and tab strip; buffer ownership elsewhere in the file |
| S04 | [ProjectsPage.tsx:170](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/ProjectsPage.tsx:170) — saveProject and selectProject failure path; autosave at line 107 |
| S05 | [App.tsx:249](/Users/Mike/Documents/GitHub/pipeline/gui/src/App.tsx:249) — route persistence, dirty guard, conditional route composition |
| S06 | [WorkspaceProjectSurface.tsx:30](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspaceProjectSurface.tsx:30) — destination hierarchy, desk persistence and assistant slider |
| S07 | [WorkspaceConversation.css:138](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspaceConversation.css:138) — assistant width and stacked minimum-height behavior |
| S08 | [App.css:144](/Users/Mike/Documents/GitHub/pipeline/gui/src/App.css:144) — report contents secondary-link colors |
| S09 | [files.css:104](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/file-workspace/files.css:104) — fixed equal-width file split |
| S10 | [ResizeHandle.tsx:31](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/ResizeHandle.tsx:31) — pointer path, bounds, keyboard behavior and separator markup |
| S11 | [TasksPage.tsx:9](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/TasksPage.tsx:9) — internal step labels; tabs and list/detail at lines 96–98 |
| S12 | [ReportWorkspace.tsx:247](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/ReportWorkspace.tsx:247) — reusable accessible report-tab behavior |
| S13 | [NavRail.tsx](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/NavRail.tsx) — top-level labels, hierarchy and resize behavior |
| S14 | [tasks.md](/Users/Mike/Documents/GitHub/pipeline/docs/tasks.md) and [workbench README](/Users/Mike/Documents/GitHub/pipeline/docs/workbench/README.md) — product and runtime ownership |
| S15 | [deskLayout.ts:3](/Users/Mike/Documents/GitHub/pipeline/gui/src/lib/deskLayout.ts:3) — unversioned desk preferences and defensive parsing |
| S16 | [usePersistentPanelWidth.ts:14](/Users/Mike/Documents/GitHub/pipeline/gui/src/hooks/usePersistentPanelWidth.ts:14) — uncaught optional storage access |
| S17 | [TasksPage.css](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/TasksPage.css) — independent semantic palette and layout scale |
| S18 | [SettingsPage.tsx:108](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/SettingsPage.tsx:108) and [SettingsPage.css](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/SettingsPage.css) — settings controller and presentation |
| S19 | [tailwind.config.js](/Users/Mike/Documents/GitHub/pipeline/gui/tailwind.config.js) and [WorkspaceConversation.css](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspaceConversation.css) — typography, neutral mapping and chat tokens |
| S20 | [PipelinePage.tsx](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/PipelinePage.tsx) and [RunSetupPanel.tsx](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/RunSetupPanel.tsx) — designer/controller and setup composition |
| S21 | [ObjectPane.tsx:11](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/research-desk/ObjectPane.tsx:11) — generic record rendering |
| S22 | [research-programs/shared.tsx:345](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/research-programs/shared.tsx:345) — manual expected-hash entry; shared inspection and async behavior |
| S23 | [WorkspaceConversationView.tsx:63](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspaceConversationView.tsx:63) — message rendering, requests/errors and composer |
| S24 | [ResearchActivity.tsx:67](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/ResearchActivity.tsx:67) — polling, focus and drawer semantics |
| S25 | [WorkspacePage.tsx:160](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/WorkspacePage.tsx:160) — session-list loading, hydration, stream/draft orchestration |
| S26 | [ResearchTablePreview.tsx:30](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/ResearchTablePreview.tsx:30) — data-table cells and preview limits |
| S27 | [ArtifactExplorer.tsx](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/ArtifactExplorer.tsx) — artifact navigation, bounded reads and viewer composition |
| S28 | [SidebarPanel.tsx](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/SidebarPanel.tsx), [useModalDialog.ts](/Users/Mike/Documents/GitHub/pipeline/gui/src/hooks/useModalDialog.ts), and [DialogService.tsx](/Users/Mike/Documents/GitHub/pipeline/gui/src/components/DialogService.tsx) — existing reusable foundations |
| S29 | [file-workspace.md](/Users/Mike/Documents/GitHub/pipeline/docs/file-workspace.md) — editor/reader adapters and expected-hash boundary |
| S30 | [tauri.conf.json:19](/Users/Mike/Documents/GitHub/pipeline/gui/src-tauri/tauri.conf.json:19) — minimum native window size |
| S31 | [CLAUDE.md](/Users/Mike/Documents/GitHub/pipeline/CLAUDE.md), [workbench_plan.md](/Users/Mike/Documents/GitHub/pipeline/notes/workbench_plan.md), and [release-qualification.md](/Users/Mike/Documents/GitHub/pipeline/docs/workbench/release-qualification.md) — architecture, scope and qualification limits |

The first implementation slice should address UI-01 and demonstrate UI-05 on the project desk. Those two changes resolve concrete failures and make the proposed modular design reviewable before it spreads through the rest of the suite.
