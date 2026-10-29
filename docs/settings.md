# Settings

Open **Settings** to change connections, model defaults, appearance, storage,
and notifications. Search by a setting's name or purpose to jump to its control.

| Section | What you can change |
| --- | --- |
| General | Theme, text size, spacing, editor font and wrapping, and startup page |
| Connections | ChatGPT sign-in, Claude subscription or API access, other API keys, and local model servers |
| Conversations | Send shortcut and automatic conversation titles |
| Reviews | Models and reasoning defaults, parallel calls, simultaneous reviews, per-Review cost, call, and time limits, timeouts, retries, and PDF extraction |
| Data & Storage | Research data folder, storage usage, retention, cleanup, backups, and Trash |
| Notifications | Completion, failure, and attention notices; desktop delivery and sound |
| Advanced & About | Logging, research tools and diagnostics, app version, and update checks |

## Save changes

Most settings save automatically. **API keys require Save**; Cancel discards
an unfinished key edit. Save status appears beside the settings you changed.
Pipeline prompts before leaving a section with unfinished edits or an operation
in progress.

Review defaults apply to later runs. A saved workflow can override them, and
changing settings does not change a run that has already started. Changing the
research data folder requires a restart and does not move existing files.

ChatGPT conversations and subscription Reviews use the same managed account.
API keys are separate. See [ChatGPT](chatgpt-account.md) and
[Claude Code](claude-code.md) for connection details.

## Appearance and keyboard

The defaults are system theme, 100% interface and report text size, comfortable
spacing, and a 13-pixel editor font with wrapping off. You can also adjust the
current editor or report view without changing its default.

Enter sends a message by default. Choose Command+Enter on macOS or Ctrl+Enter
on Windows and Linux if you prefer a separate send shortcut.

Pipeline normally restores the last project or conversation at startup.
Opening Settings does not replace that remembered destination.

**General → Startup → Guided setup** reopens the
[Get started guide](README.md#get-started-in-pipeline).

## Storage and cleanup

Choose a [research data folder](storage.md) under **Data & Storage**. Use the
storage report to inspect disk use before changing retention or cleaning up.

Retention limits save when you press Enter or leave a valid number field.
Enter `0` for no limit; an empty field is an unfinished edit. Escape restores
the saved value.

**Review cleanup…** previews the reports that will move to recoverable Trash.
Confirm the preview to apply it. Restore items from Trash or permanently
delete them when you no longer need them.

## Notifications

In-app notices for completion, failure, and required attention are on by
default. Desktop notifications and sound are off. Enable desktop delivery to
request permission from your operating system. You can also suppress notices
while Pipeline has focus.

**Test completion notification** uses your current category and focus settings.
Errors from actions such as saving remain visible even if notifications are
off. Notification text omits manuscript content, prompts, account details,
and private file paths.

Notifications and scheduled work require Pipeline to remain running. Closing
the window can leave it running if **Keep running when the window closes** is
enabled. These settings do not wake a sleeping computer.

## Updates

Pipeline installed from an official release checks GitHub for a newer version
about a minute after it starts and then about once a day while it runs. When
one is available, a banner offers **Download**, which opens the installer for
your computer in your browser, and **What’s new**, which opens the release
notes. Pipeline does not install updates itself; run the downloaded installer
to upgrade. **Dismiss** hides the banner until a later version is released.

Turn off **Check for updates automatically** under **Advanced & About** to stop
these checks. **Check for updates** always works. Each check sends GitHub your
network address and app version, nothing about your research. If GitHub is
limiting requests from your network, Pipeline waits as long as GitHub asks
before trying again. Copies built from source check only when you ask.

## Implementation notes

Device presentation settings use the versioned `pipeline.ui.preferences`
local-storage record; theme uses its existing key. Invalid values use defaults.
These settings are excluded from Review snapshots, credentials, research
archives, and conversation runtime configuration.

Review autosave serializes snapshots. API-key drafts remain local to their
fields until Save, and search preserves the mounted section and its drafts.
Conversation-title saves are serialized; only the newest response updates the
form. Storage changes use their own commands so a delayed Review-settings save
cannot overwrite them.

Notification handlers combine Review, conversation, automation, and research
events into completion, failure, and attention categories. They deduplicate
nearby events and release listeners on unmount. Stopped work is not a failure;
partial Reviews and batches with failed items use the failure category.

Desktop delivery uses the Tauri notification plugin. Only the explicit control
requests OS permission. Sound uses an in-app tone and remains subject to
browser audio activation and system policy.

`src-tauri/src/updates.rs` is the update-check facade. `updates/schedule.rs`
runs automatic checks only in builds compiled with
`PIPELINE_OFFICIAL_RELEASE=1`, and `updates/github.rs` makes the bounded
`releases/latest` request and selects this platform's installer by the names
`scripts/release/draft-release.mjs` publishes; a release test keeps them
aligned. The preference, last result, and rate-limit wait live in the
device-local `~/.pipeline/updates.json`. An unreadable file turns automatic
checks off until the researcher saves the setting again. `update_status` reads
that file without a request, `update_set_automatic` saves the preference, and
`check_for_update` checks immediately. Checks that find a newer version emit
`update-available` for the banner.

## Validation — 2026-09-09

Focused tests cover compatibility defaults, failed preference writes, search
opening advanced controls, private key drafts, numeric validation, serialized
saves, denied/granted notification permission, event categories, deduplication,
listener cleanup, and existing connection/storage behavior. The frontend build,
702 frontend tests, native compile, 32 Rust settings tests, and 60 release checks
pass. Formatting checks for changed frontend files, the source-size check, and
the repository's local Markdown-link check also pass.

Computer use could not attach to the unbundled Tauri development window on this
host. Native OS notification presentation, sound, keyboard focus, and visual
layout still require a manual development-app walkthrough in light/dark themes
at the minimum window size and increased text size. This is not packaged or
cross-platform release qualification.
