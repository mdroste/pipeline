# Pipeline

[![CI](https://github.com/mdroste/pipeline/actions/workflows/build.yml/badge.svg)](https://github.com/mdroste/pipeline/actions/workflows/build.yml)
[![Latest release](https://img.shields.io/github/v/release/mdroste/pipeline?label=release)](https://github.com/mdroste/pipeline/releases/latest)
[![License: MIT](https://img.shields.io/github/license/mdroste/pipeline)](LICENSE)

Pipeline is a desktop application for doing and reviewing academic research
with language models. It runs on macOS, Windows, and Linux. There is no
Pipeline server and no Pipeline account: the application runs on your computer
and calls the model providers you configure.

It does three things.

- **Reviews** apply a fixed sequence of model calls to a paper, grant proposal,
  or code folder and return a written report. The built-in paper review reads
  the paper, selects reviewers suited to its field and methods, consolidates
  their comments, and checks each comment against the text.
- **Conversations** are ChatGPT sessions attached to a research project. With
  your permission, the assistant can read the project's papers and sources,
  edit and build a LaTeX manuscript, and run configured Stata or Python jobs.
  Pipeline keeps notes, claims, and the evidence offered for them.
- **Automations** chain the two: for example, draft, review, and revise until
  a review raises no high-priority concern or a set limit is reached. They can
  run on a schedule.

A model's report is a list of things to check. It is not a referee's judgment,
and a clean review does not establish that a paper is correct.

[Privacy and data flow](PRIVACY.md) · [Supported platforms](SUPPORT.md) ·
[Security policy](SECURITY.md) · [Changelog](CHANGELOG.md) ·
[Contributing](CONTRIBUTING.md)

## Install

Download the installer for your platform from
[Releases](https://github.com/mdroste/pipeline/releases/latest).

| Platform | File | Requirements |
| --- | --- | --- |
| macOS, Apple Silicon | `Pipeline-<version>-macOS-AppleSilicon.dmg` | macOS 15 or later |
| macOS, Intel | `Pipeline-<version>-macOS-Intel.dmg` | macOS 15 or later |
| Windows, x86-64 | `Pipeline-<version>-Windows.exe` | Windows 11, or Windows 10 22H2 with security updates |
| Linux, x86-64 | `Pipeline-<version>-Linux.AppImage` | glibc 2.35 or later (Ubuntu 22.04) and FUSE 2 |

The macOS builds are signed and notarized. The Windows installer is not
signed, so SmartScreen will warn before running it; choose **More info**, then
**Run anyway**. Each installer is published with a SHA-256 checksum.

The installers contain the application and the Poppler PDF tools. They do not
require Python or any other language runtime. Pipeline tells you when a newer
release exists but never updates itself.

## Connect a model provider

Open **Settings → Connections**.

| Provider | With a subscription | With an API key |
| --- | --- | --- |
| OpenAI | Install the [Codex CLI](https://github.com/openai/codex) (`npm install -g @openai/codex`), then choose **Sign in to ChatGPT** | OpenAI API key |
| Anthropic | Install [Claude Code](https://docs.anthropic.com/en/docs/claude-code) (`npm install -g @anthropic-ai/claude-code`) and sign in | Anthropic API key |
| Google | Not available | [Gemini API key](https://aistudio.google.com/apikey) |
| Local models | — | Any OpenAI-compatible server: Ollama by default, or LM Studio, llama.cpp, or vLLM by changing the URL |

Reviews can use any of these providers, and a single review can mix them.
Conversations and Automations use the ChatGPT sign-in only. One ChatGPT
sign-in serves both.

For OpenAI and Anthropic you choose subscription or API mode explicitly; a
stored key is ignored while subscription mode is selected. API keys are
encrypted before they are written to disk. Google's terms do not allow
third-party software to use its subscription sign-in, so Google models are
always called through the Gemini API.

## Reviews

### Inputs

A review accepts a PDF, a LaTeX project, or a DOCX file. A folder must be
declared as one of three things: a single LaTeX project, a source tree that
reviewers inspect file by file, or a batch of separate documents. Pipeline does
not guess. Legacy `.doc` files are not supported.

LaTeX source is read directly and gives the most reliable text, equations, and
tables. For PDFs there are three extraction methods:

- **Poppler.** Bundled `pdftotext` extraction and page rendering. Needs no
  setup.
- **Model extraction.** The selected remote provider reads the PDF. Page and
  file-size limits depend on the provider.
- **PaddleOCR-VL Full Parser.** An optional local parser, installed from
  Settings, that preserves reading order, headings, formulas, tables, and
  figure regions. It occupies several gigabytes and installs its own Python
  environment and model weights under `~/.pipeline/`. It is not available on
  Intel Macs.

By default Pipeline uses the Full Parser when it is installed and model
extraction otherwise. You can fix the method globally or for one review. If
the chosen method fails or returns incomplete text, the review stops; it does
not fall back to another method.

### Included reviews

- **Automatic Paper Review (Full)**, the default. A first call identifies the
  paper's field, subfield, and principal methods. Pipeline then runs three
  general reviews, one or two field specialists, and one to four method
  specialists, chosen from a fixed catalog. A consolidation step merges their
  comments and a validation step re-checks each comment against the paper. A
  run has seven to eleven steps. A purely theoretical paper is not assigned a
  reviewer for empirical identification.
- **Automatic Paper Review (Quick)**. The same design with at most two field
  and two method specialists and without the contribution-and-literature
  review, for six to eight steps.
- **Grant Proposal Review**. Separate reviews of aims, feasibility,
  readability for a panel, and internal consistency, followed by consolidated
  comments.

The designer's gallery adds four starting points: Revision Response Check,
Literature Positioning Scan, Thesis Chapter Review, and Rubric-Based Review.

### What a run does

1. Pipeline extracts the input into text, structure, equations, tables,
   figures, and page images, and records extraction warnings.
2. It runs the review's steps in dependency order. Steps marked parallel run
   concurrently and cannot read one another's output. Each step sees only the
   material on its access list.
3. It saves the report, every intermediate response, the extracted document,
   token usage, and logs under `~/.pipeline/runs/`.

The finished run shows the report, a table of individual issues, and the
sources behind them. Issues can be accepted, rejected, or annotated. Two runs
on the same paper can be compared step by step, and a project can keep a
ledger of issues across successive drafts. An issue is never marked resolved
merely because a later report omits it.

Before a run starts, a preview lists its steps, providers, and the material
each step may read. Cost figures are estimates at published API list prices;
for subscription runs they are a point of comparison, not a charge.

### Writing your own

The designer sets each step's prompt, provider, model, reasoning effort, web
search, dependencies, and access list. Steps can be conditional, can require
structured JSON output, and can fan out over files matching a pattern. A
review definition is a single JSON file that can be exported, shared, and
imported from a file or URL.

## Projects, Conversations, and Automations

A project gathers a paper's sources, manuscript, conversations, reviews, and
results. Inside a project, work is organized under Overview, Library, Analyze,
Write, Automate, and Activity. The overview reports the state of the paper:
the current version, the last build, outstanding review findings, and numbers
or claims that no longer match their underlying results.

A conversation runs through the Codex runtime under your ChatGPT account. You
choose the model and reasoning effort, and you approve what the assistant may
read, write, and execute. Local computation is limited to the LaTeX, Stata,
and Python commands you configure. Material enters the record in distinct
classes: notes you accepted, source revisions, execution receipts, evidence,
and model proposals. A numerical check is never recorded as a proof.

Research can be exported in two forms. A `.pwrx` backup restores an entire
store onto another machine. A `.pwex` package sends selected items to a
coauthor, without conversations, credentials, or execution settings.

Automations are described in [docs/tasks.md](docs/tasks.md). Longer-running
research agendas, including the self-discovery modes, are documented in
[docs/research-missions.md](docs/research-missions.md) and
[docs/self-discovery.md](docs/self-discovery.md).

## Limits

- Reviews are the most thoroughly tested part of Pipeline. Conversations,
  project research tools, and Automations are newer and have been exercised
  mainly on macOS.
- On Windows, the assistant can read project files but cannot yet apply edits
  to them, and isolated working copies for tasks are unavailable. Reviews are
  unaffected.
- Each model call must fit within the provider's context window. A long paper,
  the selected material, tool results, and the response all count.
- PDF extraction is imperfect. Use LaTeX source when you have it, and inspect
  the saved extraction and page images when equations or tables matter.
- Web search depends on the provider. The Anthropic and Google APIs support it;
  OpenAI Chat Completions and local servers do not. A search result is not a
  substitute for reading the cited paper.
- Subscription plans may queue concurrent calls, so parallel steps do not
  always shorten a run proportionally.

## Privacy

Pipeline collects no telemetry. A remote model call sends that step's prompt
and the document material on its access list to the provider you selected.
Runs, extracted text, page images, conversations, and logs are stored
unencrypted under `~/.pipeline/`, and uninstalling the application does not
remove that directory. Read [PRIVACY.md](PRIVACY.md) before working with
confidential or restricted material, including manuscripts under review.

## Command line

`pipeline-cli` runs reviews without the desktop interface, using the same
settings, definitions, and run store. The installers place the binary beside
the application executable (on macOS, `Pipeline.app/Contents/MacOS/pipeline-cli`)
without adding it to your `PATH`. From a source checkout:

```bash
cd gui/src-tauri
cargo run --locked --bin pipeline-cli -- run \
  --profile auto-review \
  --input /path/to/paper.pdf \
  --out /path/to/report.md
```

See the [CLI guide](docs/cli/README.md) for batches, variables, validation
without a model call, and exit codes.

## Build from source

Use Node.js 24.18.0 (`gui/.nvmrc`) and Rust 1.97.1 (`rust-toolchain.toml`).

```bash
git clone https://github.com/mdroste/pipeline.git
cd pipeline/gui
npm ci
npm run tauri build
```

Installers are written to `gui/src-tauri/target/release/bundle/`. Development
setup and the test suite are described in [CONTRIBUTING.md](CONTRIBUTING.md);
the release procedure is in [RELEASING.md](RELEASING.md).

## License

Pipeline is released under the [MIT License](LICENSE). The bundled Poppler
tools are licensed under the GPL; see
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
