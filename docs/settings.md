# Settings

Open **Settings** to change connections, model defaults, appearance, storage,
and notifications. Search at the top of the Settings sidebar to find a setting
by name or purpose.

Opening Settings folds the main sidebar to icons. Leaving Settings restores
your previous sidebar preference.

| Section | What you can change |
| --- | --- |
| General | Theme, text size, spacing, editor font and wrapping, startup page, and guided setup |
| Models | Your default model, review-stage models, backup provider, ChatGPT sign-in, Claude, API keys, and local model servers. See [Models](connections.md) |
| Conversations | Send shortcut, model settings in one menu or separate selectors, and automatic conversation titles |
| Reviews | Parallel calls, simultaneous reviews, review limits, timeouts, retries, and PDF extraction |
| Data & Storage | Research data folder, disk use, retention, cleanup, backups, and Trash |
| Notifications | Completion, failure, and attention notices, desktop notifications, and sound |
| Advanced & About | Logging, research tools, command-line tools, diagnostics, app version, and updates |

## Save changes

Most settings save as soon as you change them. API keys need **Save**,
**Cancel** discards an unsaved key, and **Remove** deletes a saved one. A status
note appears beside each setting you change. If you leave a section with unsaved
edits, Pipeline asks first.

Review defaults apply to reviews you start afterward. Use **New review →
Advanced** to opt into saved stage and workflow model choices. A running review
keeps the settings it started with.

