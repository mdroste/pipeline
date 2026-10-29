# Privacy and data flow

Pipeline stores research on your computer and sends material to the AI
providers you choose. It has no Pipeline account, cloud storage service,
analytics, or application telemetry. Provider accounts and local model servers
have their own data policies.

## What Pipeline stores

Research data goes under `~/.pipeline/` by default. You can choose another
[research data folder](docs/storage.md) in Settings.

| Data | Examples |
| --- | --- |
| Reviews | Input copies or extracted text, page images, figures, model responses, reports, annotations, logs, and usage |
| Projects and conversations | Messages, drafts, imported papers, sources, notes, claims, evidence, and saved command results |
| Automations | Definitions, schedules, progress, results, and activity history |
| Device settings | Preferences, API keys, provider sign-ins, installed parsers, model weights, and caches |

Reports, documents, conversations, logs, and exports are **not encrypted by
Pipeline**. Saved API keys are encrypted with AES-GCM; the encryption key is
stored separately under the same operating-system account. Protect that
account and its files: someone with access to both files can recover the keys.

Changing the research folder moves none of the existing files. Device
settings, credentials, provider session files, managed tools, and disposable
caches stay under the local `~/.pipeline/` directory.

## What reaches an AI provider

### Conversations

A conversation uses ChatGPT or your Claude Code subscription. Depending on
the task and enabled tools, it can send:

- your messages and recent conversation history;
- assistant instructions and selected project notes or evidence;
- document excerpts, page images, and file contents read by an allowed tool;
- tool requests and results, including local command output; and
- responses to approval requests or questions, plus model and usage metadata.

Importing a paper into a project does not by itself send the paper to a model.
Material is sent when selected as context or returned by a tool. Local command
outputs may therefore reach a provider later, even if the command itself ran
without network access.

ChatGPT conversations and subscription Reviews share a Pipeline-managed
account stored under `~/.pipeline/providers/chatgpt/`. Their native session
history is stored separately. Claude Code manages its own subscription
sign-in; Pipeline does not import its subscription credentials.

### Reviews and extraction

A Review sends each step's prompt, assigned document material, selected prior
responses, and tool results to that step's provider. Inspect the run preview
to see the provider and permitted inputs. A custom workflow may use several
providers in the same run.

API connections send requests to the configured OpenAI, Anthropic, Google,
or OpenAI-compatible endpoint. Subscription connections use the installed
Codex or Claude Code tool. The advanced legacy Codex connection uses the
terminal's account. Pipeline cannot change a provider's retention or training
policies.

Poppler and the optional PaddleOCR-VL Full Parser extract PDFs locally.
**Model extraction uses the selected model connection and can send document
content to a remote provider.** Choosing local extraction does not make a
subsequent remote Review local.

An OpenAI-compatible connection sends data to its configured URL. The label
“local model” does not ensure that the server runs on your computer.

### Automations

Automations use the same conversation, Review, and local-command connections.
Starting one permits its configured steps to run with the material and tools
shown in its preparation screen. Scheduled work can send requests while
Pipeline runs in the background.

## Other network activity

Pipeline may also:

- check GitHub Releases for an update;
- complete provider sign-in and retrieve available models or account limits;
- use web search when supported and enabled;
- query Crossref when literature search or a saved literature check is enabled;
- download the optional PDF parser and model weights when you choose Install;
- fetch a workflow from a URL you choose to import; and
- fetch repository updates when you choose **Check GitHub** or **Check host**
  for an attached project.

Search queries can contain terms from your research. Project Git checks use
your Git credentials and download repository updates; they do not commit,
merge, or push your working files. Local commands can contact other services
when you enable network access for them.

## Exports and sharing

Exported files are ordinary files at the location you choose. Anyone who
receives them can read their contents.

- A research backup (`.pwrx`) includes the research database, retained documents,
  conversations, and app-owned conversation files. It excludes credentials,
  native provider session homes, external project folders, and the separate
  Review and Automation stores.
- A project package (`.pwex`) includes selected research and referenced items.
  It excludes conversations, credentials, raw datasets, and execution settings.
- A replication capsule (`.pwrc`) can include selected scripts and data, along
  with instructions and expected results. Its preview lists included and
  missing inputs. It excludes credentials and execution permissions.
- Report and transcript exports include the content you select. They may
  contain unpublished research or private conversation text.

Check the export preview before sharing. Backups and packages are not
continuous synchronization between computers.

## Retention and deletion

Deleting a Review moves it and its saved files to recoverable Trash. Permanent
deletion removes the trashed copy. Archiving a project or conversation hides it
without erasing its records. Storage cleanup also previews what it will move
to Trash before you apply it.

Uninstalling Pipeline leaves its data behind. To remove all local Pipeline
data, quit the app, preserve anything you need, and delete `~/.pipeline/` and
any custom research data folder you used. Exports and original files outside
those folders remain where you saved them. Provider tools may also retain
their own account or session files.

Review connection records under `~/.pipeline/providers/workflows/attempts/`
can contain prompts, responses, paths, and usage. They are separate from
ordinary report exports and run deletion; include them when assessing or
removing local copies of research.

Deleting local files does not delete material already sent to a provider,
search service, or custom endpoint. Use that service's account controls for
retention and deletion. Before processing restricted material, check your
institution's rules and the selected provider account's terms.

## Reporting a problem

Remove confidential research and credentials from public bug reports. Report
security vulnerabilities privately using [the security policy](SECURITY.md).
