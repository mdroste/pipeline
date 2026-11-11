# Write and review

This guide covers the **Paper**, **Reviews**, and **Changes** areas of a
project, and the commands Pipeline runs for it. Building a manuscript and
editing files need an attached project folder. See
[Projects and conversations](projects.md) for creating projects and the other
areas.

## Paper

### Manuscript

**Manuscript** edits TeX, Markdown, BibTeX, and text files up to 2 MB. It opens
your main TeX file by itself, and the file menu at the top switches files. Under
**Edit in**, choose the **Project working copy** or an edit's copy. This menu
appears when an edit has its own copy. Unsaved drafts are kept for each file. If
a file changed elsewhere, Pipeline tells you when you come back to it, and stops
any save. Choose **Compare with current file** to keep your draft and see it
against the current file. A file with no draft updates by itself.
**Source** and **Changes** appear once you have a draft.

To build the manuscript:

1. Choose **Build**. Pipeline suggests the root TeX file and the files beside
   it. Check the engine: pdflatex, xelatex, lualatex, or latexmk.
2. List every section, bibliography, table, and figure the build uses. Pipeline
   tracks changes only in the files you list. **More build settings** holds the
   expected PDF, working directory, and timeout.
3. Choose **Save build settings**, then **Review test run** and
   **Authorize and start test**. After a passing test, **Build** goes straight
   to **Review run**.

Shell escape is always off. A successful build keeps its PDF, log, and SyncTeX
file. When a build ends, its result opens beside the source: the PDF, or its
errors with file and line if there is no PDF. Double-click the PDF to find the
source line.

The panel beside the source has two views. **PDF** shows the build. **Builds**
lists past builds and the chosen build's messages, with **Find between source and
PDF**, **Compare with another build**, and **Record a page check**. The button
at the end of the top bar hides or shows this panel.

### Saved versions

**Saved versions** keeps copies of the paper and other documents as they were
when you imported them, so a review or a note always points at the text it was
written about.

**Import** takes PDF, Word,
LaTeX, Markdown, text, BibTeX, code, JSON, CSV, and TSV files.
**Add new version** adds a revision. A saved passage always reopens the
version it came from.

Select a passage to **Ask**, **Add note**, **Create action item**, or
**Propose revision**. **Compare with another** opens a second document
beside the first.

## Reviews

### Reviews & findings

**Reviews & findings** shows reviews of this project's documents. Attach a
folder to connect them. **Start a review** opens a new one.

After a review finishes, choose **Track findings** to add it to **Findings
across reviews** and to the Overview's finding counts. Choose it again after
each later review.

**Findings across reviews** follows each finding through later reports as
**Open**, **Regressed**, **Addressed**, or **Dismissed**. A finding stays
open when a later report omits it, until you mark it addressed. You can also
combine duplicates. See [Reviews](reviews.md) for reading a single report.

### Send for review

**Send for review** reviews a saved copy of your paper. Choose the
**Paper to review**, then **Review this revision…**. In the preview, choose
the review and models, then start it. The review uses your review settings
and account, and project notes stay out of it. See [Reviews](reviews.md).

### Responses

**Responses** organizes your replies to referees and editors. The tab appears
once the paper's stage is past **Drafting**, and always when no stage is set.

1. Under **Import referee or editor comments**, choose
   **Import local report**, then **Preview paragraph split**. Check the
   numbering and choose **Import selected comments**. One import holds 500
   comments, and the preview says if a report has more. You can also import
   findings from a finished review with **Enable importing review findings**.
2. In the **Response matrix**, give each comment a category, severity, and
   disposition: addressed, deferred, or rejected. Draft the reply and link the
   manuscript change, run, or evidence behind it. Choose
   **Save response decision**.
3. Under **Response-letter export**, choose Markdown or LaTeX, then
   **Preview selected responses and check links**.

The preview flags replies that claim an added analysis without linked
evidence, empty drafts, and deferrals or rejections with no reason. A flagged
letter exports as a draft.

**Focused re-review** sends only the passages you changed, and their
context, to a new review. Choose them, then
**Preview review of selected passages**.

## Changes

### Review edits

**Review edits** lets the assistant change files in a separate copy. Your
folder changes only when you accept.

Edit conversations use **File changes → Review copy** by default. Pipeline
prepares the copy automatically, keeps the same conversation, and collects
changes when the assistant finishes or stops. Choose **Review edits** in the
conversation to inspect, accept, or discard them. **Undo** remains available
after acceptance. Your next message after accepting or discarding starts a
fresh copy from the project.

For a smaller selection of files, create a copy yourself:

1. Under **Start an edit**, choose the action item the edit is for and tick
   the files the assistant may change.
2. Choose a **Working copy** method. **Plain copy of the files** copies them.
   **Git branch in a separate worktree** puts them on a new `codex/` branch.
   Then start the edit.
3. Choose **Open conversation** to work with the assistant in the copy.
4. When it's done, choose **Check for changes** and read each file's diff.
5. Tick the files to keep and accept them. Within a text file, you can accept
   only selected changes. The diff then shows what is left, and you can accept
   the rest later. **Discard this edit** drops what you haven't accepted and
   keeps the copy for reference.

