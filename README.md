# Pipeline

[![CI](https://github.com/mdroste/pipeline/actions/workflows/build.yml/badge.svg)](https://github.com/mdroste/pipeline/actions/workflows/build.yml)
[![Latest release](https://img.shields.io/github/v/release/mdroste/pipeline?label=release)](https://github.com/mdroste/pipeline/releases/latest)
[![License: MIT](https://img.shields.io/github/license/mdroste/pipeline)](LICENSE)
[![macOS](https://img.shields.io/badge/macOS-000000?logo=apple&logoColor=white)](#install)
[![Windows](https://img.shields.io/badge/Windows-0078D4)](#install)
[![Linux](https://img.shields.io/badge/Linux-FCC624?logo=linux&logoColor=black)](#install)

Pipeline is a desktop app for academic research. Use it to review a paper,
work on a research project with an AI assistant, or automate a sequence of
writing and review tasks.

- **Reviews** ask several AI reviewers to assess a paper, slide deck, or grant
  proposal, then combine and check their comments. You can inspect each
  reviewer's response and the source material behind the final report.
- **Projects** keep manuscripts, sources, conversations, results, and review
  comments together. Work with ChatGPT or Claude Code to read, write, and
  analyze material from your project.
- **Automations** connect these activities. For example, draft a paper, review
  it, and revise it up to a limit you choose. Run once or set a schedule.

[Download](https://github.com/mdroste/pipeline/releases/latest) ·
[User guides](docs/README.md) · [Changelog](CHANGELOG.md) ·
[Privacy](PRIVACY.md) · [Contributing](CONTRIBUTING.md)

## Install

Download the installer for your computer from
[Releases](https://github.com/mdroste/pipeline/releases/latest).

macOS releases are signed and notarized. The Windows installer is unsigned,
so SmartScreen may show a warning. If you downloaded it from this repository's
release page, **More info → Run anyway** lets you continue. Each installer has
an accompanying SHA-256 checksum.

The app includes the Poppler PDF tools. Python is not required to use the app;
the optional PDF parser installs its own runtime. Subscription connections
require the provider's command-line tool, as described below. Pipeline checks
for new releases and lets you choose when to download and install them.

See [platform support and troubleshooting](SUPPORT.md) for more detail.

## Connect a provider

Open **Settings → Connections** and set up at least one provider.

| Connection | Setup | Available for |
| --- | --- | --- |
| ChatGPT subscription | Install [Codex](https://github.com/openai/codex), then choose **Sign in to ChatGPT** in Pipeline | Conversations and Reviews |
| Claude subscription | Install [Claude Code](https://code.claude.com/docs/en/setup), run `claude auth login`, and sign in with your subscription | Conversations and Reviews |
| OpenAI, Anthropic, or Google API | Enter the provider's API key and choose API access where offered | Reviews |
| Local or custom model server | Enter an OpenAI-compatible server URL and model; add a key if the server requires one | Reviews |

ChatGPT uses one Pipeline sign-in for conversations and Reviews. Claude uses
your Claude Code sign-in. Subscription connections use your provider account's
limits and billing settings; they do not switch to a saved API key if sign-in
fails. API access is a separate choice.

In a conversation, choose **ChatGPT** or **Claude Code** from the model picker.
For Reviews, choose the provider and model in the review settings or designer.
See the [ChatGPT connection guide](docs/chatgpt-account.md) and
[Claude Code connection guide](docs/claude-code.md) for setup and limitations.

## Run your first review

1. Choose **New review** from Home.
2. Select a PDF, DOCX, `.tex` file, or LaTeX project folder. Use LaTeX source
   when available to preserve equations and document structure.
3. Choose **Paper Review**, or **Paper Review (Fast)** for fewer reviewers.
   Choose **Slide Review** for a slide deck exported as PDF, or **Grant
   Proposal Review** for a proposal.
4. Check the provider, model, and PDF extraction settings, then choose
   **Start review** to open the run preview.
5. Inspect the planned steps and their inputs, then choose **Generate report**
   (or **Add to queue** when other work is running). When it finishes, read
   the report and open **Sources** to inspect the document and supporting material.

Paper Review selects subject and method specialists based on the document.
It combines their comments, then asks a final reviewer to check those comments
against the source. The report remains an AI assessment: check consequential
claims, calculations, and citations yourself.

Pipeline saves reports, individual responses, sources, and usage information.
You can annotate findings, compare runs, and track comments across drafts.
See the [review guide](docs/reviews.md) for PDF extraction, custom reviews,
batch runs, and troubleshooting.

## Work on a project

Create a project from **Projects**. Attach a folder to work with existing
manuscripts and code, or start without a folder to collect sources and notes.
A project has six main areas:

| Area | What you do there |
| --- | --- |
| Overview | Check the current manuscript, recent work, and items needing attention |
| Library | Read papers, manage sources, and browse project files |
| Analyze | Work with results, experiments, and research checks |
| Write | Edit the manuscript and review proposed changes |
| Automate | Set up repeated or scheduled work |
| Activity | Inspect notes, evidence, decisions, and their effects |

Conversations use the project context and tools you enable. You can customize
assistant instructions and configure local LaTeX, Stata, or Python commands.
See [projects and conversations](docs/projects.md) for getting started and
[editing files](docs/file-workspace.md) for the editor and PDF reader.

## Automate repeated work

Open **Automations** under Tools, or choose **Run an automation** from a
conversation's menu. Start with **Review and revise**, **Follow up**, or
**Wait for input**. Choose the conversation, review, timing, and limits, then
inspect the prepared automation before starting it.

Pipeline must stay running and the computer must stay awake. **Keep running
when the window closes** lets it continue in the background. See
[Automations](docs/tasks.md) for schedules, stopping and recovery, and
[research automations](docs/research-missions.md) for longer investigations.

## Data and privacy

Pipeline has no telemetry and requires no Pipeline account. Remote AI
providers receive the prompts and material needed for the calls you run.
Using a local model server keeps those calls local only when its URL points
to a service on your computer.

Research data is stored under `~/.pipeline/` by default. Reports, documents,
conversations, and logs are not encrypted by Pipeline; saved API keys are.
You can choose another [research data folder](docs/storage.md). Uninstalling
the app does not erase your research data. Read [Privacy and data flow](PRIVACY.md)
for storage, exports, deletion, and provider access.

## Current limitations

- Reviews have received more testing than project research tools and
  Automations. Much of the latter testing has been on macOS.
- On Windows, project files can be read, but accepting file edits and creating
  isolated task copies are not yet available. Reviews are unaffected.
- PDF extraction can lose equations, table structure, or reading order.
  Inspect **Sources** when these matter; see the [extraction options](docs/reviews.md#pdf-extraction).
- Model context windows and account limits constrain document size and
  concurrency. More parallel reviewers do not always make a review faster.
- Claude conversations run shell commands only in a read-only sandbox without
  network access, and not on Windows. They have no browser tools. Remaining
  Claude usage appears after Claude has replied in the current app session.

## Command line and source builds

`pipeline-cli` runs reviews without the desktop interface. It shares the app's
settings and saved runs. Installers place it beside the application executable;
they do not add it to `PATH`. See the [CLI guide](docs/cli/README.md).

To run the desktop app from source, install Node.js 24.18.0 and Rust 1.97.1,
then run:

```bash
git clone https://github.com/mdroste/pipeline.git
cd pipeline/gui
npm ci
npm run tauri dev
```

Use `npm run tauri build` to create an installer. See
[Contributing](CONTRIBUTING.md) for platform dependencies and tests, and
[Releasing](RELEASING.md) for the publication process.

## License

Pipeline uses the [MIT License](LICENSE). Bundled Poppler tools use the GPL;
see [third-party licenses](THIRD_PARTY_LICENSES.md).