Choose each conversation’s provider, model, and thinking level with the
selectors below its message box. **Assistant settings → Manage agent profiles… → Access & inheritance** controls tools and
file access, including command setup. See
[Conversation access](connections.md#conversation-access).

## Default model

**Settings → Models → Default model** sets one provider, connection, model, and
thinking level for reviews and new conversations.

Choose **Anthropic**, **OpenAI**, **Google**, or **Local / custom** under
**Provider**. For Anthropic, **Connection** offers **Claude Code · Subscription**,
**Claude Code · Console billing**, and **Direct API · API key**. The description
below the field explains which sign-in or key it uses. OpenAI offers a ChatGPT
account or the OpenAI API. **Set up connection** opens the provider's setup.

- **New review** uses it as Primary for the whole review, including PDF model
  extraction. Add extra reviewers for independent opinions, or use **Advanced**
  to enable stage choices from **Settings → Models → Review models** and saved
  workflow pins.
- A new conversation copies it when it starts. Changing the default leaves
  existing conversations as they are, and you can change any conversation with
  the selectors below its message box.
- Automations keep the provider and model of the conversation they came from.

Until you choose a default, reviews use Claude and new conversations start on
ChatGPT. To set a thinking level, choose a model first. The connection list marks
any connection that isn't ready as **(not connected)**.

**Settings → Reviews → Step time limit** defaults to 45 minutes per model
call, including document bundle preparation. Choose a limit from 10 to 120
minutes. **Reviews → PDF extraction → Extraction time limit** separately
controls PDF reading, including retries, and defaults to 45 minutes. Choose
a limit from 5 to 120 minutes. Saved limits are preserved when you upgrade.

## Appearance and keyboard

By default, Pipeline follows your system theme, uses 100% text size and
comfortable spacing, and shows the editor in a 13-pixel font without
wrapping. You can also adjust the open editor or report without changing the
default.

In **General → Appearance & reading**, use the **System**, **Light**, or
**Dark** buttons for the default colors. Choose **Custom** to show the color
scheme selector without changing your current appearance. **System**, **Light**,
and **Dark** remain at the top of the list, with custom schemes below.
**Harbor** uses cool slate and ocean blue, **Parchment** uses warm paper and
amber, and **Iris** uses violet and plum. **Grove** adds sage and forest green,
**Ember** adds warm ash and copper, and **Lagoon** adds sea glass and teal.
Every custom scheme has light and dark variants. Selecting a theme applies its mode
and palette together, including matching editor colors, and saves it on this
device. **System** follows your device’s light or dark appearance using the
original Pipeline colors.

**Spacing** adjusts the room between controls and sections. Control labels
keep the same text size in **Comfortable** and **Compact**. Use
**Interface text size** to enlarge both text and spacing.

**Typeface** chooses Inter (the default), Instrument Sans, IBM Plex Sans, or
Source Sans 3 + Source Serif 4. The Source pairing uses sans-serif text for
the interface and serif text for reports. Changes apply immediately and are
saved on this device. All four choices work offline, including italics and
their supported character sets. Code editors and mathematical notation keep
their specialized fonts.

Drag the sidebar's right edge to adjust its width. Pipeline remembers the
width you choose. Double-click the edge to restore the compact default width.

Enter sends a message. If you prefer, choose Command+Enter on macOS or
Ctrl+Enter on Windows and Linux.

At startup, Pipeline opens **Home** by default. To reopen your last project,
conversation, or review, choose **Continue where I left off** under
**General → Startup**. With that setting, **Settings** and **Help** still return
to **Home** on the next launch.
**General → Startup → Guided setup** reopens the **Get started** guide.

## Local PDF parser and graphics cards

In **Reviews → PDF extraction**, install the **PaddleOCR-VL Full Parser** to
read PDFs on this computer. On supported 64-bit Windows systems, Pipeline
automatically downloads graphics card support and its recognition model after
the parser is installed. This can add up to 2 GB to the download. Existing
parser users receive the same setup after upgrading Pipeline.

**Advanced parser settings → Use graphics card** is on by default. Turn it
off before installation to use the processor. Unsupported graphics cards and
failed optional downloads leave the CPU parser installed. Settings shows the
setup result and offers **Install** or **Repair** to retry. Cancelled setup and
uninstalled graphics support stay paused until you install it again. Automatic
setup waits while reviews use the parser, and pauses if a review needs the
parser during a download. It resumes about 15 minutes later.

Graphics support is used only for the Full Parser's page recognition on Windows.
Page layout remains on the processor. PDF viewing, Poppler extraction, and cloud
models do not need it. Removing the Full Parser removes its managed graphics
runtime and models too.

## Research data folder

Pipeline keeps your research in `~/.pipeline/` unless you choose another
folder:

1. Open **Settings → Data & Storage → Research data folder**.
2. Choose an existing folder set aside for Pipeline.
3. Quit and reopen Pipeline.

If the folder already holds Pipeline data, the app opens it. If it's empty,
Pipeline starts a new, separate library there. **Use default folder** switches
back to `~/.pipeline/` at the next launch. **Open current folder** shows the
folder in use.

The research data folder holds conversations, imported documents, notes,
results, reviews, annotations, saved workflows, automations, and their logs.
Your manuscripts and datasets stay in your own project folders.

Some files always stay in `~/.pipeline/` on this computer: device settings,
API keys, provider sign-ins, installed parsers and caches.

**Choosing a folder doesn't move your existing data.** To get back to earlier
work, choose the previous folder and restart. To move projects and
conversations to another computer, use a research backup or project package.
Reviews and automations aren't included. See
[Back up and share](projects.md#back-up-and-share).

If you moved or copied a folder, choose it here and Pipeline updates its saved
file locations when it opens. If any files didn't come across, **Disk use**
keeps every research file until they're back. Temporary command files and
database backups can still be removed.

Use the folder on one computer at a time, and pause cloud sync while Pipeline
is open. If the folder goes missing, for example because an external drive is
unplugged, Pipeline shows an error. Reconnect the drive or choose another
folder, then restart.

## Storage and cleanup

The storage report under **Data & Storage** shows what uses disk space. Check
it before you change retention or clean up.

Retention limits save when you press Enter or leave the field. Enter `0` for
no limit. Press Escape to restore the saved value.

**Preview cleanup…** shows which reports will move to Trash. Confirm to apply
it. You can restore items from Trash, or delete them permanently once you no
longer need them.

## Notifications

Pipeline shows notices in the app when work finishes, fails, or needs your
attention. Desktop notifications and sound are off until you turn them on.
Turning on desktop notifications asks your operating system for permission.
You can also hide notices while Pipeline is in front.

**Test completion notification** shows a sample using your current settings.
Notices never include manuscript text, prompts, account details, or private
file paths.

Notices and scheduled work need Pipeline to be running. To keep it going
after you close the window, turn on **Keep running when the window closes** on
the **Automations** page. Pipeline won't wake a sleeping computer.

## Command-line tools

Under **Advanced & About → Command-line tools**, choose **Install command** to
use Pipeline from a terminal or script. This optional launcher uses the installed
app's engine and PDF tools, without another download. Settings shows commands
to try it and enable `pipeline-cli` in your terminal. Add `~/.pipeline/bin` to
your user `PATH` to keep it available in future terminals.

Choose **Repair command** after moving the app if the command stops working.
**Uninstall command** removes the user launcher and keeps your data, models,
and the existing command inside the app. See [Command line](cli.md) for examples.

## Updates

An installed release checks GitHub for a newer version each time it starts, and
about once a day while it stays open. When one is available, a banner offers:

- **Install update**, which downloads the update while you keep working;
- **What’s new**, which opens the release notes;
- **Check automatically**, which turns these checks on or off; and
- **Dismiss**, which hides the banner until the next version.

When the download finishes, choose **Restart now** to install the update. Pipeline
warns you first if restarting would discard unsaved changes or stop a running
review, conversation, or automation. Choose **Later** to keep working, and
Pipeline offers the update again the next time it starts.

On macOS, your computer may ask for an administrator password if your account
can't change the Applications folder. On Windows, a small installer window shows
its progress, and then Pipeline reopens.

If Pipeline can't install an update, the banner says why and offers **Try again**
and **Download instead**, which opens the installer for your computer in your
browser.

On macOS, if an update reports a read-only location, quit Pipeline and use Finder
to move Pipeline.app to Applications. If it's on a disk image, copy it to
Applications and eject the disk image. Reopen Pipeline from Applications and try
the update again.

To stop the checks, turn off **Check automatically** in the banner, or
**Check for updates automatically** under **Advanced & About**. **Check for
updates** checks right away. Each check sends GitHub your IP address and app
version, and nothing about your research. Copies built from source check only
when you ask, and you update them by downloading the installer.
