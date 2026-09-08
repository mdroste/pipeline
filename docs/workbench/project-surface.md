# Project home, reading, and isolated tasks

This document describes PI-00–PI-03 from `PIPELINE_IMPROVEMENT.md`. Release
claims belong to [release-qualification.md](release-qualification.md); the
[local evidence record](project-surface-qualification.json) contains observed
outcomes and measurements, with unqualified cases stated separately.

## Using the project surface

In the Workspace UI a workspace is called a **project**. The side panel shows
the selected project as a card (name, folder, **Open project**); the composer's
**+ → Projects** entry opens the same view. **New project** is a one-step dialog
that names the project and optionally attaches a folder and imports a paper;
each step is recorded so a retry after a partial failure neither creates a
second project nor imports the paper twice. Overview, Documents, and Edits
work without ChatGPT sign-in. A project without a folder keeps its saved
research. An ordinary unfiled conversation does not load these modules.

The desk offers **Project**, **Assistant**, and **Show both** when both panes
fit. At narrow widths, one pane fills the available height; hidden panes retain
their local drafts and scroll state. **Show navigation** reopens a collapsed
project/conversation list. A narrow inspector temporarily fills the work area
and its Close action returns to the desk. Pending-request counts and a running
response's Stop action remain available from the project view. Creating a
conversation from a research selection opens the assistant pane.

New project contains keyboard focus, makes background controls inert, and
returns focus to its opener on dismissal. Escape/backdrop dismissal is disabled
while creation is pending, retaining the existing partial-failure retry behavior.

The project view uses plain-language labels for the underlying records. The
table below maps them to the persisted concepts; storage, commands, and
record names are unchanged.

| UI label | Record |
|---|---|
| Current version of the paper | accepted manuscript revision (`manuscriptRevisionId`) |
| Reference results run | accepted computational baseline (`baselineExecutionId`) |
| Project summary / Show in project summary | brief notes (`briefNoteIds`) |
| Hide from the assistant | context exclusion (`excludedNoteIds`) |
| What the assistant is told | assembled project context preview |
| Task status Open / In progress / Later / Done / Dropped | `open` / `investigating` / `deferred` / `completed` / `rejected` |
| Action items, New action item | local research task records (`ResearchTask`); distinct from durable Tasks chains |
| Edit, working copy, Start an edit | isolated task checkpoint (plain copy or Git worktree) |
| Check for changes / Accept files / Discard this edit | capture, apply, reject |
| Restore original files / Undo | recovery and undo of an application |
| Saved passages | annotation anchors and revision mappings |

**Overview** holds the paper card (current version and reference results),
the folder card (attach or change the folder, refresh the file list, optional
30-second polling, paths to leave out, and the folder-versus-paper status
line), the project summary with its context preview, notes, and action items.
A newer import or execution does not replace the chosen version or reference
run. Rejected and suggested notes remain inspectable but are not accepted
memory. Note edits preserve the preceding wording; the overview shows the
latest 50 changes. Use **Continue in a conversation** to open a fresh Research
assistant conversation with that context; no message is sent automatically.

Refresh the file list after external edits. Optional focus refresh and
30-second polling update the inventory, not the document currently being read.
The status line compares the current paper version's captured manifest with
the last inventory. It does not certify undeclared dependencies.
Relocation preserves project/object IDs; files outside the relocated scope
remain explicitly unavailable. Previously started edits retain their original
root identities and cannot apply to a replacement folder.

**Documents** imports PDF, TeX, DOCX, Markdown, BibTeX, research code, JSON,
CSV/TSV, or an explicit LaTeX folder. Documents use immutable versions. Read
source text, render Markdown/math, search an exact version, or inspect a PDF
page. Source spans use UTF-8 byte offsets, a content hash, and surrounding
text; PDF regions use an exact version/page and normalized rectangle.
Selections support Ask, Add note, Create action item, and Propose revision. These save
the anchor first. A question opens a draft conversation, and a task remains
separate from chat.

Compare a version beside another; the pin and per-version scroll/page
position persist locally. A saved passage always reopens its original version.
Mapping an unchanged hash is exact; a unique moved passage is only a
candidate, repeated text is ambiguous, and absent text or changed page
geometry is missing. Mapping never silently changes an annotation. Scanned or
lossy PDF extraction shows its limits; page inspection remains available when
the PDF renderer is installed. Tables preserve literal cell text and quoted
CSV fields, with bounded rows/cells. File previews capture immutable
text/image bytes; large and other binary files show size/hash rather than
fabricated previews.

**Edits** starts a working copy for an open task and reviews, accepts, or
discards the resulting changes; accepted-edit history with undo and recovery
lives there too. The lifecycle below uses the record vocabulary.

## Isolated task lifecycle

1. Record an objective, optional source anchor, and intended outputs/checks.
2. Select current working files, including dirty or untracked files, and choose
   a local copy or Git worktree. The checkpoint records their bytes/hashes,
   root identities, task instructions, source selection, and accepted baseline.
3. Open the task conversation, or work in the displayed task directory. Native
   tools receive that task root. Legacy host execution tools are disabled in
   task conversations so an old profile cannot accidentally run in the source.
