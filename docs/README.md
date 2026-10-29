# Pipeline guides

Start with the [README](../README.md) to install Pipeline, connect a provider,
and run your first review. These guides describe the current source; the
[release notes](../CHANGELOG.md) list changes by version.

## Get started in Pipeline

On a first launch, Pipeline opens a short **Get started** guide. Reopen it
from Home (**Set up Pipeline**) or from **Settings → General → Guided setup**.
The guide changes no settings.

1. **What do you work with?** Choose the closest match, such as Stata and
   LaTeX or Word and Zotero. The answer only chooses which tips and tools the
   guide shows first. It is stored on this device.
2. **Connections** shows whether ChatGPT is signed in, whether Claude Code is
   installed and signed in, which API keys are saved, and whether a local model
   server is running. ChatGPT and Claude Code serve Conversations and Reviews;
   API keys and local servers serve Reviews only. Each row opens
   **Settings → Connections**.
3. **Tools on this computer** looks for Stata, R, Python, Jupyter, Julia,
   Quarto, and MATLAB only when you choose **Check this computer**. It looks in
   standard install folders and on your `PATH`, installs nothing, and does not
   start Stata or MATLAB. Found tools can run as
   [local checks](projects.md#run-local-checks) in a project. The step also
   shows the bundled PDF tools and whether the optional Full Parser is
   installed.
4. **Next steps** opens the example Review, **New review**, the **Reference
   Check** or **Replication Package Readiness** template in the gallery, or a
   new project.

The **example Review** (also on Home as **See an example Review**) shows sample
findings on a fictional paper excerpt, with severity, categories, cited
evidence, and quote checks. No model is called, and the example is not added to
Review history; decisions you make in it are not saved.

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
