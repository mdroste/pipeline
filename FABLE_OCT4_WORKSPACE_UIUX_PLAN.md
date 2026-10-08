# Project workspace: visual and interaction polish plan

**Prepared:** October 4, 2026 (Claude Fable 5.1)
**Status:** W-19, W-20, and W-01 to W-04 are implemented in the working tree; see the implementation update at the end. W-05 to W-18 remain proposals.
**Scope:** The project workspace only: the chat pane and its composer, the bar and navigation around it, and the project pages beside it. The suite shell, Reviews, and Automations are out of scope except where they take space from the workspace.
**Baseline:** Working tree on `main` after commit `0705a6c` ("oct1"), version 0.9.5.
**Relationship to earlier plans:** [FABLE_SEP30_UIUX_PLAN.md](FABLE_SEP30_UIUX_PLAN.md) fixed structure (routes, vocabulary, the automation composer, the derived overview). This plan is about how the workspace looks and feels in use. It follows WP-12 in that series; its items are numbered W-01 to W-20.

## How this was assessed

I read the workspace components and styles, then ran the full-app browser fixture ([gui/e2e/app-full.html](gui/e2e/app-full.html)) and inspected the workspace at 1440×860 and 1024×700, in light and dark themes, in split, project-only, and chat-only layouts. I opened the composer menu, conversation menu, assistant settings, navigation drawer, and view picker, and visited Overview, Files, Documents, Manuscript, Results, and Research notes.

Not exercised: the native Tauri window, a live model turn (streaming, approvals, and errors were read from code), and a project with real files.

## Summary

The workspace has good bones: a calm neutral palette, a sensible split layout, and a strong overview page. What makes it feel unfinished is concentrated in four places.

1. **Native form controls stand in for designed controls.** The model picker, thinking picker, project switcher, conversation switcher, source roles, and every settings field are operating-system `<select>` menus.
2. **The chat pane hides what the assistant is doing and buries its own controls.** Tool activity is never shown, conversations are two levels deep, and settings replace the transcript.
3. **Small type and stacked chrome.** Most interface text is 11–12 px, and two toolbars with two identical "···" buttons sit on top of each other.
4. **No shared visual tokens.** One screen shows seven corner radii, two dark backgrounds, and hard-coded colors beside themed ones.

| # | Change | Area | Size | Impact |
|---|---|---|---|---|
| W-01 | Replace the model and thinking selects with one designed picker | Composer | M | High |
| W-02 | Rebuild the composer layout: auto-growing field, icon send/stop | Composer | S | High |
| W-03 | Turn the "+" menu into an attach menu; add drag-and-drop | Composer | S | Medium |
| W-04 | Show attached sources as chips with readable roles | Composer | S | Medium |
| W-05 | Add project-aware starter prompts to the empty conversation | Composer | S | Medium |
| W-06 | Show what the assistant is doing while it works | Conversation | M | High |
| W-07 | Quiet the message chrome: hover actions, timestamps, tighter type | Conversation | S | High |
| W-08 | Redesign approval and question cards; make errors dismissible | Conversation | S | Medium |
| W-09 | Fix transcript navigation: floating "latest" pill, automatic history | Conversation | S | Low |
| W-10 | Make the conversation title a switcher; drop browser prompts | Chat pane | M | High |
| W-11 | Open settings and inspectors as a sheet, in plain language | Chat pane | M | High |
| W-12 | Merge the stacked bars; one layout control | Chat pane | S | Medium |
| W-13 | Replace the drawer and wrapping tab row with one project sidebar | Navigation | L | High |
| W-14 | Collapse the app rail to icons inside a project | Navigation | S | Medium |
| W-15 | Upgrade the ⌘K picker to a grouped command palette | Navigation | M | Medium |
| W-16 | One page header for project pages; remove leftover panel chrome | Project pages | S | Medium |
| W-17 | Real empty states and readable labels everywhere | Project pages | S | Medium |
| W-18 | Put content before configuration in Write and Library | Project pages | M | High |
| W-19 | Design tokens and a split of the workspace stylesheet | Foundation | M | High |
| W-20 | One popover/menu primitive, tooltips, motion, and skeletons | Foundation | M | High |

Sizes use the earlier plan's scale: S is up to two days, M up to a week, L one to three weeks of focused agent work, before qualification.

## Constraints every item respects

