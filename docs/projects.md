# Projects and conversations

A project keeps a paper, its folder, your notes, and the conversations about
it together. This guide covers projects, conversations, and the **Overview**,
**Library**, and **Activity** areas. [Analyze and write](analysis-and-writing.md)
covers **Analyze** and **Write**, and [Automations](automations.md) covers
**Automate**.

## Create a project

Open **Projects** and choose **New project**. Enter a name, and optionally
choose the folder that holds the paper and its code, and the paper itself.
Then choose **Create project**.

A folder lets you browse and edit files, run your own code, and see Git
status. Without one, a project still holds conversations, notes, sources,
decisions, and imported data. You can attach a folder later in
**Project settings**. If setup stops after the project is created, choose
**Try again** to finish the same project.

**Projects** lists your work under **Pinned** and **Recently active**. Search
by name, research brief, or folder. Each row shows the latest activity and
what needs you, such as an interrupted edit or suggested notes. Choose the
name to open the Overview, or **Resume conversation** to go back to the last
conversation. **Pin project** keeps a project at the top on this computer.

Inside a project, choose the project name in the top bar to open navigation.
Press Command/Ctrl+K to go to any view by name. The layout menu switches
between **Project only**, **Chat only**, and **Project + chat**. Project areas
work without a model connection. Conversations and reviews need one. See
[Connections](connections.md).

## Work in a conversation

Start a conversation from the Overview, or with **New chat** above the chat.
For a conversation outside any project, choose **Open assistant** on **Home**.

### Choose a model

