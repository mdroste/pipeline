# Reviews

A review asks several AI reviewers to assess a document, then combines and
checks their comments. Pipeline saves the report, each reviewer's response,
and the source material. Start from **Home → New review**. You don't need a
project.

## Choose an input

Choose a PDF, a Word `.docx` file, a `.tex` file, or a LaTeX project folder.
Older `.doc` files aren't supported, so save them as `.docx` first. The
[command line](cli.md) also accepts Markdown and plain text.

Use LaTeX source when you have it. Pipeline reads the text and equations
directly and follows `\input` and `\include` to other files. If a compiled PDF
sits beside the source, with the same name as the main file or named
`paper.pdf`, `main.pdf`, or `manuscript.pdf`, Pipeline uses it for page
images.

A `.tex` file that isn't saved as UTF-8 is read as Windows-1252, and the report
notes it so you can check accented characters. A project can bring up to 512
files and 256 MB into a review. Figures past either limit are left out and
listed in the report's quality notes, and the review continues.

A PDF has to be converted to text first, which can lose equations, tables, or
reading order. See [PDF extraction](#pdf-extraction). For slides, export the
deck as PDF and choose **Slide Review**.

## Choose a review

| Review | What it does |
| --- | --- |
| Paper Review | Assesses the argument, presentation, and contribution. Adds subject and method specialists, combines their comments, and checks them against the paper |
| Paper Review (Fast) | Uses fewer specialists and skips the separate contribution-and-literature assessment |
| Slide Review | Assesses a deck's argument, figures, readability, and slide order. Comments cite slide numbers |
| Grant Proposal Review | Assesses aims, feasibility, readability for the panel, and consistency, then combines the comments |
| Declaude | Finds formulaic prose and suggests replacements suited to the document's subject, audience, and style |

The gallery also has **Revision Response Check**, **Literature Positioning
Scan**, **Thesis Chapter Review**, **Rubric-Based Review**, **Reference
Check**, and **Replication Package Readiness**. Each template lists the inputs
it needs.

A review of a folder, such as a replication package, reads a private copy. The
copy leaves out hidden items, symbolic links, and generated folders such as
`.git` and `node_modules`. It holds up to 2,000 files and 256 MB,
with no file over 32 MB, and keeps the smallest files when a folder is larger.
A review of three or more folders gives each a smaller share of the file limit.
The preview says when files won't fit, and the review's log lists everything
left out. The review sees the names and sizes of large files but can't open
them.

### Editing prose with Declaude

Declaude identifies the document type and selects one to three topic reviewers.
It checks stock phrases, unnecessary contrasts, defensive framing, misplaced
jargon, repetition, and formulaic paragraphs. A final reviewer checks that
each replacement preserves the source's claims and qualifications.

The Markdown report lists each original passage, its location, a suggested
replacement, and the reason for the edit. It identifies incomplete coverage
and wording that needs your judgment. Declaude assesses writing style; it
does not determine whether AI wrote the document. Your source stays unchanged.

Use the supported document inputs above. Export slides or website pages to
PDF; the CLI also accepts text and Markdown snapshots.

### How Paper Review picks reviewers

Paper Review first reads the document to identify its field, methods, and
type. It then runs:

- three core assessments: argument, presentation, and contribution;
- one or two subject specialists, such as labor economics or number theory;
- one to four method specialists, such as difference-in-differences or
  formal proofs;
- a step that combines all reports into up to 40 comments; and
- a final step that checks each comment against the paper, fixes small
  errors, and removes comments the paper doesn't support.

That makes 7 to 11 review steps. Paper Review (Fast) uses up to two method
specialists and runs 6 to 8 steps. Slide Review picks reviewers the same way,
with instructions for slide decks.

Every reviewer is told what kind of document it's reading. For example,
reviewers don't expect a published comment or reply to make a new
contribution, or a null result to find an effect. The document types are research articles, replications,
comments and replies, surveys, data descriptors, methods and software papers,
registered reports, null results, case reports, and slide decks.

To see every specialist, open a review in the designer and choose **Browse
catalog** in the **Auto-filled from orientation** slots. The reviewers chosen
for your document appear as separate steps when the run starts.

### Try the example review

**See an example review** on Home opens a finished review of an invented paper
on the same screen as a real report. You can read the report, check the evidence
for each comment, try accepting and rejecting comments, and see what each
reviewer wrote. It doesn't call a model, and nothing is saved.

## PDF extraction

Choose the default method in **Settings → Reviews**. You can change it for a
single review.

| Method | How it works | What to consider |
| --- | --- | --- |
| Poppler (`pdftotext`) | Extracts text on your computer with the bundled PDF tools | Nothing to install. Equations, tables, and reading order may be incomplete |
| Model extraction | Sends the document to the model connection you selected | Needs a working connection. The document leaves your computer |
| PaddleOCR-VL Full Parser | Extracts text, layout, formulas, tables, and figures on your computer | Install it from Settings. It needs about 4 GB of disk space. On Windows it runs on the processor, so it's slower and needs about 6 GB of free memory, or about 10 GB for a few seconds as it starts |

**By default, Pipeline uses the Full Parser if it's installed, and otherwise
sends the PDF to your model connection.** If extraction fails, the review
stops and shows the error. Pipeline never switches to another method on its
own.

Local extraction keeps the PDF on your computer during extraction. The review
itself still sends material to the provider you chose.

After a run, open **Sources** to check the extracted text, page images,
figures, and warnings. If equations or tables are missing, use the LaTeX
source or another extraction method and run the review again.

## Run several reviews

Reviews use your [default model](settings.md#default-model). To use another,
choose one for a stage under **Settings → Reviews**, or for a step in the
review designer. Different steps can use different providers. See
[Connections](connections.md) for the options.

Steps that don't depend on each other run at the same time. To see each
step's inputs before anything starts, open **Steps and what each can read** in
the run preview.

You can queue reviews. **Settings → Reviews → Reviews at once** sets how many
run at once: two by default, four at most. **Activity** at the bottom of the
sidebar lists running and queued reviews. Stopping one review leaves the others
running.

## Estimated use and limits

The run preview shows a rough range of model calls and tokens. For API
connections, it also shows the cost at the provider's list price. The
estimate comes from the document's length and the planned steps. Retries and
the specialists Paper Review picks change the actual use.

**Settings → Reviews → Review limits** sets a cost, call, or time limit for
each review. Enter 0 for no limit. When a review reaches a limit:

- steps already running finish;
- the remaining steps are marked as stopped at the limit; and
- the results so far are saved as a partial report.

The cost limit counts API connections at list prices. Subscription calls count
toward the call and time limits. The preview warns you when the estimate could
pass a limit.

If you reach your account's usage or credit limit,
**Settings → Reviews → Continue after an account usage limit** retries the call
with a backup provider. A review doesn't need the backup to start. If the backup
isn't connected, the run preview says so, and reaching a limit stops the review.

## Search the web during a review

In the **Review designer**, select a step and turn on **Web search**.
Search works with every connection. For API and local models, choose a model
that can use tools. Ask the step to cite the pages it uses.

OpenAI API and local/custom servers use Bing search and public page reads.
Queries can include terms from your paper, even when the model runs locally.
See [review search](connections.md#web-search-in-reviews) for model requirements
and limits.

## Read and use the results

The report combines the reviewers' comments. You can open each reviewer's
response and the source material to see where a comment came from. When a
review produces findings, you can accept, reject, or annotate each one. A
decision applied to several selected findings adds its rationale after any
note already there, and changes only the findings your filters are showing.

Compare runs to see how the reports changed. In a project, a finding stays
open across drafts until you resolve it, even if a later review doesn't
repeat it.

Each run is saved in the `runs/` folder of your
[research data folder](settings.md#research-data-folder), `~/.pipeline/runs/`
by default:

| File | Contents |
| --- | --- |
| `report.md` | The combined review |
| `context/document.md` | The text Pipeline extracted from your document |
| `artifacts/pages/` | Page images, up to 300 pages |
| `manifest.json` | The settings, models, and usage for the run |

Each reviewer's response is saved in the run too. Open it from the report.

Deleting a run moves it to Trash. You can restore it until you empty Trash.

### Share the results with coauthors

The report's **Export** menu creates files that open without Pipeline. Your
saved run and your manuscript stay as they are.

| Format | What it contains | Notes |
| --- | --- | --- |
| **Export as HTML** | One file with the report, each finding's severity, confidence, category, and quoted evidence, plus filters | Opens offline in any browser. Equations appear as LaTeX source |
| **Export as Word** | A new `.docx` with the report, findings, and quoted evidence | Works for any input. Simple formatting |
| **Word with comments on the original** | A copy of your `.docx` with each finding as a Word comment on the passage it quotes | For `.docx` inputs. Quotes in footnotes, text boxes, or headers are listed in one comment at the start |
| **PDF with highlights** | A copy of your PDF with each quoted passage highlighted and a note attached | For PDF inputs. Scanned PDFs get notes only. Highlights are added as a new revision, so the original content and any signatures are left untouched |

Pipeline matches quotes while ignoring differences in spacing, letter case,
quote marks, dashes, and hyphenation. It skips quotes shorter than about a
dozen letters, which could match the wrong passage. If a quote is on a
different page from the one cited, the highlight goes where the text is and
the note says so.

Comments and highlights go on the exact file that was reviewed. If you've
edited or moved it since, export as Word or HTML instead, or run the review
again. Exported files contain quotes from your manuscript and model-written
text.

## How Pipeline checks quotes

After each review, Pipeline looks for every quoted passage in the extracted
document, or in the cited file for a folder input. Each piece of evidence gets
a label:

| Label | Meaning |
| --- | --- |
| Quote found | The text is where the comment says. Exports add "(formatting differs)" when it matched only after ignoring punctuation, case, hyphenation, LaTeX markup, or an ellipsis |
| Partial match | The text is somewhere else in the document, or only a close paraphrase was found |
| Quote not found | The text isn't in the extracted document. Check the source before relying on the comment |
| Not checkable | There's no quote, the evidence cites an outside source, or the cited file wasn't saved with the run |

**Not checked** means the review has no quote-check results.

**Quote found** means the quoted text is in your document where the comment
says. It confirms the quote, not the comment's reasoning. A PDF that lost its
equations during extraction can make a real quote look missing. The details
are saved in the run as `products/evidence_verification.json`, listed in
**Sources** under **Findings and products**.

## Pipeline checks

A workflow can include checks that Pipeline runs itself, without a model. In
the designer, choose **+ Add step**, then **Pipeline check**. Or start from the
**Reference Check** or **Replication Package Readiness** template.

| Check | What it does | Network |
| --- | --- | --- |
| Reference check | Finds the bibliography and reports undefined citation keys, duplicate entries, and malformed DOIs. With lookups on, it checks each reference against Crossref: found, mismatched details, another version (such as a working paper later published), retracted, expression of concern, or corrected | Only when you turn on lookups |
| Replication package check | Reads a package folder and checks the README against the Social Science Data Editors template. Also checks for a main script, absolute paths, package installation, random seeds, missing files, exported tables and figures, very large files, and a license. It doesn't run the code | None |

The templates pair each check with a model step that explains the problems
found. If Crossref can't find a reference, it may be a book, report, or
dataset that Crossref doesn't list, or its details may be wrong. If the
reference check can't find a bibliography, or can't read all of one, it says
so in a low-priority finding and doesn't report citation keys as undefined.

A reference is reported as retracted only when its Crossref record is confirmed
to be the cited work. If the record may be a different paper or another version,
the finding says the retraction could not be confirmed. If Crossref can't be
reached at all, you still get the checks that need no network, and a finding
says which online checks did not run. If only some references could be looked
up, or the list is longer than the limit, a finding says how many were not
checked.

With lookups on, the preview shows what's sent: the text of each reference as
written in your bibliography, and nothing else from the manuscript. Lookups
run about one reference per second, within Crossref's public limits.

## Customize a review

The designer lets you edit prompts, choose providers and models, set reasoning
effort, turn on tools, and choose what each step can read. A saved review can
include conditional steps, structured responses, and steps that repeat over a
set of files.

Export a review as JSON to share it. Import one from a file or URL. The file
is called a **workflow**. It contains steps and settings, never API keys. Before
you run an imported review, check its prompts, providers, tools, and file access.

For batches, workflow files, and scripting, see the
[command-line guide](cli.md).

## When a review can't finish

| Problem | What to do |
| --- | --- |
| The provider is unavailable | Open **Settings → Connections** and check the account or API key |
| PDF extraction fails | Read the error. Repair or install the parser, choose another method, or use LaTeX source |
| The document is too long for the model | Use a shorter document, or give the step less context |
| Calls are queued or rate-limited | Check your provider's limits and run fewer reviews at once |
| The run was interrupted | Open the saved run, check which steps finished, and use the recovery options it offers. **Resume** always runs the review design saved with that run. **Regenerate** does too, unless that design is the one selected now, in which case your edits apply. If the run used a secret variable, Pipeline asks for it again, because a run never keeps secret values |

To report a bug, see [Support](../.github/SUPPORT.md).