- Presentation only. No change to Workspace, Review, or Tasks runtime, storage, or cancellation boundaries, and no change to command names or serialized fields.
- Plain chat stays light: lazy routes, tab-scoped loading, bounded transcripts, and coalesced streams are preserved.
- Researcher-facing copy uses Project, Conversation, Review, and Automation. Implementation terms stay out of the interface.
- New and touched surfaces use `gui/src/ui/`. No new page-scoped CSS files beyond the split in W-19.
- Source-size budget: `WorkspaceConversation.css` is 1,239 lines, already over the 1,200-line limit, and [WorkspacePage.tsx](gui/src/components/WorkspacePage.tsx) is 1,079. Every item below should shrink them, not grow them.

---

## A. Composer

### W-01 — One designed model and thinking picker

**Today.** [WorkspaceComposerControls.tsx](gui/src/components/WorkspaceComposerControls.tsx) renders two native `<select>` elements at 11 px with transparent borders.

- In the split layout the second one truncates to "Default th…".
- In the chat-only layout the two selects stretch across the whole composer, so each chevron sits roughly 350 px from its label.
- The opened menu is the operating system's, so it ignores the app theme and typography.
- The model catalog already supplies a description per model, a default flag, and a description per thinking level. None of it is shown.
- "Auto model" never says which model it resolves to.
- While the assistant works, both controls are disabled and the only explanation is a hover tooltip.

**Change.** One compact chip in the composer, for example "GPT-5 · High", that opens a popover above it.

- Model list: name, one-line description, a "Default" tag, and a check on the current choice. "Automatic" is first, with a line saying what it uses when that is known.
- Thinking: a segmented row under the list (Default, Low, Medium, High, …) limited to what the selected model supports, with the level's description beneath.
- A saved model that is no longer offered appears as a warning row, not as a silent extra option.
- Header line with the connected ChatGPT account, and remaining usage when the rate-limit data is available. That data exists today but appears only in Settings.
- Full keyboard support: arrows, type-ahead, Enter, Escape. The chip sizes to its content and never stretches.
- The existing `onChange(model, effort)` contract and saved-selection behavior stay as they are.

**Note on "provider."** The workspace assistant has one provider, the ChatGPT account. The picker should say so in its header instead of implying a provider choice that does not exist.

### W-02 — Composer layout

**Today.** The message field has a manual resize grip and fixed heights. Send is a text pill; Stop is a differently shaped red-outlined text button, so the row jumps when a turn starts. The shortcut hint is rendered and then hidden with `display: none`. Controls in the action row are 11 px.

**Change.** Auto-growing field from one line to about eight, with no resize grip. A single circular button on the right that is Send (arrow) when idle and Stop (square) while working, same size and position. Shortcut shown in the Send tooltip and the placeholder. Focus ring on the composer uses the accent color. Draft persistence is untouched.

### W-03 — "+" becomes an attach menu

**Today.** [WorkspaceComposerMenu.tsx](gui/src/components/WorkspaceComposerMenu.tsx) mixes five unrelated things. Three rows use text glyphs as icons (＋ ▱ ≋) and two rows have none, so labels do not align. "Voice input" advertises a feature that the menu itself says is unavailable. "Projects" is navigation inside an attach menu. "Assistant settings" duplicates the gear button directly beside it. Files cannot be dropped or pasted into the composer.

**Change.** The menu keeps only things you add to a message: Add files, Add a project document or source, Run an automation. Real icons from `ui/icons`. Remove Voice input, Projects, and Assistant settings from it. Add drag-and-drop and paste of files onto the composer with a visible drop target, routed through the existing attachments flow.

### W-04 — Sources as chips

**Today.** [WorkspaceContextTray.tsx](gui/src/components/WorkspaceContextTray.tsx) hides attached sources behind a collapsed "Sources · 3" disclosure. Opening it shows each source with a native select of raw role identifiers such as `data_dictionary`, `prior_draft`, and `referee_report`.

**Change.** A single always-visible chip row above the message field: file-type icon, name, remove button, and "+3 more" overflow. Clicking a chip opens a small popover to change its role, with readable labels ("Main paper", "Data dictionary", "Earlier draft", "Referee report").

### W-05 — Starter prompts

**Today.** An empty conversation shows an icon, "What are you working on?", and one sentence.

