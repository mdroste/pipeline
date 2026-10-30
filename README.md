# Pipeline 1.2.1

[![CI](https://github.com/mdroste/pipeline/actions/workflows/build.yml/badge.svg)](https://github.com/mdroste/pipeline/actions/workflows/build.yml)
[![Version 1.2.1](https://img.shields.io/badge/version-1.2.1-blue)](CHANGELOG.md)
[![License: MIT](https://img.shields.io/github/license/mdroste/pipeline)](LICENSE)
[![macOS](https://img.shields.io/badge/macOS-000000?logo=apple&logoColor=white)](#install)
[![Windows](https://img.shields.io/badge/Windows-0078D4)](#install)
[![Linux](https://img.shields.io/badge/Linux-FCC624?logo=linux&logoColor=black)](#install)

Pipeline is a desktop app for academic research. Use it to review a paper,
work on a research project with an AI assistant, or automate a sequence of
writing and review tasks.

- **Reviews** ask several AI reviewers to assess a paper, slide deck, or grant
  proposal, then combine and check their comments. You can read each
  reviewer's response and the source material behind the report.
- **Projects** keep manuscripts, sources, conversations, results, and review
  comments together. Work with ChatGPT or Claude Code to read, write, and
  analyze your material.
- **Automations** connect these. For example, draft a paper, review it, and
  revise it up to a limit you choose, once or on a schedule.

[Download](https://github.com/mdroste/pipeline/releases/latest) ·
[User guides](docs/README.md) · [Changelog](CHANGELOG.md) ·
[Privacy](PRIVACY.md) · [Contributing](.github/CONTRIBUTING.md)

## Install

Download the installer for your computer from
[Releases](https://github.com/mdroste/pipeline/releases/latest). Each
installer comes with a SHA-256 checksum.

macOS releases are signed and notarized. The Windows installer isn't signed,
so SmartScreen may warn you. If you downloaded it from this repository's
release page, choose **More info → Run anyway**.

The app includes the Poppler PDF tools, and you don't need Python. Pipeline
checks for new releases and lets you decide when to install them. See
[Support](.github/SUPPORT.md) for system requirements.

On first launch, a short **Get started** guide shows which connections are
ready and finds research tools on your computer, such as Stata, R, and
Python. You can reopen it from Home.

## Connect a provider

Open **Settings → Connections** and set up at least one connection.

| Connection | What you need | Works for |
| --- | --- | --- |
| ChatGPT subscription | Install [Codex](https://github.com/openai/codex), then choose **Sign in to ChatGPT** in Pipeline | Conversations and reviews |
| Claude subscription | Install [Claude Code](https://code.claude.com/docs/en/setup) and run `claude auth login` with your subscription account | Conversations and reviews |
| OpenAI, Anthropic, or Google API | An API key | Reviews |
| Local or custom model server | An OpenAI-compatible server URL and model | Reviews |

Subscription use counts against your plan. If you're signed out, Pipeline
asks you to sign in again instead of switching to an API key. See
[Connections](docs/connections.md) for details.

## Run your first review

1. Choose **New review** from Home.
2. Select a PDF, a Word `.docx` file, a `.tex` file, or a LaTeX project
   folder. LaTeX source keeps equations and structure intact.
3. Choose **Paper Review**, or **Paper Review (Fast)** for fewer reviewers.
   For a slide deck exported as PDF, choose **Slide Review**. For a proposal,
   choose **Grant Proposal Review**.
4. Check the provider, model, and PDF extraction settings, then choose
   **Start review** to see the plan.
5. Check the planned steps, then choose **Generate report**, or **Add to
   queue** if other reviews are running.

When the review finishes, read the report and open **Sources** to see the
document and the evidence behind each comment. Paper Review picks subject and
method specialists for your paper, combines their comments, and checks each
one against the text. Before you rely on a comment, check its claims,
calculations, and citations.

Pipeline saves every report with the individual responses and sources. You
can annotate findings, compare runs, and track comments across drafts. See
the [review guide](docs/reviews.md).

## Work on a project

Create a project from **Projects**. Attach a folder to work with existing
manuscripts and code, or start without one to collect sources and notes. Each
project has six areas:

| Area | What you do there |
| --- | --- |
| Overview | Resume work, read the research brief, and see next steps |
| Library | Read documents, files, and papers, and search them |
| Analyze | Work with data, experiments, theory, and results |
| Write | Edit the manuscript, handle revisions and reviews, and prepare drafts |
| Automate | Track action items, recurring checks, and revision work |
| Activity | Keep notes, decisions, and evidence, and see what a change affects |

Conversations use the project material and tools you choose. You can set
assistant instructions and run your own LaTeX, Stata, R, or Python commands.
See [Projects and conversations](docs/projects.md) and
[Analyze and write](docs/analysis-and-writing.md).

## Automate repeated work

Open **Automations**, or choose **Run an automation** from the **+** menu
below a conversation's message box. Start from **Review and revise**, **Follow up**, or **Wait for input**,
choose the conversation, review, timing, and limits, and check the plan before
you start.

Automations run only while Pipeline is open and the computer is awake.
**Keep running when the window closes** keeps Pipeline going in the
background. See [Automations](docs/automations.md).

## Data and privacy

Pipeline has no telemetry and needs no Pipeline account. AI providers receive
the prompts and material for the calls you run.

Your research is stored in `~/.pipeline/` unless you choose another
[research data folder](docs/settings.md#research-data-folder). Pipeline
encrypts saved API keys, but not reports, documents, conversations, or logs.
Uninstalling the app leaves your research data in place. See
[Privacy](PRIVACY.md).

## Current limitations

- Reviews are the most mature part of Pipeline, and macOS is the
  best-supported platform. See [Support](.github/SUPPORT.md) for Windows and Linux.
- PDF extraction can lose equations, table structure, or reading order. Check
  **Sources** when these matter, or see the
  [extraction options](docs/reviews.md#pdf-extraction).
- Each model's input limit caps document length, and your account's limits
  cap how many calls run at once. Past that cap, more parallel reviewers
  won't help.
- Conversation shell writes need an isolated working copy. Web search,
  command internet access, and browser interaction have separate settings.
  Browser interaction needs an optional browser download; Claude shell access
  on Windows needs WSL2. See [conversation access](docs/connections.md#conversation-access).
- Usage displays the latest account report and its time. Claude needs a reply
  before its first report is available; saved reports carry across app sessions.

## Command line and source builds

`pipeline-cli` runs reviews without the desktop app, using the same settings
and saved runs. The installer puts it beside the app but doesn't add it to
your `PATH`. See the [command-line guide](docs/cli.md).

To run the app from source, install Node.js 24.18.0 and Rust 1.97.1, then:

```bash
git clone https://github.com/mdroste/pipeline.git
cd pipeline/gui
npm ci
npm run tauri dev
```

`npm run tauri build` creates an installer. See [Contributing](.github/CONTRIBUTING.md)
for platform dependencies and tests.

## License

Pipeline uses the [MIT License](LICENSE). The bundled Poppler tools use the
GPL. See [third-party licenses](THIRD_PARTY_LICENSES.md).
