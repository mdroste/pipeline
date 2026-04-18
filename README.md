# Pipeline

[![CI](https://github.com/mdroste/pipeline/actions/workflows/build.yml/badge.svg)](https://github.com/mdroste/pipeline/actions/workflows/build.yml)
[![Latest release](https://img.shields.io/github/v/release/mdroste/pipeline?label=release)](https://github.com/mdroste/pipeline/releases/latest)
[![Release date](https://img.shields.io/github/release-date/mdroste/pipeline)](https://github.com/mdroste/pipeline/releases/latest)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-blue)](https://github.com/mdroste/pipeline/releases/latest)
[![License: MIT](https://img.shields.io/github/license/mdroste/pipeline)](LICENSE)

Pipeline generates referee reports for academic papers. It takes a PDF or LaTeX source, runs several independent analyses in parallel (contribution, technical correctness, empirical strategy, internal consistency, exposition), then consolidates the results into a single structured report.

It is a desktop app for macOS, Windows, and Linux. No API key is required if you have a Claude subscription.

## Installation

Download the latest build for your platform from [Releases](https://github.com/mdroste/pipeline/releases).

To build from source (requires [Node.js](https://nodejs.org/) >= 18 and the [Rust toolchain](https://rustup.rs/)):

```bash
git clone https://github.com/mdroste/pipeline.git
cd pipeline/gui
npm install
npm run tauri build
```

The output is in `gui/src-tauri/target/release/bundle/`.

### LLM setup

You need at least one LLM provider:

- **Claude** (default): Install [Claude Code](https://docs.anthropic.com/en/docs/claude-code) (`npm install -g @anthropic-ai/claude-code`) and sign in. Or set an Anthropic API key in Settings.
- **Codex**: Install the [Codex CLI](https://github.com/openai/codex) (`npm install -g @openai/codex`), or set an OpenAI API key in Settings.
- **Gemini**: Install the [Gemini CLI](https://github.com/google/gemini-cli) (`npm install -g @google/gemini-cli`), or set a Google API key in Settings.

### PDF support

Pipeline bundles `pdftoppm` and `pdftotext` (from [poppler](https://poppler.freedesktop.org/)) on all platforms, so PDF extraction works out of the box with no extra install. `pdftoppm` is what Claude Code uses internally to render PDF pages for the LLM; `pdftotext` is the fallback extractor.

For a higher-quality alternative that preserves equations as LaTeX, optionally install [marker-pdf](https://github.com/VikParuchuri/marker): `pip install marker-pdf`.

LaTeX source files are always read natively and don't need either tool.

Bundled poppler is GPL-2.0+; see [`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md).

## What it does

Pipeline processes a paper in three stages:

1. **Extract**: Reads the paper text. LaTeX files are parsed directly (with `\input{}` resolution). PDFs go through LLM extraction, marker, or pdftotext.
2. **Orient**: One LLM call builds a structured map of the paper -- sections, formal results, tables, notation, stated contribution. This map is validated against a schema and shared with all subsequent steps.
3. **Execute**: Runs the configured pipeline steps. Parallel steps run concurrently; sequential steps run afterward and receive all prior outputs.

### Default pipeline (Deep Review)

| Step | Phase | Focus |
|------|-------|-------|
| Contribution | Parallel | Novelty, literature positioning, promise vs. delivery. Web search enabled. |
| Technical Correctness | Parallel | Proofs, derivations, assumptions, sign errors. |
| Empirical Strategy | Parallel | Identification, threats to validity, robustness. |
| Internal Consistency | Parallel | Cross-references, notation, abstract-body alignment, table-text agreement. |
| Exposition | Parallel | Organization, notation burden, figures, framing. |
| Consolidate | Sequential | Merges parallel outputs, deduplicates, orders by severity. |
| Validate | Sequential | Re-reads the paper to verify each comment. Disabled by default. |

Two other built-in profiles are included: **Quick Review** (2 parallel steps + consolidation) and **Empirical** (4 parallel steps + consolidation + validation).

Each parallel step receives only the paper text, the orientation map, and its own prompt. The prompts instruct the LLM to identify issues, not to summarize or praise. The consolidation step deduplicates across all parallel outputs and ranks by severity.

## Configuration

The pipeline editor in the GUI lets you add, remove, reorder, enable, and disable steps. Each step has a phase (Parallel or Sequential), a prompt, optional tools (e.g., WebSearch), and one or more LLM agents. Assigning multiple agents to a step (e.g., Claude + Gemini) runs them independently; their outputs are merged automatically.

Custom profiles can be created, exported, and imported. Settings and profiles are stored in `~/.pipeline/`.

## Revision tracking

Reports are keyed by a hash of the paper content and stored in `~/.pipeline/history/`. Running Pipeline on a revised draft of a previously reviewed paper produces a diff: which issues were addressed, which persist, and what is new.

## Limitations

- Each parallel step reads the full paper. On subscription plans with usage limits, a single report consumes a meaningful share of your allowance.
- Concurrent LLM calls may queue on some subscription tiers. Wall-clock time varies.
- LLM training data has a knowledge cutoff. The contribution step's web search partially compensates for this, but coverage of very recent work is not guaranteed.
- PDF extraction without marker or LLM extraction will garble equations. Use LaTeX source when possible.

## License

MIT
