# Command line

`pipeline-cli` runs reviews and other saved workflows without the desktop
app. It uses the same settings, connections, workflows, and run history.

## Find or build the command

The installer puts `pipeline-cli` beside the app. It doesn't add it to your
`PATH`. On macOS it's inside the app bundle:

```bash
"/Applications/Pipeline.app/Contents/MacOS/pipeline-cli" --help
```

To build it from a source checkout:

```bash
cd gui/src-tauri
cargo build --locked --bin pipeline-cli
./target/debug/pipeline-cli --help
```

The examples below call it `pipeline-cli`. Use the full path if it isn't on
your `PATH`. During development you can also run
`cargo run --locked --bin pipeline-cli -- <COMMAND> [OPTIONS]` from
`gui/src-tauri/`.

## Run a review

First set up a connection in the desktop app under **Settings → Connections**.
Then check that the input, connection, and PDF parser are ready:

```bash
pipeline-cli check \
  --profile auto-paper-review \
  --input /path/to/paper.pdf
```

Run the review and save the report:

```bash
pipeline-cli run \
  --profile auto-paper-review \
  --input /path/to/paper.pdf \
  --out /path/to/report.md
```

Without `--out`, the report goes to standard output. Progress and errors go to
standard error. The full run is also saved in your research data folder's
`runs/` folder, `~/.pipeline/runs/` by default.

`--force` lets the command replace an existing output file. Without
`--profile`, the command uses the workflow selected in the desktop app.
Choosing a profile for one command leaves the app's selection as it is.

## Commands

| Command | What it does |
| --- | --- |
| `profiles` | List saved workflows |
| `profiles show <ID>` | Print a workflow as JSON |
| `workflow` | Print the template or schema, validate a file, or install a workflow |
| `check` | Check a planned run without extracting the document or calling a model |
| `run` | Run one workflow |
| `batch` | Run a workflow on every document in a folder |
| `engines` | Check, install, repair, or remove the optional PDF parser |

`pipeline-cli help <COMMAND>` lists every option. Commands that accept
`--json` print structured output for scripts.

## Choose inputs

A single document can be a PDF, DOCX, TeX, Markdown, or plain-text file. For a
folder, say how to read it:

```bash
pipeline-cli run \
  --input /path/to/latex-project \
  --interpret-as latex-project

pipeline-cli run \
  --workflow /path/to/source-review.json \
  --input /path/to/repository \
  --interpret-as source-tree
```

`--interpret-as` accepts `document`, `latex-project`, or `source-tree`. The
workflow must support that input. Leave out `--input` for workflows that don't
need a document.

A custom workflow can declare variables and extra named inputs. Pass each one
with the key the workflow declares:

```bash
pipeline-cli run \
  --workflow /path/to/custom-review.json \
  --input /path/to/paper.pdf \
  --var journal=AER \
  --var audience=editor \
  --extra-input appendix=/path/to/appendix.pdf
```

## Check before you run

`check` confirms the workflow, input files, variables, connections, and parser
are ready. It doesn't call a model or extract the document. It also prints a
rough range of model calls and, for API connections, the cost at list price.

Add `--json` to see the planned stages, the workflow's fingerprint, the most steps and provider calls the run can use, and the usage
estimate with the review limits that apply.

PDF inputs need:

- `pdftotext` and a working connection for model extraction;
- `pdftotext` for Poppler extraction; and
- the Full Parser for PaddleOCR-VL extraction.

TeX and DOCX files don't need PDF tools. If something required is missing, the
run stops before extraction or any model call.

## Run a batch

`batch` reads the PDF, TeX, and DOCX files directly inside one folder. It skips
hidden files and subfolders, and runs the files one at a time:

```bash
pipeline-cli batch \
  --profile auto-paper-review \
  --input-dir /path/to/papers \
  --out-dir /path/to/reports
```

Each result is saved in your run history. `--out-dir` also writes a Markdown
report for each file. Pipeline checks for clashing output names before it
starts, and `--force` replaces existing files.

A failed file doesn't stop the rest of the batch. The command exits with a
failure status if any file fails. The batch keeps the workflow and settings it
started with, even if you change them in the app.

## Limit cost, calls, or time

