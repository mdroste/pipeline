# Connect ChatGPT

Conversations and subscription Reviews use one ChatGPT account in Pipeline.

## Sign in

1. Install the [Codex command-line tool](https://github.com/openai/codex).
2. Open **Settings → Connections → OpenAI → ChatGPT account**.
3. Choose **Sign in to ChatGPT** and complete the browser sign-in.
4. For Reviews, select **Subscription** in the OpenAI connection settings.
   For conversations, choose **ChatGPT** in the model picker.

Pipeline reads available models and usage limits from the connected account.
The same account panel also appears in conversation settings.

## Change or disconnect an account

Sign out or change accounts in the same panel. Finish active conversation and
Review work first: the account cannot change while it is in use. Signing out
affects both conversations and subscription Reviews.

Pipeline's account is separate from your terminal's Codex sign-in. Signing in
with `codex login` alone does not connect Pipeline's default subscription mode.
The **Legacy Codex CLI** option under advanced connection settings uses that
terminal account. Direct API mode uses its saved key.

## Upgrading an existing installation

Pipeline reuses a valid saved managed sign-in. If an older installation has
different Conversation and Review accounts, choose **Use Conversations account**
or **Use Reviews account**, or sign in again. Native histories stay in place.

Credentials remain on this device when you change the research data folder and
are excluded from research backups and project packages. See
[Privacy](../PRIVACY.md) for the storage details.

## Implementation reference

The account service and execution rules are described below. Review-specific
connection and recovery details are in [the Review adapter guide](workflow-codex.md).

### Ownership

`gui/src-tauri/src/agent_runtime/codex/chatgpt/` owns the account service:

- `mod.rs`: managed native connection, account leases, login/cancel/logout,
  runtime registration, and selection of an existing sign-in.
- `credentials.rs`: bounded private credential reads and one-time migration.
- `refresh.rs`: account-identity checks and serialized native token refresh.
- `commands.rs`: app-owned UI commands and account-change notifications.

`ChatgptConnection.tsx` provides the common account panel. Existing Workspace
and Workflow account commands delegate to the service for compatibility. Native
authentication refresh requests are intercepted inside the shared transport,
answered through a bounded handler, and never broadcast to conversation,
Review, approval, tool, or persistence event consumers.

```text
~/.pipeline/providers/chatgpt/
  account.lock          exclusive Pipeline-process ownership
  account-migration-v1  prevents re-import after intentional sign-out
  codex/                managed credentials and native account configuration
  empty/                account service working directory

~/.pipeline/workbench/codex/           Conversation native history/config
~/.pipeline/providers/workflows/      Review native history/config/receipts
```

The account service starts no threads or model turns. Codex owns browser OAuth,
refresh-token rotation, and the authoritative credential file. Pipeline reads
only this bounded local file to supply the access token, account ID, and plan
through the documented external-token interface. Neither execution runtime
receives the refresh token or persists the supplied credentials. Private file
reads reject symlinks and non-regular/oversized inputs; errors do not include
credential contents. Credential records have no `Debug` implementation and never enter
frontend DTOs, journals, or error data. No credentials are read from `~/.codex`.
Inherited provider keys, endpoints, and `CODEX_ACCESS_TOKEN` are removed from
native subprocess environments.

The external-token interface is experimental. The
[official App Server documentation](https://learn.chatgpt.com/docs/app-server#3c-log-in-with-externally-managed-chatgpt-tokens-chatgptauthtokens)
describes `chatgptAuthTokens` and the host refresh request. Both are present in
the checked-in 0.147.0 schema; the older snapshot labels the login type internal,
while the current public documentation describes host applications. The local
0.153.4 probe verifies admission. An incompatible runtime fails explicitly.

### Account changes and cancellation

A shared read lease spans each complete Review invocation and its uncertain-
outcome cleanup. Workspace stores its lease alongside the active-turn permit
through the identity-matched terminal event. Background Conversation calls also
hold a lease; a dropped or failed submitted call stops its native connection
before releasing account ownership. A native-call cancellation does not sign out
the account or stop the other mode's independently owned process.

Browser login holds the shared write lease until completion, cancellation, or
bounded cleanup. Sign-out and account selection require the same write lease.
Cancellation verifies the login ID and originating connection epoch. The UI
reads pending login identity from the service, so a remounted panel can cancel
an in-progress login. The old Workspace cancel command retains its ID-only
shape but checks that ID against the service's single pending login.

Before an account mutation, execution runtimes discard their old in-memory
authentication. If deauthentication cannot be confirmed, that runtime's
connection is closed. New work obtains credentials from the current managed
account. The execution homes, native threads, research stores, permission
profiles, Review journals, task adapters, and cancellation owners stay separate.

Token refresh checks the previous account ID and the account/user identity
before and after refresh. Refreshes are serialized. A refresh operation retains
its serialization and account lease even if the requesting runtime times out;
a failed or ambiguous managed refresh closes the account connection before
another attempt. Native response deadlines remain bounded. The transport does
not expose auth-refresh requests as model tools or user approval cards.

### Existing installations

On the first launch of the shared account service:

- One valid saved managed sign-in is reused automatically.
- Matching Conversation and Review account/user identities reuse the
  Conversation sign-in.
- Different identities produce explicit **Use Conversations account** and
  **Use Reviews account** choices. Signing in again is also available.
- Missing or unreadable legacy sign-ins require browser sign-in.

Migration copies only the selected managed credential file; native histories
stay in their existing homes. The sources remain untouched for compatibility.
The durable migration marker prevents old credentials from reappearing after
sign-out. The shared account and execution homes remain device-local when the
research folder changes and remain excluded from research archives/exchanges.
A second Pipeline process cannot concurrently own the managed account.

### Validation — September 15, 2026

On macOS arm64 with Codex 0.153.4:

- The full Rust suite passed 953 library tests and 14 CLI tests, with 11
  opt-in tests ignored. The full frontend suite passed 718 tests. The production frontend build,
  all-target/all-feature Clippy with warnings denied, Rust formatting, and
  source-size checks passed.
- The no-model shared-account test launched one managed account service and
  two independently owned native runtimes using synthetic credentials. Both
  execution runtimes accepted external-token login without writing auth files;
  sign-out was rejected while either held a lease, then cleared all three
  account states after both leases were released.
- Both existing native probes passed, retaining Review and Workspace filesystem
  restrictions, thread contracts, and process cleanup.
- Deterministic tests cover saved-account migration/conflicts, repeat startup
  after logout, bounded/symlink-safe credential reads, login lease lifetime,
  stale cancellation IDs/epochs, and private refresh routing outside product
  events. UI tests cover one account panel, account-change updates, cancellation
  after remount, migration choice, active-work errors, and Review recovery.

The shared-account native test can be repeated without a real account or model
turn from `gui/src-tauri/`:

```bash
cargo test --locked --lib agent_runtime::codex::chatgpt::tests::native_shared_account_installs_ephemeral_tokens_and_logs_out_both_runtimes -- --ignored --exact
```

Synthetic credentials establish native protocol admission and local account
coordination, not provider authentication. Browser completion, real provider
token renewal, authenticated model turns, packaged apps, and other operating
systems remain separate release qualification gates.

Automatic conversation titles run in a separate, temporary native process with
its own account-use lease. Failure, timeout, cancellation and successful cleanup
terminate only that side process; its temporary home and account lease remain
owned until cleanup settles. Foreground conversation process ownership is unchanged.