4. Capture changes and inspect source diffs, image/binary previews, and recorded
   execution receipt summaries. Capturing or applying cannot race an active
   Workspace turn; stop it or wait for completion first.
5. Accept all changed files or a selected subset. A file's source content and
   executable flag must still match the checkpoint. Unrelated files and the
   source Git index/branch are preserved. Recheck the resulting combination;
   acceptance is not a research-correctness judgment.
6. Undo an acceptance only while its output files still match. Rejecting a
   proposal changes task state and keeps the snapshots/copy for inspection.
   Neither operation performs a destructive reset, commit, or push.

Git setup uses the local launch policy (`/bin/zsh -lic` on this Mac), with
arguments passed positionally, hooks and fsmonitor disabled, a new `codex/`
branch, and `worktree add --no-checkout`. Only selected working bytes are
copied. Tracked symlinks/submodules, unsupported files, excessive metadata,
and absent repositories fail explicitly. Sparse/LFS dependencies are not
materialized automatically. The live fixture proves preservation of a staged
index, unstaged content, and a selected untracked file. Task copies are private
regular-file copies, not hardlinks to the source. A task-directory-owned
`.gitignore` excludes copies from the source repository's untracked-file list;
the source `.gitignore` is not edited. Source-tree document imports exclude
Git metadata, task copies and in-flight application recovery files.

## Safety and durability contracts

Migration 6 appends versioned `project_records`, operation receipts, and local
execution grants. Existing migrations remain unchanged. New files are under
`workbench/project/`; storage, processes, credentials, cancellation, and native
bindings remain independent of Workflows. No new implicit Review bridge exists.

Project mutations have operation IDs and optimistic record revisions. An
unknown receipt is recorded before a filesystem side effect, so a replay does
not silently repeat an ambiguous action. Refresh and inspect recovery history.
A per-project filesystem lock serializes host execution against acceptance;
the process-global native-turn permit also covers checkpoint, capture,
accept/reject, recovery and undo commands.

On Unix, directory-descriptor-relative reads and replacement reject symlinks,
Git/task metadata, traversal, control characters, nonregular files and oversized
inputs. Acceptance journals every file before effects, retains originals under
unique recovery names, uses no-clobber placement, syncs files/directories, and
updates durable progress. Existing read/write permissions are retained.
Interrupted acceptance/rollback can reconcile the file hashes and restore
originals. External edits during recovery cause a visible conflict; neither
original snapshots nor unrelated edits are discarded. The application journal
remains authoritative if a crash delays the checkpoint-state update.

Whole-store `.pwrx` archives retain the registered snapshot blobs and project
records. Import retires host execution grants and local operation receipts as
well as native bindings. Restored task paths are not promised portable:
identity validation fails closed until a fresh task is created. Archives do
not grant execution access on the receiving machine.

Bounds: 5,000 inventory files / 20,000 visited entries / five seconds per scan;
32 MiB per file and 256 MiB per task capture; 16 MiB document text with 128 KiB
reader segments; 512 KiB text previews; 200 table rows, 60 columns and 64 KiB
cells. Common cache/build directories and task/Git metadata are excluded.
Inventory/preview limits are visible. A partial task inventory cannot be
accepted as a complete capture. Further paging, richer binary viewers, hunk
acceptance and automatic scratch cleanup are subsequent work.

## Execution qualification

Research profiles launch host commands, outside the native conversation
sandbox. The UI presents exact argv, cwd, declared inputs/outputs and launch
identity, then requires explicit host authorization. A model tool cannot grant
that authorization. Grants bind the profile revision, resolved executable,
directly referenced launch scripts, known zsh startup files, declared input hashes, root identity, environment and
timeout; changes invalidate them. Transitive dependencies and arbitrary host
network/file access are not sandboxed. Executable resolution preserves argv[0]
for aliases such as `pdflatex`.

Input coverage is **declared-only**. A before/after hash check does not establish
an atomic immutable snapshot, so live-file consistency remains uncertain.
Inputs that change during execution and untouched preexisting declared outputs
invalidate success. Only completed, validated receipts are eligible as accepted
baselines. Incompatible units, estimands or transformations cannot be overridden
with a rationale in numeric comparisons.

Stata always uses `/bin/zsh -lic` and `oldstata`; no direct executable or probe
is used. Cancellation is observed by the process owner. Because this machine's
wrapper has no signal trap, it gets up to 30 seconds to return through Legacy
Time Off. The local cooperative-timeout fixture reached `exit, clear`, returned
through the wrapper, restored the normal date and remained `timed_out`, not
successful. A forced kill records `stataCleanupRequired`; the receipt instructs
the user to verify exit and run Legacy Time Off. That forced path is **not**
qualified as automatic cleanup.

Settings → Workspace ChatGPT includes a capability inventory and actionable
missing-tool information. Discovery is not qualification. File acceptance is
unavailable on Windows; Unix implementation is locally qualified on macOS only.
Authenticated model behavior, packaged builds, other operating systems, and
broader PDF/accessibility/performance evaluations remain release gates.

For disposable GUI testing, debug builds alone accept
`PIPELINE_WORKBENCH_DEV_ROOT=/absolute/temporary/store`. Start with
`cd gui && npm run tauri dev`; never use the installed app for this workflow.
Release builds ignore that environment variable.
