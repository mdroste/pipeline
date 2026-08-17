# Privacy and data flow

Pipeline is a desktop application. The project does not operate a Pipeline
cloud service, require a Pipeline account, collect analytics, or include
application telemetry. That does **not** mean every workflow is local: the LLM
provider, extraction engine, and tools selected for a run determine which data
leaves the computer.

## What stays on the computer

Pipeline extracts documents locally before a run and stores application data
under `~/.pipeline/`. Depending on the workflow, this can include:

- settings and encrypted API-key values in `settings.json`;
- the local encryption key in `keyfile`;
- profiles, prompts, and credential-bound API model-catalog caches
  (model-selection policy is bundled with the signed application);
- full input copies or extracted representations, page images, figures,
  intermediate model responses, final reports, manifests, annotations, and
  logs under `runs/`; and
- optional managed extraction engines, model weights, and package caches.

API-key values are encrypted with AES-GCM before settings are written. The
encryption key is stored separately under the same user account with restrictive
filesystem permissions where the operating system supports them. This protects
against casual disclosure of `settings.json`; it is not a substitute for
protecting the operating-system account and the whole `~/.pipeline/` directory.

Deleting a run from History removes that run and its artifacts. Uninstalling
the application does not necessarily remove `~/.pipeline/`. To remove all
Pipeline data, first preserve anything needed and then delete that directory
manually.

## Data sent to an LLM provider

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
