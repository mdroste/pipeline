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

The Overview reports state the application already computed. Three rules hold
for every section: its content is derived rather than typed in, a section with
nothing to report does not render, and each row has exactly one action (open a
view, or open a new conversation with an unsent draft for the assistant).

- **Research brief.** Accepted curated notes, or accepted questions and pinned
  notes when none are curated. Proposed notes never become brief content. With a
  paper but no brief, Draft a brief from the paper opens a conversation whose
  unsent draft asks the assistant to propose brief notes; nothing is sent.
- **Where you left off.** The most recently updated active conversation: its
  last request (when its transcript is loaded), any unsent draft, the runs
  started from it, and accepted next-step or handoff notes. Resume sends nothing.
- **State of the draft.** The current version of the paper, the last build
  receipt, outstanding and regressed review findings from the linked issue
  ledgers, and whether the current version was reviewed.
- **Out of sync.** Interrupted edits, source files changed since the current
  version was imported, linked numbers that no longer match their results,
  claims resting on stale or missing evidence, and change-impact propagation.
- **Revision round.** Progress through review comments, comments without a
  drafted reply, and comments naming a check that has not run. Shown only when
  comments or a revision campaign exist.
- **Repository.** See below. Shown only when the folder is a Git repository.
- **Since you were last here.** Folder changes, new versions, commits, finished
  reviews and automations, scheduled-check attention, and finished runs since
  the previous visit ended.
- **Needs your decision** and **Other conversations.** Suggested notes, up to
  three open action items, and the remaining recent conversations.

A visit ends after 30 minutes without activity in the project. The marker and a
truncated-hash snapshot of the file list (at most 3,000 files) are device
preferences under `pipeline.project.visit.<id>`; they hold no file content.

The derived sources load after first paint and independently; one that fails or
is slow omits its rows. They reload on return to the Overview, on Check again,
and at most every two minutes on background refresh. Project settings holds one
optional target date and label, shown under the project name when set.

### Conversation-only projects

A project with no attached folder and no imported document has nothing to
derive. Its Overview is reduced to the brief, the conversation to resume, other
conversations, pending decisions, and the import actions, and it loads none of
the derived sources. When such a project is opened on its Overview and no
layout has been chosen for it on this device, the desk shows the conversation
alone with the first brief note under its title. That default is not saved, so
a project that later gains a paper opens on the split layout again.

### Repository

When the attached folder is inside a Git work tree, the Overview reads the
branch, uncommitted and untracked counts, merge conflicts, recent commits that
touch the folder, and the ahead/behind counts recorded by the last fetch. This
uses only the local clone. A remote URL is reduced to host, owner, and name
before it leaves the backend; only `github.com` remotes produce links (the
repository, a compare view, a branch's commits), opened in the browser.

Check GitHub (or Check *host*) is the one network operation. It runs
`git fetch` for the tracked remote with the researcher's own Git credentials,
only on request, with credential prompts disabled and a 30-second limit. It
updates remote-tracking refs only: nothing is committed, merged, or pushed, and
the working tree is untouched. Git runs with hooks and the filesystem monitor
disabled, outside the database workers. Pull requests, issues, and CI status
are not read; they would require GitHub API authentication.

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
- `lib/projectOverview.ts` holds every Overview derivation as pure functions
  (blocks, rows, visits, the thin-project test); `hooks/useProjectSignals.ts`
  loads their inputs; `lib/repositoryClient.ts` and
  `workbench/project/repository.rs` own repository status and fetch.
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

## Validation — 2026-10-01 (derived overview and repository)

The full frontend suite passed (105 files, 773 tests), with TypeScript, the
production build, and Prettier. Native tests for `workbench::project` passed
(52 passed, 6 ignored), including a real clone-and-fetch round trip against a
local bare remote; the full Rust suite was not rerun. New tests cover every
derivation, silence when a source fails, single-action rows, visit tracking,
the conversation-first default, target-date validation, remote-URL credential
stripping, and porcelain parsing.

The full-app fixture (`gui/e2e/app-full.html`) gained `?project=rich|thin|bare`.
The Overview was inspected at 1440×860 in light and dark and at 1024×700, as
were the conversation-only entry and the Repository settings card. Not
exercised: the native window, a fetch against github.com, and the derived
sources against a large real project.
