# Refactored feature map (R0–R5)

Use this map to find an owner before reading an entire feature. Public APIs,
IPC command names, stored JSON, provider supervision and product runtime
boundaries are unchanged. The [implementation record](r0-r5-progress.md)
contains validation evidence; the [full plan](../refactoring-plan.md) tracks
later stages.

## Artifact Explorer

`gui/src/components/ArtifactExplorer.tsx` composes the Sources shell. Read:

- `gui/src/lib/artifactTypes.ts` for manifest/content/selection DTOs;
  `artifactClient.ts` for typed run-owned reads, also used by ReportWorkspace
  and the read-only file-workspace adapter.
- `components/artifact-explorer/useArtifactSelection.ts` for manifest-first
  loading, preloads, cancellation of stale results, selection and back history.
- `ArtifactTree.tsx`, `PageNavigator.tsx`, `navigation.ts`, `grouping.ts` for
  artifact grouping and compact/legacy page navigation.
- `ArtifactViewer.tsx`, `PdfArtifactPreview.tsx`, `ArtifactPreviews.tsx` for
  dispatch and preview fallbacks. The existing PDF.js/file workspace stays lazy.
- `DocumentBundleView.tsx` and `documentBundle.ts` for bundle presentation and
  equation-number coalescing. Format helpers are in `format.ts`.

Relative component filenames above are under `gui/src/components/artifact-explorer/`.
Run Sources still uses its owning manifest and read-only adapter; Workspace
file saves remain scoped to its own backend with expected-hash validation.

## Settings and workflow editor

`gui/src/components/SettingsPage.tsx` composes the selected settings section.
Under `components/settings/`, `useSettingsDraft.ts` owns settings loading,
immutable snapshots and one serialized autosave queue. `useProviderCatalogs.ts`
owns credential/transport-sensitive discovery, request identity and stale-data
invalidation; successful saves explicitly notify this owner. Navigation,
controls and each settings section are separate modules. Workspace account
state remains in its existing isolated connection component.

`gui/src/components/PipelinePage.tsx` composes the selected workflow editor.
Under `components/pipeline-editor/`:

- `useWorkflowEditor.ts` owns the profile-scoped draft, selection, schema
  validity, undo/redo and local recovery. Its replacement transition resets
  history when switching profiles.
- `useProfileOperations.ts` owns loading, mutation identity, saving, profile
  operations and save-before-export. It uses the draft owner rather than
  maintaining a second copy of the configuration.
- `useStepActions.ts` owns edit confirmations and step operations;
  `editorState.ts` projects extraction changes and shares dependency/artifact/
  condition cleanup between disabling and removing a step.
- `WorkflowNavigator.tsx` renders profile, step and schema navigation.
  `useWorkflowView.ts` holds shared visual selection, and
  `useWorkflowShortcuts.ts` keeps keyboard handlers current.
- `useWorkflowCatalogs.ts` loads the editor's persisted provider catalogs.
  Existing editor panels, schema editors and dialogs keep their prior roles.

## Workflow executor

`gui/src-tauri/src/pipeline/executor.rs` is the public facade. Its modules are
private to the executor feature; sibling imports name their dependencies.

| Module | Owner |
|---|---|
| `schedule.rs` | One dependency readiness algorithm used by preview and runtime; skip and dependency policies |
| `run.rs` | Mutable run state, completion ordering, resume, checkpoints and phase dispatch |
| `parallel.rs`, `sequential.rs` | Phase-specific dispatch; both use the supervised unit-call primitive |
| `step_call.rs` | Retry/validation flow, cancellation checks, bounded report ingestion and response journaling |
| `artifact_context.rs` | Private selector-filtered artifact views, staging and shared context preparation |
| `prompts.rs` | Bounded substitutions, evidence guidance and output-format instructions |
| `units.rs` | Deterministic file/artifact fan-out and agent expansion |
| `findings.rs` | Ordered finding lineage, validation dispositions and preserved-findings collapse |
| `checkpoints.rs` | Atomic completed/failed products, replacement/removal and run output budget |
| `outputs.rs` | Step output construction and schema-safe merge acceptance |
| `paths.rs` | Deterministic step identity and output directories |

`pipeline/provenance.rs` shares call-record construction with `merge.rs`.
Callers choose explicit roles and retain their existing accounting policy;
logical step attempts and provider attempts are distinct. Provider calls still
enter through `pipeline/call.rs`, with no changes to cancellation registries,
credentials, stores, or Workspace lifecycle.

Executor tests live in `executor/tests/{schedule,artifacts,findings,units,outputs,
prompts,placeholders}.rs`, with shared fixtures in `executor/tests.rs`. Other
large inline test modules moved beside their original source under
`<source-stem>/tests.rs`. Command and profile tests are grouped by behavior.

## Size reporting

Run `node scripts/source-size-report.mjs` from the repository root. It reports
source/tests above 1,200 lines or 50,000 bytes, long physical lines, and growth
relative to `source-size-baseline.json`. `--json` exposes the full inventory.
This is report-only: current debt does not fail unrelated work. Generated-file
exceptions, ownership and review triggers are in `scripts/source-size-policy.json`.
Do not refresh the baseline simply to hide new growth. R6 onward retain the
remaining large-file work; formatting exposed additional debt in WorkspacePage
and TasksPage, whose controller extraction remains R9.