Wait for the assistant to finish before you check for or accept changes. A
file can be accepted only if your original hasn't changed since the copy was
made or since you last accepted part of it. Afterward, rebuild the manuscript
or rerun your checks.

**Accepted edits** lists what you've accepted. **Undo** reverses an
acceptance while the files still match it. If accepting was interrupted,
**Restore original files** puts the originals back.

Accepting an edit never commits or pushes. With a Git branch, you decide what
to commit.

### Version control

**Version control** tracks this project's folder with Git. Pipeline commits,
pushes, and pulls only when you choose it here. Assistants, reviews, and
automations don't use this page.

Choices here that change your files wait while a response is running, as they do
in [Files](projects.md#files). They also wait for a local job, which the message names. A commit
goes ahead if it leaves out the job's files. Git uses saved files, so Pipeline asks first if a file it would
touch has an unsaved draft in **Files** or **Manuscript**. Your commit message and a running push stay in place
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

A new folder that holds its own Git repository, such as an Overleaf clone, can't
be included in a commit here. Git would save a link to it and none of its files,
so coauthors would get an empty folder. Commit it in its own repository, or add
it to `.gitignore`.

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

Pipeline keeps the inputs of programs it generates in a hidden folder and leaves
it out of Git. If an earlier version committed that folder, its files still
appear here when they change, and you can commit or discard them, but they don't
open in **Files**.

Git lists a new folder as one item. If that folder holds a file you unchecked,
the whole folder stays out of the commit. Select it to add that file to
.gitignore and commit the rest.

**Discard changes**, in a file's **…** menu, returns the file to its last
commit. Your current copy goes to the Trash. A file you deleted simply comes back.

**Add to .gitignore** keeps a new file out of Git. Use it for raw data and other
large files. GitHub refuses files over 100 MB, so Pipeline lists any new file
that size above your changes. It checks up to 5,000 new files at a time and says
so when there are more.

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
connect an empty repository, or combine the two from a terminal. **Change
remote…** changes the address of the remote shown at the top of the page. Any
other remotes stay as they are.

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

## Command settings

**Command settings** sets up the programs Pipeline runs on your computer. Open
**Project settings**, then the **Command settings** tab. A manuscript build
sets up its own command, so you need this page only for other programs.

1. Under **New execution profile**, name the command and choose its kind.
2. Enter the arguments as a JSON list, then the input files and expected
   outputs. Give scripts as paths inside the project folder.
3. Set **Time limit (seconds)**. Pipeline stops a command that runs longer.
   The default is 300 seconds, and the longest allowed is 7,200 seconds.
4. Choose **Save profile**.
5. Choose **Test**. **Review command before running** shows the exact command,
   folder, inputs, outputs, and time limit. Choose
   **Authorize this command and test**.
6. After a passing test, **Run** works the same way.

Choose **Edit** on a saved profile to change it. Saving a change clears its
test result, so test it again before running.

Conversations with any provider can use these commands when you enable the
research execution tools and choose **File changes → Direct edits** in
**Assistant settings → Manage agent profiles… → Access & inheritance**. For general shell commands, see
[Conversation access](connections.md#conversation-access).

| Kind | Runs | Also checks or records |
| --- | --- | --- |
| Stata do-file | Your installed Stata in batch mode: `stata-mp -b do main.do`, or `StataMP-64.exe /e do main.do` on Windows | The log must be an expected output. A log with an error code such as `r(198)` fails. |
| R script | `Rscript analysis.R` | The R version, `renv.lock`, and `.Rprofile` |
| Python script | `python3 analysis.py`, or the project's `.venv` interpreter | The Python version, and files such as `uv.lock`, `requirements.txt`, and `environment.yml` |
| Julia script | `julia --project=. analysis.jl` | The Julia version, `Project.toml`, and `Manifest.toml` |
| Jupyter notebook | `jupyter nbconvert --execute`, into a new file | The executed copy must be an expected output. Your notebook stays as it was. |
| Quarto or R Markdown document | `quarto render report.qmd` | The rendered file must be an expected output |
| LaTeX | latexmk, pdflatex, xelatex, lualatex, or tectonic | Shell escape stays off. The PDF must be an expected output. |
| Command | Any program | Expected outputs only |

Arguments go straight to the program, so shell operators such as `;`, `|`,
and `$` aren't allowed. Your approval covers that exact command. If you change
the program, a script, or a lockfile, Pipeline asks you to approve it again.
Commands run with your computer's file and network permissions.

**Detect installed tools** looks in standard install folders, on your `PATH`,
and in the project folder, such as a `.venv` environment. Choose **Use** next
to a tool to fill in its path.

If you start Stata through a shell alias, choose **Stata do-file** and enter a
launcher such as `/bin/zsh -lic "stata -q -b do main.do"`. When you stop a
run, the launcher gets up to 30 seconds to exit before Pipeline forces it to
stop. An installed Stata stops at once. After a forced stop, check that Stata
has exited and that anything your launcher changes, such as the clock, is
restored.
