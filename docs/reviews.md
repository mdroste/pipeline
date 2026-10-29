# Reviews

A Review runs a set of AI assessments and saves the resulting report, source
material, and individual responses. Start from **Home → New review**. You do
not need to create a project first.

## Choose an input

The desktop file picker accepts PDF, DOCX, and `.tex` documents. You can also
select a LaTeX project folder. The review engine accepts UTF-8 Markdown and
plain text through the CLI and automation snapshots; these formats are not yet
listed in the desktop review picker. Custom workflows can read a source folder,
request additional documents, or run without a primary document.

Use LaTeX source when you have it: Pipeline reads the text and equations
directly and follows supported references to included files. A PDF requires
extraction, which can lose information. For slides, export the deck as PDF
and choose **Slide Review**.

## Choose a review

| Review | What it does |
| --- | --- |
| Paper Review | Assesses the argument, presentation, and contribution, adds subject and method specialists, combines their comments, and checks them against the paper |
| Paper Review (Fast) | Uses fewer specialists and omits the separate contribution-and-literature assessment |
| Slide Review | Assesses a PDF deck's argument, figures, readability, and slide order, with comments that cite slide numbers |
| Grant Proposal Review | Assesses aims, feasibility, readability for the panel, and consistency, then combines the comments |

Paper Review first identifies the field, methods, and document type. It then
selects one or two subject specialists and one to four method specialists
from the [reviewer catalog](auto-review-catalog.md). Alongside the three
core assessments, consolidation, and validation, this produces seven to eleven
review steps. The Fast version uses six to eight. These counts exclude the
initial classification call and any retries or additional calls within a step.

The review gallery also offers **Revision Response Check**, **Literature
Positioning Scan**, **Thesis Chapter Review**, and **Rubric-Based Review**.
Read each template's description and required inputs before adding it.

## PDF extraction

Open **Settings → Reviews** to choose the default extraction method. A review
can override that default.

| Method | How it works | What to consider |
| --- | --- | --- |
| Poppler (`pdftotext`) | Uses the bundled PDF tools to extract text locally | No installation needed; equations, tables, and reading order may be incomplete |
| Model extraction | Uses the selected model connection to extract the document | Requires a working provider connection and Poppler; the material the model can inspect depends on the connection |
| PaddleOCR-VL Full Parser | Uses an optional local parser to extract text, layout, formulas, tables, and figure regions | Install it from Settings; it downloads model weights and its own runtime and needs several gigabytes of disk space |

With the default setting, Pipeline uses the Full Parser if it is installed
and model extraction otherwise. An explicitly selected method stays selected.
If extraction fails its checks, Pipeline stops the review and reports the
problem. It does not switch methods automatically.

Model extraction can send document content to a remote provider. Poppler and
the Full Parser process the document locally, although the subsequent review
still sends material to the provider you select.

After a run, inspect **Sources** to check the extracted text, page images,
figures, and warnings. If equations or tables are missing, use the source
files or change the extraction method before rerunning the review.

## Models, parallel reviews, and usage

Choose a provider and model in Settings or in the review designer. Reviews
support ChatGPT and Claude subscriptions, direct OpenAI, Anthropic, and Google
APIs, and OpenAI-compatible model servers. Different steps can use different
providers.

Independent review steps can run at the same time. Each sees the inputs and
prior outputs assigned to it. The final consolidation step receives the
selected reviewer reports. The preview shows these dependencies and access
settings before execution.

You can queue several reviews. **Settings → Reviews → Simultaneous Reviews**
controls how many run at once: the default is two and the maximum is four.
The sidebar shows running and queued reviews. Stopping one review does not
stop the others. Batches and automation-owned reviews run separately from
this queue.

Provider limits may delay calls even when Pipeline has a free slot. Cost
figures use API list-price estimates. For subscription calls, they are a
comparison measure rather than a bill from Pipeline.

Web search depends on the connection. The Anthropic and Google API adapters
support it; OpenAI Chat Completions and local-server connections do not.
Check the selected provider's available tools when configuring a step.

## Read and use the results

The report combines the reviewers' comments. Individual responses and the
source material remain available so you can trace a comment back to its
inputs. When a review produces structured findings, Pipeline also shows them
as individual issues that you can accept, reject, or annotate.

Compare runs to see how the reports changed. A project can track findings
across drafts; a finding remains open until it is resolved explicitly, even
if a later review omits it.

Every run saves its report, extracted material, intermediate responses,
settings, logs, and usage under the research data folder's `runs/` directory.
The default is `~/.pipeline/runs/`. Export a report when you want a separate
copy. Deleting a run moves it to Trash, where it can be restored until you
permanently delete it.

## Customize a review

The designer lets you edit prompts, choose providers and models, set reasoning
effort, enable supported tools, and choose which inputs each step can read.
A saved review definition is called a **workflow**. Workflows can include
conditional steps, structured JSON responses, and repeated steps over selected
files.

Export a workflow as JSON to share it or keep a copy. Import from a file or
URL to add one. The definition contains the steps and settings, but not your
API keys. Inspect an imported workflow's prompts, providers, tools, and file
access before running it.

For batches, workflow validation, and scripts, use the
[command-line guide](cli/README.md).

## When a review cannot finish

| Problem | Next step |
| --- | --- |
| Provider is unavailable | Open **Settings → Connections** and check the selected account or API key |
| PDF extraction fails | Read the extraction error; install or repair the selected parser, choose another method, or use LaTeX source |
| Input exceeds the model's context window | Use a smaller document or reduce the context assigned to that step |
| Calls are queued or rate-limited | Check the provider's account limits and reduce concurrency if needed |
| Run is interrupted | Open its saved run and inspect completed steps and the reported recovery options before starting another run |

For bug reports, include the version, provider, extraction method, and a short
error excerpt. See [Support](../SUPPORT.md) for reporting instructions.
