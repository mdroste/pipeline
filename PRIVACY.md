# Privacy and data flow

Pipeline is a desktop application. The project does not operate a Pipeline
cloud service, require a Pipeline account, collect analytics, or include
application telemetry. That does **not** mean every operation is local: the
Workspace account and permissions, or the provider, extraction engine, and
tools selected for a Workflow run, determine which data leaves the computer.

## What stays on the computer

Pipeline extracts documents locally before a run and stores application data
under `~/.pipeline/`. Depending on the orchestration mode and enabled features,
this can include:

- settings and encrypted API-key values in `settings.json`;
- the local encryption key in `keyfile`;
- profiles, prompts, and credential-bound API model-catalog caches
  (model-selection policy is bundled with the signed application);
- full input copies or extracted representations, page images, figures,
  intermediate model responses, final reports, manifests, annotations, and
  logs under `runs/`;
- Workspace's private SQLite store, immutable paper/source/artifact blobs,
  conversation transcripts, drafts, research notes, claims/evidence, execution
  receipts, recipes, and isolated Codex state under `workbench/`; and
- optional managed extraction engines, model weights, and package caches.

Workspace's managed ChatGPT sign-in is held in its isolated Codex home under
`~/.pipeline/workbench/`; it is separate from Workflow API keys and CLI state.
Workspace `.pwrx` archives include the validated research database, referenced
immutable blobs, and readable/structured transcripts. They exclude the
isolated Codex home and credentials. An exported archive or transcript is an
ordinary sensitive file at the path the user selected and is no longer
protected by Pipeline.

API-key values are encrypted with AES-GCM before settings are written. The
encryption key is stored separately under the same user account with restrictive
filesystem permissions where the operating system supports them. This protects
against casual disclosure of `settings.json`; it is not a substitute for
protecting the operating-system account and the whole `~/.pipeline/` directory.

Deleting a Workflow run from History removes that run and its artifacts.
Archiving a Workspace or conversation is reversible and does not erase its
records. Uninstalling the application does not necessarily remove
`~/.pipeline/`. To remove all Pipeline data, first preserve anything needed and
then delete that directory manually.

## Data sent to an LLM provider

### Workspace

Workspace currently communicates with OpenAI through a Workspace-owned Codex
App Server process and managed ChatGPT sign-in. Depending on the active harness,
permissions, and conversation, it can send user messages, effective developer
instructions, selected notes and evidence, paper/source excerpts or page
images, tool requests/results, approval or question responses, and operational
metadata such as model and token usage. Imported research material remains
local until it is selected as context or returned by an allowed tool.

Configured research execution runs locally, but its receipts, logs, adopted
artifacts, or structured results can later be returned to the model when an
enabled tool or context module requests them. Enabling command network access
can also allow an approved local command to contact third-party services. The
permission mode and pending approval must be reviewed before authorizing it.

Workspace does not inherit Workflow provider settings or fall back to another
provider. Native per-thread web search is currently disabled because the
required protocol control has not been qualified.

### Workflows

Before a run starts, review the selected provider, transport, profile, steps,
tools, and input allowlists. A remote-provider run can send the following to
the selected provider:

- system and step prompts;
- the document text, selected source, page images, figures, or named inputs
  allowed for that step;
- outputs from prior steps explicitly referenced by the step;
- tool requests and results; and
- operational metadata such as model name and token usage.

In API mode, Pipeline sends this material directly to the configured Anthropic,
OpenAI, Google, or OpenAI-compatible endpoint. In CLI mode, Pipeline launches
the installed Claude or Codex CLI; that CLI and its account settings
control transmission, retention, training, and tool behavior. Google requests
always use the Gemini API directly; Pipeline does not launch the Antigravity
CLI. Pipeline cannot
override a provider's policies. A “local” OpenAI-compatible endpoint is local
only if the configured URL actually points to a service controlled by the user.

Do not process confidential, embargoed, personally identifiable, export-
controlled, or otherwise restricted material until the applicable provider,
account, institutional policy, and data-processing terms have been reviewed.

## Tools and other network requests

When enabled for a step, web-search or other provider tools can transmit search
queries and receive content from third-party sites. Queries can contain terms
derived from the document or prompt.

Pipeline can also make these non-document network requests:

- check GitHub Releases for a newer Pipeline version;
- sign in to managed ChatGPT, fetch visible models and rate-limit state, and
  exchange Workspace messages and tool traffic through the isolated Codex App
  Server;
- query the selected provider's CLI or API for its available models and cache
  that discovery locally; signed, bundled policy metadata supplies role,
  pricing, and deprecation labels without a remote policy fetch;
- download the optional managed PaddleOCR-VL runtime and model weights after
  the user chooses Install.

The basic bundled Poppler extractor does not require a network request.

## Retention and deletion outside Pipeline

Deleting local artifacts does not delete data already sent to an LLM provider,
CLI service, custom endpoint, search provider, or downloaded-content host.
Their retention and deletion mechanisms are governed by their own account and
service terms. Consult those terms before processing sensitive material.

## Security reports

Do not put a paper, API key, access token, or other sensitive data in a public
issue. Follow [SECURITY.md](SECURITY.md) for private vulnerability reporting.

## Workflow Codex App Server preview

When selected, the Workflow App Server backend stores its independent ChatGPT
credentials and native history in `~/.pipeline/providers/workflows/codex/`.
Pipeline asks Codex to manage sign-in; it does not copy Workspace or legacy CLI
credentials. Per-call receipts in the adjacent `attempts/` directory retain
expanded prompts, reviewer instructions, artifact paths, model settings,
responses, and usage for recovery. These files may contain unpublished research.
They remain local and are retained until manually archived; they are not
included automatically in ordinary report exports. Unix permissions are
owner-only. See [the implementation record](docs/workflow-codex.md) for storage
limits and platform qualification status.
