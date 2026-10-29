# Settings

Open **Settings** to change connections, model defaults, appearance, storage,
and notifications. Type in the search box to jump to a setting by name or
purpose.

| Section | What you can change |
| --- | --- |
| General | Theme, text size, spacing, editor font and wrapping, startup page, and guided setup |
| Connections | ChatGPT sign-in, Claude, API keys, and local model servers. See [Connections](connections.md) |
| Conversations | Send shortcut and automatic conversation titles |
| Reviews | Default models and reasoning, parallel calls, simultaneous reviews, review limits, timeouts, retries, and PDF extraction |
| Data & Storage | Research data folder, disk use, retention, cleanup, backups, and Trash |
| Notifications | Completion, failure, and attention notices, desktop notifications, and sound |
| Advanced & About | Logging, research tools, diagnostics, app version, and updates |

## Save changes

Most settings save as soon as you change them. API keys need **Save**, and
**Cancel** discards an unsaved key. A status note appears beside each setting
you change. If you leave a section with unsaved edits, Pipeline asks first.

Review defaults apply to reviews you start afterward. A saved workflow can
override them, and a running review keeps the settings it started with.

## Appearance and keyboard

By default, Pipeline follows your system theme, uses 100% text size and
comfortable spacing, and shows the editor in a 13-pixel font without
wrapping. You can also adjust the open editor or report without changing the
default.

Enter sends a message. If you prefer, choose Command+Enter on macOS or
Ctrl+Enter on Windows and Linux.

At startup, Pipeline reopens your last project or conversation.
**General → Startup → Guided setup** reopens the **Get started** guide.

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
API keys, provider sign-ins, installed parsers, and caches.

**Choosing a folder doesn't move your existing data.** To get back to earlier
work, choose the previous folder and restart. To move research to another
computer, use a backup or project package. See
[Back up and share](projects.md#back-up-and-share).

Use the folder on one computer at a time, and pause cloud sync while Pipeline
is open. If the folder goes missing, for example because an external drive is
unplugged, Pipeline shows an error. Reconnect the drive or choose another
folder, then restart.

## Storage and cleanup

The storage report under **Data & Storage** shows what uses disk space. Check
it before you change retention or clean up.

Retention limits save when you press Enter or leave the field. Enter `0` for
no limit. Press Escape to restore the saved value.

**Review cleanup…** shows which reports will move to Trash. Confirm to apply
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

## Updates

An installed release checks GitHub for a newer version about a minute after it
starts, then about once a day. When one is available, a banner offers:

- **Download**, which opens the installer for your computer in your browser;
- **What’s new**, which opens the release notes; and
- **Dismiss**, which hides the banner until the next version.

Run the downloaded installer to upgrade.

To stop the checks, turn off **Check for updates automatically** under
**Advanced & About**. **Check for updates** checks right away. Each check sends
GitHub your network address and app version, and nothing about your research.
Copies built from source check only when you ask.