**Change.** Three or four suggestion chips derived from project state the overview already computes: for example "Fix the failed build", "Summarize what changed since Sep 28", "Draft a reply to the outstanding review finding". Choosing one fills the composer as an unsent draft, which is the mechanism "Draft a brief from the paper" already uses. A project with no signals falls back to general prompts.

## B. Conversation

### W-06 — Show the work

**Today.** While a turn runs, the only signals are "ChatGPT is working…" in the header and a pulsing dot. The transcript keeps only message items (`snapshot.items.filter(isMessage)`), and the frontend listens for message text only. The backend already normalizes item-started and item-completed events, so commands run, files read, and searches made are known but never displayed.

**Change.** A live activity line in the transcript during a turn ("Reading paper/main.tex", "Running the build", elapsed time). When the turn ends it collapses to one quiet line, "Worked for 1 m 12 s · 4 steps", that expands to the step list. A soft shimmer placeholder covers the wait before the first token. Rendering stays bounded and coalesced.

**To verify first.** Which item events the Workspace event bridge forwards today. If it drops them, this item needs a small, separately reviewed addition to the bridge.

### W-07 — Quieter messages

**Today.** Every message, including your own, carries a permanent bordered "Copy" button at least 74 px wide. Your own bubble is labeled "You". The assistant's avatar is the generic chat icon. There are no timestamps. Body line height is 1.8, which reads loose at 14 px in a 380 px pane.

**Change.** Icon-only actions that appear on hover or keyboard focus, and stay visible on the latest message. Remove the "You" label. Show time on hover and a date separator between days. Line height near 1.65, with consistent bubble radius. Add "Save as note" beside Copy on assistant messages, feeding the existing proposed-notes flow, if you want that shortcut (see open questions).

### W-08 — Approval, question, and error cards

**Today.** `RequestCard` in [WorkspacePage.tsx](gui/src/components/WorkspacePage.tsx) is an amber block docked between transcript and composer in a region capped at 38% height. The command or reason is a plain paragraph, options are a native select, and the buttons use one-off styles. Errors appear as a red box with no way to dismiss them.

**Change.** Cards render in the transcript flow where the request happened. Commands appear in a monospace block. Options are radio buttons. Buttons come from the kit, with "Allow once" primary and "Decline" secondary. No Enter-to-approve shortcut. Errors get a dismiss button and, where the cause is a lost connection, a "Reconnect" action.

### W-09 — Transcript navigation

**Today.** "↓ Latest" is a small text link pinned top right. Older history loads through bordered "Show earlier messages" and "Show 200 newer messages" buttons.

**Change.** A floating round "jump to latest" button at bottom center that appears when you scroll up, with a dot when new text arrived. Earlier messages load automatically when you reach the top, with a spinner. Paging limits are unchanged.

## C. Chat pane

### W-10 — Conversation title as switcher

**Today.** Switching conversations takes either the navigation drawer, where they sit in a collapsed "Conversations · 1" disclosure at the bottom, or a native select inside the "···" menu that appears only when there is more than one. Rename uses `window.prompt` and delete uses `window.confirm` ([useWorkspacePageController.ts:665](gui/src/hooks/useWorkspacePageController.ts:665)), which render as unstyled browser dialogs.

**Change.** The title in the chat header becomes a button with a chevron. It opens a popover with search, recent conversations with relative times, a working or needs-attention dot, "New conversation", and an "Archived" toggle. Double-click the title to rename in place. Delete and move go through `DialogService`. The "+" stays as the quick new-conversation button.

### W-11 — Settings and inspectors as a sheet

**Today.** Assistant settings, Outline, Context, and Activity each replace the transcript, so you lose sight of the conversation you are configuring. The settings panel is titled "Assistant settings" above a tab also named "Assistant settings", next to a "Recipes" tab. Its summary line reads "Inspect · no command network · 64 KiB · 1 of 2 modules active · workbench-inspect". Every field is a native select.

**Change.**

- Inspectors open as a sheet that slides over the chat pane from the right edge, with the transcript still visible behind when width allows, and Escape to close.
- Plain summary: "Read only · commands have no internet access · 1 of 2 tools on".
- Access mode becomes a two-option segmented control (Read only, Allow edits). Because it is the most consequential per-message setting, it also appears as a small chip in the composer beside the model picker.
- Rename the duplicate tab to "Profile". "Recipes" needs a researcher-facing name (see open questions).

