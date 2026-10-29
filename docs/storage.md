# Research data folder

Pipeline stores research under `~/.pipeline/` by default. To use a different
location:

1. Open **Settings → Data & Storage → Research data folder**.
2. Choose an existing folder dedicated to Pipeline data.
3. Quit and reopen Pipeline.

If the folder already contains Pipeline records, the app opens them. If it is
empty, the app starts a separate research library there. **Use default folder**
selects `~/.pipeline/` for the next launch. **Open current folder** shows the
location the running app is using.

## What the folder contains

The research folder holds conversations, imported documents, notes, results,
Reviews, annotations, saved workflows, Automations, and their logs. Original
manuscripts and datasets stay in the project folders where you keep them.
Exports go to the location you choose when saving.

Device settings (including the update-check preference in `updates.json`), API
keys, provider sign-ins and session files, managed tools, and disposable caches
stay under the local `~/.pipeline/` directory. In
particular, the shared ChatGPT account remains under
`~/.pipeline/providers/chatgpt/`. Research backups exclude credentials.

## Change locations or move to another computer

Choosing a folder **does not move, merge, or delete existing records**. Select
the previous folder and restart to return to them.

Use a research backup (`.pwrx`) to restore the Workspace research store into
an empty destination, or a project package (`.pwex`) to transfer selected
items. Neither format copies all Pipeline data: Review and Automation stores,
external project files, and credentials are excluded. See
[backup and sharing](projects.md#back-up-or-share-research) for the differences.
Copying only the research database does not transfer its referenced files or
update paths for another computer.

This setting does not synchronize computers. Pause cloud sync while Pipeline
uses the folder, and do not open the same research store on two computers at
once. Project paths, installed tools, and execution permissions need to be
configured on each machine.

If a selected folder becomes unavailable, Pipeline reports the problem. It
does not create replacement records in the default folder. Choose an available
location in Settings and restart.

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
work on blocking workers. `StorageSettings.tsx` loads only in Data & Storage settings;
it is separate from provider autosave so stale settings snapshots cannot undo a
pending storage selection. The Workflow CLI uses the same process-pinned root.

Tests cover selection persistence, restart boundaries, missing folders, invalid
and corrupted settings, returning to default, existing-data preservation, folder
alias changes, local Workspace credentials, and the Settings picker/error flows.
Real cloud sync and cross-device session continuation are not qualified by these
tests or claimed by this feature.
