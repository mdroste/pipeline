# Projects and conversations

A project keeps a paper, its folder, your notes, and the conversations about
it together. This guide covers projects, conversations, and the **Overview**,
**Files**, and **Notes** areas. [Write and review](analysis-and-writing.md)
covers **Paper**, **Reviews**, and **Changes**.

## Create a project

Open **Projects** and choose **+ New project**. Enter a name, and optionally
choose the kind of paper, the folder that holds the paper and its code, and the
paper itself. Then choose **Create project**.

A folder lets you browse and edit files, run your own code, and see Git
status. Without one, a project still holds conversations, notes, and imported
documents. You can attach a folder later in
**Project settings**. If setup stops after the project is created, choose
**Try again** to finish the same project.

**Projects** brings your projects and chats together. Projects appear under
**Pinned** and **Recently active**. Search by project name, research brief,
folder, or chat title. Each row shows the paper's stage, venue, and deadline
when they're set, the latest activity, and what needs you, such as an
interrupted edit or suggested notes. **Sort by deadline** puts the nearest
deadline first. Choose the
name to open the Overview, or **Resume conversation** to go back to the last
conversation. **Pin project** keeps a project at the top on this computer.

Inside a project, the sidebar lists its six areas and its conversations:

| Area | What it holds |
| --- | --- |
| **Overview** | Where the paper stands and what to do next |
| **Paper** | The manuscript, its builds, and saved versions |
| **Files** | Everything in the project folder |
| **Reviews** | Reviews, their findings, and replies to referees |
| **Changes** | Edits the assistant proposed, and version control |
| **Notes** | Notes the assistant can use, and action items |

Each area's views are tabs at the top of the page. Choose the project's name in
the sidebar to move to another project or open **Project settings**. Press
Command/Ctrl+K, or choose **Search** in the sidebar, to find any project,
conversation, review, automation, or view by name. The layout menu switches
between **Project only**, **Conversation only**, and
**Project + conversation**. Project areas work without a model connection.
Conversations and reviews need one. See [Connections](connections.md).

## Work in a conversation

Start a conversation from the Overview, with **New conversation** above the conversation, or
from **New → New conversation** in the sidebar. Outside a project, **New →
New conversation** starts one with no project. **+ New chat** on the Projects
page also starts a chat without a project. The **Chats** list on that page
includes conversations across projects, with **No project** and **Archived**
filters. Choose a chat to resume it, or its project name to open the project.
**Move to project…** adds a standalone chat to a project; **Change project…**
lets you move it elsewhere or choose **Unfiled** to remove its project.
The transcript stays with the chat. Recent chats and the open conversation
appear inside the **Projects** sidebar section.

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

Select a file the assistant names to open it, or choose **Reveal working files**
from the conversation's actions menu. Deleting the conversation deletes its
folder. **Move to project…** leaves the files in that folder, and the
conversation works in the project instead. A conversation that uses an agent
profile or recipe made for its current project can't move until you switch it
to a built-in or shared one, or disable the recipe.

### Choose a model

Open the **Model and thinking** chip below the message box. Choose
**ChatGPT**, **Claude Code**, **OpenAI API**, **Anthropic API**, **Gemini API**,
or **Local / custom**, then a model. **Automatic** uses the connection’s
default. API and local connections also accept a name under **Model ID**.
**Thinking** shows the levels available for the selected model.

The conversation remembers your choice. Switching providers keeps recent
history available to the next provider. For ChatGPT, the picker also shows
the signed-in account and remaining usage. See [Connections](connections.md)
for sign-in, keys, local servers, and command setup.

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
- On **Access & inheritance**, set file changes, web search, the browser, and
  command internet access.

