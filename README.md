# Pipeline

[![CI](https://github.com/mdroste/pipeline/actions/workflows/build.yml/badge.svg)](https://github.com/mdroste/pipeline/actions/workflows/build.yml)
[![Latest release](https://img.shields.io/github/v/release/mdroste/pipeline?label=release)](https://github.com/mdroste/pipeline/releases/latest)
[![Release date](https://img.shields.io/github/release-date/mdroste/pipeline)](https://github.com/mdroste/pipeline/releases/latest)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-blue)](https://github.com/mdroste/pipeline/releases/latest)
[![License: MIT](https://img.shields.io/github/license/mdroste/pipeline)](LICENSE)

Pipeline generates referee reports for academic papers. It takes a PDF, LaTeX source, or Word `.docx`, runs several independent analyses in parallel (contribution, technical correctness, empirical strategy, internal consistency, exposition), then consolidates the results into a single structured report.

It is a desktop app for macOS, Windows, and Linux. A separate API key is not
required when your authenticated Claude Code subscription includes CLI access;
provider account, plan, and usage terms still apply.

[Privacy and data flow](PRIVACY.md) · [Security policy](SECURITY.md) ·
[Supported platforms](SUPPORT.md) · [Contributing](CONTRIBUTING.md) ·
[Changelog](CHANGELOG.md) · [Release process](RELEASING.md)

## Installation

Download the latest build for your platform from [Releases](https://github.com/mdroste/pipeline/releases).

Release system baselines:

- **macOS:** macOS 15.0 or later, with separate Apple Silicon and Intel builds.
  The minimum is intentionally set to the newest requirement in the complete
  bundled Poppler library closure; the installer does not claim compatibility
  with older macOS versions that its PDF tools cannot satisfy.
- **Windows:** Windows 11 on a supported servicing release, x86-64. Windows 10
  22H2 is the compatibility floor but is supported only on systems still
  receiving Microsoft security updates, such as eligible ESU-managed devices.
  Public release CI requires the application and installer to carry a valid,
  timestamped Authenticode signature. Microsoft WebView2's offline installer
  is embedded so first installation does not depend on a WebView download;
  this adds roughly 127 MB to the installer.
- **Linux:** x86-64 Ubuntu 22.04 or a compatible newer distribution (AppImage).
  Direct execution requires FUSE 2 (`sudo apt-get install libfuse2` on a
  minimal Ubuntu 22.04 installation). If FUSE cannot be enabled, AppImage's
  documented extract-and-run mode remains available as a compatibility
  fallback.

Each public installer is accompanied by a same-named `.sha256` record, an
artifact SBOM, a build-input SBOM, Poppler provenance, and a GitHub
build-provenance attestation. The artifact SBOM records the installer's SHA-256
and the hashes of the other two evidence files. Verify the downloaded file
against the checksum before running it; the release is incomplete if any of
these records is missing.

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

### Cross-platform testing

Every pull request runs the Rust and frontend suites and opens a compiled test
binary on Ubuntu 22.04, Windows Server 2022, Apple Silicon macOS 15, and Intel
macOS 15. Release builds additionally launch the AppImage directly through its
FUSE entry point, silently install and launch the Windows NSIS package, and
assess and launch a quarantined,
notarized macOS app before release assets are finalized.

Pipeline checks GitHub for newer stable releases and links to the release page,
but it does not update itself in place. Download and install the newer signed
package manually. Prereleases are not offered by the in-app check. See the
complete [support and update policy](SUPPORT.md).

### LLM setup

You need at least one LLM provider:

- **Claude** (default): Install [Claude Code](https://docs.anthropic.com/en/docs/claude-code) (`npm install -g @anthropic-ai/claude-code`) and sign in. Or set an Anthropic API key in Settings.
- **Codex**: Install the [Codex CLI](https://github.com/openai/codex) (`npm install -g @openai/codex`), or set an OpenAI API key in Settings.
- **Gemini**: Install the latest [Gemini CLI](https://github.com/google/gemini-cli) (`npm install -g @google/gemini-cli@latest`) and run `gemini` once to sign in with OAuth, or set a Google API key in Settings. Pipeline requires a CLI version that supports `--admin-policy`.

### PDF support

Pipeline bundles `pdftoppm` and `pdftotext` (from [poppler](https://poppler.freedesktop.org/)) on all platforms, so basic PDF extraction and page rendering work out of the box with no extra install. The extractor selected for a workflow is authoritative; Pipeline does not silently switch methods after a failure.

For higher-quality local extraction, Settings → PDF Extraction can optionally
install [PaddleOCR-VL 1.6](https://huggingface.co/PaddlePaddle/PaddleOCR-VL-1.6-GGUF)
Q8 with a native [llama.cpp](https://github.com/ggml-org/llama.cpp) runtime
(about 1.9 GB). A second **Full Parser** install adds the official PaddleOCR
layout client and PP-DocLayoutV3 for structured reading order, title
hierarchy, formulas, cross-page tables, and figure regions. It reuses
Pipeline's authenticated llama.cpp server. Users do not need to install
LlamaCPP, Python, pip, Conda, or Docker: the optional parser provisions a
private, versioned Python environment beneath `~/.pipeline/`. Both engines
run locally and can be managed from the same page, along with layout, OCR,
table/title, speed, memory, retry, and cache controls.

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

1. **Extract and normalize**: Reads PDF, LaTeX, or DOCX and creates a versioned `DocumentBundle` containing text blocks, equations, tables, figures, page renders, source representations, provenance, and extraction warnings.
2. **Orient**: One LLM call builds a structured map of the paper -- sections, formal results, tables, notation, stated contribution -- and enriches matching bundle nodes.
3. **Execute**: Runs the configured dependency graph. Ready parallel steps run concurrently and cannot read any step output; sequential steps run alone and receive only the artifacts selected in their exact allowlists. Primary text, structure, visuals, source, survey data, and named inputs are likewise available only when selected.

Each saved run includes a readable document, canonical JSON, streaming JSONL
blocks, page images, and extracted or source-native figures. The run Artifact
Explorer has a DocumentBundle inspection view for checking source provenance,
quality warnings, semantic blocks, equations/tables, and links to every page
or figure image.

### Default pipeline (Paper Review (Full))

| Step | Phase | Focus |
|------|-------|-------|
| Contribution | Parallel | Novelty, literature positioning, promise vs. delivery. Web search enabled. |
| Technical Correctness | Parallel | Proofs, derivations, assumptions, sign errors. |
| Empirical Strategy | Parallel | Identification, threats to validity, robustness. |
| Internal Consistency | Parallel | Cross-references, notation, abstract-body alignment, table-text agreement. |
| Exposition | Parallel | Organization, notation burden, figures, framing. |
| Consolidate | Sequential | Merges parallel outputs, deduplicates, orders by severity. |
| Validate | Sequential | Re-reads the paper to verify each comment. Disabled by default. |

The built-in parallel steps explicitly select the paper text, document
structure, visual assets, original source, and orientation survey. Consolidation
steps select only the reports they synthesize; verification steps additionally
select the primary evidence they need to re-check those reports.

### Built-in profiles

The app ships five profiles you can use as-is or copy and edit:

- **Paper Review (Full)** and **Paper Review (Quick)** — full and abbreviated referee-report workflows.
- **Codebase Review** — seven code-review passes, consolidation, and verification over a source folder.
- **Replication Package Audit** — for a folder containing a paper's replication package.
- **Grant Proposal Review** — aims, feasibility, panel readability, consistency.

## Configuration

The pipeline editor in the GUI lets you add, remove, reorder, enable, and disable steps. Each step has a phase (Parallel or Sequential), a prompt, optional tools (for example, WebSearch), and one or more LLM agents. Assigning multiple agents to a step (for example, Claude + Gemini) runs them independently; their outputs are merged automatically.

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

Custom profiles can be created, exported, imported from a file or a URL, and shared. Settings and profiles are stored in `~/.pipeline/`.

## Batch and watch

The **Batch** panel runs the active profile over many papers, or a whole folder, one at a time. **Watch a folder** does the same automatically as files are added. A separate **run history** lets you reopen, re-run (reusing prior work), compare, and annotate past runs.

## Revision tracking

Reports are stored as self-contained runs under `~/.pipeline/runs/`. Pipeline links revisions by their stable input path and content hash; running it on a revised draft can therefore show which issues were addressed, which persist, and what is new.

## Limitations

- Each model generation still accounts for the paper context it receives.
  Shared sessions and provider caches can make repeated input cheaper, but do
  not make those logical tokens disappear from usage reports.
- Concurrent LLM calls may queue on some subscription tiers. Wall-clock time varies.
- LLM training data has a knowledge cutoff. The contribution step's web search partially compensates for this when the selected transport supports search, but coverage of very recent work is not guaranteed.
- Basic pdftotext extraction will garble equations. Use LaTeX source, LLM
  extraction, or PaddleOCR-VL when equations matter.

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
