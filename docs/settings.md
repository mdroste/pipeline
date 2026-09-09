# Settings

Settings opens on General. The sidebar has seven categories; search matches
labels, descriptions, and synonyms, shows current values where loaded, and
opens the matching control, including controls inside collapsed Review stages
and parser options. Existing Workspace sign-in, API-key, model, extraction,
engine-install, and storage links retain their destinations.

## Inventory and ownership

| Category | Preferences and actions | Persistence and effective time |
| --- | --- | --- |
| General | System/light/dark appearance; interface size; report reading size; comfortable/compact spacing; source-editor font and wrapping; startup destination | Device webview preferences; immediate, except startup on next launch |
| Connections | Claude subscription/API, separate OpenAI conversation and Review sign-ins, Google API, compatible server endpoint/model/key, connection checks | Review settings/credentials and conversation account stores remain separate; ordinary Review fields autosave, API keys require Save |
| Conversations | Enter or Command/Ctrl+Enter to send; automatic titles, title model and reasoning | Send shortcut is device presentation state; title preferences use the existing Workspace command and private store |
| Reviews | Orientation, parallel, merge and sequential defaults; quota fallback; preferred provider; concurrency; timeout; retries; extraction method and parser options; revision comparison | Existing Review settings and field names; subsequent runs use saved defaults and retain workflow overrides |
| Data & Storage | Research directory; report-count/size retention; cleanup preview; extraction-cache reuse; research archive, restore, usage and Trash | Existing storage, Review and Workspace commands; directory selection requires restart and does not move files |
| Notifications | Completion, failure, attention, focus suppression, optional sound and desktop delivery | Device preferences; applied to subsequent events while Pipeline runs |
| Advanced & About | Verbose logging; research capabilities and diagnostics; installed version; manual update check | Existing owners; compatibility connection selection remains with the relevant connection and is searchable |

Device presentation preferences use the versioned `pipeline.ui.preferences`
local-storage record. Theme retains its existing key. Missing or malformed
preferences use defaults; numeric presentation values are bounded. A failed
write leaves the current preference unchanged and displays a local error.
These preferences are not embedded in Review snapshots, credentials, archives,
or conversation runtime configuration.

Default interface/report scale is 100%, editor text is 13px, wrapping is off,
spacing is comfortable, Enter sends, and startup restores the last project or
conversation. Review setup opens from Home after restart. Visiting Settings
does not replace the remembered research destination. The source editor and
report reader can override their defaults for the current view.

Review autosave retains the serialized immutable-snapshot queue. API-key drafts
stay in their field until Save; Cancel discards the draft. Search leaves the
current section mounted so it cannot erase unfinished input. Leaving a section
with an unfinished credential, numeric edit, or independent operation is guarded.
Conversation-title writes are serialized and only the newest result updates the
form. Save feedback appears at the relevant section; pending operations aggregate
only for navigation and never share writable stores.

Retention numbers commit on blur or Enter only when valid. An empty field is an
unfinished edit, not unlimited retention. Escape restores the saved number.
Explicit 0 means unlimited. **Review cleanup…** retains the existing exact preview
and confirmation token before moving reports to recoverable Trash.

## Notifications

The shell adapts Review terminal state, Workspace turn/request events, task and
mission notices, research attention events, and batch completion into three
notification categories. Cancelled/stopped work is not reported as a failure;
partial Reviews and batches with failed jobs use failure notifications. Duplicate
notices are coalesced briefly, with bounded memory. Listeners are released when
the shell unmounts. Existing task/mission dock alerts no longer bypass preferences.

Completion, failure and attention are on by default for in-app notices; desktop
delivery and sound are off. Focus suppression is opt-in. Direct responses to
user actions, such as a failed Save, remain visible regardless of outcome
notification preferences. Notification text uses generic outcome descriptions
and does not include manuscript content, prompts, account identifiers, or errors
containing local paths.

Desktop delivery uses the official
[Tauri notification plugin](https://v2.tauri.app/plugin/notification/), with only
permission-check, permission-request and notification-send capabilities enabled.
Only the explicit Desktop notifications control requests OS permission. Denial
leaves the preference off; background events never prompt. Test completion
notification respects the same category and focus controls. Sound uses a short
in-app tone; browser audio activation and system notification policy still apply.
Pipeline must remain running, including in its existing background mode; these
preferences do not introduce an OS scheduler or wake a sleeping computer.

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
