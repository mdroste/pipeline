# Pipeline CLI

`pipeline-cli` runs reviews and other saved workflows without the desktop
interface. It uses the app's settings, providers, workflow definitions, and
run history.

## Find or build the command

Desktop installers place `pipeline-cli` beside the application executable and
do not add it to `PATH`. On macOS it is inside the app bundle:

```bash
"/Applications/Pipeline.app/Contents/MacOS/pipeline-cli" --help
```

From a source checkout, build it with:

```bash
cd gui/src-tauri
cargo build --locked --bin pipeline-cli
./target/debug/pipeline-cli --help
```

The examples below use `pipeline-cli`. Replace that name with the full path to
your binary if it is not on `PATH`. During development you can also use
`cargo run --locked --bin pipeline-cli -- <COMMAND> [OPTIONS]` from
`gui/src-tauri/`.

## Run a review

Set up a provider in the desktop app's **Settings → Connections** first. Check
that the input, provider, and parser are ready:

```bash
pipeline-cli check \
  --profile auto-paper-review \
  --input /path/to/paper.pdf
```

Then run the review and save the report:

```bash
pipeline-cli run \
  --profile auto-paper-review \
  --input /path/to/paper.pdf \
  --out /path/to/report.md
```

Without `--out`, the report goes to standard output. Progress and errors go
to standard error. Pipeline also saves the full run in the research data
folder's `runs/` directory, which defaults to `~/.pipeline/runs/`.

An existing output file is protected unless you pass `--force`. Without
`--profile`, the command uses the desktop app's active workflow. Selecting a
profile for one command does not change that desktop selection.

## Available commands

| Command | Purpose |
| --- | --- |
| `profiles` | List saved workflows |
| `profiles show <ID>` | Print a workflow as exportable JSON |
| `workflow` | Print the template or schema, validate a file, or install a workflow |
| `check` | Check a proposed run without extracting the document or calling a model |
| `run` | Run one workflow |
| `batch` | Run a workflow on documents in a directory |
| `engines` | Inspect, install, repair, or remove the optional PDF parser |

Use `pipeline-cli help <COMMAND>` for the full option list. Commands that
support `--json` can return structured output for scripts.

## Choose inputs

Single-document runs accept PDF, DOCX, TeX, UTF-8 Markdown, and text. For a
folder, specify how to interpret it:

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
selected workflow must support the input. Omit `--input` for workflows that
need no primary document.

A custom workflow can declare variables and additional named inputs. Pass each
one separately, using the keys declared by that workflow:

```bash
pipeline-cli run \
  --workflow /path/to/custom-review.json \
  --input /path/to/paper.pdf \
  --var journal=AER \
  --var audience=editor \
  --extra-input appendix=/path/to/appendix.pdf
```

This example requires a workflow declaring `journal`, `audience`, and
`appendix`. Each key must be unique within its option type.

## Check dependencies before running

`check` resolves the workflow, input files, variables, providers, and parser
requirements. It makes no model call and does not extract the document.
Add `--json` to inspect the planned stages, selected reviewers, workflow
fingerprint, and upper limits on step executions and provider attempts.
`check` also prints a rough range of model calls and, for direct API
connections, API list-price cost; `--json` includes it as `usage_estimate`
with the Review limits that will apply.

For PDF inputs:

- model extraction needs the selected provider and `pdftotext`;
- Poppler extraction needs `pdftotext`; and
- PaddleOCR-VL extraction needs the managed Full Parser.

The same checks apply to additional PDF inputs. TeX and DOCX files do not need
PDF extraction tools. Missing required dependencies stop the run before
extraction or model calls.

## Run a batch

`batch` reads PDF, TeX, and DOCX files directly inside one directory. It skips
hidden files and does not search subdirectories. Files run one at a time:

```bash
pipeline-cli batch \
  --profile auto-paper-review \
  --input-dir /path/to/papers \
  --out-dir /path/to/reports
```

Each result is saved in normal run history. `--out-dir` also writes a Markdown
report for each input. Pipeline checks for output-name collisions before
starting; existing files require `--force` to replace.