Editing a custom profile changes every conversation that uses it, starting
with the next message. These settings apply to every connection.
[Conversation access](connections.md#conversation-access) explains file access,
web search, browser interaction, and commands.

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

The Overview shows where the paper stands. A section appears only when it has
something to report. Each row offers one action, such as opening a view or
starting a conversation with a drafted request that you send yourself. A row
appears once: something listed under **Next** isn't repeated further down.

The line under the project name shows the kind of paper, its venue, and the
target date. **Stage** records where the paper is: **Idea**, **Drafting**,
**Ready to submit**, **Under review**, **Revising**, or **Accepted**. Change it
as the paper moves on.

| Section | What it shows |
| --- | --- |
| **Next** | Up to three things to do now, chosen by the stage |
| **Research brief** | Pinned notes, or your accepted questions. **Draft a brief from the paper** asks the assistant to propose notes. |
| **Where you left off** | The latest conversation, your last request, any unsent draft, and next steps |
| **The paper** | The current version, the last build, and open or regressed review findings |
| **Documents in this project** | Each imported document with its role, once there is more than one |
| **Out of sync** | Files that changed since the current version was imported, and a newer import not yet marked current |
| **Revision round** | Progress through reviewer comments |
| **Repository** | Git status, when the folder is a Git repository |
| **Needs you** | An interrupted edit, merge conflicts, proposed edits, suggested notes, and up to three open action items |
| **Since you were last here** | Changed files, new versions, commits, finished reviews, and automations since your last visit. A visit ends after 30 minutes away. |

What **Next** leads with depends on the stage:

| Stage | Leads with |
| --- | --- |
| **Idea** | Nothing. The brief and your conversations come first. |
| **Drafting** | A failed build, an unreviewed draft, open findings |
| **Ready to submit** | An unreviewed version, open findings, a failed build, uncommitted work |
| **Under review** | Uncommitted and unpushed work |
| **Revising** | Reviewer comments still open or without a reply, then open findings |
| **Accepted** | Uncommitted and unpushed work, a failed build |

With no stage set, **Next** uses a general order.

**Project notes**, at the bottom, holds every note. **Pin in project brief**
chooses what the brief shows. **Hide from the assistant** keeps a note out of
conversations.

### Project settings

Choose **Project settings** at the top of the Overview to set up the project.
Its **Command settings** and **Share project** tabs are covered under
[Command settings](analysis-and-writing.md#command-settings) and
[Project package](#project-package).

- **Import paper** and **Import LaTeX folder** add the manuscript.
- **Paper** sets the current version, which notes and comparisons refer to.
  A newer import doesn't replace it until you choose.
- **About this paper** sets the **Kind**, **Stage**, and **Venue**. The stage
  decides what the Overview shows first.
- **Folder** has **Attach folder**, **Refresh file list**,
  **Check for changes every 30 seconds**, and
  **Folders and files to leave out**. **Refresh file list** reads every file.
  The 30-second check, and the check when you return to Pipeline, compare each
  file's size and modified time.
- **Target date** shows a deadline under the project name.
- **Earlier research records** appears only in a project that has some. See
  [Earlier research records](#earlier-research-records).

A project with no folder and no paper has a shorter Overview: the brief,
conversations, anything that needs you, and buttons to add a paper or folder.

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
see two files, and **Hide file list** for more room. A file link in a
conversation or report opens the file at that line, heading, or PDF page.

The **+** button offers **New file**, **New folder**, and **Add files**, which
act on the folder you selected. You can also drop files from your desktop onto
the list. Right-click an item, or use the **…** button on its row, to **Ask about
this**, **Add to Documents**, **Rename**, **Move**, or **Move to Trash**. A
rename or move offers **Undo**, and open tabs and unsaved drafts follow the file.
Items go to your computer's Trash and are never deleted permanently. On Windows,
a network or removable drive has no Recycle Bin, so Pipeline says so and leaves
the item in place. **Add to Documents** copies a file into [Saved versions](analysis-and-writing.md#saved-versions), where you can keep notes on it. File
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
Every open file has these in its **…** menu, along with **Reload**. Pipeline
will not open a program or script with **Open externally** or **Open external
editor**. On a Mac, that includes Python files. On an exFAT drive or a network
share, which can't mark a file as a program, Pipeline reads the start of the
file instead, and also won't open a file with no extension.

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

A backup includes conversation working files. It leaves out links, special
files, files it can't read, and files too large for a backup, and says so when
it does.

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

### Trash

To free space, open **Settings → Data & Storage**, then
**Disk use**, and choose **Show disk use**. Select categories, choose
**Preview deletion**, then **Move to Trash**. Reviews have their own
cleanup, described in [Settings](settings.md#storage-and-cleanup). Cleanup offers only files
nothing refers to, such as finished run files. **Restore** brings items back
until you choose **Empty Trash**, which deletes them permanently. A folder with
too many items to count shows its size as "at least" that amount.
