# Projects and conversations

A project keeps papers, slide decks, other documents, files, notes, and conversations together. This guide covers projects, conversations, and the **Overview**,
**Files**, and **Notes** areas. [Write and review](analysis-and-writing.md)
covers **Documents**, **Reviews**, and **Changes**.

On **Home**, choose **Explore an example project** for a guided walkthrough
using the fictional Port Harlan paper from the Reviews tutorial. Try the
project controls, edit sample notes, and track action items without a model
connection. Sample changes are discarded when you close the tutorial.

## Create a project

Open **Projects** and choose **New project**. Choose a folder and enter a name.
Every new project needs a folder; an existing local Git repository works too.
You can also choose an initial document and its type: **Paper**, **Slides**,
**Appendix**, **Response letter**, or **Other**. Then choose **Create project**.

Choose **Open folder** to start from an existing research folder or local Git repository. Pipeline proposes a name and inspects nearby documents. If several possible roots are found, choose one or browse the folder. Opening it does not build or execute its code. Publication details are optional.

A folder lets you browse and edit files, run your own code, and see Git
status. Git is optional. Projects saved by an older version without a folder
keep their conversations, notes, and documents. Attach their folder in
**Project settings**. You can change a project's folder there too. If setup
stops after the project is created, choose
**Try again** to finish the same project.

**Projects** lists projects under **Pinned** and **Recently active**. Search
by project name, research brief, or folder. Choose a project row to open its
last working view. Choose **Overview** on the row to open the dashboard. **Pin** keeps it at the top on this computer; **Unpin** returns it
to the recent list. When projects have deadlines, choose **Deadline** in
**Sort projects** to put the nearest first and see each deadline. Approaching deadlines and interrupted edits also appear on the row.

Inside a project, the sidebar lists its six areas and its conversations:

| Area | What it holds |
| --- | --- |
| **Overview** | Where the paper stands and what to do next |
| **Documents** | Named papers, slide decks, and other documents, each with its own versions |
| **Files** | Everything in the project folder |
| **Reviews** | Reviews, their findings, and replies to referees |
| **Changes** | Edits the assistant proposed, and version control |
| **Notes** | Notes the assistant can use, and action items |

Each area's views are tabs at the top of the page. Choose the project's name in
the sidebar or desk toolbar to move to another project or open **Project settings**.
The desk toolbar keeps the project name visible when the sidebar is collapsed. Press
Command/Ctrl+K, or choose **Search** in the sidebar, to find any project,
conversation, review, automation, document, note, finding, or file. Choose
**This project** or **All projects**, then narrow results with **Type**.
Notes and saved findings are searchable by their text. Enter at least two
characters to search the current captured versions of documents and text files
in project folders. Content search matches the phrase you type. Results show
the surrounding text, project, and document version or file path; opening one
takes you to its passage, page, or line. Current files are searched even when
they are absent from the saved file list. Search reads local text and does not
start extraction, reviews, or models.

Coverage reports missing captured text, unavailable folders, skipped files,
and search limits. **Load more matching passages** expands document results;
narrow the phrase when a document or folder reaches its limit. Findings
initially cover the newest 50 reviews; **Search older findings** includes more.
**Search older automations** includes additional saved step automations.
Step automations retain their project context when opened from search.
Archived records, older document versions, conversation messages, binary
files, and ignored folders are excluded. **Actions** opens pages or review
setup; it does not start a review. The layout menu switches
between **Project only**, **Conversation only**, and
**Project + conversation**, and places the conversation on the **Left**,
**Right**, or **Bottom**. You can also drag a conversation tab onto the
project to move the conversation to that side, and drag the edge between them
to resize it. Each project remembers its layout, and **Reset layout** puts the
conversation back on the right. Project areas work without a model connection.
Conversations and reviews need one. See [Models](connections.md).

## Work in a conversation

Start a conversation from the Overview, with **New conversation** above the conversation, or
from **New → New conversation** in the sidebar. Outside a project, **New →
New conversation** starts one with no project. **Browse conversations** at the bottom of **Projects** opens the conversation list.
Use **New conversation** there to start a conversation without a project. Search conversations or use
the **No project** and **Archived** filters. Choose a chat to resume it, or its project name to open the project.
**Move to project…** adds a standalone chat to a project; **Change project…**
lets you move it elsewhere or choose **Unfiled** to remove its project.
The transcript stays with the conversation. Choose **Conversations** in a project to search its conversations, open **Archived**,
or **Show older conversations**. A chat’s actions menu offers rename, move,
archive, restore, and delete. Choose **Browse conversations** above a conversation to
return to the list. Standalone conversations use a separate list marked **No project**.

