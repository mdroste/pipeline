# Projects and conversations

A project keeps a paper, its folder, your notes, and the conversations about
it together. This guide covers projects, conversations, and the **Overview**,
**Library**, and **Activity** areas. [Analyze and write](analysis-and-writing.md)
covers **Analyze** and **Write**, and [Automations](automations.md) covers
**Automate**.

## Create a project

Open **Projects** and choose **+ New project**. Enter a name, and optionally
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

Inside a project, the sidebar lists its six areas and its conversations.
**Show all** opens the full list, and **Show older conversations** reads further
back in a long one. Each area's main views are tabs at the top of the page, and the rest are under
**More**. Choose the project's name in the sidebar to move to another project or
open **Project settings**. Press Command/Ctrl+K, or choose
**Search** in the sidebar, to find any project, conversation, review,
automation, or view by name. The layout menu switches between **Project only**, **Conversation only**, and **Project + conversation**. Project areas
work without a model connection. Conversations and reviews need one. See
[Connections](connections.md).

## Work in a conversation

Start a conversation from the Overview, with **New conversation** above the conversation, or
from **New → New conversation** in the sidebar. Outside a project, **New →
New conversation** starts one with no project. **Conversations** in the sidebar
lists every conversation, and filters to **Unfiled** and **Archived**.

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
conversation works in the project instead.

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
  **Folders and files to leave out**. **Refresh file list** reads every file.
  The 30-second check, and the check when you return to Pipeline, compare each
  file's size and modified time.
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
are. It ignores the folder's own Git settings, as described under
[Version control](#version-control), where you can also commit, push, and pull.

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
the item in place. **Add to Documents** copies a file into the reader, where you can keep notes on it. File
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
editor**. On a Mac, that includes Python files.

The editor handles LaTeX, BibTeX, Markdown, Stata, Python, R, Julia, MATLAB,
and other common languages. Markdown has **Source** and **Preview** views, and
notebooks and HTML have their own view beside **Source**. LaTeX completion
suggests labels and citation keys.

**Save** and **Discard draft** appear once you change a file. You can also press
Command/Ctrl+S to save. Unsaved drafts survive closing a tab. Pipeline watches
open files while **Files** is showing. A file with no unsaved changes updates by
itself. If you have a draft, Pipeline tells you, and you can choose **Compare
with current file** or **Reload**. Select text and choose **Ask about selection**
to ask the assistant.

You can read PDF, image, audio, and video files of any practical size, and edit
text files up to 2 MB. Larger text files open read-only, with **Load more**.
Files that aren't UTF-8 also open read-only. Files added with **Add files** can
be up to 32 MB.

### Version control

**Version control** tracks this project's folder with Git. Pipeline commits,
pushes, and pulls only when you choose it here. Assistants, reviews, and
automations don't use this page.

Choices here that change your files wait while a response is running, as they do
in **Files**. They also wait for a local job, which the message names. A commit
goes ahead if it leaves out the job's files. Git uses saved files, so Pipeline asks first if a file it would
touch has an unsaved draft. Your commit message and a running push stay in place
when you leave the page and come back.

A folder's own Git settings can name programs, such as filters or an SSH
command, and can decide where a connection goes. Pipeline ignores them when it
reads a repository or looks at a remote, so opening or checking a folder you
were sent can't start them. That covers **Check GitHub** on the project's
overview, **Check for updates**, and the first step of **Pull**. **Commit**,
**Push**, and the merge that finishes **Pull** do use them, as Git does in a
terminal, so look at a folder you don't trust before you choose one.

If a repository needs a connection setting of its own, such as an SSH command
for a second account, those checks can fail. So can a remote that's a group of
remotes or that uses a Git helper program. The error names the setting. If you
set it up yourself, run `git fetch` from a terminal in the folder.

A submodule shows as changed when its commit changes, not when it has
uncommitted work inside.

**To start**, choose **Start tracking this folder**. Pipeline creates a
repository and a starter `.gitignore`. Your files stay on your computer until
you connect a remote and push.

**To save your work**:

1. Open **Changes** and select a file to see what changed.
2. Clear the checkbox beside anything you want to leave out.
3. Write a summary, then choose **Commit**.

The first time, Pipeline asks for the name and email to record on your commits.
They're saved for this repository only. **Undo last commit**, in the **…** menu,
puts a commit's changes back in your list if you haven't pushed it.

**Discard changes**, in a file's **…** menu, returns the file to its last
commit. Your current copy goes to the Trash.

**Add to .gitignore** keeps a new file out of Git. Use it for raw data and other
large files. GitHub refuses files over 100 MB, so Pipeline lists any new file
that size above your changes.

**History** lists the commits that touch this folder. Select one to see its
message and changed files. Commits marked **Not pushed** exist only on this
computer.

**Branch**, at the top, shows the current branch. Open it to switch branches or
choose **New branch…**. If a project folder is part of a larger repository, the
page shows only that folder, but branches apply to the whole repository.

#### Connect GitHub or Bitbucket

1. Choose **Connect remote**, then **Create on GitHub** or **Create on
   Bitbucket**.
2. Create an empty repository on the host's site, with no README or license.
3. Paste its HTTPS or SSH address and choose **Connect**.
4. Choose **Publish branch**.

Other Git hosts work the same way, including Overleaf. If **Publish branch** says
the remote already has commits, choose **Change remote…** in the **…** menu to
connect an empty repository, or combine the two from a terminal.

Pipeline signs in with your computer's Git setup and never sees your password or
token. If a push or pull is refused, set up Git once:

| Host | What to set up |
| --- | --- |
| GitHub | Run `gh auth login` from [GitHub CLI](https://cli.github.com), install [Git Credential Manager](https://github.com/git-ecosystem/git-credential-manager), or add an SSH key |
| Bitbucket | Create an [API token](https://support.atlassian.com/bitbucket-cloud/docs/using-api-tokens/) with repository access, or add an SSH key. App passwords don't work |

#### Work with coauthors

The main button shows the next step: **Push**, **Pull**, **Publish branch**, or
**Check for updates**. **Stop** ends a push or pull that's taking too long.

On a branch other than `main` or `master`, **Open pull request** opens the
pull request page on GitHub or Bitbucket.

When a pull finds the same lines changed on both sides, Pipeline stops and
marks those files as conflicts. For each one, choose **Keep this branch's
version** or **Keep incoming version**. Pipeline asks first, and your current
copy of the file goes to the Trash. To combine them, edit the file, remove
Git's conflict markers, and choose **I fixed this file**. Then commit to finish.
**Abort** puts everything back as it was before the pull.

Finish a rebase from a terminal. **Version control** shows that one is
unfinished and can abort it.

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
4. Sign in again, approve and test your commands, and turn scheduled checks
   back on.

### Project package

To export, open **Write → Share project** and tick what to include, such as
**Research notes**, **Sources**, **All papers**, and
**All completed executions**. Pipeline adds anything those items refer to.
**Paper files** sends each paper's own file, so your coauthor can open its
pages and start a Review. Files over 64 MB stop the export; turn off
**Paper files** to send extracted text only. Choose **Preview package**, then
**Export package…**. Raw data stays out unless you turn on
**Allow raw data in sharing packages** in **Data & samples**.

To import, choose **Inspect package…**, then
**Into a new separate project (default)** or **Merge into this project**.
**Preview import** shows what's new, identical, or in conflict. Imported
command runs show their results but never run again.

Decisions stay with the person who made them. Notes and claims the sender
accepted and evidence the sender confirmed arrive as proposals marked
**Imported**. They stay out of future conversations and don't count as support
until you accept or confirm them yourself.

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
**Disk use**, and choose **Show disk use**. Select categories, choose
**Preview deletion**, then **Move to Trash**. Reviews have their own
cleanup, described in [Settings](settings.md#storage-and-cleanup). Cleanup offers only files
nothing refers to, such as finished run files. **Restore** brings items back
until you choose **Empty Trash**, which deletes them permanently. A folder with
too many items to count shows its size as "at least" that amount.
