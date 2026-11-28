# Pipeline

[![CI](https://github.com/mdroste/pipeline/actions/workflows/build.yml/badge.svg)](https://github.com/mdroste/pipeline/actions/workflows/build.yml)
[![Version 1.6.0](https://img.shields.io/badge/version-1.6.0-blue)](CHANGELOG.md)
[![License: MIT](https://img.shields.io/github/license/mdroste/pipeline)](LICENSE)
[![macOS](https://img.shields.io/badge/macOS-000000?logo=apple&logoColor=white)](#install)
[![Windows](https://img.shields.io/badge/Windows-0078D4)](#install)
[![Linux](https://img.shields.io/badge/Linux-FCC624?logo=linux&logoColor=black)](#install)

Pipeline is a desktop app for academic research with AI assistants. It supports
paper reviews, work on manuscripts and code, and repeated tasks such as review
and revision. You can use a ChatGPT or Claude subscription, an API key, or a
local model.

[Download](https://github.com/mdroste/pipeline/releases/latest) · [User
guides](docs/README.md) · [Changelog](CHANGELOG.md) ·
[Contributing](.github/CONTRIBUTING.md)

## Paper reviews

Pipeline reads your paper and automatically selects AI reviewers to fit its
subject and methods. The reviewers work in parallel, each assessing the paper
from their assigned perspective. Pipeline then combines their comments and
checks each one against the text. The report includes the individual responses
and sources, so you can examine the reasoning behind a finding and assess it
yourself.

Choose **New review** from Home, select your paper, and choose **Paper Review**.
Check the model and extraction settings, then choose **Start review** to inspect
the plan. Choose **Start review** to run it.

You can review papers or slides in PDF, Microsoft Word, or LaTeX source format.
Pipeline can use your selected AI model to read PDFs, or (optionally) you can
install a really good PDF parser (PaddleOCR) from **Settings → Reviews** that
typically does a better job extracting equations and tables from PDFs than
Claude, ChatGPT, or Gemini do.

**Paper Review (Fast)** uses fewer reviewers. **Slide Review** and **Grant
Proposal Review** cover presentations and proposals.

You can annotate findings, compare reviews, and track comments across drafts.
See the [review guide](docs/reviews.md).

## Research projects

A project keeps related conversations and research material together. Create one
from **Projects** and attach a folder containing your manuscripts and code, or
begin by collecting sources and notes.

In each conversation, you choose the material and tools available to the
assistant. You can give it instructions and run LaTeX, Stata, R, or Python
commands. Edit conversations work on a separate copy of the project; **Review
edits** lets you inspect and accept changes.

Automations connect these activities. For example, **Review and revise** links a
review to a conversation that works on revisions. Open **Automations** to choose
the task, timing, and limits. Tasks run while Pipeline is open and the computer
is awake.

See [Projects and conversations](docs/projects.md), [Write and
review](docs/analysis-and-writing.md), and [Automations](docs/automations.md).

## Install

Download the macOS, Windows, or Linux installer from
[Releases](https://github.com/mdroste/pipeline/releases/latest). The **Get
started** guide checks your connections and finds research tools installed on
your computer.

Open **Settings → Models** to connect a model provider:

| Connection | Setup |
| --- | --- |
| ChatGPT subscription | Install [Codex](https://github.com/openai/codex), then choose **Sign in to ChatGPT** |
| Claude subscription | Install [Claude Code](https://code.claude.com/docs/en/setup), then run `claude auth login` |
| OpenAI, Anthropic, or Google API | Enter an API key |
| Local or custom model | Enter an OpenAI-compatible server URL and model |

Choose a model with the selector below the conversation’s message box. Subscription
use counts against your plan. Tool and command support depends on the connection
and operating system; see [Models](docs/connections.md) for setup and
[Support](.github/SUPPORT.md) for system requirements.

## Data and privacy

Research data is saved on your computer, by default in `~/.pipeline/`. AI
providers receive the prompts and material used in your requests. Enabled web
tools send queries and page requests to search services and websites, including
when you use a local model.

Saved API keys are encrypted. Reports, documents, conversations, and logs remain
readable on disk. Your research data stays in place when you uninstall the app.
See [Privacy](PRIVACY.md).

## Command line and development

The bundled `pipeline-cli` runs reviews from the terminal and shares the app’s
settings and saved runs. See the [command-line guide](docs/cli.md) for its
location and usage.

To run from source, install Node.js 24.18.0 and Rust 1.97.1, then:

```bash
git clone https://github.com/mdroste/pipeline.git
cd pipeline/gui
npm ci
npm run tauri dev
```

`npm run tauri build` creates an installer. See
[Contributing](.github/CONTRIBUTING.md) for platform dependencies and tests.

Pipeline uses the [MIT License](LICENSE). The bundled Poppler tools use the GPL;
see [third-party licenses](THIRD_PARTY_LICENSES.md).
