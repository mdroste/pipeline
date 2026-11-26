# Models

Pipeline sends your prompts to an AI provider that you connect. Set up at
least one connection under **Settings → Models**.

## Choose a connection

| Connection | What you need | Conversations | Reviews | Web search in reviews |
| --- | --- | --- | --- | --- |
| ChatGPT | A ChatGPT plan and the [Codex](https://github.com/openai/codex) command-line tool | Yes | Yes | Yes |
| Claude Code | A Claude Pro, Max, Ultra, Team, or Enterprise plan and [Claude Code](https://code.claude.com/docs/en/setup) | Yes | Yes | Yes |
| Claude Code (API) | Claude Code signed in to an Anthropic Console account | Yes | Yes | Yes |
| OpenAI API | An OpenAI API key | Yes | Yes | Yes |
| Anthropic API | An Anthropic API key | Yes | Yes | Yes |
| Google Gemini API | A Gemini API key | Yes | Yes | Yes |
| Local or custom server | The address of an OpenAI-compatible server | Yes | Yes | Yes |

Every conversation connection can use web search with a model that supports
tool calls. Turn on **Web search** in
**Assistant settings → Manage agent profiles… → Access & inheritance**. The table’s last
column applies to reviews, where you enable search on individual steps.

Each conversation remembers its provider. New conversations start on your
[default model](settings.md#default-model), or on ChatGPT until you choose one.
Switching providers gives the next provider recent conversation history. Automations use the conversation’s provider and saved model unless
a step selects another model.

In a conversation, open the model picker beside the message field. Choose a
connection from the dropdown at the top, then select a model. Longer lists
include **Search models**. Point to a model or use the arrow keys to read its
description. Choose an effort level from **Thinking**. Expand **Account & usage**
for ChatGPT account details, or **Claude usage** for Claude's usage reports.

ChatGPT and Claude Code use your subscription. If you're signed out,
Pipeline asks you to sign in again instead of switching to an API key.

## Web search in reviews

In the **Review designer**, select a step and turn on **Web search**.
For OpenAI API and local/custom servers, choose a model that can use tools.

For OpenAI API and local/custom servers, Pipeline sends search queries to Bing
and reads public HTML or text pages. It sends the results, source URLs, and
page text back to the selected model. No separate search key is needed.
A local model still needs internet access for these searches.

Each model call can search eight times and read sixteen pages, with
1 MB of returned web content in total. Search follows the step's time limit
and stops when you cancel the step or review. Private network pages and
website sign-ins are unavailable through these review tools.

If a local/custom server rejects a request with search enabled, choose a
model that supports tool calls or turn off **Web search** for that step.
The run log shows search errors.
Check the cited pages before relying on a review's claims.

## Connect ChatGPT

1. Install version 0.165.0 or newer of the Codex command-line tool.
2. Open **Settings → Models → OpenAI**. Under **ChatGPT account**,
   choose **Sign in to ChatGPT** and finish signing in in your browser.
3. For reviews, choose **ChatGPT account** under **Connection** on the same card.
   If OpenAI is your default provider, change **Connection** under **Default model**.
   For a conversation, choose **ChatGPT** in the conversation's model picker.

One sign-in covers both conversations and reviews. **Models & usage** lists
the models your plan offers and how much of your usage limit is left.

Pipeline keeps its own ChatGPT sign-in. Signing in with `codex login` in a
terminal doesn't connect Pipeline, so sign in inside Pipeline even if Codex
already works in your terminal. For troubleshooting reviews only, **Advanced
connection settings → Legacy Codex CLI** uses your terminal's account instead.

### Change or disconnect the account

Use **Change account** or **Sign out** on the same panel. The change applies
to conversations and reviews together. Wait for running conversations and
reviews to finish first, because Pipeline can't change the account while it's
in use.

The sign-in stays on this computer. It isn't included in backups or project
packages, and it doesn't move when you change the research data folder.

If the panel shows **Use Conversations account** and **Use Reviews account**,
conversations and reviews are signed in to different accounts. Choose one, or
sign in again.

### Recover an interrupted review

If a ChatGPT review call is cut off, for example by a lost connection,
Pipeline may not know whether ChatGPT finished it. It doesn't resend the call,
and it won't start new ChatGPT reviews until you resolve it under **Settings →
Models → OpenAI → Reviews**:

- **Check saved result** looks up what ChatGPT recorded for that call, shows
  it, and saves a copy. The text isn't added to the interrupted review.
- **Continue without it** closes the record so you can run reviews again.

The interrupted call still counts against your plan.

## Connect Claude Code

1. Install Claude Code using
   [Anthropic's instructions](https://code.claude.com/docs/en/setup).
2. In a terminal, run `claude auth login` and sign in with your Claude
   subscription account.
3. For reviews, open **Settings → Models → Anthropic** and choose
   **Claude Code · Subscription** under **Connection**. If Anthropic is your
   default provider, change **Connection** under **Default model**.
4. For a conversation, choose **Claude Code** in the conversation's model
   picker, then pick a model and a **Thinking** level.

Claude Code handles its own sign-in, and Pipeline never reads your Claude
credentials. Pipeline checks the selected billing mode before every Claude
process, including review calls, titles, and idea generation. Subscription
mode accepts only a Claude subscription. If the sign-in is missing, changed
to an API account, or cannot be verified, the request stops. Saved API keys
and API-billed backup providers cannot take over a subscription request.

Subscription use counts against your Claude plan, including any extra usage
you've turned on in your Claude account.

### Use Claude Code with API billing

Run `claude auth login --console` in a terminal to sign in to Anthropic
Console through Claude Code. Choose **Claude Code · Console billing** in the Anthropic
settings for reviews, or **Claude Code (API)** in a conversation's model
picker. On Windows, sign in inside WSL too if the conversation uses WSL.
Claude Code owns this login; Pipeline does not pass it the API key saved in
Settings. **Direct API · API key** and the **Anthropic API** conversation connection
continue to use that saved key.

This connection currently accepts the Console login that saves an API key
inside Claude Code. If the native login offers a choice, choose **Create an
API key**. Keyless Console profiles and cloud-provider logins are not yet
supported; an unrecognized login stops before a prompt is sent.

This mode bills your Console account separately from a Claude subscription.
An existing conversation keeps its saved connection even when you change the
review default. To switch billing for that conversation, change its connection
in the model picker. If Claude Code is signed into the wrong account type, sign in
again with the intended type before sending a request.

Claude Code can make multiple API calls inside one agent turn. Pipeline cannot
yet limit that loop's cost, so reviews and automations
with a dollar budget stop before starting a Claude Code API request. Use
**Direct API · API key** when you need Pipeline's estimated dollar budget. Call and
time limits still apply to native Claude Code runs.

### What Claude can do in a conversation

Claude uses the assistant instructions, selected project context, and research
tools enabled for the conversation. File, web, and command access use the same
settings as ChatGPT; see [Conversation access](#conversation-access).

On macOS, Claude uses the system command sandbox. On Linux, install
`bubblewrap` and `socat` to enable shell commands. On Windows, follow
[Claude shell on Windows](#claude-shell-on-windows).

**Claude usage** in the model picker shows saved reports for your current
account, including separate hourly and weekly windows when Claude reports
them. Each report shows its time. Reports survive app restarts. After a window
resets, Pipeline asks for a fresh report instead of displaying the old balance.
The first report arrives with a Claude reply. Other Claude sessions may have
used more since the displayed report.

Pipeline titles a new Claude conversation with one extra call to the
cheapest available model using the same billing mode, using short excerpts of the first message and
reply. Turn this off with **Name new conversations automatically** under
**Settings → Conversations**.

Each turn includes your most recent messages, up to about 20,000 words. Older
messages stay in the conversation but aren't sent.

Stopping a turn stops Claude and any research jobs it started. A turn
interrupted by quitting Pipeline stays in the history and isn't sent again.

## Conversation access

These settings apply to every conversation provider. In a conversation, open
**Assistant settings**, choose **Manage agent profiles…**, and then choose
**Access & inheritance**. Each setting has a column for **Built-in**, **Global**,
**Project** (this project), and **This conversation**. A column overrides the
ones to its left.

The **Access mode** menu in **Assistant settings** sets the same choice for this
conversation: **Read only** is **Inspect**, and **Allow edits** is **Edit**.

| Setting | What it permits |
| --- | --- |
| **Access mode → Inspect** | Read project files and explain findings. |
| **Access mode → Edit** | Create and edit project files using the method selected under **File changes**. |
| **File changes → Review copy** | Prepare a separate copy automatically when you send a message. Review changes before copying them into your project. |
| **File changes → Direct edits** | Edit the project directly with changes you can undo, and use configured research commands. General commands can only read project files. |
| **Internet access for commands** | Let commands reach the internet, for example to download data or install packages. |
| **Allowed command sites** | Restrict command internet access to listed domains, separated by commas. For example, `pypi.org, *.pythonhosted.org`. In ChatGPT and Claude Code conversations, blank means public internet. API and local or custom server conversations need at least one site: while **Internet access for commands** is on and this is blank, commands don't run. |
| **Web search** | Search the web and read public pages. With Edit access and a project, download files into new project paths. |

The assistant checks that a file has not changed since it read it before
saving an edit. Creating a file never replaces an existing one. Changes in
the main project appear in **Review edits** and can be undone.

**Review copy** is the default for Edit conversations. Pipeline prepares the
copy before your message starts and keeps it for subsequent messages in the
same conversation. It collects changes when the assistant finishes or stops.
Open [Review edits](analysis-and-writing.md#review-edits) to accept or discard
them, including files created by commands. After you settle the copy, your next
message prepares a fresh one from the project. Conversation history stays available.

Copies exclude Git metadata, build output, caches, and dependency folders.
Each file must fit within 32 MB, and the copy within 256 MB and 5,000 files.
If a project cannot be copied completely, select files in **Review edits**,
exclude large data folders in **Project settings**, or choose **Direct edits**.
Accept or discard an existing copy before switching to direct edits.
Automations and conversations started from **Review edits** keep their chosen file scope.

The shared web search tool sends queries to Bing and returns links and
snippets. Subscription providers can also use their native search tools. Read
the original pages before relying on search results.

Turning on **Web search** lets the assistant find and read public pages.
**Internet access for commands** controls command access separately.
For API and local or custom server conversations on Windows, the first networked
command after opening Pipeline asks for administrator approval for a temporary
firewall guard. Pipeline and its commands keep their ordinary permissions.
Declining approval prevents that command from running. Web search and page
reading remain available.
**Allowed command sites** applies to commands; it does not restrict web search
or page reading. Web downloads are limited to 32 MB per file. Pipeline keeps
the source URL, retrieval time, and file fingerprint with the download result.
Web page reads return up to 128 KB of text from a response of up to 2 MB.
These web tools accept public HTTP and HTTPS addresses on standard ports.

For pages that require signing in or interacting with a website, use your
browser and bring downloaded material into the project.

The ChatGPT model picker shows both usage windows when available. If refresh
fails, it shows a saved report for the signed-in account, with its time. Expired
reports ask for a fresh reading. These reports reflect account-wide use.

## Claude shell on Windows

Install [WSL2](https://learn.microsoft.com/windows/wsl/install) and set the
Linux distribution you want to use as the default. Inside that distribution,
install Python 3, `bubblewrap`, `socat`, and Claude Code. Make `claude` available
in `~/.local/bin`, `/usr/local/bin`, or `/usr/bin`. Sign in there with
`claude auth login`; this sign-in is separate from native Windows Claude Code.

Pipeline uses the default WSL2 distribution when these dependencies are ready.
It maps the project and working-copy folders into Linux and uses the same
access settings as on macOS and Linux. Windows projects must be accessible
from that distribution. Native Windows research commands in **Command settings**
continue to run on Windows. Without WSL2, Claude retains project and research
tools but has no shell.

## Provider terms

Anthropic lets a product run the unmodified Claude Code tool when:

- each user signs in and pays under their own Anthropic agreement;
- the product's developer accepts Anthropic's Commercial Terms;
- the tool and its sign-in methods are left intact; and
- the product never collects or passes on subscription credentials.

Pipeline runs the published tool and uses Anthropic's own sign-in. Anthropic
hasn't endorsed Pipeline. See
[Anthropic's product-hosting and authentication guidance](https://code.claude.com/docs/en/legal-and-compliance#can-customers-offer-claude-code-in-their-products).

## Add an API key

API keys work for conversations and reviews. The provider bills you for each call.

| Provider | Where | Field |
| --- | --- | --- |
| OpenAI | **OpenAI** card, under **Connection**, choose **OpenAI API** | **OpenAI API Key** |
| Anthropic | **Anthropic** card, under **Connection**, choose **Direct API · API key** | **Claude API Key** |
| Google | **Google** card | **Gemini API Key** |

For your default provider, select the connection under **Default model**;
the provider card links back to that field. Enter the key, then choose **Save**.
Pipeline stores keys encrypted on this
computer; see [Privacy](../PRIVACY.md). In a conversation, open the model picker
and choose **OpenAI API**, **Anthropic API**, or **Gemini API**. Review access
settings do not change an existing conversation’s connection. Gemini always uses
an API key.

## Use a local or custom server

Connect a server that supports OpenAI-compatible chat completions, such as
Ollama, LM Studio, llama.cpp, or vLLM. On the **OpenAI-compatible** card:

1. Enter the **Server URL**, such as `http://localhost:11434/v1` for Ollama.
2. Add a key under **Local API Key** if the server needs one.
3. Choose a **Model** from the list the server reports.

A saved key belongs to its server. Changing the **Server URL** removes the key,
so Pipeline asks first, and you can then add a key for the new server.

For a conversation, choose **Local / custom** in the model picker. Choose a
listed model or enter its name under **Model ID**.

Calls go to the address you enter. They stay on your computer only if that
address is on your computer. Use HTTP for a server on this computer and HTTPS
for a remote server. Model requirements are described below.

## API conversations

API conversations use your assistant instructions and recent history. They can
use enabled project, research, web search, and page-reading tools when the model
supports function calling. Paper images require a model that accepts images.

In a conversation with no project, OpenAI, Anthropic, and Gemini models can run
commands in the conversation's [private working folder](projects.md#work-with-files-outside-a-project).
**Local / custom** models get commands there once you choose **Allow edits** or
enable a tool. Until then they can chat without function calling.

Commands are included with Pipeline and work offline. They run in Bash on macOS
and Linux, and PowerShell on Windows. Pipeline restricts access to the selected
folder and a temporary folder, and limits commands to two minutes.

Linux needs `bubblewrap` and access to `/dev/fuse` for protected file access.
Some systems also require an administrator to permit user namespaces for Pipeline.
Command internet access also needs `slirp4netns`,
`iproute2`, `iptables`, and `util-linux`. Windows needs an updated Windows 11
installation with native ProcessContainer support. **Access & inheritance**
shows whether commands are available. Unsupported systems can still use
research commands configured in [Command settings](analysis-and-writing.md#command-settings).

Commands follow **Internet access for commands** and **Allowed command sites**.
API and local models need named sites when command internet access is enabled.
Direct connections outside those sites are blocked.

With **Access mode → Edit** and **File changes → Review copy**, commands can change
files in the automatically prepared copy. **Direct edits** uses Pipeline's
file tools for changes to the main project and lets you undo those changes.

Replies stream as they arrive; a custom server that returns a complete reply
shows it when the call finishes. Available **Thinking** choices depend on the
model. **Stop** ends the local wait and cancels running tools; the server may
still finish and charge for an accepted request. Pipeline does not resend
interrupted calls automatically.

See [Automations](automations.md) for scheduled turns and research discovery
using the same connection.

Automatic conversation titles use an extra call to the same API model. Turn
this off with **Name new conversations automatically** under **Settings →
Conversations**.

## Usage and cost

- **Subscriptions:** use counts against your plan's limits. The model picker
  shows what's left.
- **API keys:** the provider charges for each call. A review's preview
  estimates calls, tokens, and list-price cost. See
  [estimated use and limits](reviews.md#estimated-use-and-limits).
- **Account limits during a review:** turn on **Continue after an account usage
  limit** under **Settings → Models** to retry a call once with a backup
  provider you choose. Otherwise the call fails when the limit is reached.

## If a connection fails

Choose **Check connection** on the provider's card and read the message. For
common problems, see [Support](../.github/SUPPORT.md#common-problems).
