# Workspace interface modernization

Status: implemented in the working tree on September 8, 2026. Frontend tests,
production build and browser geometry checks pass. Native Tauri and packaged
qualification remain pending; see the implementation record below.

The following design records the approved direction and the original findings.

The accompanying interactive conversation sketch uses sample project content
and illustrative model/effort choices. It demonstrates the arrangement,
destination switching, inspector placement, pins, and composer location; it
does not connect to Pipeline or qualify native pixel geometry. Static DOM and
interaction checks passed for those switches, draft retention, settings, reset,
and inert Send. Browser preview was blocked by the browser's local-file URL
policy, so no visual browser verification is claimed.

## Direction

Make Workspace a project surface with the assistant alongside it. This follows
the user's explicit preference for **project first, with chat alongside**.
The project should open directly to useful material, expose its research tools
by name, and keep the controls for the next assistant message at the composer.
Unfiled chat remains available without a project or research-service startup.

The first release of this redesign should address three concrete outcomes:

1. Opening a panel never creates horizontal scrolling of the application or
   puts its header, content, close button, or resize handle outside the window.
2. A user can reach files, manuscript, results, notes, evidence, and assistant
   configuration without guessing what is inside “Research.”
3. Model and thinking controls are visible at the bottom of the assistant,
   adjacent to the message they govern.

This is a presentation and navigation change over existing services. Preserve
the Workspace/Workflow runtime boundary and the distinctions among presets,
recipes, project action items, and durable task chains.

## Findings before implementation

### Original panel sizing diagnosis

`WorkspacePage.tsx` renders Research inside `.workspace-inspectors`, a flex
wrapper beside `WorkspaceDesk`. `WorkspaceResearchPanel.tsx` requests a 432 px
sidebar, persisted between 320 and 560 px. `SidebarPanel.tsx` installs the shared
`ResizeHandle`, whose sizing code measures the pane's immediate parent and
subtracts a default 240 px reservation for a flexible sibling.

For this inspector, the measured parent is the inspector wrapper; the desk is
outside it. The wrapper's width also depends on the child whose width the
handle mutates. This creates a possible feedback loop: approximately 432 px
becomes a 192 px allowed width, the wrapper shrinks, and a subsequent
measurement can shrink the pane again. Content and fixed header controls can
then extend beyond the collapsed pane. This is a concrete code-level defect
candidate, not a confirmed reconstruction of the user's exact scroll symptom.

Other relevant behaviors:

- Opening either Research or Outline always collapses Workspace navigation,
  even in a wide window (`WorkspacePage.tsx`).
- The focused-inspector breakpoint uses shell width, while the project/chat
  split uses a separate local-width threshold. These independent decisions
  make it difficult to account for all visible panes consistently.
- Research's loading fallback requests 28 rem, whereas its loaded panel
  defaults to 432 px. Loading and loaded geometry should share one shell.
- Focused mode overrides the inspector width with CSS while the resize handle
  still computes a sibling reservation. Full-area views should not resize as
  though a hidden sibling were still beside them.
- Existing panel tests check state, saved widths, and isolated components;
  they do not establish actual browser geometry for this nested composition.

The configured native window is 1400 × 800, with a 1024 × 700 minimum. The
user's actual default width has not been measured. The running Tauri dev
process was identified, but the computer-use tool could not attach to its
unbundled executable. An attempted separate dev launch stopped because the
existing Vite server already occupied port 1420. No native reproduction or
fix is claimed here.

### Navigation and configuration

Research has seven tabs: Setup, Documents, Recipes, Memory, Evidence, Results,
and Release. Separately, `WorkspaceProjectSurface.tsx` has Overview, Library,
Analyses, Writing, and Action items. `WorkspaceResearchStudio.tsx` adds another
level of tool navigation within some of those destinations. The result is
multiple entry points and several layers of tabs for related work.

Setup combines preset selection, access selection, diagnostics, and a link to
the full harness editor. That editor temporarily replaces the conversation.
Release combines project sharing, whole-store backup, review handoffs, and
performance details. The composer “+” menu also contains project navigation
and research settings, while model and reasoning choices occupy the header.

Reuse the existing project, file, context, harness, and split-view components;
the problem is their arrangement and discoverability, not missing capabilities.

## Proposed interface

### Project navigation and working area

