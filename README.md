# Pipeline

[![CI](https://github.com/mdroste/pipeline/actions/workflows/build.yml/badge.svg)](https://github.com/mdroste/pipeline/actions/workflows/build.yml)
[![Latest release](https://img.shields.io/github/v/release/mdroste/pipeline?label=release)](https://github.com/mdroste/pipeline/releases/latest)
[![Release date](https://img.shields.io/github/release-date/mdroste/pipeline)](https://github.com/mdroste/pipeline/releases/latest)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-blue)](https://github.com/mdroste/pipeline/releases/latest)
[![License: MIT](https://img.shields.io/github/license/mdroste/pipeline)](LICENSE)

Pipeline is an agent orchestration tool to review papers, slides, and documents. 

Pipeline ships with several orchestration profiles ('workflows') that are useful for reviewing academic papers and slides in economics. 

[Privacy and data flow](PRIVACY.md) · [Security policy](SECURITY.md) ·
[Supported platforms](SUPPORT.md) · [Contributing](CONTRIBUTING.md) ·
[Changelog](CHANGELOG.md) · [Release process](RELEASING.md)

## Installation

Download the latest build for your platform from [Releases](https://github.com/mdroste/pipeline/releases).

If you know what you're doing and have a good reason for doing it, you can alternatively build the app from source.
To build from source, use the pinned [Node.js](https://nodejs.org/) 24.18.0 
version in `gui/.nvmrc` and the Rust 1.97.1 toolchain declared in
`rust-toolchain.toml`:

```bash
git clone https://github.com/mdroste/pipeline.git
cd pipeline/gui
npm ci
npm run tauri build
```

The output is in `gui/src-tauri/target/release/bundle/`.

### LLM Provider Setup

Pipeline works with Claude, ChatGPT, and Gemini. If you have a subscription plan to one 
or more of these services and want to use them with Pipeline, you need to be able to access one of the following 

- **Claude** (default): Install [Claude Code](https://docs.anthropic.com/en/docs/claude-code) (`npm install -g @anthropic-ai/claude-code`) and sign in. 
- **Codex**: Install the [Codex CLI](https://github.com/openai/codex) (`npm install -g @openai/codex`) and sign in.
- **Gemini**: Install the latest [Gemini CLI](https://github.com/google/gemini-cli) (`npm install -g @google/gemini-cli@latest`) and sign in.

If instead you want to use an API to access one or more of these platforms, you do not
need these CLIs installed; just enter your API key(s) in Pipeline's Settings menu. 

### PDF Support / Extraction

Pipeline bundles `pdftoppm` and `pdftotext` (from [poppler](https://poppler.freedesktop.org/)) on all platforms, 
so basic PDF extraction and page rendering work out of the box with no extra install. 
The extractor selected for a workflow is authoritative; Pipeline does not silently switch methods after a failure.

For higher-quality local extraction, Settings → PDF Extraction can optionally
install the **PaddleOCR-VL 1.6 Full Parser**. It combines the official
[PaddleOCR-VL 1.6](https://huggingface.co/PaddlePaddle/PaddleOCR-VL-1.6-GGUF)
Q8 recognition model, a native [llama.cpp](https://github.com/ggml-org/llama.cpp)
server, the official PaddleOCR layout client, and PP-DocLayoutV3. The parser
preserves structured reading order, title hierarchy, formulas, cross-page
tables, and figure regions. Users do not need to install llama.cpp, Python,
pip, Conda, or Docker: Pipeline provisions the complete private, versioned
runtime beneath `~/.pipeline/` and manages it as one local extractor.

Marker extraction is unavailable in Pipeline 1.0.1 because the Python
dependency versions compatible with Pipeline's integration contain known
security vulnerabilities. Pipeline neither installs nor executes Marker.
Upgrades that retain an older Pipeline-managed Marker environment show a
one-time removal control in Settings; saved runs and historical Marker
artifacts remain readable.

LaTeX source files are read natively; a compiled companion PDF is used for
visual page inspection when present. Word `.docx` papers are read from OOXML,
including table grids, equation OMML, and embedded images. Legacy `.doc` is
not supported.

Bundled Poppler is GPL-licensed. Exact platform inputs, source hashes, dynamic
library notices, and SBOM details are in
[`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md). The same notice, license
texts supplied by the packages, a CycloneDX build-input SBOM, and exact Poppler
file hashes are available offline inside every release build.

## What it does

Pipeline processes a paper in three stages:

1. **Extract and orient**: Reads PDF, LaTeX, or DOCX and creates a versioned
   `DocumentBundle` containing text blocks, equations, tables, figures, page
   renders, source representations, provenance, and extraction warnings.
3. **Parallel agents**: A set of agents run in parallel, taking as input
   the `DocumentBundle` from Step 1, using a pre-configured prompt and tools,
   and producing as output one or more artifacts (e.g. a Markdown report).
3. **Sequential agents**: A set of agents run sequentially, taking as inputs
   the `DocumentBundle` from Step 1 and any relevant artifacts from the parallel
   agents in Step 2. 

Each saved run includes a readable document, canonical JSON, streaming JSONL
blocks, page images, and extracted or source-native figures. The run Artifact
Explorer has a DocumentBundle inspection view for checking source provenance,
quality warnings, semantic blocks, equations/tables, and links to every page
or figure image.

### Built-in profiles

The app ships three profiles you can use as-is or copy and edit:

- **Paper Review (Full)** and **Paper Review (Quick)** — full and abbreviated referee-report workflows.
- **Grant Proposal Review** — aims, feasibility, panel readability, consistency.

## Configuration

The workflow editor in the GUI lets you add, remove, reorder, enable, and disable steps. 
Each step has a phase (Parallel or Sequential), a prompt, optional tools (for example, WebSearch), 
and one or more LLM agents. Assigning multiple agents to a step (for example, Claude + Gemini) 
runs them independently.

Every step also has an exact **Artifact access** allowlist. Presets cover the
common cases, while individual controls expose readable input text, document
structure, pages/figures, original source, the survey, named inputs, upstream
reports, and upstream supporting files. Supporting files can be narrowed with a
glob such as `**/*.csv`. Only Sequential steps can select step outputs;
Parallel steps remain mutually independent. Selecting an upstream artifact
automatically creates the necessary dataflow dependency. **Wait for** adds a
separate order-only dependency without revealing the producer's output. An
empty allowlist is an intentionally isolated step.

For long, multi-step inputs, **Pipeline Settings → Reuse shared input context**
optionally prepares the primary text and/or survey selected by each step. API
providers reuse a warmed prompt prefix; Claude and Codex CLI calls fork a
warmed base session. Unselected material is never added to the warmed context.
The setting is per profile: Paper Review (Full) enables it by default, while
generic and custom profiles remain opt-in. Unsupported providers fall back to
ordinary self-contained calls.

The Console and History use one accounting convention across providers:
**logical input = fresh input + cache-read input + cache-write input**.
Cache-read and cache-write counts are therefore subsets of logical input, not
tokens to add on top; fresh input is obtained by subtracting both. The report's
run summary prices saved per-model calls and their partitions separately at
known list rates, while noting any run-wide work it cannot attribute to a
saved model row. For CLI and subscription calls this is explicitly an
API-equivalent estimate, not an amount charged to the subscription.
Provider-reported model round trips and tool calls are shown alongside the
token totals, which makes repeated read/search loops visible.

When a step needs several independent passages or images, direct APIs expose
bounded batch readers in addition to the single-item tools. Prompts ask every
transport to group independent text reads, visual inspections, and eligible
web queries into one tool turn when supported, then continue sequentially for
anything missing or truncated. This changes retrieval granularity, not the
evidence or review instructions. Anthropic and Google direct APIs use hosted
search when a step declares `WebSearch`; OpenAI Chat Completions and local
OpenAI-compatible endpoints retain text/image batching but do not expose
hosted search.

Steps also support **conditions** (run a step only when the survey or an upstream
step matches), a **JSON output shape** (which turns the report into a sortable,
annotatable issues table), **variables** (values the app asks for at run time,
referenced as `{var:name}`), **extra named inputs**, and **fan-out** (run a step
once per file matching a glob, with `{item}` bound to each file).

Custom profiles can be created, exported, imported from a file or a URL, and shared.
Settings and profiles are stored in `~/.pipeline/`.

## Batch inputs

On **New run**, select several documents—or select a folder and declare it a
**Batch of documents**—to run the active profile over each document, one at a
time. The same input control can declare a folder to be one LaTeX paper or a
browsable source tree; Pipeline no longer infers that semantic choice from the
path alone.
A separate **run history** lets you reopen, re-run (reusing prior work), compare, 
and annotate past runs.

## Revision tracking

Reports are stored as self-contained runs under `~/.pipeline/runs/`. 
Pipeline links revisions by their stable input path and content hash; 
running it on a revised draft can therefore  show which issues were addressed, 
which persist, and what is new.

## Limitations

- Each model generation still accounts for the paper context it receives.
  Shared sessions and provider caches can make repeated input cheaper, but do
  not make those logical tokens disappear from usage reports.
- Concurrent LLM calls may queue on some subscription tiers. Wall-clock time varies.
- LLM training data has a knowledge cutoff. Agents that are configured with web search
  partially compensate, but users should be aware that web search is imperfect for
  (e.g.) literature reviews on active topics.
- Extracting PDFs into a `DocumentBundle` is hard. Use LaTeX source whenever available, 
  or PaddleOCR-VL when equations matter.

## Privacy

Pipeline does not operate a hosted service or collect application telemetry,
but remote-provider runs transmit the prompt and the document material allowed
for each step to the selected provider or CLI. Runs can persist full source
material, page images, intermediate responses, reports, and logs under
`~/.pipeline/`; uninstalling the app does not necessarily remove that data.
Read [PRIVACY.md](PRIVACY.md) before processing confidential or restricted
material.

## License

MIT
