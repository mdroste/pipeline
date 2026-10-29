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

Web search is available in reviews only, on steps where you turn on **Web
search** in the review designer.

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

In a conversation, Claude can:

- use the assistant instructions, the project context you select, and the
  research tools you've turned on;
- list project files and read text files up to 256 KB;
- edit existing text files when **Allow edits** is on in **Assistant
  settings**. Claude must read a file before editing it, and the edit fails if
  the file changed in the meantime. Edits appear in **Review edits**, where
  you can undo them;
- in a conversation on a working copy, leave its edits there until you accept
  them; and
- run shell commands for quick checks and calculations, on macOS and Linux.

Shell commands can read the project folder, but not your home folder, other
projects, or Pipeline's data. They can write only to a temporary folder that
is deleted after each reply. They can't change project files or reach the
network, even when **Internet access for commands** is on. On Linux, the
shell needs `bubblewrap` and `socat` installed. Claude has no shell on
Windows.

Claude conversations have no web search, browser, or subagents.

**Claude usage** in the model picker shows the latest usage report from Claude
Code and when it arrived. It appears after Claude's first reply in the current
session.

Pipeline titles a new Claude conversation with one extra call to the
cheapest model your plan offers, using short excerpts of the first message and
reply. Turn this off with **Name new conversations automatically** under
**Settings → Conversations**.

Each turn includes your most recent messages, up to about 20,000 words. Older
messages stay in the conversation but aren't sent.

Stopping a turn stops Claude and any research jobs it started. A turn
interrupted by quitting Pipeline stays in the history and isn't sent again.

### Provider terms

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