Keep the suite navigation and a single Workspace navigation column. Put the
project picker at the top; it also offers New project and Unfiled conversations.
Selecting a project opens its last valid destination, or Overview on first
use. Restore that project's assistant conversation independently.

Expose six project destinations:

| Destination | Contents |
| --- | --- |
| Overview | Current paper, project brief, recent work, decisions and next action items |
| Library | Documents, files, sources, search and collections, acquisition, literature notes |
| Analyses | Results, experiments, data and samples, execution plans, specification grids, theory and assumptions |
| Writing | Manuscript, responses, edits, tables and figures, revision campaigns, deliverables |
| Notes & evidence | Saved project notes, claims, linked evidence, and confirmation state |
| Action items | Project action items and checks; explicit links to owning task-chain and mission views |

Each destination uses one local navigation level with direct tool selection.
For example, Analyses → Results opens results immediately; the user need not
select “Experiments & theory” and then another nested tab. Use compact vertical
tool navigation or an accessible local chooser where a horizontal row will not
fit. Avoid adding another global “Research” destination over these destinations.

Allow up to four pinned shortcuts in Workspace navigation, initially Files,
Manuscript, and Results. Provide a visible Customize action with pin/unpin and
Move up/Move down controls; drag reorder can be an additional convenience.
Pins are shortcuts to canonical destinations, not duplicate editors or data.
Store them per project on this device. Hiding or pinning a tool changes only
navigation; enabling assistant access is a separate setting.

Overview should emphasize continuing real work: the current paper or selected
file, recent edits/results, and a short action list. Empty projects offer Add
files, Attach folder, and Write project brief. Avoid a mandatory setup wizard
or a dashboard of decorative counts. A project need not have a paper or folder.

### Assistant alongside the project

Use a stable right-hand assistant pane with its own conversation chooser and
New conversation action. Opening project material preserves the conversation,
draft, streaming response, and reading position. Opening material for review
does not silently attach it to the next turn; “Use in chat” selects exact context
through the existing context tray.

The assistant header contains the conversation identity, working/attention
state, and a compact menu for Context, Outline, Activity, and conversation
actions. Context, Outline, and Activity use the assistant's existing panel slot
or a bounded overlay; they do not create a fourth persistent column. The
transcript remains mounted. Closing returns focus and scroll to the prior view.
An explicit Expand action can open a complex object in the main project area.

Keep pending approvals and Stop reachable when chat is collapsed or an
inspector is open. Use an attention indicator on the Chat view control that
opens the relevant conversation; do not move focus merely because a response
arrives. Conversation export/rename/archive live in the conversation menu.

### Composer controls

Place a compact control row below the text field, inside the composer:

`[Add files]  [Model ▾]  [Thinking ▾]                 [Send / Stop]`

Immediately above the text field, show selected-context chips and one assistant
setup chip such as `Research assistant · Read only`. Selecting this chip opens
assistant settings. At narrow assistant widths, wrap setup/context chips and
place Send/Stop in a dedicated trailing row; never force the whole composer
to scroll horizontally. Keep Latest next to the transcript's scroll position
when the user is reading earlier messages, rather than permanently competing
with message controls.

- Reuse the existing catalog-backed model list and serialized session save
  path. Do not hard-code model names or introduce another source of defaults.
- Label reasoning effort “Thinking” in ordinary UI. Offer the selected model's
  advertised values with readable labels; do not invent a universal on/off
  switch or imply that every model supports every effort.
- Keep Thinking visible when model choice is Automatic. If the effective model
  and efforts are known, show them; otherwise show an unavailable “Default”
  control with an explanation instead of guessing supported options.
- Show conversation overrides and inherited defaults in the selector. Offer
  “Use default” to remove an override. Project model defaults, if not supported
  by the current backend, are not added as part of this presentation change.
- Preserve the current active-turn restrictions for the initial migration:
  settings that cannot change while working remain visible and disabled with
  a brief reason. Changes affect subsequent messages, not a running response.
- Retain save errors, switching guards, draft persistence, and follow-up use of
  the same model/effort state. Verify rapid selection followed by Send and
  switching conversations while a selection save is pending.

### Assistant settings that can be adjusted directly

Replace the generic Setup entry point with **Assistant settings**, accessible
from the composer and a stable Workspace navigation footer. Provide:

1. Preset selection with a short description and an explicit Customize action.
2. Instructions with a readable editor and the scope of the edit.
3. Tools and context modules with effective On/Off/Unavailable state and the
   reason for unavailability. Simple changes should not require navigating
   the full inheritance table.
