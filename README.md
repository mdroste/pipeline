# Pipeline

[![CI](https://github.com/mdroste/pipeline/actions/workflows/build.yml/badge.svg)](https://github.com/mdroste/pipeline/actions/workflows/build.yml)
[![Latest release](https://img.shields.io/github/v/release/mdroste/pipeline?label=release)](https://github.com/mdroste/pipeline/releases/latest)
[![License: MIT](https://img.shields.io/github/license/mdroste/pipeline)](LICENSE)

Pipeline is an application for AI-assisted research. It runs on macOS, Windows, and Linux. This application allows for users to work with ChatGPT (subscription or API), Claude (API), or Gemini (API). 

You might use Pipeline to do one or more of the following three things:

- **Reviews** Automatically review academic papers with a bunch of LLM calls at the same time. 
- **Conversations** You can use Pipeline to track research projects and invoke LLMs for coding and whatnot. You can, to some extent, customize the prompt. 
- **Automations** chain the two: for example, draft, review, and revise until
  a review raises no high-priority concern or a set limit is reached. These can run on a schedule if you are a busybody.


[Privacy and data flow](PRIVACY.md) · [Supported platforms](SUPPORT.md) ·
[Security policy](SECURITY.md) · [Changelog](CHANGELOG.md) ·
[Contributing](CONTRIBUTING.md)

## Install

Download the installer for your platform from
[Releases](https://github.com/mdroste/pipeline/releases/latest).

| Platform | File | Requirements |
| --- | --- | --- |
| macOS (Apple Silicon )| `Pipeline-<version>-macOS-AppleSilicon.dmg` | macOS 15 or later |
| macOS (Intel) | `Pipeline-<version>-macOS-Intel.dmg` | macOS 15 or later |
| Windows (x86-64) | `Pipeline-<version>-Windows.exe` | Windows 11, or Windows 10 22H2 with security updates |
| Linux (x86-64) | `Pipeline-<version>-Linux.AppImage` | glibc 2.35 or later (Ubuntu 22.04) and FUSE 2 |

The macOS builds are signed and notarized. The Windows installer is not
signed (very sorry, but I'm not spending $500 on buying a certification specifically to fix this), so SmartScreen will warn you before running it; choose **More info**, then
**Run anyway**. I promise there are no viruses or whatever. You can check! Each installer is published with a SHA-256 checksum.

The installers contain the application and the Poppler PDF tools. They do not
require Python or any other language runtime. Pipeline tells you when a newer
release exists but never updates itself.

## Connect a model provider

Open **Settings → Connections**.

| Provider | With a subscription | With an API key |
| --- | --- | --- |
| OpenAI | Install the [Codex CLI](https://github.com/openai/codex) (`npm install -g @openai/codex`), then choose **Sign in to ChatGPT** | OpenAI API key |
| Anthropic | Install [Claude Code](https://docs.anthropic.com/en/docs/claude-code) (`npm install -g @anthropic-ai/claude-code`) and sign in | Anthropic API key |
| Google | Not available (Google says this use case is not OK | [Gemini API key](https://aistudio.google.com/apikey) |
| Local models | — | Any OpenAI-compatible server: Ollama by default, or LM Studio, llama.cpp, or vLLM by changing the URL |

Reviews can use any of these providers. Conversations and Automations use the ChatGPT sign-in only. Really the only reason this isn't enabled everywhere is because I've been spooked Anthropic is not cool with this, and Google definitely isn't cool with this, and by 'this' I mean building an application that uses 'headless' calls to LLMs through the command line through a subscription (oauth) plan. 

For OpenAI and Anthropic you choose subscription or API mode explicitly; a stored key is ignored while subscription mode is selected. You can sleep peacefully knowing that your API keys are encrypted before they are written to disk. Google's insanely restrictive terms of service do not allow third-party software to use its subscription sign-in, so Google models are always called through the Gemini API, which is a real shame. 

## Reviews

### Inputs

Reviews take as inputs a PDF, or a folder containing .tex files, or a .docx file. 

LaTeX source is read directly and gives the most reliable text, equations, and tables, so use that if you have it. 

If you want to a review a paper that is only available as a PDF, there are three methods of text extraction. Read carefully, because this actually matters quite a bit! Here are the three methods:

- **Poppler.** Bundled `pdftotext` extraction and page rendering. Needs no setup, but is generally not very good for PDFs that contain a lot of equations. 
- **Model extraction.** Your local LLM reads the PDF. While this sounds neat, it's actually not ideal. At the time of me writing this, comamnd line LLMs typically end up using -pdftotext- and whatnot behind the scenes (unlike the web version e.g.) so in practice the extracted PDF isn't really that good. 
- **PaddleOCR-VL Full Parser.** An optional local parser, installed from Settings, that preserves reading order, headings, formulas, tables, and figure regions. It occupies several gigabytes and installs its own Python environment and model weights under `~/.pipeline/`. I tested a bunch of local PDF parsers in summer 2026 that would be feasible to run quickly on most consumer hardware, and this was the best I tried, so I strongly recommend installing this from the Settings page and using it. 

By default Pipeline uses the Full Parser when it is installed and model extraction otherwise. You can fix the method globally or for one review. If the chosen method fails or returns incomplete text, the review stops; it does not fall back to another method.

### Included reviews

- **Automatic Paper Review (Full)** (default). A first call identifies the
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

There is a "gallery" containing some other stuff that ChatGPT generated for me one night, which apparently also includes: Revision Response Check, Literature Positioning Scan, Thesis Chapter Review, and Rubric-Based Review. You can add these within the app if you want. 

### What a review does

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

### Writing your own 'pipeline'

The designer sets each step's prompt, provider, model, reasoning effort, web
search, dependencies, and access list. Steps can be conditional, can require
structured JSON output, and can fan out over files matching a pattern. A
review definition is a single JSON file that can be exported, shared, and
imported from a file or URL.

## Projects, Conversations, and Automations

I made a somewhat awkward attempt to have a 'project' feature that you can use to maintain a complicated research project.  A project gathers a paper's sources, manuscript, conversations, reviews, and results. Inside a project, work is organized under Overview, Library, Analyze, Write, Automate, and Activity. The overview reports the state of the paper: the current version, the last build, outstanding review findings, and numbers or claims that no longer match their underlying results.

A conversation runs through the Codex runtime under your ChatGPT account. You choose the model and reasoning effort, and you approve what the assistant may read, write, and execute. Local computation is limited to the LaTeX, Stata, and Python commands you configure. Material enters the record in distinct classes: notes you accepted, source revisions, execution receipts, evidence, and model proposals. A numerical check is never recorded as a proof.

Research can be exported in two forms. A `.pwrx` backup restores an entire store onto another machine. A `.pwex` package sends selected items to a coauthor, without conversations, credentials, or execution settings.

Automations are described in [docs/tasks.md](docs/tasks.md). Longer-running research agendas, including the self-discovery modes, are documented in [docs/research-missions.md](docs/research-missions.md) and [docs/self-discovery.md](docs/self-discovery.md).

## Limits

- Reviews are the most thoroughly tested part of Pipeline. Conversations, project research tools, and Automations are newer and have been exercised mainly on macOS.
- On Windows, the assistant can read project files but cannot yet apply edits to them, and isolated working copies for tasks are unavailable. Reviews are unaffected.
- Each model call must fit within the provider's context window. A long paper, the selected material, tool results, and the response all count.
- PDF extraction is imperfect. Use LaTeX source when you have it, and inspect the saved extraction and page images when equations or tables matter.
- Web search depends on the provider. The Anthropic and Google APIs support it; OpenAI Chat Completions and local servers do not. A search result is not a substitute for reading the cited paper.
- Subscription plans may queue concurrent calls, so parallel steps do not always shorten a run proportionally.

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