Each conversation you open gets a tab above its messages. Choose a tab to
switch conversations, drag tabs to reorder them, or close a tab to put the
conversation away. Closing a tab deletes nothing; reopen the conversation from
the conversation menu or the conversation list. A reply keeps running when you
switch to another tab. Each project remembers its open tabs.

The conversation’s **Outline** searches the messages loaded so far. In a long
chat, choose **Search older messages** to include the preceding page.

**Artifacts** starts collapsed. Use the folder button in the conversation
header to browse files, or select a file link to preview it beside the chat.
Drag the divider to resize the pane. Use **Show file list** to browse from a
file preview.

### Work with files outside a project

A conversation with no project folder has its own private working folder. The
assistant can create and change scripts, data, and other files there, and they
stay available in later messages. This works under **Read only** too, because
the folder holds only what the conversation made. Commands can't reach your
other folders.

API and **Local / custom** models need general commands for this, which run on
macOS, or Linux with `bubblewrap` and `socat`. **Local / custom** models also
need **Allow edits** or an enabled tool. See
[API conversations](connections.md#api-conversations).

**Artifacts** lists the files in the conversation's working folder. Deleting
the conversation deletes its folder. **Move to project…** leaves the files in
that folder, and the conversation works in the project instead. A conversation
that uses an agent profile or recipe made for its current project can't move
until you switch it to a built-in or shared one, or disable the recipe.

In a conversation with a project folder, **Artifacts** lists local files linked
by the assistant. Choose **Find files in earlier messages** to look farther back.
Use **Source** to read Markdown with syntax highlighting, or **Preview** to read
the rendered document. Scripts open in the source viewer. Choose **Edit**
to change a supported text file up to 2 MB, then choose **Save**. When the
conversation uses a review copy, saves go to that copy. For files in the
project folder, **More file actions → Open in Files** opens the full editor.

For a Python script or Stata do-file, choose **Run** and review the working
folder before starting. Save any edits first. The script runs with your local
file access, and its output appears in the pane. Choose **Stop** to cancel.
Runs time out after 10 minutes. Recent output stays available while Pipeline
is open.

Choose **More file actions → Open a copy with…** to pick an editor or runner on
your computer. The app receives a temporary copy with the original filename.
Use **Save As** in that app to keep changes.

### Choose a model

Below the message box, the first selector shows the provider and model. Open
it to choose **ChatGPT**, **Claude Code**, **Claude Code (API)**, **OpenAI API**, **Anthropic API**, **Gemini API**,
or **Local / custom**, then a model. **Automatic** uses the connection’s
default. API and local connections also accept a name under **Model ID**.
The thinking selector beside it shows the levels available for the selected
model.

The conversation remembers your choice. Switching providers keeps recent
history available to the next provider. For ChatGPT, the model selector also
shows the signed-in account and remaining usage. See [Models](connections.md)
for sign-in, keys, local servers, and command setup.

For **ChatGPT** and either **Claude Code** connection, the third selector sets
tool approvals. **Ask** lets ChatGPT request approval when it needs additional access
and asks you before Claude Code runs a shell command. **Sandbox only** runs
commands the sandbox allows without prompting. **Auto** uses the provider's
automatic approval review. The conversation saves your choice for future messages.

When a command offers **Always allow**, that choice also approves matching
requests in the same conversation. Use **Clear saved approvals** in the tool
approvals selector to remove those choices.

To keep the provider, model, thinking, and tool approvals in one menu, turn on
**Settings → Conversations → Combine model settings in one menu**.

### Add sources

A document reaches the assistant only when you add it as a source or an
enabled tool reads it. Select an item and choose **Add to conversation**.
Adding a source sends nothing until your next message.

Sources appear as chips above the message. Choose a chip to set its role:
**Main paper**, **Source**, **Data dictionary**, **Earlier draft**,
**Referee report**, **Result**, or **Supporting material**. A conversation
holds up to 12 sources, with one main paper. To see everything the assistant
receives, choose **Context** in the conversation menu.

### Set assistant instructions

Open **Assistant settings**, the gear below the message. Choose an agent
profile, which is a saved set of instructions and tools. Set
**Access mode** to **Read only** or **Allow edits**.

**Manage agent profiles…** opens the profile editor:

- Built-in profiles, such as Writing and Economics research, are read-only.
  Choose **Duplicate profile** to make your own copy.
- On **System prompt**, keep the provider's base prompt or choose
  **Replace base prompt**. **Additional instructions** are added either way.
- On **Tools & context**, choose which Pipeline tools the profile can use.
- On **Access & inheritance**, set file changes, web search, and
  command internet access.

Editing a custom profile changes every conversation that uses it, starting
with the next message. These settings apply to every connection.
[Conversation access](connections.md#conversation-access) explains file access,
web search and commands.

Replies stream as the assistant works. Choose **Stop** to interrupt the turn
and its tools. API billing after an interruption is explained under
[API conversations](connections.md#api-conversations).

Pipeline runs one response at a time. If you send a message while another
conversation or an automation is running, it isn't sent: it stays in the message
box, and a notice names what's running with a button to open it. A response
that needs your approval shows the same notice wherever you are in the app.

### Queue your next message

While the assistant works, choose **Activity & follow-ups** in the
conversation menu. Under **Follow-up messages**, write your next message and
choose **Queue next message**. It's sent only when you choose **Run next**.
You can queue up to 20 messages, reorder them, or cancel them. If your
sources or settings change, **Review current context** shows the difference.
If Pipeline can't tell whether a message was sent, it's marked `attention`.
**Check message status** looks again, and **Dismiss** ends it without resending.

### Start fresh with selected sources

Under **New conversation from selected context**, choose
**Preview selected context**, tick the sources to keep, and choose
**Start a new conversation with these sources**. The new conversation starts
without the chat history or edit permission.

## Overview

The Overview keeps your research artifacts and next actions close at hand.
Publication stage, kind, and venue live in **Options**. Stage never hides
Responses or determines which blockers you see.

| Section | What it shows |
| --- | --- |
| **Resume** | Recent file, document, or manuscript work, with a conversation fallback and access to recent conversations |
| **Working set** | The main paper and pinned or recent documents; **Edit** opens attached source, **Read** opens a saved version, and **Compare** pins that version for comparison |
| **Attention** | Blockers, proposed decisions, and open tasks; **All items** searches the full loaded list |
| **Recent changes** | Inspectable files, new versions, commits, reviews, and automation results; routine build auxiliaries are omitted |
| Repository footer | Local changes and ahead/behind counts as of the last remote fetch |

Use an attention item's menu to **Pin for today** or choose **Later · 24 hours**.
A deferred item returns after 24 hours or when its source revision changes.
Blockers cannot be deferred. Ordinary uncommitted work stays in repository
status. If action items span multiple pages, the full attention panel says so
and links to **All action items**.

**Context** combines the brief and notes. **Brief** shows accepted, pinned notes
together; **All notes** and **Suggested** keep other records accessible without
losing unsaved edits. **Pin in project brief** chooses its contents.
**Hide from the assistant** keeps a note out of conversations. A transcript's
**Remember this decision** action lets you edit a proposed note before saving
it, with a reference to the original message.

**Builds & reviews → Sources and freshness** explains scope, last load time,
partial results, and unavailable sources, with independent refresh buttons.
Build results belong to the document recorded in their receipt. Review coverage
uses matching content identity, not import dates. Local Git checks never fetch
from a remote; use the repository panel to check the remote.

**Inspect changes → Mark these changes seen** advances the activity baseline.
Returning to a project or leaving it open does not acknowledge changes.

Files and Write share unsaved source text for the same file and working copy.
Saved snapshots and isolated edit copies stay separate. In Write, choose
**Review findings beside source** to keep feedback visible; select source text
and choose **Ask about selected source** to carry that passage to a conversation.

### Project settings

Choose **Options** at the top of the Overview to set up the project. Its
**Project**, **Documents**, **Folder**, **Exclusions**, and **Git** sections
keep common settings within the panel. **More tools** opens command settings,
sharing, version control, and review tools. **Full project settings** opens the
complete settings page, including earlier research records.

**Command settings** and **Share project** are covered under
[Command settings](analysis-and-writing.md#command-settings) and
[Project package](#project-package).

- **Add document** and **Import LaTeX folder**, also available under **Add** on the dashboard, add papers, decks, and other documents.
- **Main paper** selects an optional reference version for the project. Each document has its own current version and history in **Documents**.
- **Publication details (optional)** sets the **Kind**, **Stage**, and **Venue**. These are optional publication metadata.
- **Folder** has **Attach folder**, **Refresh file list**,
  **Check for changes every 30 seconds**, and
  **Folders and files to leave out**. **Refresh file list** reads every file.
  The 30-second check, and the check when you return to Pipeline, compare each
  file's size and modified time.
- **Target date** shows a deadline under the project name.
- **Earlier research records** appears only in a project that has some. See
  [Earlier research records](#earlier-research-records).

An older project with no folder and no paper has a shorter Overview: the brief,
conversations, anything that needs you, and **Add** to import a document or attach
a folder. The dashboard and common Options sections fit typical desktop windows;
narrow windows, enlarged text, and long notes can scroll so content stays readable.

### Earlier research records

Earlier versions of Pipeline had separate views for theory notes, experiments,
literature notes, claims and evidence, decisions, symbols, datasets, tables
and figures, and other structured records. Those views are gone, but their
records stay in Pipeline, in backups, and in project packages.

**Earlier research records** in **Project settings** lists what the project
holds. Choose **Export as Markdown…** and pick a folder, such as the project
folder. Pipeline writes a new **Earlier research records** folder there, with
a `README.md` and one Markdown file per kind of record. Exporting again writes
a second folder and never replaces files.

Abandoned theory approaches, with the reason you gave, become accepted
decision notes pinned to the brief, so the assistant still knows what didn't
work. This happens by itself when Pipeline starts. Decisions were already
saved as notes.

### Repository status

When the folder is in a Git repository, **Repository** shows the branch,
uncommitted files, merge conflicts, and recent commits. It also shows how far
you are ahead of or behind the remote. Pipeline reads this from your local
copy.

**Check GitHub** runs `git fetch` with your own Git credentials, only when you
choose it. For other hosts, the button names the host. It updates what
Pipeline knows about the remote and leaves your files and branches as they
are. It ignores the folder's own Git settings, as described under
[Version control](analysis-and-writing.md#version-control), where you can also
commit, push, and pull.

## Files

**Files** shows the attached folder as a list on the left. Click a folder to
open it and a file to read it. Type in **Find a file**, or press Command/Ctrl+P
from anywhere, to find a file by name. Press Enter to open the first match, or
choose **Search inside files** to search its text. Use tabs or **Split pane** to
see two files, and **Hide file list** for more room. A file link can point to a
line, heading, or PDF page. Conversation links open in **Artifacts**.

The **+** button offers **New file**, **New folder**, and **Add files**, which
act on the folder you selected. You can also drop files from your desktop onto
the list. Right-click an item, or use the **…** button on its row, to **Ask about
this**, **Add to Documents**, **Rename**, **Move**, or **Move to Trash**. A
rename or move offers **Undo**, and open tabs and unsaved drafts follow the file.
Items go to your computer's Trash and are never deleted permanently. On Windows,
a network or removable drive has no Recycle Bin, so Pipeline says so and leaves
the item in place. **Add to Documents** copies a file into [Saved versions](analysis-and-writing.md#documents-and-versions), where you can keep notes on it. File
changes wait while a response is running.

Files opens these types:

| Type | What you see |
| --- | --- |
| PDF | The PDF reader |
| PNG, JPEG, GIF, WebP, BMP, AVIF, SVG | The image with zoom, rotation, and its size. Rotation changes only the view |
| TIFF | The first page, reduced if it is very large |
| Excel (.xlsx, .xlsm) | A table, 200 rows at a time, with a sheet menu |
| CSV and TSV | A table of the first 100 rows, exactly as written in the file. **Load more** adds the next 100. Large files open at once, and the row total appears once you reach the end |
| Jupyter notebooks | Cells and saved outputs. Nothing runs |
| HTML | The rendered page, with scripts and online content off |
| Word (.docx) | The text, without the page layout |
| Audio and video | A player, if your computer can decode the format |

Other files show their size, with **Open externally** and **Reveal in folder**.
This includes Stata (.dta) files, which are usually too large to read here.
A changed spreadsheet keeps its worksheet and page if both still exist. If the
worksheet was removed, Pipeline opens the first one. **Reload table**
retries a table that could not be read. Choose **Retry playback** if an audio
or video file fails to play.

Every open file has these in its **…** menu, along with **Reload**. Pipeline
will not open a program or script with **Open externally** or **Open external
editor**. On a Mac, that includes Python files. On an exFAT drive or a network
share, which can't mark a file as a program, Pipeline reads the start of the
file instead, and also won't open a file with no extension. A file in an edit
conversation's copy opens externally as a read-only snapshot, because the
conversation can still change the copy. Edit it in Pipeline instead, or use
**Reveal in folder**.

The editor handles LaTeX, BibTeX, Markdown, Stata, Python, R, Julia, MATLAB,
and other common languages. Markdown has **Source** and **Preview** views, and
notebooks and HTML have their own view beside **Source**. LaTeX completion
suggests labels and citation keys.

**Save** and **Discard draft** appear once you change a file. You can also press
Command/Ctrl+S to save. **Discard draft** asks first. Unsaved drafts survive
closing a tab. Pipeline watches open files while **Files** is showing. A file
with no unsaved changes updates by itself. If you have a draft, Pipeline tells
you, and you can choose **Compare with current file** or **Reload**. Select text
and choose **Ask about selection** to ask the assistant.

You can read PDF, image, audio, and video files of any practical size, and edit
text files up to 2 MB. Larger text files open read-only, with **Load more**.
Files that aren't UTF-8 also open read-only. Files added with **Add files** can
be up to 32 MB.

### PDFs

The PDF reader has search, an outline, thumbnails, zoom, and rotation. It reads
pages as you reach them, so large PDFs open quickly. In imported papers, you can
save quotations and page regions, tied to the version you read. In **Files**,
select text in a PDF and choose **Ask about selection**. When a manuscript build has SyncTeX data, double-click its PDF to
jump to the matching source line.

When you compare two PDFs, they can scroll together or separately.
**Find changed pages** compares pages with the same number, so an inserted
page shifts every comparison after it. The reader opens PDFs up to 5,000
pages.

## Notes

**Notes** holds questions, assumptions, decisions, next steps, notation, and
handoffs. A note you add with **Add reviewed note** is accepted at once. A
note the assistant suggests waits for **Accept** or **Reject**. Accepted notes
go to future conversations unless you hide them. Each note shows whether it's
proposed, accepted, or rejected, and who created it. Keep anything you would
once have recorded as a theory note, a decision, or notation here.

### Action items

**Action items** is the project's to-do list. Type a task and choose **Add
action item**. **Start an edit** opens
[Review edits](analysis-and-writing.md#review-edits) for that item.

## Back up and share

Pipeline has two export formats. Each shows a preview before you save.
Exports are copies, so export again to pass on later changes.

| | Research backup (`.pwrx`) | Project package (`.pwex`) |
| --- | --- | --- |
| Use it to | Restore all your projects and conversations | Send selected research to a coauthor |
| Find it in | **Settings → Data & Storage** | **Project settings → Share project** |
| Includes | Every project and conversation, notes, documents, imported data, and agent profiles | The records you select and everything they refer to |
| Leaves out | Project folders, reviews, automations, Trash, and sign-ins | Conversations, sign-ins, command settings, agent profiles, and raw data |
| Largest size | 2 GB | 2 GB |

Commands from any import need your approval before they run. Keep separate
backups of your project folders and datasets.

### Research backup

Open **Settings → Data & Storage**, then **Manage research data**, and choose
**Export research backup**.

A backup includes conversation working files and private project edit copies,
including changes that have not yet been captured. For conversations without a
project folder, it leaves out links, special files, unreadable files, and files
too large for a backup, and reports those omissions. An unreadable, changing,
or oversized private edit copy stops the backup so its work is not lost silently.
Private Git edit worktrees restore as ordinary edit copies; keep a separate
backup of the source repository and its history.

If a saved file is missing from your research data folder, Pipeline lists what
it affects and asks whether to back up without it. Everything else is saved.
After a restore, those items stay in your projects and show as unavailable, and
Pipeline lists them when the restore finishes.

A backup restores only into an empty research data folder:

1. Choose an empty folder under
   [Research data folder](settings.md#research-data-folder) and restart.
2. Open **Manage research data** and choose **Restore research backup**.
3. Under **Choose project folders**, point each project to its folder.
4. Sign in again, and approve and test your commands.

### Project package

To export, open **Project settings → Share project** and tick what to
include, such as **Research notes**, **Sources**, **All papers**, and
**All completed executions**. Pipeline adds anything those items refer to.
**Paper files** sends each paper's own file, so your coauthor can open its
pages and start a Review. Files over 64 MB stop the export; turn off
**Paper files** to send extracted text only. Choose **Preview package**, then
**Export package…**. Raw data files stay out of a package.

To import, choose **Inspect package…**, then
**Into a new separate project (default)** or **Merge into this project**.
For a new project, choose **Choose project folder…** to select its folder on
this computer.
**Preview import** shows what's new, identical, or in conflict. Imported
command runs show their results but never run again.

Decisions stay with the person who made them. Notes and claims the sender
accepted and evidence the sender confirmed arrive as proposals marked
**Imported**. They stay out of future conversations and don't count as support
until you accept or confirm them yourself. Accepted edits arrive as history
and change nothing in your files.

When an incoming item differs from yours, Pipeline keeps your version and
lists it under **Import conflicts awaiting review**. **Compare** shows both,
plus the shared earlier version when there is one. Choose **Keep local** or
**Take imported**. Your old text stays in the item's history.

**Create a review design from saved research steps**, on the same page, turns
completed action items into a draft review. Choose **Draft review**, then
**Save draft…**, and open the file in the review designer.

### Research workflow tools

**Project options → Documents** separates the primary manuscript from its
historical reference version. New captures update the primary document without
advancing the reference. Builds, reviews, working edits, and submissions retain
their own version identities.

In **Write**, select source text and choose **Ask about selected source** to add
that exact selection to an unsent conversation. Source chips can be inspected,
reclassified, or removed. Unsaved selections are labeled as drafts. In
**Responses**, create a linked task, inspect proposed edits and check receipts,
and draft a reply from an accepted change. Accepting an edit does not resolve
the original objection automatically.

In **Files**, **Run…** prepares an execution profile for a supported script.
Review the program, individual arguments, inputs, and expected outputs before
launching. **Runs** connects files to recorded executions using path and hash.
Execution details can compare current inputs with the recorded input versions;
these checks cover declared dependencies only.

**Project options → Git** connects an existing source folder through one active
transport. Version control can clone into an empty attached folder, add a remote
without replacing another, select its destination branch, fetch, compare, pull,
and publish. Changed destinations must be reconnected before exchange. A
destination applies to the branch you selected it on. Other branches publish
under their own names, and Pipeline won't publish a branch to a remote branch
with another name unless you select that branch as its destination.

- **Overleaf:** use an eligible Overleaf plan and native Git credentials with
  username `git`. Select the remote's existing `main` or `master` branch.
  Comments and tracked changes do not round-trip completely through Git.
- **Dropbox:** use a folder managed by the Dropbox desktop application. Keep
  files available offline. Pipeline observes local bytes; it cannot establish
  cloud upload completion. New task copies live in private app storage.
  Legacy `.pipeline-tasks` folders must be cleared or moved before connecting.
- **GitHub:** the collaboration panel uses an installed, signed-in `gh` CLI.
  Check pull requests and CI for the selected commit, import conversation
  comments or issues as local tasks, and prepare a draft pull request. Creating
  the draft is a separate publication action and requires the exact branch
  commit to exist on GitHub. CI artifacts remain on GitHub until opened there.

**Write → Citations** indexes selected BibTeX or CSL JSON files, searches by
author, title, or key, inserts a citation at the editor selection, and checks
literal TeX citation commands for missing keys. Bibliography files remain
the source of your references. The optional local Zotero preview imports selected collection
metadata with database and item versions; it does not acquire attachments or
edit the bibliography automatically.

**Project options → Milestones & checks** provides named submission snapshots
and optional local citation checks. Choose saved files, document versions, and
evidence, then edit the handoff note. Review the retained file manifest before
exporting an ordinary ZIP containing documents, evidence, a README, and hashes.
Later edits do not change a snapshot. Source-tree document references require
selecting their source files separately; unsaved buffers are excluded.

Citation checks are off by default. Enable them for a selected TeX file and
bibliographies to check after saving those files in Pipeline. Changes settle
for 500 ms and matching inputs reuse their receipt. Failures appear in
Attention; routine success stays in history. Opening a project launches
nothing. Builds, analyses, and model reviews keep their existing manual launch
and cancellation controls.

### Trash

To free space, open **Settings → Data & Storage**, then
**Disk use**, and choose **Show disk use**. Select categories, choose
**Preview deletion**, then **Move to Trash**. Reviews have their own
cleanup, described in [Settings](settings.md#storage-and-cleanup). Cleanup offers only files
nothing refers to, such as finished run files. **Restore** brings items back
until you choose **Empty Trash**, which deletes them permanently. A folder with
too many items to count shows its size as "at least" that amount.