### W-12 — One bar, one layout control

**Today.** The workspace bar ("Chat" toggle and a "···" layout menu) sits directly above the chat header ("+" and a "···" conversation menu). The two "···" buttons are about 30 px apart and do different things. "Chat" reads as a button, not a state. In chat-only layout a third control, "Back to project", appears, and the current-view button for the hidden project pane stays in the bar.

**Change.** Replace the Chat toggle, layout menu, and "Back to project" with one three-segment icon control: Project, Split, Chat. Split is disabled with a tooltip when the window is too narrow. Reset layout moves to the conversation menu. The "···" text character is replaced by a proper icon, and the only overflow menu left in the pane is the conversation's.

## D. Navigation

### W-13 — One project sidebar

**Today.** Project navigation is spread over three mechanisms.

- A drawer titled "Projects", opened from the project name, next to an app rail that already lists Projects and recent projects. It uses a native select as the project switcher and plain text for "Keep navigation open".
- Its six sections have no icons, although [workspaceNavigation.ts](gui/src/lib/workspaceNavigation.ts) already defines one for each.
- A row of 11–12 px text tabs under the bar for the current section. With nine entries in Write and eight in Analyze it wraps to two lines at 1440 px.

Three different gear icons are also in view: connection settings in the drawer, assistant settings in the composer, and a "Project settings" link on the overview.

**Change.** A slim, persistent project sidebar: project switcher at the top as a popover with search, the six sections with icons, and the current section expanded to show its pages. It collapses to icons. The wrapping tab row is removed, and the bar shows a breadcrumb ("Write / Manuscript"). Settings are labeled by what they configure, not by three identical gears.

**This reverses a recent decision.** The September 8 redesign made navigation closed by default. If you prefer to keep the drawer, the fallback is smaller: add icons, replace the select, move conversations out (W-10), and turn the tab row into a single scrollable line with an overflow menu.

### W-14 — Icon rail inside a project

**Today.** The app rail is 216 px wide at all times. At the minimum window size of 1024×700 that leaves 808 px for the workspace, and the split layout needs 848, so project and chat can never be side by side at minimum size.

**Change.** Inside a project the app rail collapses to an icon rail of about 56 px, expanding on hover or by a toggle. Split view then fits at 1024 px, and the project pane gains 160 px at every size.

### W-15 — Command palette

**Today.** ⌘K opens "Open a project view": a flat list of page names with the section name at the right, plus a few actions. It has a close button and a title bar, and nothing in the workspace hints that it exists except a tooltip.

**Change.** Results grouped under headings (Recent, Pages, Conversations, Documents, Actions) with icons and match highlighting. Recent items first when the query is empty. A "Search or jump to… ⌘K" field in the bar makes it discoverable. Conversations and project documents become searchable from it.

## E. Project pages

### W-16 — One page header

**Today.** Pages built from the research panel (Results, Research notes, and others) keep sidebar chrome on a full page: the project name repeated as a subtitle and a "×" close button that has nothing sensible to close. Other pages use their own header styles.

**Change.** One `PageHeader` in `ui/`: title, one-line description, primary action on the right. Remove the "×" and the repeated project name from every full-page view.

### W-17 — Empty states and readable labels

**Today.**

- Results shows the heading "RUN HISTORY", one caveat sentence, and nothing else.
- Files shows three disabled buttons and "Find a file above to open it."
- Research notes offers a select whose visible value is `next_step` and a button labeled "Add reviewed note".
- Loading is plain text: "Opening project…", "Loading sources…".

**Change.** Every empty page uses `ui/EmptyState` with one sentence and one starting action ("No results yet. Run an analysis or ask the assistant to capture one."). All identifiers shown to researchers go through one label map. Files opens on a browsable list of the project folder instead of an empty editor.

### W-18 — Content before configuration

**Today.** Manuscript opens on two forms: a source-path form and a "Build settings" panel with ten fields (configuration, build name, working directory, engine, root document, expected PDF, declared inputs, timeout), under a "Local jobs · 0 active · 0 retained" disclosure. Documents opens with three rows of equal-weight buttons above the reader.

**Change.** Manuscript opens on the document: editor and compiled preview, with a status chip ("pdflatex · last build failed · 1 error") that opens build settings in a sheet. The jobs strip appears only when a job exists. Documents gets one toolbar with a version picker, one primary action, and an overflow menu for Compare, Stack, and Import.

