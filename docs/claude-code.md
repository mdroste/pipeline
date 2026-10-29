# Connect Claude Code

Pipeline can use your Claude Code subscription for Reviews and conversations.
It runs the official Claude Code command-line tool with your own sign-in.

## Set up the connection

1. Install the command-line tool using
   [Anthropic's installation instructions](https://code.claude.com/docs/en/setup).
2. In a terminal, run `claude auth login` and sign in with your Claude
   subscription account.
3. For Reviews, open **Settings → Connections → Claude** and choose
   **Subscription**.
4. For a conversation, choose **Claude Code** in the model picker, then select
   an available model and thinking level.

The provider choice is saved for each conversation. Existing conversations
stay on ChatGPT until you change them. An automation that uses an existing
conversation uses that conversation's saved provider.

To use an Anthropic API key for Reviews, choose **API** instead and save the
key. Conversation access currently uses subscriptions only.

## Usage and sign-in problems

Subscription calls use your Claude account's limits and billing settings,
including any extra usage you have enabled. They do not fall back to a saved
API key when the subscription is unavailable.

Pipeline checks the sign-in before each conversation turn and on the first
Claude call in a Review. An API or Console account does not satisfy the
subscription check. If the check fails, follow the sign-in or update instruction
in the error, then try again.

Pipeline does not import your Claude subscription token. Claude Code completes
and stores its own sign-in.

## Conversation tools and history

Claude receives the assistant instructions, selected project context, and
enabled Pipeline tools. It can list and read project files. Editing existing
source files requires **Allow edits** and checks that the file has not changed.
Local commands use configured and authorized Pipeline research tools.

Pipeline keeps conversation messages and supplies recent text history on each
turn, up to its context limit. Switching providers supplies the same limited
history to the new provider. The Claude connection currently lacks native shell,
browser, and subagent tools, automatic titles, remaining-quota reporting, and
a project-wide provider default. Windows file editing remains unavailable.

Stopping a turn stops its Claude process and associated research jobs. An
interrupted turn remains in history and is not automatically sent again after
a restart.

## Provider terms

Anthropic publishes conditions for products that run the unmodified Claude
Code binary: users authenticate and pay under their own agreements, developers
agree to the applicable Commercial Terms, and the binary and its authentication
methods remain intact. Its guidance also prohibits collecting or intermediating
subscription credentials. Pipeline uses the published binary and Anthropic's
sign-in flow. This describes the integration; Anthropic has not endorsed
Pipeline. See [Anthropic's product-hosting and authentication guidance](https://code.claude.com/docs/en/legal-and-compliance#can-customers-offer-claude-code-in-their-products),
checked October 28, 2026.

## Implementation reference

The sections below document authentication checks, tool access, and validation.

### Subscription billing boundary

`agent_runtime/claude.rs` builds subscription commands for both owners. It runs
the resolved executable's `auth status` and requires a logged-in, first-party
Claude subscription with the native forced login method confirmed. API/Console/
cloud authentication, missing fields, unknown plans and failed status commands
stop the request. Project turns check before every prompt. A Review run checks
on its first Claude call, keeps the verified executable in its run scope, and
sends that run's later Claude prompts to the same executable; concurrent first
calls share one check, and a failed check is retried by the next call.

Both the check and model process remove inherited `ANTHROPIC_*` and `CLAUDE_*`
variables, including API keys, tokens, custom endpoints, cloud-provider switches,
and alternate credential homes. They disable user/project settings sources and
API-key helpers, and force `claudeai` for the invocation. No API fallback occurs.
A configured Review API key is used only when API mode is explicitly selected.
The status probe has an independent process owner, so Review cancellation cannot
kill a project's authentication check.

This prevents accidental API-key billing. It does not disable paid extra usage
that the user has separately enabled on their Claude subscription. Pipeline
does not claim that every subscription invocation is free of additional charges.

[Headless documentation](https://code.claude.com/docs/en/headless#start-faster-with-bare-mode)
says bare mode skips subscription authentication and is planned as a future
`-p` default. Pipeline does not enable `--bare`. If CLI authentication or protocol
behavior changes, the adapter must fail closed and be requalified; supplying a
key to restore subscription mode is not an acceptable fallback.

### Project execution contract

`workbench/claude/` owns project processes, streaming, cancellation and the
bounded MCP bridge. It shares only stateless authentication/process construction
with Reviews. It does not share Review run state, API keys, process registries,
or output roots. The existing Workspace turn permit covers either provider.

Each turn runs `claude -p` with stream-json input/output in a private temporary
folder. Pipeline retains messages and provider attribution in its own store.
Recent text history is supplied on each turn, bounded to 128 KiB and the store's
transcript window; truncation is disclosed in the supplied context. Switching
providers transfers that same bounded history. Native Claude session files and
OAuth credentials are not part of project archives. Stale accepted turns become
interrupted after restart and are never automatically replayed.

The model receives the resolved assistant instructions, selected context, and
only the captured Pipeline tool catalog. Native CLI tools, slash commands,
Chrome, hooks and discovered MCP configurations are disabled. The explicit
`pipeline` MCP bridge exposes enabled research tools and scoped project file
listing/reading. Existing source-file edits require Edit access, the current
content hash, and the Studio's recoverable edit transaction. Paths use existing
safe-root checks; a scoped role cannot redirect edits to the main project root.
Commands require configured and authorized research execution profiles.
Stopping a turn kills its process group and cancels its research jobs.

Current limits: no native shell/browser/subagents, automatic conversation titles,
Claude remaining-quota display, arbitrary filesystem mutations, or project-wide
provider default. Windows file acceptance keeps the Studio's existing fail-closed
restriction. These are capability limits, not a subscription prohibition.

### Validation — October 24, 2026

Qualification is recorded separately from policy permission. Focused tests cover
credential stripping and authentication rejection; model/effort admission;
provider routing; persisted history and interruption recovery; MCP catalog
boundaries; and project traversal, symlink, mode and stale-edit protection.

The opt-in no-model probe is:

```sh
cd gui/src-tauri
cargo test --locked --lib claude_subscription_handshake_probe -- --ignored --nocapture
```

It uses the production subscription command, context-file flags, stream-json
initialization and MCP configuration, then stops without submitting a prompt.
Run with access to the user's normal Claude Code sign-in. A sandbox that cannot
access the keychain is expected to fail authentication.

On macOS arm64, Claude Code **2.1.232** passed the initial production no-model
handshake. After the user signed in again, a final version-reporting probe on
**2.1.310** completed a live Haiku turn, called an inert MCP fixture, and returned
its verification marker.
The first failed turn was not retried by the application or sent to an API key.
This verifies the CLI/control transport; it is not a packaged GUI qualification.

```sh
cargo test --locked --lib claude_subscription_model_probe -- --ignored --nocapture
```

The live probe also passed with `ANTHROPIC_API_KEY=pipeline-invalid-test-key`
in the parent environment, verifying that the subscription command strips an
inherited key. Both the normal and planted-key probes used an inert MCP fixture,
not private project data.

Repository checks passed: 1,073 Rust tests across all targets (13 opt-in tests
ignored), 857 frontend tests, the production frontend build, Clippy with warnings
denied, Rust/Prettier formatting, Markdown links, and source-size regression checks.
The Rust suite needed access to the existing engine lock outside the sandbox;
the initial sandbox run's sole failure was that lock permission. Focused Claude
tests were rerun after the final reconnect/liveness fix.

The model probe consumes a small amount of subscription quota and sends only a
synthetic marker task. Full project UI/model workflows, packaged artifacts,
other CLI versions, and other platforms require their own evidence under [release qualification](workbench/release-qualification.md).
