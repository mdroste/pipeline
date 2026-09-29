# Projects index and overview

Projects in the suite navigation opens the project index. It does not select or
load a project's working area. Home's View all action opens the same index.
Selecting a project name opens its Overview; Resume conversation opens the exact
active conversation identified by the index. Archived projects and conversations
are excluded. Startup restoration still honors the user's saved page preference.

The index supports search by project name, saved brief, or folder, and local
project pins. Pins are device presentation preferences under
`pipeline.projects.pinned`; they do not alter research records or permissions.
Rows show the most recent conversation, document, accepted note, or action-item
activity, plus pending note decisions, interrupted edits, or the next action item.
Attention links open the owning project's Overview, Edits, or Action items view.
The index refreshes on entry, window focus, completed conversation events, and
explicit Refresh. Errors retain the last loaded rows and offer retry.

## Project overview

The Overview contains:

- A direct resume action for the most recently updated active conversation in
  this project, or Start conversation when none exists. Neither action sends a
  message or inserts a synthetic prompt. Folderless projects are supported.
- A research brief drawn from accepted curated notes. With no curated brief,
  accepted questions and pinned notes supply a short initial brief. Origin/state
  and timestamps remain explicit; proposed notes never become brief content.
- Pending suggested notes, interrupted edit recovery, stale evidence, and up to
  three open action items. Deferred items follow active ones.
- Recent conversations, exact document revisions, and accepted notes.

Add research question saves a user-authored, accepted, pinned note. Manage brief
notes opens the existing project note controls, including note editing, pinning,
and acceptance/rejection of proposals. Pinning chooses the curated brief; an
unpinned accepted question can still appear in the initial fallback brief.
Folder, selected manuscript revision, reference execution, file watching, and
ignored-path controls now live in Project settings, a view within Overview.
Moving these controls changes neither manuscript selection nor execution authority.

All projects returns to the index. Conversation drafts are saved before leaving
Workspace; a save failure keeps the working area open. Explicit project entry
reveals the project pane even if the saved layout previously showed only chat.
Existing within-project retained views continue to preserve their local drafts.

## Owners and data boundary

- `components/ProjectIndexPage.tsx`, `hooks/useProjectIndex.ts`, and
  `lib/projectIndex.ts` own index presentation, refresh, and typed metadata.
- `components/project-surface/ProjectOverview.tsx`, `ProjectNotes.tsx`,
  `ProjectSettings.tsx`, and `ProjectActionItems.tsx` own the project pages.
- `hooks/useWorkspaceEntry.ts` interprets external pane-entry requests;
  `useRecentProjects.ts` owns the suite rail's existing recent-project list.
- `workbench/project/index.rs` and `index.sql` provide
  `workbench_project_index` through the normal `run_store` database gate.

Frontend paths above are relative to `gui/src/`; backend paths to
`gui/src-tauri/src/`. The index uses bounded SQLite metadata queries: at most
500 projects, 600-character brief previews, and 240-character activity titles.
It does not load transcripts, drafts, artifacts, inventories, or model runtimes.
Review and Automation runtime ownership remains separate. No schema migration
or new synthesized research summary is required.

## Validation — 2026-09-13

The full frontend suite passed (97 files, 716 tests), as did the Rust suite
(959 tests passed, 10 ignored). Production build, frontend formatting, Rust
formatting, Clippy with warnings denied, source-size checks, and Git whitespace
checks passed. The Markdown link check reports only pre-existing references to
missing temporary logs in `ASTRA_SEP7_BUGREPORT.md`; new documentation links resolve.

Native regression tests cover project scoping, archive exclusion, exact latest
conversation selection, accepted-note provenance, Unicode preview bounds,
activity ordering, task priority, and interrupted-edit attention. Frontend tests
cover index navigation with an existing selected project, search, persisted pins,
load/resume failures, precise resume callbacks, empty projects, proposed-note
exclusion, and composer saving before navigation.

The browser fixture at `gui/e2e/projects.html` uses the real index and project
components with explicitly synthetic data. It was inspected at 1024-pixel and
wide layouts in light and dark appearances; project opening, note editing,
pending-note inspection, and resume destinations were exercised. The Tauri
development build launched with disposable research and task stores; the
computer-use adapter could not attach to its unbundled native window. Browser
fixture checks are not native WebKit, authenticated model, or packaged-release
qualification.
