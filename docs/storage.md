# Research data folder

Open **Settings → General → Research data folder**, choose an existing dedicated
folder, then quit and reopen Pipeline. Pipeline opens existing records there or
creates a separate research library if the folder is empty. **Use default
folder** selects `~/.pipeline/` for the next launch. **Open current folder** opens
the location used by the running process, including while a change is pending.

The selected folder stores Workspace conversations, research records, imported
documents, blobs and execution artifacts; reviews, annotations and Workflow
projects; Workflow profiles and prompt overrides; durable Tasks and research
missions; and preprocessing logs. Each service keeps its existing subdirectory
and ownership boundary. Original manuscripts and external datasets stay in their
registered folders. Manual exports retain their explicitly selected destinations.

App preferences, API keys, provider sign-ins and native session files, managed
tools, and disposable extraction/model caches stay under the device's local
`~/.pipeline/`. Workspace's native home remains
`~/.pipeline/workbench/codex/` even with custom research storage.

Changing folders does **not** move, merge, or delete existing data. Select the
previous location to reopen it. Workspace archive/exchange tools remain the
supported ways to transfer research records; simply copying its database to a
different path does not rebase internal references or transfer native sessions.

This setting does not implement cross-device synchronization. Live SQLite files
must not be replaced by a sync client while Pipeline is using them. Keep cloud
syncing paused while the app runs, and do not use the same research data on two
devices simultaneously. Registered paths and execution permissions remain
machine-specific. A Dropbox folder does not establish portable conversations,
distributed task ownership, or conflict handling.

## Implementation

`src-tauri/src/storage.rs` reads a small, bounded `~/.pipeline/storage.json`
containing `directory` (an absolute path or `null` for the default). The active
location is immutable for the process lifetime. Folder selection canonicalizes
the path, validates a writable directory, rejects overlaps with the local
settings tree or nesting inside the active data directory, and persists the
selection atomically. A failed selection leaves the prior configuration intact.
An unavailable custom folder never causes fallback writes or recreation of a
missing mount. Settings remains available to select a different folder, with a
restart required for recovery.

The `get_storage_settings` and `set_storage_directory` commands run filesystem
work on blocking workers. `StorageSettings.tsx` loads only in General settings;
it is separate from provider autosave so stale settings snapshots cannot undo a
pending storage selection. The Workflow CLI uses the same process-pinned root.

Tests cover selection persistence, restart boundaries, missing folders, invalid
and corrupted settings, returning to default, existing-data preservation, folder
alias changes, local Workspace credentials, and the Settings picker/error flows.
Real cloud sync and cross-device session continuation are not qualified by these
tests or claimed by this feature.