`run` and `batch` accept `--max-cost <USD>`, `--max-calls <N>`, and
`--max-minutes <N>`:

```bash
pipeline-cli run --input paper.pdf --max-cost 2 --max-minutes 30
```

These replace **Settings → Reviews → Review limits** for that command. In a
batch, they apply to each document. When a limit is reached:

- steps already running finish;
- the remaining steps are marked as stopped at the limit;
- the partial report is saved; and
- the command exits with a failure status.

The cost limit counts API calls at list prices. Subscription calls count
toward the call and time limits.

## Create and share workflows

The CLI and the app use the same JSON format. A workflow holds its steps,
prompts, dependencies, inputs, variables, tools, and output settings. It never
holds account credentials.

List or export a saved workflow:

```bash
pipeline-cli profiles
pipeline-cli profiles --json
pipeline-cli profiles show auto-paper-review
```

Start a new one from the template, and look up fields in the schema:

```bash
pipeline-cli workflow template
pipeline-cli workflow schema
pipeline-cli workflow validate /path/to/workflow.json --json
```

Validation reports unknown fields, broken dependencies, unsupported
connections or tools, and file access the workflow isn't allowed. A workflow
also needs at least one enabled step set to Sequential, which writes the
report. Older
exports are converted to the current format, version 11.

### Give a step the document

Each step chooses which forms of the main document it can read:

| Setting | What the step can read |
| --- | --- |
| `primary.text` | The extracted text |
| `primary.structure` | An index of equations, tables, figures, and captions |
| `primary.visuals` | Page images and figures |
| `primary.source` | The original file or folder you selected |

### Add a Pipeline check

A step with `"kind": "analyzer"` runs a [Pipeline check](reviews.md#pipeline-checks)
instead of a model call. It names the check and its version:

```json
"analyzer": {"id": "reference_audit", "version": 1, "options": {"network": true}}
```

Such a step can't set a prompt, provider, model, effort, output schema, or
fan-out (repeating the step over a list of files). The checks are `reference_audit`, with the options `network`,
`check_retractions`, and `max_references`, and `replication_static`, with the
option `max_files`. Pipeline 1.1.1 and earlier reject a workflow with an
analyzer step.

### Run or install a workflow file

Use a file without installing it:

```bash
pipeline-cli check \
  --workflow /path/to/workflow.json \
  --input /path/to/paper.pdf \
  --json

pipeline-cli run \
  --workflow /path/to/workflow.json \
  --input /path/to/paper.pdf \
  --out /path/to/review.md
```

`--workflow` works with `check`, `run`, and `batch`, but not together with
`--profile`. `--workflow -` reads the JSON from standard input, up to 10 MB.
Running from a file doesn't save it or change the app's selected workflow.

To add it to your saved workflows:

```bash
pipeline-cli workflow install /path/to/workflow.json --json
```

Installing gives the workflow a new ID, so it never replaces an existing one.
Every run saves the workflow it used in `context/workflow.json`, with its
SHA-256 fingerprint in `manifest.json`. You can see what ran even after you
edit the original.

The repository includes a `pipeline-workflow` skill for Codex and Claude that
creates workflows from a plain request, using the template, schema,
validation, and run check. See
[the skill instructions](skills/pipeline-workflow/SKILL.md).

## Install the optional PDF parser

```bash
pipeline-cli engines status
pipeline-cli engines status --json
pipeline-cli engines install paddle
```

The Full Parser download is about 2.9 GB and takes about 3.8 GB once
installed. The CLI uses the same verified downloads as Settings. Progress goes
to standard error. Press Ctrl+C to cancel the install.

To remove it:

```bash
pipeline-cli engines uninstall paddle --yes
```

This deletes the parser from `~/.pipeline/native/`. Your saved runs and any
Python on your system stay as they are.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Finished, or the planned run is ready |
| `1` | A missing dependency, a failed run, a failed report write, a parser problem, or a failed file in a batch |
| `2` | Invalid command or input, found before anything ran |
| `130` | You interrupted a review or install |

## Running alongside the app

The CLI and the app share settings, workflows, parsers, and run history, so
only one of them runs reviews at a time. If the app is running a review, batch,
or automation, the CLI says the run lock is taken. Wait for that work to
finish or stop it, then try again.
