# Pipeline

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

### Optional: PDF extraction tools

By default, PDFs are extracted using the configured LLM. Two alternatives are available if preferred:

- **marker-pdf**: `pip install marker-pdf` (preserves equations as LaTeX)
- **pdftotext**: `brew install poppler` / `sudo apt install poppler-utils` / `choco install poppler` (equations will be garbled)

LaTeX source files are always read natively and don't need either tool.

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
