# Pipeline

[![CI](https://github.com/mdroste/pipeline/actions/workflows/build.yml/badge.svg)](https://github.com/mdroste/pipeline/actions/workflows/build.yml)
[![Version 1.4.5](https://img.shields.io/badge/version-1.4.5-blue)](CHANGELOG.md)
[![License: MIT](https://img.shields.io/github/license/mdroste/pipeline)](LICENSE)
[![macOS](https://img.shields.io/badge/macOS-000000?logo=apple&logoColor=white)](#install)
[![Windows](https://img.shields.io/badge/Windows-0078D4)](#install)
[![Linux](https://img.shields.io/badge/Linux-FCC624?logo=linux&logoColor=black)](#install)

Pipeline is a desktop app for academic research. Use it to review a paper,
work on a research project with an AI assistant, or automate a sequence of
writing and review tasks.

[Download](https://github.com/mdroste/pipeline/releases/latest) ·
[User guides](docs/README.md) · [Changelog](CHANGELOG.md) ·
[Privacy](PRIVACY.md) · [Contributing](.github/CONTRIBUTING.md)

## Install

Download the installer for your computer from
[Releases](https://github.com/mdroste/pipeline/releases/latest).

See [Support](.github/SUPPORT.md) for system requirements.

On first launch, a short **Get started** guide shows which connections are
ready and finds research tools on your computer, such as Stata, R, and
Python. You can reopen it from Home.

## Connect a provider

Open **Settings → Connections** and set up at least one connection.

| Connection | What you need |
| --- | --- |
| ChatGPT subscription | Install [Codex](https://github.com/openai/codex), then choose **Sign in to ChatGPT** in Pipeline |
| Claude subscription | Install [Claude Code](https://code.claude.com/docs/en/setup) and run `claude auth login` with your subscription account |
| OpenAI, Anthropic, or Google API | An API key |
| Local or custom model server | An OpenAI-compatible server URL and model |

Choose the connection in the conversation’s **Model and thinking** picker.
Reviews and conversations list the thinking levels available for each model.
API and local connections can use project and research tools, web search,
and browser interaction. See [conversation setup](docs/connections.md#api-conversations)
for model requirements and the optional command engine.

Reviews support **Web search** with every connection. Enable it on individual
steps in the **Review designer**. OpenAI API and local/custom servers use
Pipeline’s search and page-reading tools with a model that supports tool calls.
See [review search](docs/connections.md#web-search-in-reviews).

Subscription use counts against your plan. If you're signed out, Pipeline
asks you to sign in again instead of switching to an API key. See
[Connections](docs/connections.md) for details.

## Data and privacy

Pipeline has no telemetry and needs no Pipeline account. AI providers receive
the prompts and material for the calls you run. Enabled web tools send queries
and page requests to search services and websites, including with local models.

Your research is stored in `~/.pipeline/` unless you choose another
[research data folder](docs/settings.md#research-data-folder). Pipeline
encrypts saved API keys, but not reports, documents, conversations, or logs.
Uninstalling the app leaves your research data in place. See
[Privacy](PRIVACY.md).

## Current limitations

- PDF extraction can lose equations, table structure, or reading order. Check
  **Sources** when these matter, or see the
  [extraction options](docs/reviews.md#pdf-extraction).
- In Edit conversations, Pipeline prepares a separate copy of your project.
  Use **Review edits** to accept or discard changes; accepted changes can be undone.
  Large projects may need a smaller selection of files or **Direct edits**.
- Searching the web, giving commands internet access, and letting the assistant
  operate a browser are separate choices in
  **Assistant settings → Manage agent profiles… → Access & inheritance**. Pipeline
  downloads command and browser software when needed. Web search needs no browser.
- API and local models can run general commands on macOS, or Linux with
  `bubblewrap` and `socat`. On Windows, these models can run research commands you
  configure, but cannot run general commands. Claude Code needs Windows
  Subsystem for Linux (WSL2) to run general commands on Windows.

See [conversation access](docs/connections.md#conversation-access) for setup,
and [Review edits](docs/analysis-and-writing.md#review-edits) for working with copies.

## Command line and source builds

`pipeline-cli` runs reviews without the desktop app, using the same settings
and saved runs. The installer puts it beside the app but doesn't add it to
your `PATH`. The [command-line guide](docs/cli.md) gives its location on each
system.

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