The batch keeps the workflow and settings selected at its start. Changes made
in the app during execution apply to a later batch.

## Limit cost, calls, or time

`run` and `batch` accept `--max-cost <USD>`, `--max-calls <N>`, and
`--max-minutes <N>`. They override **Settings → Reviews → Review limits** for
that command only; in a batch they apply to each document's Review. When a
limit is reached, steps already running finish, later steps are recorded as
stopped at the limit, and the partial report is saved; the command then exits
with a failure status. Cost counts direct API calls at API list prices;
subscription calls count toward the call and time limits.

```bash
pipeline-cli run --input paper.pdf --max-cost 2 --max-minutes 30
``` A failed item does not stop
the remaining files, but the command exits with a failure status if any item
fails.

## Create and share workflows

The CLI and desktop app use the same versioned JSON format. A workflow
contains its steps, prompts, dependencies, input declarations, variables,
tools, and output settings. It contains no account credentials.

List or export an existing workflow:

```bash
pipeline-cli profiles
pipeline-cli profiles --json
pipeline-cli profiles show auto-paper-review
```

Start a new one from the complete template and consult the schema for fields
and allowed values:

```bash
pipeline-cli workflow template
pipeline-cli workflow schema
pipeline-cli workflow validate /path/to/workflow.json --json
```

Validation rejects unknown or mistyped fields, newer schema versions, circular
or invalid dependencies, unsupported providers or tools, invalid file access,
unbounded repeated steps, invalid output schemas, and definitions without an
enabled Sequential report step. Supported older exports are converted to the
current schema, version 8.

A step with `"kind": "analyzer"` runs a Pipeline check instead of a model call.
It names the check and its version, for example
`"analyzer": {"id": "reference_audit", "version": 1, "options": {"network": true}}`,
and must not set a prompt, providers, model, effort, output schema, or fan-out.
The available checks are `reference_audit` (options `network`,
`check_retractions`, `max_references`) and `replication_static` (option
`max_files`); see [Pipeline checks](../reviews.md#pipeline-checks). Pipeline
versions without analyzer steps reject such a workflow as having unknown
fields rather than running it as a model step.

Use a file directly without installing it:

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

`--workflow` works with `check`, `run`, and `batch` and cannot be combined with
`--profile`. Use `--workflow -` to read up to 10 MB of UTF-8 JSON from standard
input. Running from a file does not save a profile or change the desktop's
active workflow.

To add the definition to your saved workflows:

```bash
pipeline-cli workflow install /path/to/workflow.json --json
```

Installation assigns a new profile ID without overwriting an existing profile.
Every run keeps the normalized definition in `context/workflow.json` and its
SHA-256 fingerprint in `manifest.json`, so you can inspect the version used
even after editing the original.

The repository includes a `pipeline-workflow` skill for Codex/ChatGPT and
Claude. It uses the CLI's template, schema, validation, and run preview to
create workflows from a request. See
[the skill instructions](../../.agents/skills/pipeline-workflow/SKILL.md).

## Install the optional PDF parser

Check the parser's status or install and repair it:

```bash
pipeline-cli engines status
pipeline-cli engines status --json
pipeline-cli engines install paddle
```

The Full Parser download is roughly 2.9 GB and uses roughly 3.8 GB when
installed. The CLI uses the same verified downloads as Settings. Progress
goes to standard error; interrupting the command cancels installation and its
child processes.

To remove it:

```bash
pipeline-cli engines uninstall paddle --yes
```

This removes Pipeline's managed Paddle files under `~/.pipeline/native/`.
It keeps saved runs and system Python installations.

## Exit codes and concurrent use

| Code | Meaning |
| --- | --- |
| `0` | Completed successfully, or the proposed run is ready |
| `1` | Dependency, execution, report-writing, or parser-management failure; also returned if a batch item fails |
| `2` | Invalid command syntax or input validation failure before execution |
| `130` | A running review or installation was interrupted |

The CLI shares settings, profiles, parsers, and history with the desktop app.
It takes a cross-process run lock to prevent conflicting work against the same
store. If another operation owns the lock, finish or stop that work before
trying again.