4. Permissions with clear Read only / Allow edits wording and separate network
   and host-execution disclosures where those existing controls apply.

Default changes to This conversation; make the scope visible. Retain the
advanced inheritance view with Built-in, Global, Project, and This conversation
provenance. “Use inherited setting” removes an override. Built-in presets stay
read-only; customizing offers a named copy and preserves explicit Save/Cancel.
Full editing opens in the main working area with chat retained alongside.

Describe changes that cause a successor native thread before applying them:
saved project context carries forward, and the earlier transcript remains
visible. Do not present a new binding as guaranteed full conversational memory.
Unavailable native web search remains visibly unavailable. Simpler labels do
not authorize broader access or bypass host-execution tests and grants.

### Where the old Research contents go

| Current entry | New canonical home |
| --- | --- |
| Setup / Edit harness | Composer setup chip → Assistant settings; full editor in working area |
| Documents | Library; attach/use exact revisions from the composer or context tray |
| Recipes | Assistant settings → Conversation recipes, with a composer shortcut while a recipe is active |
| Memory | Notes & evidence → Notes; selected/pinned notes also visible in Context |
| Evidence | Notes & evidence → Claims and evidence |
| Results / execution profiles | Analyses → Results / Execution settings; full-width editing |
| Paper review handoff | Writing or manuscript action → Send for review, using existing preview |
| Selective project exchange / coauthor materials | Project menu → Share project; Writing can link to the same destination |
| Whole-store backup / restore / retention | App Settings → Research data, labelled with their actual scope |
| Performance and qualification information | Advanced diagnostics |

Keep compatibility redirects until each old route and action has a working
replacement. The migration checklist must cover all seven tabs, including
less-used export, restore, execution, and recipe actions.

## Responsive layout and customization contract

One layout owner computes available width after the suite navigation. It then
allocates Workspace navigation and the project/assistant area. Saved widths
are requested preferences; viewport clamping never overwrites them.

- Preferred layout: project and chat side by side, with project given the
  larger share. Initial targets: navigation 224 px; assistant 380 px, adjustable
  from 360 to 560 px; usable project minimum 480 px. These are initial design
  values to validate in the native webview, not substitutes for measurement.
- Keep project and chat side by side while the actual local width accommodates
  both plus dividers. Collapse Workspace navigation to a labelled Projects
  control first. For example, a 1400 px window minus the current 176 px suite
  rail and 224 px project navigation leaves about 1000 px before dividers.
- When project plus chat no longer fits, show Project or Chat as a full-area
  view, with explicit controls for both and an attention indicator. On initial
  project entry default to Project; on live resize preserve the focused pane.
- Offer an explicit layout control: Project + chat, Project, Chat. Preserve the
  requested choice across temporary size constraints and restore it when space
  returns. Remember widths/layout on this device; Reset layout is available.
- Use the same shell for loading, loaded, empty, and error states. Width
  calculation references the actual layout container, never a wrapper whose
  intrinsic width is set by the panel being resized.
- Keep shared accessible dividers: pointer/keyboard resizing, numeric size
  options, Escape cancellation, and double-click reset. In full-area views,
  disable resizing and reserve no space for absent siblings.
- Constrain all flex/grid intermediates with `min-width: 0` / `min-height: 0`
  and use explicit track allocation. Contain long text, controls, and wrapping
  headers. Give genuinely wide tables/code their own horizontal scroller.
  Hiding root overflow alone is not a fix for offscreen content.
- Composer popovers open above their trigger and are bounded/flipped within
  the visible app area. They must escape clipping ancestors without losing
  outside-click, keyboard dismissal, or focus-return behavior.

## Implementation sequence

| Slice | Changes | Completion gate |
| --- | --- | --- |
| 1. Repair geometry | Reproduce Research/Outline composition; give pane sizing an explicit stable container/budget; unify loading geometry and focused mode | Panel and close/divider controls stay in bounds through opening, resizing, and restoring saved widths |
| 2. Move turn controls | Extract a composer-controls component; relocate Model/Thinking; compact the assistant header | Same selection/persistence/Send behavior, visible controls at narrow widths, usable menus |
| 3. Establish project-first shell | One project navigation, stable assistant slot, unified layout state, last-project/destination restoration | Project opens directly; pane switching retains edits, chat draft, and activity |
| 4. Redistribute Research | Direct tool destinations, Notes & evidence, assistant settings, scoped sharing/backup routes, old-route redirects | Every existing Research action remains reachable; common work takes at most two navigation actions |
| 5. Personalization and polish | Pins, visible Customize/Reset, keyboard interactions, consistent spacing and empty states | Preferences survive reload; hiding UI never changes assistant access; native geometry and focus checks pass |

