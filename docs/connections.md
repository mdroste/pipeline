# Connections

Pipeline sends your prompts to an AI provider that you connect. Set up at
least one connection under **Settings → Connections**.

## Choose a connection

| Connection | What you need | Conversations | Reviews | Web search in reviews |
| --- | --- | --- | --- | --- |
| ChatGPT | A ChatGPT plan and the [Codex](https://github.com/openai/codex) command-line tool | Yes | Yes | Yes |
| Claude Code | A Claude Pro, Max, Team, or Enterprise plan and [Claude Code](https://code.claude.com/docs/en/setup) | Yes | Yes | Yes |
| OpenAI API | An OpenAI API key | No | Yes | No |
| Anthropic API | An Anthropic API key | No | Yes | Yes |
| Google Gemini API | A Gemini API key | No | Yes | Yes |
| Local or custom server | The address of an OpenAI-compatible server | No | Yes | No |

Turn on **Web search** in **Assistant settings** for a conversation, or on
individual steps in the review designer.

ChatGPT and Claude Code use your subscription. If you're signed out,
Pipeline asks you to sign in again instead of switching to an API key.

## Connect ChatGPT

1. Install the Codex command-line tool.
2. Open **Settings → Connections → OpenAI**. Under **ChatGPT account**,
   choose **Sign in to ChatGPT** and finish signing in in your browser.
3. For reviews, choose **Subscription** under **Reviews** on the same card.
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
Connections → OpenAI → Reviews**:

- **Check saved result** looks up what ChatGPT recorded for that call, shows
  it, and saves a copy. The text isn't added to the interrupted review.
- **Acknowledge and allow a new run** closes the record so you can run
  reviews again.

The interrupted call still counts against your plan.

## Connect Claude Code

1. Install Claude Code using
   [Anthropic's instructions](https://code.claude.com/docs/en/setup).
2. In a terminal, run `claude auth login` and sign in with your Claude
   subscription account.
3. For reviews, open **Settings → Connections → Anthropic** and choose
   **Subscription**.
4. For a conversation, choose **Claude Code** in the conversation's model
   picker, then pick a model and a **Thinking** level.

Claude Code handles its own sign-in, and Pipeline never reads your Claude
credentials. Pipeline checks the sign-in before each conversation turn and at
the first Claude call of each review. The check accepts only a Claude
subscription, not an API key, an Anthropic Console account, or a
cloud-provider account. If the check
fails, follow the error, which usually asks you to run `claude auth login`
again or update Claude Code.

Use counts against your Claude plan, including any extra usage you've turned
on in your Claude account.

### What Claude can do in a conversation

Each conversation remembers its provider, and new conversations start on
ChatGPT. An automation that uses the conversation uses the same provider. If
you switch providers partway through, the new one receives the recent
history.

Claude uses the assistant instructions, selected project context, and research
tools enabled for the conversation. File, web, and browser access use the same
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
cheapest model your plan offers, using short excerpts of the first message and
reply. Turn this off with **Name new conversations automatically** under
**Settings → Conversations**.

Each turn includes your most recent messages, up to about 20,000 words. Older
messages stay in the conversation but aren't sent.

Stopping a turn stops Claude and any research jobs it started. A turn
interrupted by quitting Pipeline stays in the history and isn't sent again.

## Conversation access

These settings apply to both ChatGPT and Claude Code. Open **Assistant
settings** in the conversation. Project and global settings supply defaults;
a conversation can override them.

| Setting | What it permits |
| --- | --- |
| **Access mode → Inspect** | Read project files and explain findings. |
| **Access mode → Edit** | Create files and edit existing files through the recoverable editor. Shell writes require an isolated working copy. |
| **Internet access for commands** | Let shell commands reach the internet. |
| **Allowed command sites** | Restrict command internet access to listed domains, separated by commas. For example, `pypi.org, *.pythonhosted.org`. Blank means public internet. |
| **Web search** | Search the web and read public pages. With Edit access and a project, download files into new project paths. |
| **Browser interaction** | Open and operate a separate, visible browser for this conversation. |

File reads return a content fingerprint. Edits fail if the file has changed
since it was read. New files can include new folders and never replace an
existing file. Changes in the main project appear in **Review edits** and can
be undone. Changes in an isolated working copy wait for acceptance, including
files produced by shell commands.

Web search, command internet access, and browser interaction are independent.
**Allowed command sites** applies to commands; it does not restrict web search
or the browser. Web downloads are limited to 32 MB per file. Pipeline keeps
the source URL, retrieval time, and file fingerprint with the download result.
Web page reads return up to 128 KB of text from a response of up to 2 MB.
These web tools accept public HTTP and HTTPS addresses on standard ports.

After enabling **Browser interaction**, choose **Install browser**. Setup needs
Node.js 22 or later and downloads Chromium. Linux may also need
[Playwright's browser system dependencies](https://playwright.dev/docs/browsers#install-system-dependencies).
Each conversation has its own browser profile, which remembers its sign-ins.
Enter passwords and complete CAPTCHA prompts directly in the visible window.
The browser closes when the turn stops; its profile stays on this device.
Browser downloads use the conversation's file download tool.

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
[Anthropic's product-hosting and authentication guidance](https://code.claude.com/docs/en/legal-and-compliance#can-customers-offer-claude-code-in-their-products),
checked October 28, 2026.

## Add an API key

API keys work for reviews only. The provider bills you for each call.

| Provider | Where | Field |
| --- | --- | --- |
| OpenAI | **OpenAI** card, under **Reviews**, choose **API** | **OpenAI API Key** |
| Anthropic | **Anthropic** card, choose **API** | **Claude API Key** |
| Google | **Google** card | **Gemini API Key** |

Enter the key, then choose its **Save** button, such as **Save Claude API
Key**. Pipeline stores keys encrypted on this
computer; see [Privacy](../PRIVACY.md). A saved key stays unused while that
provider is set to **Subscription**. Gemini always uses an API key.

## Use a local or custom server

Any server that offers an OpenAI-compatible interface works, including
Ollama, LM Studio, llama.cpp, and vLLM. On the **OpenAI-compatible** card:

1. Enter the **Server URL**, such as `http://localhost:11434/v1` for Ollama.
2. Add a key under **Local API Key** if the server needs one.
3. Choose a **Model** from the list the server reports.

Calls go to the address you enter. They stay on your computer only if that
address is on your computer. Some local models can't read attached files.

## Usage and cost

- **Subscriptions:** use counts against your plan's limits. The model picker
  shows what's left.
- **API keys:** the provider charges for each call. A review's preview
  estimates calls, tokens, and list-price cost. See
  [estimated use and limits](reviews.md#estimated-use-and-limits).
- **Account limits during a review:** turn on **Continue after an account usage
  limit** under **Settings → Reviews** to retry a call once with a backup
  provider you choose. Otherwise the call fails when the limit is reached.

## If a connection fails

Choose **Check connection** on the provider's card and read the message. For
common problems, see [Support](../.github/SUPPORT.md#common-problems).
