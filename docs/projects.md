# Projects and conversations

A project keeps related papers, files, conversations, results, and review
comments together. Create one from **Projects** and give it a name. Attach an
existing research folder if you want to work with its files. You can also
start without a folder and add sources and notes first.

## Find your work

The project Overview shows the manuscript, recent activity, and items needing
attention. The other areas organize work by purpose:

- **Library:** papers, sources, notes, and files in the attached folder.
- **Analyze:** results, evidence, experiments, and research checks.
- **Write:** manuscript work, proposed edits, and responses to reviewers.
- **Automate:** repeated work, schedules, and longer research investigations.
- **Activity:** previous conversations, jobs, and results.

The project index lets you search, pin projects, and resume a conversation.
You can also have conversations outside a project.

## Start a conversation

Choose **ChatGPT** or **Claude Code** in the conversation's model picker,
then choose an available model and thinking level. Set up the connection in
**Settings → Connections** first. ChatGPT uses Pipeline's shared sign-in;
Claude uses your Claude Code subscription sign-in.

Give the assistant a concrete task and select the material it needs. The
context controls show the research information included with the conversation.
Assistant instructions and agent profiles let you choose a recurring role,
context, tools, and permissions. The advanced editor calls this configuration
a *harness*.

Project instructions and enabled tools determine what the assistant can use.
A document in your Library is not automatically included in every message;
it reaches the model when selected as context or read through an enabled tool.

Claude conversations use Pipeline's file and research tools. They can read
project files and edit existing source files when **Allow edits** is enabled.
On macOS and Linux, Claude can also run shell commands in a sandbox that can
read the project but cannot change it or reach the network. Claude has no
browser tools. Pipeline supplies recent conversation text on each turn within
a size limit. See [Claude Code](claude-code.md) for details.

## Read and edit files

Open **Library → Files** to browse the attached folder. The editor supports
LaTeX, BibTeX, Markdown, Stata, Python, and other common research languages.
Open a PDF beside its source or compare versions. See
[Files and PDFs](file-workspace.md) for shortcuts, saved drafts, and limits.

Research tasks can use a separate working copy so you can review proposed
changes before applying them to your original files. **Write → Review edits**
shows those changes and lets you accept them. File edits and isolated task
copies are currently unavailable on Windows; reading files is supported.

## Run local checks

Configure local commands in the project's research settings. Choose the kind
of work the command does:

| Kind | Runs | Pipeline also checks or records |
| --- | --- | --- |
| Stata do-file | Your installed Stata in batch mode: `stata-mp -b do main.do` on macOS and Linux, `StataMP-64.exe /e do main.do` on Windows | The log (for example `main.log`) must be an expected output; a log with an error code such as `r(198)` fails the check |
| R script | `Rscript analysis.R` | R version and the hash of `renv.lock` and `.Rprofile` |
| Python script | `python3 analysis.py`, or the project's `.venv` interpreter | Python version, the virtual environment, and the hashes of `uv.lock`, `requirements.txt`, `environment.yml` and similar files |
| Julia script | `julia --project=. analysis.jl` | Julia version and the hashes of `Project.toml` and `Manifest.toml` |
| Jupyter notebook | `jupyter nbconvert --execute` into a new file | The executed copy must be an expected output; the original notebook is never overwritten |
| Quarto or R Markdown | `quarto render report.qmd` | The rendered document must be an expected output |
| LaTeX | latexmk, pdflatex, xelatex, lualatex or tectonic | Shell escape stays off; the PDF must be an expected output |
| Command | Any program and arguments | Expected outputs only |

Scripts and documents must be inside the project folder, given as relative
paths. Arguments go straight to the program, so shell operators such as `;`,
`|` or `$` are refused.

**Detect installed tools** looks in the standard install folders for your
system, on your `PATH`, and in the project folder (for example `.venv`,
`renv.lock` or `Project.toml`). It fills in the program's path when you choose
a tool. Detection installs nothing and never starts Stata or MATLAB; Stata's
version comes from its install folder. A profile test confirms that the tool
and its license work.

If you start Stata through a shell alias, choose **Stata do-file** and enter a
custom launcher such as `/bin/zsh -lic "stata -q -b do main.do"`. Stopping a
run gives a custom launcher up to 30 seconds to exit on its own before it is
forced to stop; an installed Stata stops immediately.

Review the command, input files, output locations, and permissions before
authorizing a check. Changing a lockfile or the program changes what you
authorized, so Pipeline asks for a new review. Pipeline records the command's
result and keeps its outputs with the project.

Model suggestions, your accepted notes, and results from local commands are
recorded separately. A successful command tells you that it ran; inspect the
output before using it as evidence for a claim.

## Bring reviews into the project

Send a saved manuscript version to a Review to assess that version. You can
copy selected findings into the project, organize responses, and compare
results across revisions. Later manuscript edits do not alter the copy that
was reviewed.

Use [Review and revise](tasks.md) to automate several rounds. Use
[research automations](research-missions.md) to investigate a broader question
with a planner, investigator, and separate assessment of the results.

## Back up or share research

Pipeline offers different exports for different purposes:

| Format | Use it for | Scope |
| --- | --- | --- |
| Research backup (`.pwrx`) | Restore the research store into an empty destination | Includes conversations, research records, retained documents, and app-owned conversation files; excludes external project folders, Review and Automation stores, Trash, and credentials |
| Project package (`.pwex`) | Share selected research with a coauthor | Includes the selected records and their referenced material; excludes conversations, raw datasets, sign-ins, and execution permissions |
| Replication capsule (`.pwrc`) | Share a set of computation instructions and selected inputs | Includes declared commands, input information, and expected outputs; the recipient supplies missing data and tools and authorizes execution locally |

Inspect the export preview to see what it includes. Keep separate backups of
external manuscripts and datasets. A research backup does not copy your entire
working directory, and a project package does not synchronize two computers.
Imported results remain available for inspection without running their commands.

See [storage](storage.md), [project packages](workbench/project-exchange.md),
and the [replication capsule reference](workbench/research-programs.md#replication-capsule)
for more detail.