Ship slices 1 and 2 independently of the larger navigation migration. Do not
delay the display repair until every tool has moved.

Primary implementation owners: `WorkspacePage.tsx`, `WorkspaceDesk.tsx`,
`WorkspaceConversationView.tsx`, `WorkspaceConversation.css`,
`WorkspaceResearchPanel.tsx`, `WorkspaceProjectSurface.tsx`,
`WorkspaceResearchStudio.tsx`, `WorkspaceComposerMenu.tsx`, harness-editor
components, and `deskLayout.ts`. Shared sizing changes belong in
`SidebarPanel.tsx` / `ResizeHandle.tsx` with cross-surface regression coverage.

Use a typed destination registry to map navigation, pin choices, and legacy
destinations to existing lazy surfaces. Keep layout preferences separate from
research records and permission configuration. Version optional layout state;
validate saved destinations, discard obsolete pins, and fall back gracefully.
Preserve pending drafts and exact source identities when moving views. Mount
only surfaces the user has opened and fetch data for the active destination;
retaining an opened editor must not initialize every research module at launch.

## Validation and acceptance

Geometry requires a real rendering engine. Add a composed Research/Outline
regression fixture using production shells and sizing logic, then qualify in
Tauri dev. Unit tests with mocked measurements remain useful for layout decisions
but do not prove absence of overflow.

Exercise 1024 × 700, 1280 × 800, 1400 × 800, and 1600 × 900; the user's actual
window size once observable; enlarged text/zoom; light and dark appearance;
minimum/default/maximum and legacy saved widths; navigation expanded/collapsed;
project/chat/both; loading/error/content states; long titles/paths; wide results;
and settings popovers close to window edges.

Required observations:

- App/shell scroll width does not exceed client width beyond rounding. Research,
  its header, close button and divider all have visible in-bounds rectangles.
  Resizing settles without a repeated shrinking or ResizeObserver feedback loop.
- Opening Research/its replacement never requires horizontal navigation. Wide
  tables may scroll locally while their title and controls remain visible.
- Open/close, Escape, keyboard resize, numeric resize, and focus return work.
  Background content is inert for modal views, but usable for docked panes.
- Model/Thinking changes persist to the intended conversation and match the
  subsequent turn, including save failure, reload, unsupported effort, and rapid
  model-change/Send cases. A running response keeps Stop available.
- An unsent message, unsaved file edit, and transcript reading position survive
  destination changes and narrow/wide transitions. Switching projects cannot
  attach another project's context or direct a pending save to a new target.
- Representative tasks: open a file; use a specific paper revision in chat;
  change thinking; adjust instructions; inspect a result and its evidence;
  find a saved note; pin a tool; share the project; locate whole-store backup.
- Plain unfiled chat remains functional without a research project. Navigation
  visibility does not grant tool access, and immutable review handoffs retain
  the normal Workflow preview.

Run focused frontend suites for Workspace, harness, layout, and shared panels;
then `npm test` and `npm run build` for the integrated redesign. Run appropriate
Rust/Workspace/Workflow suites if service or shared runtime changes are needed.
Native dev layout observations do not establish packaged or cross-platform
qualification; report those checks separately under the existing release policy.


## Implementation record — September 8, 2026

All five frontend slices above are implemented. The project-first shell has
six sections, named local tools and up to four ordered per-project pins.
Project/Chat/Project + chat and the assistant's pixel width persist per project;
old desk routes are validated through the shared typed destination registry.
The default navigator is 224 px and assistant is 380 px. The working area needs
848 px for a split and reserves 480 px for project content. Resizing clamps the
visible width without overwriting the preferred width. Selecting another
project does not replay old pane requests over that project's saved view.