## F. Foundation

W-19 and W-20 are prerequisites. In practice they are built first, with W-01 as the first consumer.

### W-19 — Tokens and stylesheet split

**Today.** Measured on one workspace screen: corner radii of 0, 4, 6, 8, 10, 13 px and 50%; type at 11, 12, 13, 14, and 20 px with most labels at 11–12; dark theme backgrounds of `#0a0a0a` for chat and bar but `#171717` for the project pane. Floating menus and the view picker hard-code `#fff`, `#ddd`, and `#262626` with a separate dark block, while the rest uses `--chat-*` variables. The stylesheet is a stack of historical layers: `.workspace-composer-dock` is defined three times, `.workspace-chat-header` twice plus a modifier, `.workspace-composer-controls select` twice.

**Change.** Semantic tokens on the app root: surface, raised, sunken, border, text, muted, accent; three radii; one elevation scale; a type ramp with 13 px as the floor for interactive labels and 12 px for metadata. One dark surface for both panes. Split the stylesheet into per-component files written once against tokens (desk, transcript, composer, navigation), each under the size budget, and delete the override layers.

### W-20 — Popover, menu, tooltip, motion

**Today.** There are four separate floating-layer implementations with their own positioning code: `WorkspaceMenu`, `ConversationMenu` (absolutely positioned, so it can be clipped by its pane), `WorkspaceComposerMenu`, and `WorkspaceToolPicker`. Only one supports arrow keys. Layers appear and disappear instantly. Explanations rely on the browser `title` tooltip, which is slow and unstyled.

**Change.** Add to `ui/`: `Popover`, `Menu`, `Select` (listbox), `SegmentedControl`, `IconButton`, `Tooltip`, `Sheet`, `Skeleton`, and a small toast for confirmations such as "Copied" and "Moved to project". One positioning routine, arrow-key navigation, focus return, a 120 ms fade and scale that already respects the reduced-motion rule in `App.css`. Replace the four implementations and the native selects in the workspace with them.

---

## Suggested order

1. **Foundation:** W-19, W-20.
2. **Composer:** W-01, W-02, W-03, W-04. This is the part you called out and the most visible daily.
3. **Chat pane:** W-12, W-10, W-11, W-07.
4. **Conversation:** W-06, W-08, W-09, W-05.
5. **Navigation:** W-14, W-13, W-15.
6. **Project pages:** W-16, W-17, W-18.

Each step is verified in the browser fixture at 1440×860 and 1024×700 in both themes, then by the frontend test suite, build, format check, and the source-size check. Steps 1 to 3 need no backend change. W-06 may need a small event-bridge addition.

## Decisions I need from you

1. **Sidebar or drawer (W-13).** Persistent project sidebar, or keep the closed-by-default drawer and take the smaller fallback?
2. **Popover engine (W-20).** Build the positioning primitive in-house on the code that exists, or add a small dependency such as Floating UI? I recommend in-house: the app has no UI dependency today and the existing code already handles the hard cases.
3. **Access mode in the composer (W-11).** Show Read only / Allow edits as a chip beside the model picker, or keep it only in settings?
4. **"Recipes" (W-11).** Pick a researcher-facing name. Candidates: "Guided tasks" or "Saved instructions".
5. **"Save as note" on messages (W-07).** Include it, or keep message actions to Copy only?
6. **Assistant name.** Messages are labeled "ChatGPT". Keep that, or use "Assistant" with the model name in smaller text?

## Left out on purpose

- Any change to the suite shell beyond the rail width inside a project.
- Voice input, message editing, and regenerate. These need runtime work, not polish.
- Dissolving `WorkspaceResearchPanel` along its module seams. It is architecture debt already recorded in the September 30 plan.
- A new color identity or typeface. The neutral palette and Inter are right for the audience; the problem is consistency, not character.

---

*Implementation updates append below this line.*

## Implementation update — October 4, 2026 (Claude Fable 5.1)

The foundation (W-19, W-20) and the composer (W-01 to W-04) are implemented in the working tree. No backend code, command name, or serialized field changed.

