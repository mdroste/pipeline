# Pipeline CLI

`pipeline-cli` runs Pipeline workflows without opening the desktop interface.
It uses the same local settings, profiles, dependency checks, execution engine,
and run store as the app. The CLI is currently available from source builds;
it is not included in Pipeline's desktop installers.

## Build and help

From the repository root:

```bash
cd gui/src-tauri
cargo build --locked --bin pipeline-cli
./target/debug/pipeline-cli --help
```

During development, any example below can instead be run as:

```bash
cargo run --locked --bin pipeline-cli -- <COMMAND> [OPTIONS]
```

## Commands

| Command | Purpose |
| --- | --- |
| `profiles` | List available workflows, or print the list as JSON. |
| `profiles show <ID>` | Print one workflow's export JSON. |
| `workflow` | Print the schema/template, strictly validate portable JSON, or explicitly install it. |
| `check` | Validate one prospective run and its exact dependencies without running it. |
| `run` | Run one workflow and print or save its Markdown report. |
| `batch` | Run one workflow over supported documents in a directory. |
| `engines` | Inspect, install, repair, or remove the managed Paddle bundle. |

Use `pipeline-cli help <COMMAND>` for the complete option list for a command.

## Profiles

List the installed workflows:

```bash
./target/debug/pipeline-cli profiles
./target/debug/pipeline-cli profiles --json
./target/debug/pipeline-cli profiles show auto-review
```

Without `--profile`, `check`, `run`, and `batch` use the active desktop
workflow. A command-specific selection is transient and does not change that
active workflow:

```bash
./target/debug/pipeline-cli run \
  --profile auto-review \
  --input /path/to/paper.pdf
```

## Portable workflows and agent authoring

A workflow is saved as the same versioned JSON `profile` envelope produced by
the GUI export and `profiles show`. There is no second CLI-only format. The
current schema is v8 and includes steps, dependency and artifact selectors,
merge policy, published outputs, extraction/input declarations, variables,
orientation settings, and context caching. User settings and credentials are
not part of a portable workflow.

Start from a complete normalized example and consult the machine-readable
schema rather than constructing the format from memory:

```bash
./target/debug/pipeline-cli workflow template
./target/debug/pipeline-cli workflow schema
```

Validate generated JSON without changing local state:

```bash
./target/debug/pipeline-cli workflow validate /path/to/workflow.json --json
```

Validation is strict for agent safety. It rejects unknown fields (including
misspellings), malformed typed fields, newer schema versions, invalid or cyclic
dependencies, unsupported providers or tools, invalid artifact access,
unbounded fan-out, invalid output schemas, and workflows without an enabled
Sequential report step. Older supported exports receive their normal defaults
and are normalized to the current schema before hashing.

Use a validated file ephemerally with `check`, `run`, or `batch`:

```bash
./target/debug/pipeline-cli check \
  --workflow /path/to/workflow.json \
  --input /path/to/paper.pdf \
  --json

./target/debug/pipeline-cli run \
  --workflow /path/to/workflow.json \
  --input /path/to/paper.pdf \
  --out /path/to/review.md
```

`--workflow -` reads up to 10 MB of UTF-8 JSON from standard input. It is
mutually exclusive with `--profile`. Ephemeral use never edits the active
desktop workflow or saves a profile. To save a generated workflow explicitly:

```bash
./target/debug/pipeline-cli workflow install /path/to/workflow.json --json
```

Installation creates a new deduplicated profile ID; it does not overwrite an
existing profile. Every run records the workflow source and normalized SHA-256
fingerprint in `manifest.json` and retains the exact normalized definition as
`context/workflow.json`, so a result remains auditable if the source file or an
installed profile later changes.

The repository includes a Codex/ChatGPT skill at
`.agents/skills/pipeline-workflow` and exposes the same package to Claude from
`.claude/skills/pipeline-workflow`. The skill obtains the live template/schema
from the CLI, validates generated JSON, inspects the preflight plan and work
bounds, and uses ephemeral execution unless installation is explicitly asked
for.

## Check a run

`check` resolves the selected workflow, inputs, runtime variables, providers,
and parser dependencies without extracting the document or contacting a
model:

```bash
./target/debug/pipeline-cli check \
  --profile auto-review \
  --input /path/to/paper.pdf

./target/debug/pipeline-cli check \
  --profile auto-review \
  --input /path/to/paper.pdf \
  --json
```

