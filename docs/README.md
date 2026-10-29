# Pipeline guides

Start with the [README](../README.md) to install Pipeline, connect a provider,
and run your first review. These guides describe the current source; the
[release notes](../CHANGELOG.md) list changes by version.

## Using Pipeline

| Guide | What it covers |
| --- | --- |
| [Reviews](reviews.md) | Inputs, PDF extraction, included reviews, reports, and custom workflows |
| [Projects and conversations](projects.md) | Project setup, assistant instructions, research tools, and sharing |
| [Files and PDFs](file-workspace.md) | Editing, saved drafts, file comparisons, and the PDF reader |
| [Automations](tasks.md) | Review and revise, schedules, waiting for input, and recovery |
| [Research automations](research-missions.md) | Plan and investigate a question over several rounds |
| [Research discovery](self-discovery.md) | Generate, assess, and develop research proposals |
| [Settings](settings.md) | Connections, model defaults, appearance, notifications, and cleanup |
| [Research data folder](storage.md) | Storage locations, backups, and changing folders |
| [Command line](cli/README.md) | Run, validate, and share workflows without the desktop interface |

## Connections and help

- [ChatGPT](chatgpt-account.md): sign in and use the same account for
  conversations and Reviews.
- [Claude Code](claude-code.md): connect a Claude subscription and understand
  which conversation tools are available.
- [Support](../SUPPORT.md): supported systems and help with common problems.
- [Privacy](../PRIVACY.md): what Pipeline stores and what it sends to providers.
- [Security](../SECURITY.md): report a vulnerability privately.

## For contributors

[Contributing](../CONTRIBUTING.md) covers setup and checks.
[Releasing](../RELEASING.md) covers packaging and publication.
The following pages describe implementation details:

- [Workspace](workbench/README.md) and [feature modules](refactoring/module-map.md)
- [Review specialist catalog](auto-review-catalog.md)
- [Review ChatGPT connection](workflow-codex.md)
- [Project exchange and retention](workbench/project-exchange.md)
- [Release qualification](workbench/release-qualification.md)

Design proposals, dated audits, and validation records elsewhere in this
repository may describe planned work or earlier implementations. Use the guides
above for current behavior.
