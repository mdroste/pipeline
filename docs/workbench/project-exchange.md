# Project exchange, retention, and Workflow drafts: PI-11 and PI-12 (authoring half)

These are optional Workspace features under the research inspector's Release
tab. They do not change Workspace's runtime, credential, or cancellation
boundaries, and the whole-store `.pwrx` archive keeps its existing semantics.

## Implementation map

- `gui/src-tauri/src/workbench/release/exchange.rs`: selective `.pwex`
  export, inspection, import preview, import, and conflict review.
- `gui/src-tauri/src/workbench/release/retention.rs`: disk-use report,
  prune preview/apply into a journaled trash, restore, and empty.
- `gui/src-tauri/src/workbench/release/workflow_draft.rs`: portable Workflow
  drafts from completed research steps.
- `gui/src/components/WorkspaceExchangePanel.tsx` is the lazy UI; the typed
  client methods live in `workbenchClient.ts` and DTOs in `workbenchTypes.ts`.
- Migration 9 adds `exchange_imports`, `exchange_conflicts`, and
  `storage_trash`. Applied migrations 1–8 are unchanged.

## Package format

A `.pwex` package is a zip containing `manifest.json`, `README.md`,
`objects/<kind>/<id>.json`, and `blobs/<file>`. Kinds are the project record
kinds (task, anchor, checkpoint, application, build, response, experiment,
specification, series, binding, bibliography, literature, theory, check,
direction) plus `note`, `paper` (with revisions), `source` (with versions),
`execution` (with structured results), `claim` (with versions), and `evidence`
(with verification records). The manifest records the format and store schema
versions, a source namespace (`<store instance id>:<workspace id>`), the
workspace name and archived root, the selection, every object with a content
fingerprint, every blob with its SHA-256 and role, default exclusions, external
references, limitations, and a reproducibility statement. `README.md` lists the
same objects in plain Markdown, including a response matrix table, so the
package is interpretable without Pipeline.

## Selection and dependency closure

The selection names record kinds, whether to include notes, sources, and the
claim/evidence ledger, explicit papers and completed executions, and whether
to include compiled PDFs. Export walks every selected object's JSON and adds
any referenced object in the same Workspace: anchors pull their paper
revision, responses pull tasks and executions, bindings and checks pull
executions and results, literature notes pull source versions, and theory
notes pull related notes and assumptions. Adopted execution outputs and
evidence artifacts are packaged up to 64 MiB each. An execution's declared
inputs are never packaged; they are listed as external references with their
hashes left in the receipt, and the manifest states that the package does not
reproduce results on its own. Closure is bounded at 5,000 objects and 2 GiB.

Always excluded: conversations, transcripts, drafts and native bindings;
managed sign-in, Codex home and credentials; execution profiles, host
authorizations and other executable machine settings; harness presets,
Workspace settings and recipe runs; declared inputs and raw datasets;
performance samples, evaluations and Workflow runs. Captured stdout/stderr is
truncated to 64 KiB with a limitation note. Task copies and acceptance
journals travel as inert history that names the exporting machine's folders.

## Import

Inspection validates zip entry names, sizes, and counts, verifies every blob
hash, and rejects newer store schemas, unknown formats, and manifests that
disagree with the entries. The default target is a new separate Workspace named
after the package; merging into an existing Workspace is an explicit choice.
Import preview classifies every object as new, identical, remapped, or
conflict, and import records the plan in `exchange_imports`.

Identity rules: an object whose ID is unknown to the store is inserted as is.
An ID already present in the target Workspace is compared by content
fingerprint after every reference has been remapped; identical objects are
skipped, differing objects become open conflicts and the local version is kept.
An ID present in another Workspace of the same store is remapped to a
deterministic ID derived from the source namespace (and, if that also
collides elsewhere, the target Workspace), so reimporting the same package is
idempotent. Child IDs (revisions, versions, verification records) follow their
owner. Compiled PDFs and evidence artifacts resolve by content hash rather
than ID. Imported executions have no profile or session and never run;
imported evidence freshness is `unknown` with an explanatory reason. Imported
blobs are registered as `exchange_import` artifacts and retained-blob
references so pruning can never remove them.

Conflicts are reviewable in the panel. Keeping local records the decision;
taking the imported version replaces a project record or note body under a
fresh revision (the previous body stays in history). Papers, executions,
sources, claims, and evidence are immutable and can only be kept.

## Retention

The storage report separates immutable evidence blobs that any artifact, paper
or source text, evidence, exchange import, or context snapshot references from
unreferenced blobs, execution scratch, unreferenced turn-context files, and
pre-migration backups. Only the latter categories are disposable, and
conversation working files are retained. Execution scratch is unavailable while
a local job or Workspace turn is active. Storage mutations hold the native turn
permit and exclusive store gate so turn setup, new jobs and captures cannot race
a move. Pruning requires a current preview token covering selected categories,
candidate paths and file identities, including nested files. Category changes
invalidate the displayed preview. Applying it commits each `storage_trash`
recovery record before moving the item into `trash/<id>/`; reference checks still
precede every batch. Listing trash reconciles interrupted moves and restores.
Trash entries can be restored until trash is explicitly emptied. The database,
credentials, and Workflow runs are never touched.

## Workflow drafts (PI-12 authoring half)

A draft is built from completed tasks, completed recipe runs in the current
conversation, and non-abandoned theory notes, at most twelve. Each becomes a
Parallel step that reads the primary document text; a final Sequential
consolidation step selects every drafted report. The draft is validated with
the same portable-workflow parser Workflows use and returned as canonical JSON
with its fingerprint. Recipe runs that relied on local execution, tasks whose
checks name a run or build, and incomplete steps are listed as unsupported
prerequisites; prompt text never stands in for a job scheduler. Saving writes
an ordinary profile file that the Workflows page imports through its normal
validation and launch preview. Nothing is installed or executed from the
Workspace, and no provider, model, or credential choice is carried.

Concurrent Workspace turns, the other half of PI-12, are not implemented. The
one-active-turn invariant, compatibility record, storage/event contracts, and
live qualification matrix are unchanged.

## Limits

Packages: 5,000 objects, 2 GiB expanded, 256 MiB per entry, 64 MiB per blob,
20,000 entries. Conflict and trash listings return at most 500 and 1,000 rows.
Storage inspection stops at 200,000 filesystem entries.