| Item | Status | What landed |
|---|---|---|
| W-19 | **Done** | Semantic `--ui-*` tokens in `gui/src/App.css` for surfaces, borders, text, accent, three radii, three elevations, and a type ramp, with dark values under `.dark`. They are mapped into Tailwind (`bg-surface`, `text-ink-muted`, `rounded-ui-md`, `shadow-ui-3`, `text-ui-label`, …). The 1,239-line `WorkspaceConversation.css` is deleted and replaced by five files written once against the tokens, the largest 257 lines: `WorkspaceDesk.css`, `WorkspaceTranscript.css`, `WorkspaceComposer.css`, `WorkspaceNavigation.css`, `WorkspaceInspector.css`. Eleven unused class families were dropped. Chat, project pane, and embedded panels now share one surface in each theme. |
| W-20 | **Done** | `gui/src/ui/` gained `Popover`, `Menu`, `Select`, `SegmentedControl`, `Tooltip`, `IconButton`, `Sheet`, `Skeleton`, and one positioning routine (`anchoredPosition.ts`), with arrow-key and type-ahead movement, focus return, nested-layer handling, and a 120 ms entrance that honors reduced motion. The conversation menu, layout menu, per-conversation actions menu, and attach menu all run on them. |
| W-01 | **Done** | `WorkspaceComposerControls.tsx` is one chip ("GPT-5 Codex · High") opening a picker: account and remaining usage loaded only when it opens, a model list with descriptions and a Default tag, and a thinking row limited to what the model supports with the level's description. Saved choices that are no longer offered show as warnings on the chip and in the list. While a response runs the picker still opens and says why choices are locked. |
| W-02 | **Done** | The message field grows from one line to about eight and then scrolls; the resize grip is gone. Send and Stop are one round button in one position. The shortcut is in the placeholder and the Send tooltip. Focus uses the accent color. |
| W-03 | **Done, with two limits** | The "+" menu offers Add files, Add a project document, and Run an automation, with icons from the shared set. Voice input, Projects, and Assistant settings are removed from it. Files dropped on the conversation are imported through the same path and attached as sources. See "Limits" below. |
| W-04 | **Done** | Sources are a chip row above the field: icon by kind, name, remove button, and a role menu with readable labels. The number of chips follows the composer's width, with the rest behind "+N more". |

**Also done in passing.** `RequestCard` moved to `WorkspaceRequestCard.tsx` unchanged, which with the other edits takes `WorkspacePage.tsx` from 1,079 to 970 lines. The default source-role rule now has one owner (`lib/workspaceAttachments.ts`). The full-app fixture gained `?sources=N`, `?model=`, and `?effort=`, plus stubs for usage limits, session updates, and imports.

**Corrections to this plan's findings.**

- W-20 said a toast was needed. One already exists (`notify` in `DialogService.tsx`), so it is reused, not rebuilt.
- W-20 counted the view picker as a fourth positioning implementation. It is a centered modal on the shared `useModalDialog` hook. It keeps its code and takes the new tokens.

**Limits.**

- **Pasting files is not implemented.** Files pasted into the desktop webview carry no path, and import is by path. It needs a backend command that imports from bytes, which is outside a presentation change.
- **"Add a project document" lists documents only.** Library sources are still added from the Library pages. I could not confirm that library source records are accepted as conversation sources, so I did not offer them.
- **"Automatic" does not name the model it resolves to.** A connection default can override the catalog default, so the picker says it uses the connection default.
- **Native selects remain** in the approval card (W-08), the move-conversation dialog (W-10), assistant settings (W-11), and project pages (W-16 to W-18). Replaced now: model, thinking, source roles, the drawer's project switcher, and the conversation switcher in the conversation menu.
- **`Sheet` has no consumer yet.** It is built and tested for W-11.
- **`WorkspaceIcon.tsx` still exists** beside `ui/icons.tsx` (23 call sites). New controls use the shared set.

**Validation.** Frontend: 111 files / 819 tests (baseline 107 / 790), TypeScript, production build, Prettier, the source-size check, and the Markdown link check. Browser fixture at 1440×860 and 1024×700, light and dark: picker in every state, attach menu, documents view, chip role menu and overflow, auto-growing field, drawer project switcher, conversation and layout menus.

**Not exercised.** The native Tauri window. Native file drop is covered by unit tests with stubbed drag events only; it has not been tried with a real drag. No live model turn was run, so the Stop state is covered by a unit test. No Rust changed, so the Rust gates were not run.

**Next:** W-12, W-10, W-11, W-07 (the chat pane), pending the decisions listed above.