With `--json`, the report also includes the normalized workflow fingerprint,
authoritative scheduler stages, selected Parallel/Merge agents, and upper
bounds on step units, merge calls, and provider attempts.

The concrete primary and named-input paths determine whether PDF parsing can
run. For a PDF:

- LLM extraction requires the selected model provider and `pdftotext`.
- `pdftotext` extraction requires `pdftotext`.
- PaddleOCR-VL extraction requires the managed Full Parser bundle.

TeX and DOCX inputs do not inherit PDF-only requirements. A selected PDF
passed with `--extra-input key=path` is checked in the same way as the primary
input. Missing required dependencies cause `check` and `run` to exit before
extraction or a model call.

## Run one workflow

By default, the Markdown report goes to standard output and progress goes to
standard error:

```bash
./target/debug/pipeline-cli run \
  --profile auto-review \
  --input /path/to/paper.pdf
```

Save the report to a file with `--out`:

```bash
./target/debug/pipeline-cli run \
  --profile auto-review \
  --input /path/to/paper.pdf \
  --out /path/to/report.md
```

Pipeline will not replace an existing output file unless `--force` is passed.
Every execution is also saved as a normal Pipeline run under
`~/.pipeline/runs/`, regardless of whether `--out` is used.

### Input modes

Use `--interpret-as` when a directory's meaning should be explicit:

```bash
./target/debug/pipeline-cli run \
  --input /path/to/latex-project \
  --interpret-as latex-project

./target/debug/pipeline-cli run \
  --input /path/to/repository \
  --interpret-as source-tree
```

Accepted values are `document`, `latex-project`, and `source-tree`. A workflow
configured with no primary input is run by omitting `--input`.

### Variables and named inputs

Runtime variables and named inputs are repeatable:

```bash
./target/debug/pipeline-cli run \
  --profile auto-review \
  --input /path/to/paper.pdf \
  --var journal=AER \
  --var audience=editor \
  --extra-input appendix=/path/to/appendix.pdf
```

Keys must be unique within each option type. The selected workflow still
determines which variables and named inputs are valid or required.

## Batch runs

`batch` scans one directory non-recursively for PDF, TeX, and DOCX files and
runs them sequentially. It captures the workflow and execution settings once
before the first item, so edits made during a batch apply to the next batch.
Hidden files are skipped:

```bash
./target/debug/pipeline-cli batch \
  --profile auto-review \
  --input-dir /path/to/papers \
  --out-dir /path/to/reports
```

Each item is saved in Pipeline's normal run store. `--out-dir` additionally
writes one Markdown report per input. Existing reports are protected unless
`--force` is supplied, and output-name collisions are rejected before model
work starts. A failed item does not prevent later items from running, but the
batch exits non-zero if any item fails.

## PaddleOCR-VL Full Parser

Inspect the managed engine without changing it:

```bash
./target/debug/pipeline-cli engines status
./target/debug/pipeline-cli engines status --json
```

Install the bundle, or repair an existing installation:

```bash
./target/debug/pipeline-cli engines install paddle
```

The download is roughly 2.9 GB and the installed bundle roughly 3.8 GB. The
command uses the same pinned, checksum-verified provisioner as the desktop
Settings page. Progress is written to standard error, and an interrupt cancels
the installation and its child processes.

Remove the managed bundle with explicit confirmation:

```bash
./target/debug/pipeline-cli engines uninstall paddle --yes
```

Managed engine files stay under `~/.pipeline/native/`. Uninstall removes both
Pipeline-owned Paddle roots; it does not remove saved runs or system Python,
package managers, or other system software.

## Output and exit status

- `0`: the command completed successfully, or `check` found the run ready.
- `1`: dependencies, execution, report writing, engine management, or at least
  one batch item failed.
- `2`: command syntax or local pre-dispatch validation was invalid.
- `130`: an active run or installation was interrupted.

Machine-readable workflow validation/planning, profile, dependency, and engine
status output is available with the commands' `--json` options. Progress and
diagnostics remain on standard error so standard output can be redirected
safely.

## Local state and concurrency

Profiles, settings, managed engines, and run history are shared with the
desktop app under `~/.pipeline/`. Pipeline serializes workflow execution with
a cross-process lock, so the CLI and desktop app cannot run conflicting jobs
against the same store at the same time.
