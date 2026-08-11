# Pipeline

[![CI](https://github.com/mdroste/pipeline/actions/workflows/build.yml/badge.svg)](https://github.com/mdroste/pipeline/actions/workflows/build.yml)
[![Latest release](https://img.shields.io/github/v/release/mdroste/pipeline?label=release)](https://github.com/mdroste/pipeline/releases/latest)
[![Release date](https://img.shields.io/github/release-date/mdroste/pipeline)](https://github.com/mdroste/pipeline/releases/latest)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-blue)](https://github.com/mdroste/pipeline/releases/latest)
[![License: MIT](https://img.shields.io/github/license/mdroste/pipeline)](LICENSE)

Pipeline is a desktop app for structured LLM reviews of academic papers and
other research documents. A workflow specifies the questions to ask, which
parts of the source each step may read, which providers to use, and which
outputs feed later steps. Pipeline handles extraction, scheduling, validation,
and storage; the models handle the review itself.

Pipeline runs on macOS, Windows, and Linux. It can use the Claude Code, Codex,
or Gemini CLI with an existing subscription; the Anthropic, OpenAI, or Google
API; or a local OpenAI-compatible server such as Ollama. Pipeline does not run
a hosted service.

[Privacy and data flow](PRIVACY.md) · [Security policy](SECURITY.md) ·
[Supported platforms](SUPPORT.md) · [Contributing](CONTRIBUTING.md) ·
[Changelog](CHANGELOG.md) · [Release process](RELEASING.md)

## Install

Download the current build for your platform from
[Releases](https://github.com/mdroste/pipeline/releases). The desktop installers
include the application and Poppler PDF tools; they do not require Python,
pip, or another language runtime.

### Model providers

The default provider path uses a signed-in command-line client. This lets a
Pipeline run use a supported subscription without a separate API key.

- **Claude:** Install [Claude Code](https://docs.anthropic.com/en/docs/claude-code)
  with `npm install -g @anthropic-ai/claude-code`, then sign in.
- **OpenAI:** Install the [Codex CLI](https://github.com/openai/codex) with
  `npm install -g @openai/codex`, then sign in.
- **Google:** Install the [Gemini CLI](https://github.com/google/gemini-cli)
  with `npm install -g @google/gemini-cli@latest`, then sign in.

You can instead enter Anthropic, OpenAI, or Google API keys in Settings. API
keys are encrypted before they are written to disk. Settings also accepts a
local OpenAI-compatible endpoint; Ollama is the default, and LM Studio,
llama.cpp, and vLLM can be used by changing the URL.

### Documents and PDF extraction

Document workflows accept PDF, LaTeX, and DOCX inputs. A folder can be treated
as one LaTeX project, a source tree that agents inspect on demand, or a batch of
separate documents. Legacy `.doc` files are not supported.

Pipeline offers three PDF extraction paths:

- **Poppler:** Bundled `pdftotext` extraction and page rendering. This works
  without an additional install.
- **LLM extraction:** The selected remote provider reads the PDF. Availability
  and file-size limits depend on the provider and transport.
- **PaddleOCR-VL 1.6 Full Parser:** An optional local parser installed from
  Settings. Pipeline provisions its private Python environment, models,
  llama.cpp server, and layout tools under `~/.pipeline/`; it does not use the
  user's Python, pip, Conda, or Docker installation.

The Full Parser preserves reading order, headings, formulas, tables, and figure
regions. It is available on Apple Silicon macOS and on supported Windows and
Linux architectures; Intel macOS is not supported. The download and installed
runtime are several gigabytes.

The extraction method selected by the workflow is authoritative. If it fails
or produces incomplete output, Pipeline stops rather than switching methods
silently. LaTeX is read directly, with a companion PDF used for page images
when available. DOCX extraction reads OOXML tables, equations, and embedded
images.

Marker is not installed or executed by current builds because the compatible
Python dependency set contains known security vulnerabilities. Existing
Marker runs remain readable, and Settings can remove an old Pipeline-managed
Marker environment after an upgrade.

Bundled Poppler is licensed under the GPL. Its source hashes, package notices,
and software bills of materials are recorded in
[`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md) and included with each
release.

## How a run works

1. **Prepare the input.** Pipeline extracts the selected document or inventories
   the selected source tree. Document inputs become a versioned
   `DocumentBundle` with text, structure, equations, tables, figures, page
   images, provenance, and extraction warnings. A workflow may also ask a model
   to build a structured survey of the input before review steps begin.
2. **Run the workflow.** Pipeline schedules the workflow's dependency graph.
   Ready Parallel steps run concurrently. Sequential steps run alone and may
   read only the upstream reports and artifacts selected in their access lists.
   A step can use one provider or ask several providers independently and merge
   their reports.
3. **Save the evidence.** Each run is stored under `~/.pipeline/runs/` with its
   report, structured metadata, extracted document context, page and figure
   assets, intermediate responses, supporting files, provenance, usage, and
   logs. The Sources view exposes these records; each model step still sees
   only the material selected in its artifact allowlist.

The finished-run workspace shows the report, provenance, structured issues,
and source artifacts. Markdown reports support equations, tables, search, a
table of contents, and export controls.

## Included workflows

Pipeline installs four workflows:

- **Paper Review (Auto):** Uses the paper orientation call to detect the
  subject, subfield, and central methods, validates that routing against a
  host-owned catalog, and assembles only the selected reviewers for that run.
  The saved workflow stays at four steps; a run adds one or two subject
  specialists and one to four method specialists, for six to ten steps total.
  The catalog covers 28 disciplines, 191 subject roles, and 32 method roles.
  Pure theory and mathematics papers therefore do not spend a pass on
  empirical identification unless they actually contain that component.

- **Paper Review (Full):** Five independent passes on contribution, technical
  correctness, empirical strategy, internal consistency, and exposition,
  followed by issue consolidation. A separate feedback-validation step is
  available in the workflow and may be enabled or disabled.
- **Paper Review (Quick):** Contribution and consistency passes followed by
  issue consolidation.
- **Grant Proposal Review:** Separate passes on aims, feasibility, panel
  readability, and internal consistency followed by consolidated feedback.

The Workflow Gallery contains four additional starting points: Revision
Response Check, Literature Positioning Scan, Thesis Chapter Review, and
Rubric-Based Review. Installing one creates a normal local workflow that can
be edited, exported, or deleted.

## Configure a workflow

The workflow editor controls each step's prompt, phase, providers, model and
effort settings, optional web search, dependencies, and artifact access. The
artifact list is an allowlist: a step can read only the selected document
text, structure, images, source files, survey, named inputs, or upstream
outputs. Selecting an upstream output creates a data dependency. **Wait for**
adds order without exposing that step's output. Parallel steps cannot read one
another's output.

Workflows also support:

- conditional steps based on a survey value or an earlier output;
- JSON output schemas, including the structured issue format used by the
  Issues table;
- run-time variables and extra named inputs;
- fan-out over files matched by a glob;
- per-step supporting files, with downstream access restricted by producer
  and optional glob;
- shared input context for compatible providers and CLI transports.

Shared input context can reduce repeated prompt work across steps, but it does
not remove those logical tokens from usage reports. Pipeline reports fresh,
cache-read, cache-write, and output tokens separately when the provider makes
them available. Cost figures are estimates at published API list prices; for
CLI subscription runs they are comparisons, not subscription charges.

Custom workflows can be copied, edited, exported, imported from a file or URL,
and shared. Settings and workflows are stored under `~/.pipeline/`. Before a
run starts, the preview lists its steps, providers, artifact access, tools, and
maximum declared work units. The estimate does not include retries or schema
repair.

## Batches, history, and projects

The New Run screen accepts several documents or a folder declared as a batch.
Batch items run one at a time. The same folder picker can instead declare one
LaTeX project or a source tree; Pipeline does not infer the choice from the
path.

History can reopen, rename, tag, delete, resume, and rerun saved work. Comparing
two runs gives a per-step text diff. An optional model call can then classify
which concerns were addressed, which remain, and which are new. Structured
issues can be accepted, rejected, or annotated without changing the saved
report.

Projects group related runs without moving or copying their artifacts. A
project can add all runs that share a selected run's stable input path. Deleting
a project leaves the runs intact.

For reports that contain structured issue JSON, a project can build an issue
ledger across runs. The ledger retains workflow, input, step, date, run
annotation, and available page or file evidence. Matching is conservative:
ambiguous items stay separate and can be merged manually, and an issue is not
marked addressed merely because it is absent from a later report. Status and
notes persist when the ledger is refreshed. The ledger can be filtered and
exported as Markdown with provenance.

## Headless CLI

The source tree includes `pipeline-cli` for local automation. It uses the same
settings, workflows, dependency checks, execution engine, and run store as the
desktop app, but it is not included in the desktop installers.

```bash
cd gui/src-tauri
cargo run --locked --bin pipeline-cli -- profiles
cargo run --locked --bin pipeline-cli -- check \
  --profile quick-review \
  --input /path/to/paper.pdf
cargo run --locked --bin pipeline-cli -- run \
  --profile quick-review \
  --input /path/to/paper.pdf \
  --out /path/to/report.md
```

The CLI can list and inspect workflows, check an exact run without contacting a
model, run one input or a batch, write Markdown reports, and manage the local
PaddleOCR-VL bundle. See the [Pipeline CLI guide](docs/cli/README.md) for input
modes, variables, named inputs, batch behavior, and exit codes.

## Build from source

Use Node.js 24.18.0 from `gui/.nvmrc` and Rust 1.97.1 from
`rust-toolchain.toml`:

```bash
git clone https://github.com/mdroste/pipeline.git
cd pipeline/gui
npm ci
npm run tauri build
```

Build artifacts are written to `gui/src-tauri/target/release/bundle/`.

## Limits and data handling

- Each model call must fit the provider's context window. A long paper,
  selected artifacts, tool results, and the response all count toward that
  limit.
- Concurrent calls may queue on some subscription plans, so elapsed time need
  not fall in proportion to the number of parallel steps.
- Web search is provider-dependent. Anthropic and Google direct APIs support
  hosted search for steps that request it; OpenAI Chat Completions and local
  endpoints do not. Search results are not a substitute for checking the cited
  paper.
- PDF extraction remains imperfect. Use LaTeX source when available, and check
  the saved extraction and page images when equations or tables matter.

Pipeline does not collect application telemetry. Remote runs send each step's
prompt and allowed document material to the selected provider. Every run can
store full source text, images, intermediate responses, reports, and logs under
`~/.pipeline/`. Removing the application does not necessarily remove that
directory. Read [PRIVACY.md](PRIVACY.md) before processing confidential or
restricted material.

## License

Pipeline is released under the MIT License. Bundled Poppler components retain
their GPL license; see [`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md).
