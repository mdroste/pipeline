# Privacy and data flow

Pipeline stores your research on your computer and sends material to the AI
providers you choose. There's no Pipeline account, cloud storage, analytics,
or telemetry. Provider accounts and local model servers have their own data
policies.

## What Pipeline stores

Your research is stored in `~/.pipeline/` unless you choose another
[research data folder](docs/settings.md#research-data-folder).

| Data | Examples |
| --- | --- |
| Reviews | Copies or extracted text of your documents, page images, figures, model responses, reports, annotations, logs, and usage |
| Projects and conversations | Messages, drafts, imported papers, sources, notes, claims, evidence, and command results |
| Automations | Definitions, schedules, progress, results, and history |
| Device settings | Preferences, API keys, provider sign-ins, installed parsers, model files, and caches |

Reports, documents, conversations, logs, and exports are **not encrypted by
Pipeline**. Saved API keys are encrypted with AES-GCM, and the encryption key
is stored separately under the same computer account. Anyone who can read
both files can recover your keys, so protect that account and its files.

Device settings, sign-ins, installed tools, and caches always stay in
`~/.pipeline/` on this computer, even if you choose another research data
folder.

## What reaches an AI provider

### Conversations

A conversation uses the connection you choose: ChatGPT, Claude Code, an
OpenAI, Anthropic, or Gemini API key, or a local/custom model server. Depending
on the task and the tools you allow, it can send:

- your messages and recent conversation history;
- assistant instructions and the project notes or evidence you select;
- document excerpts, page images, and file contents read by an allowed tool;
- tool requests and results, including output from local commands; and
- your answers to approval requests and questions.

A paper you import into a project goes to a model only when you select it as
context or an allowed tool reads it. Output from a local command can reach a
provider later in the same way, even if the command itself ran offline.
Switching providers sends recent history with your next message to the newly
selected provider. Automatic API conversation titles use an additional call
to the same model, with excerpts from the first exchange.

ChatGPT conversations and ChatGPT reviews share one sign-in, stored in
`~/.pipeline/providers/chatgpt/`. Claude Code keeps its own sign-in. Pipeline
checks that you're signed in but never reads or copies your Claude
credentials.

### Reviews and extraction

Each review step sends its prompt, the document material assigned to it,
earlier responses it depends on, and tool results to that step's provider.
The run preview shows the provider and inputs for each step. A custom
workflow can use several providers in one run.

API connections send requests to the OpenAI, Anthropic, Google, or custom
endpoint you set up. Subscription connections go through the Codex or Claude
Code tool on your computer. The advanced legacy Codex connection uses your
terminal's Codex account instead of Pipeline's sign-in.

Poppler and the optional PaddleOCR-VL Full Parser extract PDFs on your
computer. **Model extraction sends the document to the model connection you
selected.** A review after local extraction still sends material to your
review provider.

A custom model server receives data at the URL you entered. It runs on your
computer only if that URL points to your computer.

### Automations

Automations use the same connections as conversations, reviews, and local
commands. Starting one lets its steps use the material and tools shown in its
preview. Scheduled automations can send requests while Pipeline runs in the
background.

## Other network activity

Pipeline also connects to the internet to:

- check GitHub for a newer version about once a day, which you can turn off
  in Settings. GitHub receives your network address and app version, and
  nothing about your research;
- sign in to providers and fetch available models and account limits;
- send queries to Bing when **Web search** is enabled in conversations or
  in review steps using OpenAI API or local/custom servers. Search results
  and retrieved page text go back to the selected model;
- run searches enabled on other review connections through the provider's
  search tools. Subscription conversations may also use native search;
- read public pages or download files through enabled web tools, and contact
  sites you open through browser interaction;
- look up papers on Crossref, when you use literature search or turn on
  lookups in a reference check. Lookups send the reference text as written in
  your bibliography. If you add a Crossref contact email in the **Reading
  inbox**, it's sent with each lookup, as Crossref asks;
- download data from FRED, DBnomics, or the World Bank when you ask for it;
- download the optional PDF parser and its model files when you choose
  Install;
- download Chromium and its browser tools when you choose **Install browser**,
  or the sandbox package when you choose **Install command engine**;
- fetch a workflow from a URL you import; and
- fetch updates for an attached Git repository when you choose **Check
  GitHub**.

Search queries can contain terms from your research, including when the model
runs locally. Conversation browser profiles retain website sign-ins on this
computer after the browser closes. Enter passwords directly in that window.

Git checks use your own credentials and download updates only. Pipeline never commits, merges, or
pushes. Local commands can contact other services if you allow network access
for them.

## Exports and sharing

Exports are ordinary files saved where you choose. Anyone who gets a copy can
read it. Backups, project packages, and replication capsules never include
credentials or permission to run commands. Report and transcript exports
contain what you select, which may include unpublished research.

See [Back up and share](docs/projects.md#back-up-and-share) for what each
format includes. Check the export preview before you share a file.

## Retention and deletion

Deleting a review moves it and its files to Trash, where you can restore it.
Emptying Trash deletes it permanently. Archiving a project or conversation
hides it without deleting anything. Review cleanup and project storage cleanup
both show what they'll move to Trash before you confirm.

Uninstalling Pipeline leaves your data in place. To remove all of it, quit
Pipeline, keep copies of anything you need, and delete `~/.pipeline/` and any
other research data folder you used. Exports and your own files stay where you
saved them. Provider tools may keep their own account and session files.

Pipeline keeps a record of each ChatGPT review call in
`~/.pipeline/providers/workflows/attempts/`, so it can recover results after
an interruption. These records can contain prompts, responses, file paths,
and usage. Report exports leave them out. Deleting a run or emptying Trash
doesn't remove them, so delete this folder when you remove research.

Deleting local files doesn't delete material already sent to a provider,
search service, or custom server. Use that service's account controls. Before
you process restricted material, check your institution's rules and your
provider's terms.

## Reporting a problem

Remove confidential research and credentials from public bug reports. Report
security problems privately, as described in [Security](.github/SECURITY.md).