Research and Outline now fill the allocated assistant slot without installing
a nested resize handle. Loading uses the same slot. Short windows use compact
headers and composer spacing, so the settings body remains scrollable.
Composer popovers are portaled and clamped to the viewport, with Escape and
focus return. Model and Thinking use the advertised model catalog and display
unavailable saved selections without replacing them. Model and draft writes
serialize; Send waits and retains the draft when a save fails. Assistant edits
flush outstanding composer writes before changing conversation settings.

Sources, notes, evidence, results, execution configuration, review handoffs and
project exchange are independent project destinations. Opening a paper in the
project source view does not change chat context. Review exposes a paper
selector and saved revision identifier. Whole-store backups, retention and
diagnostics live in General settings, and participate in its saving guard.
Quick instruction/module changes make an explicit new preset copy; the full
editor keeps shared-preset editing and inherited access controls.

Visited project views and quick assistant instructions remain mounted when
hidden, preserving unsaved edits; they are discarded when leaving their owning
project/conversation unless saved. Hidden execution, job, grid and check views
stop polling. No backend schema, command contract, runtime ownership,
credentials, tool authorization or immutable handoff semantics changed for
this redesign.

Validation:

- Full frontend suite: **641 tests across 84 files passed**, with no skipped
  tests. Regression coverage includes saved layout clamping, per-project pin
  ordering, unsupported model/effort selections, immediate model-change/Send,
  failed-save draft recovery, inspector draft retention, project-note retention
  and explicit review selection without a conversation.
- `npm run build`: passed. Vite retains its existing large-chunk advisory.
- `gui/e2e/workspace-layout.html` is a Vite development-only geometry fixture
  using the production Workspace components with in-memory service stubs and
  no native or model calls. Browser checks at 1024 × 700, 1280 × 800,
  1400 × 900 and 1600 × 900 found **0 px shell horizontal overflow** with
  Assistant settings open. Settings and Outline remained in bounds; keyboard
  resizing reserved project space, and composer-menu bounds and Escape were
  checked. Light/dark screenshots, compact/split switching and message draft
  retention were inspected. This fixture is not an application entry in the
  production bundle.
- A running `npm run tauri dev` / `target/debug/pipeline-gui` process was
  confirmed. Computer use could not attach to the development executable, so
  no native, packaged, cross-platform, authenticated-tool or user-default-window
  qualification is claimed. The larger stress matrix above (zoom, dense real
  projects, native dialogs and wide results) remains a release qualification
  task rather than evidence supplied by the stubbed browser fixture.

## Follow-up: reduce persistent interface density

The first implementation exposed too many controls together. The follow-up
keeps the project-first layout and replaces persistent toolbars with a single
workspace bar and controls that open when needed:

- Navigation starts closed. The project-name button opens a bounded drawer;
  Keep navigation open saves an optional pinned layout when both panes fit.
  Closing pinned navigation restores the full working area.
- The current-tool button and Command/Ctrl K open a searchable tool picker.
  All destinations are reachable directly; ordered project pins appear first.
  Arrow keys select results, Enter opens a tool and Escape returns focus.
- The Chat toggle hides or restores the companion pane. Focus and reset
  options live in its layout menu. Redundant project headers and Tool/Share
  rows are omitted in the integrated Workspace surface.
- Overview begins with the brief and contextual actions. Project setup and
  project notes are collapsed disclosures; up to three open action items
  link to the full task view. Closing these sections retains drafts.
- Chat has one title row with New chat and a conversation menu. Outline,
  Context, Activity & follow-ups, export and archive open from that menu.
  The composer contains the draft and one bottom control row: add, assistant
  settings, model, thinking and Send. Selected sources are expandable;
  empty sources and follow-up controls no longer occupy the composer.

Validation for this follow-up:

- Full frontend suite: **647 tests across 86 files passed**, none skipped.
  Added coverage checks default-collapsed navigation, pin/unpin behavior,
  keyboard tool selection, empty search results, modal focus restoration,
  action-menu dismissal, collapsed source editing and retained note drafts.
- Production build passed with the existing large-chunk advisory.
- The development browser fixture was inspected at 1024 × 700, 1280 × 800
  and 1600 × 900 in light/dark appearance. Shell overflow remained **0 px**.
  The navigation drawer was corrected to stay within 320 px, and assistant
  settings stayed within their allocated pane. Pinning/closing navigation,
  hiding/restoring chat, keyboard search and unsent-message retention were
  checked interactively.
- These are frontend fixture checks. The native, real-project, authenticated
  and packaged qualification boundaries recorded above remain unchanged.