Open the **Model and thinking** chip below the message box. Choose
**ChatGPT** or **Claude Code**, then a model and a thinking level.
**Automatic** uses your connection's default model. For ChatGPT, the picker
also shows the signed-in account and remaining usage. For what Claude
conversations can do, see [Claude Code](connections.md#connect-claude-code).

### Add sources

A Library document reaches the assistant only when you add it as a source or
an enabled tool reads it. Select an item and choose **Add to conversation**.
Documents, search results, and datasets have their own versions of this
button. Adding a source sends nothing until your next message.

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

Editing a custom profile changes every conversation that uses it, starting
with the next message.

### Queue your next message

While the assistant works, choose **Activity & follow-ups** in the
conversation menu. Under **Follow-up messages**, write your next message and
choose **Queue next message**. It's sent only when you choose **Run next**.
You can queue up to 20 messages, reorder them, or cancel them. If your
sources or settings change, **Review current context** shows the difference.

### Start fresh with selected sources

Under **New conversation from selected context**, choose
**Preview selected context**, tick the sources to keep, and choose
**Start a new conversation with these sources**. The new conversation starts
without the chat history or edit permission.

## Overview

The Overview shows where the project stands. A section appears only when it
has something to report. Each row offers one action, such as opening a view
or starting a conversation with a drafted request that you send yourself.

| Section | What it shows |
| --- | --- |
| **Research brief** | Pinned notes, or your accepted questions. **Draft a brief from the paper** asks the assistant to propose notes. |
| **Where you left off** | The latest conversation, your last request, any unsent draft, and next steps |
| **State of the draft** | The current paper version, the last build, and open or regressed review findings |
| **Out of sync** | Interrupted edits, changed files, numbers that no longer match their results, and claims with old evidence |
| **Revision round** | Progress through reviewer comments |
| **Repository** | Git status, when the folder is a Git repository |
| **Since you were last here** | Changes, commits, finished reviews, runs, and automations since your last visit. A visit ends after 30 minutes away. |
| **Needs your decision** | Suggested notes and up to three open action items |

**Project notes**, at the bottom, holds every note. **Pin in project brief**
chooses what the brief shows. **Hide from the assistant** keeps a note out of
conversations.

### Project settings

Choose **Project settings** at the top of the Overview to set up the project.

- **Import paper** and **Import LaTeX folder** add the manuscript.
- **Paper** sets the current version, which notes and comparisons refer to.
  A newer import doesn't replace it until you choose. **Reference results**
  sets the run that new results are compared with.
- **Folder** has **Attach folder**, **Refresh file list**,
  **Check for changes every 30 seconds**, and
  **Folders and files to leave out**.
- **Target date** shows a deadline under the project name.

A project with no folder and no paper has a shorter Overview: the brief,
conversations, pending decisions, and buttons to add a paper or folder.

### Repository status

When the folder is in a Git repository, **Repository** shows the branch,
uncommitted files, merge conflicts, and recent commits. It also shows how far
you are ahead of or behind the remote. Pipeline reads this from your local
copy.

**Check GitHub** runs `git fetch` with your own Git credentials, only when you
choose it. For other hosts, the button names the host. It updates what
Pipeline knows about the remote and leaves your files and branches as they
are. Pipeline never commits, merges, or pushes.

## Library

### Documents

**Documents** is the reader for imported files. **Import** takes PDF, Word,
LaTeX, Markdown, text, BibTeX, code, JSON, CSV, and TSV files.
**Add new version** adds a revision. A saved passage always reopens the
version it came from.

Select a passage to **Ask**, **Add note**, **Create action item**, or
**Propose revision**. **Compare with another** opens a second document
beside the first.

### Files

**Files** shows the attached folder. Press Command/Ctrl+P to find a file, and
use tabs or **Split pane** to see two. A file link in a conversation or report
opens the file at that line, heading, or PDF page.

The editor handles LaTeX, BibTeX, Markdown, Stata, Python, R, Julia, MATLAB,
and other common languages. Markdown has **Source** and **Preview** views.
LaTeX completion suggests labels and citation keys.

Press Command/Ctrl+S to save. Unsaved drafts survive closing a tab. If another
program changed the file, choose **Compare with current file** or **Reload**
before saving. Select text and choose **Ask about selection** to ask the
assistant.

You can open files up to 32 MB and edit text files up to 2 MB. Larger text
files open read-only.

### PDFs

The PDF reader has search, an outline, thumbnails, zoom, and rotation. In
imported papers, you can save quotations and page regions, tied to the version
you read. When a manuscript build has SyncTeX data, double-click its PDF to
jump to the matching source line.

When you compare two PDFs, they can scroll together or separately.
**Find changed pages** compares pages with the same number, so an inserted
page shifts every comparison after it. The reader opens PDFs up to 5,000
pages.

### Papers & sources

**Papers & sources** lists your papers with their versions, and the sources
you've collected. Use **Import paper**, **Import source**, or
**Import source tree** for a folder. If a new source looks like one you
already have, Pipeline keeps both and flags the possible duplicate.

### Search & collections

**Search & collections** searches the project on your computer, without a
model. It covers papers, sources, notes, decisions, claims, messages, run
results, and data dictionaries. Results show the exact version and an excerpt.
**Add exact excerpt to conversation** makes one a source. If results look
incomplete, wait for indexing or choose **Rebuild index**.

**Reading collections** each hold up to 200 items, such as the papers for one
chapter.

### Reading inbox

**Reading inbox** finds papers online. Turn it on with
**Allow Pipeline to search for literature and download data for this project**.
The same setting allows data downloads in **Data & samples**.

- **Find with Crossref** looks up a DOI or citation. Preview the matches and
  import the ones you want, with their abstracts when Crossref has them.
- **Download PDF** fetches a PDF from a public HTTPS link. Use the direct
  link to the file. Downloads can be up to 32 MB and 300 pages.
- **Import local PDF** adds a PDF from your computer.

The inbox tracks each item as new, metadata only, ready to read, read,
excluded, or failed. **Exclude source** removes an item from search and
conversations and keeps the file.

**Crossref contact email** is optional. Crossref asks for a contact address
and may limit requests per address. When you set one, Pipeline sends it with
each Crossref lookup. It applies to every project. Research backups include
it, and project packages leave it out.

### Literature notes

- **Local bibliography.** **Import BibTeX file** keeps each entry as written,
  with every field and a copy of the original file.
- **Zotero collection preview.** Turn on local application communication in
  Zotero, then choose **Connect locally and list collections**. Pipeline
  imports citation details and only reads from Zotero.
- **Citation-key navigation.** **Find literal TeX citations** links the keys in
  your manuscript to their entries.
- **Literature note and source-support checklist.** Record what a source says
  about a question, with the exact passage and how you checked it.

## Activity

### Research notes

**Research notes** holds questions, assumptions, decisions, next steps,
notation, and handoffs. A note you add with **Add reviewed note** is accepted
at once. A note the assistant suggests waits for **Accept** or **Reject**.
Accepted notes go to future conversations unless you hide them.

### Claims & evidence

**Claims & evidence** tracks your paper's claims and what supports them. Add
one with **Add proposed claim**. Evidence links connect a claim to a passage,
result, or run, and show who assessed them. A link goes stale when its source
changes. **Confirm evidence** records that you checked it.

### Decisions & impact

**Decisions & impact** records each decision with its rationale and the
alternatives you considered. A later decision can replace an earlier one,
which stays readable.

**Link a result to its input** records that one item depends on another.
**Refresh sources and impact** then lists the work to recheck under
**Change impact**, with the reason for each, such as "input changed".

**Draft a summary of saved work** lists proposed notes, open work, and
outputs at the end of a session. Pipeline builds this list itself, without a
model. Nothing is added to your notes until you accept it.

### How records show their source

Pipeline labels each record by source: a model suggestion, a note or claim
you accepted, or a command result. Notes and claims show whether they're
proposed, accepted, or rejected, and who created them. Check a command's
output before you cite it.

## Back up and share

Pipeline has three export formats. Each shows a preview before you save.
Exports are copies, so export again to pass on later changes.

| | Research backup (`.pwrx`) | Project package (`.pwex`) | Replication capsule (`.pwrc`) |
| --- | --- | --- | --- |
| Use it to | Restore all your projects and conversations | Send selected research to a coauthor | Let someone rerun your analysis and check its numbers |
| Find it in | **Settings → Data & Storage** | **Write → Share project** | **Write → Coauthors & replication** |
| Includes | Every project and conversation, notes, documents, imported data, and agent profiles | The records you select and everything they refer to | Up to 16 saved analysis plans with scripts, inputs, and expected numbers |
| Leaves out | Project folders, reviews, automations, Trash, and sign-ins | Conversations, sign-ins, command settings, agent profiles, and raw data | Paths on your computer, sign-ins, and data files unless you allow them |
| Largest size | 2 GB | 2 GB | 64 MB, with 8 MB per file |

Commands from any import need your approval before they run. Keep separate
backups of your project folders and datasets.

### Research backup

Open **Settings → Data & Storage**, then **Manage research data**, and choose
**Export research backup**.

A backup restores only into an empty research data folder:

1. Choose an empty folder under
   [Research data folder](settings.md#research-data-folder) and restart.
2. Open **Manage research data** and choose **Restore archive**.
3. Under **Choose project folders**, point each project to its folder.
4. Sign in again, approve and test your commands, and turn scheduled checks
   back on.

### Project package

To export, open **Write → Share project** and tick what to include, such as
**Research notes**, **Sources**, **All papers**, and
**All completed executions**. Pipeline adds anything those items refer to.
Choose **Preview package**, then **Export package…**. Raw data stays out
unless you turn on **Allow raw data in sharing packages** in
**Data & samples**.

To import, choose **Inspect package…**, then
**Into a new separate project (default)** or **Merge into this project**.
**Preview import** shows what's new, identical, or in conflict. Imported
command runs show their results but never run again.

When an incoming item differs from yours, Pipeline keeps your version and
lists it under **Import conflicts awaiting review**. **Compare** shows both,
plus the shared earlier version when there is one. Choose **Keep local** or
**Take imported**. Your old text stays in the item's history.

### Replication capsule

Build a capsule under **Replication package** in
**Write → Coauthors & replication**:

1. Choose **Add captured plan in execution order** for each saved analysis
   plan.
2. Set **Expected numeric outputs and tolerances**.
3. Choose **Preview replication materials**. Each missing input needs
   instructions for getting it, such as a data license.
4. Choose **Export this capsule**.

To use a capsule you received, choose **Inspect capsule archive**, then
**Import package without running commands**. For each plan, choose a command
profile on your computer, supply missing inputs, and choose
**Prepare local verification plan**. Approve, test, and run it in
**Saved analysis plans**. Then **Check selected outputs and tolerances**
compares the numbers. A value matches when
|actual − expected| ≤ absolute tolerance + relative tolerance × |expected|.

### Trash

To free space, open **Settings → Data & Storage**, then
**Storage and retention**, and choose **Inspect disk use**. Select categories,
choose **Preview deletion**, then **Move to trash**. Reviews have their own
cleanup, described in [Settings](settings.md#storage-and-cleanup). Cleanup offers only files
nothing refers to, such as finished run files. **Restore** brings items back
until you choose **Empty trash**, which deletes them permanently.
