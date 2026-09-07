# Research studio: PI-04–PI-10

Project **Research tools** contains Manuscript, Responses, Experiments, Result
links, Literature, and Theory. These are optional Workspace surfaces. Ordinary chat and
Workflows retain their independent stores, credentials and lifecycle.

## Implementation map

- `gui/src-tauri/src/workbench/project/studio/`: typed editor/build, response,
  experiment/result-binding, literature and theory services; shared operation
  receipts and workspace scope are in `studio.rs`.
- `gui/src-tauri/src/workbench/research/jobs.rs`: two-slot local queue, bounded
  log cursors, ownership and reconciliation; `execution.rs` separates prepare,
  process wait and finalization.
- `gui/src/components/research-studio/`: lazily loaded project tools;
  `gui/src/lib/studioClient.ts` is their app-owned Tauri contract.
- Migration 7 extends project record kinds, adds revision history and job
  ownership/adoption journals. Migration 8 adds the `theory`, `check` and
  `direction` kinds and recreates the same history trigger. Applied migrations
  1–7 are unchanged.

## Manuscripts and builds

The limited TeX/Markdown/BibTeX/text editor reads at most 2 MiB. Saves compare the
loaded hash, stage an independent task copy and use the existing recoverable
application journal for working-copy acceptance. An external edit refuses the
save. Task edits stay in the selected task. Source and simplified prose diffs
are distinct views; equations and citations remain inspectable in source.
Unsaved drafts are retained locally when browser storage permits. A restored
draft keeps its original hash, so reopening does not bypass a conflict.

Build forms configure a relative working directory, root TeX file, expected
PDF, declared inputs, engine and timeout. Current profiles support pdflatex,
xelatex, lualatex and latexmk. Effective TeX commands disable shell escape;
latexmk ignores ambient rc files. Advanced argv remains constrained to the
engine policy. Custom shell build-script qualification is still separate;
ordinary command profiles do not qualify arbitrary scripts as safe TeX builds.
Saving/importing settings never launches a command. Host authorization and a
successful test are required before normal runs.

A successful build retains its PDF, log and SyncTeX file. Failed builds never
expose a previous PDF as their successful output. Diagnostics retain warning
versus error severity and source/line references. Readers can compare retained
PDFs, navigate exact-build SyncTeX candidates, and explicitly mark pages
inspected. Compilation, diagnostics and page inspection are separate facts.
Missing mapping data or a missing SyncTeX executable leaves source/page fallback.
Accepting a changed subset still invalidates the task's checks and requires a
build of the accepted combination.

## Jobs and process ownership

The queue accepts at most 16 jobs and runs two at a time. A workspace filesystem
lock spans queuing, the process and finalization; conflicting managed writes are
rejected. Different projects can execute independently. A project with an
active detached job cannot start a native turn; starting a detached job during
an active native turn is rejected. Turn-owned host tools still return their
terminal receipt, while their asynchronous wait consumes no database worker or
archive gate.

Bookkeeping crosses the four-worker database boundary only for short prepare,
start, read and finalization operations. The process wait runs in a separate
blocking task. Standard output/error are each captured through a 4 MiB cap;
log cursors return at most 64 KiB. The UI keeps at most 200,000 displayed
characters. Duration is observed; CPU and peak-memory usage are not measured.

The job surface distinguishes queued, running, cancelling, completed, failed,
cancelled, timed out and unknown. The older execution receipt continues to use
`interrupted`/`outcome_unknown` for compatibility. Cancellation is a request,
not an early successful terminal receipt. Owned descendants are stopped before
capture threads join. `oldstata` gets its existing 30-second cooperative cleanup
grace; forced termination keeps cleanup verification visible.

UI launch explicitly selects detached ownership: jobs continue when a research
turn stops, while Pipeline stays open. Model-initiated executions are turn-owned
and stop with that turn or connection. Graceful app exit requests cancellation
and waits up to 35 seconds. This is not a daemon; there is no restart or machine-
sleep survival promise. Abrupt app/OS termination remains a qualification gate.

An operation ID with different arguments is refused. Repeated delivery returns
the original receipt. A persisted adoption journal can finalize retained outputs
after interruption, with blob-integrity checks; it never reruns the command.
Without that journal, the outcome stays unknown. Archive export/restore is
rejected while local jobs are active. Restored executable settings remain inert.

Declared inputs up to a combined 256 MiB are retained as immutable artifacts,
separately from their hashes. The process still reads live files: snapshot
consistency remains unverified, and undeclared/transitive dependencies are not
claimed captured. Input changes while queued prevent launch; before/after drift
or unchanged preexisting outputs prevents successful validation.

## Findings and response letters

The optional explicit bridge imports a version-1 exchange package of selected
canonical Workflow findings. Preview happens before Workspace mutation. It
retains run/step/finding IDs and a package snapshot; reimport preserves decisions
and retains changed source versions in history. The canonical Workflow ledger
is not edited. The two modes remain usable with the bridge disabled.

Local report imports propose paragraph splits with editable numbering and exact
byte spans. Response decisions separately retain category, severity, intended
response, task, exact manuscript revision, accepted application, execution,
evidence selections, draft, rationale, disputed premise, counterargument and
resolving check. Deferred or rejected comments are not scientific resolutions.

Letter preview flags absent links for declared/common “analysis added” wording,
empty drafts and unexplained defer/reject decisions. This checks declared links
and common wording, not every semantic assertion or the scientific relevance of
a selected execution. Flagged exports are visibly drafts. Markdown and escaped
LaTeX retain comment numbering and manuscript/execution/application references.
Focused re-review builds an immutable coverage document of selected passages,
comments and declared dependencies, then opens the ordinary Workflow preview.
It does not claim full-manuscript coverage or automatically launch a review.

## Experiments, results and manuscript bindings

Experiments retain a question, explicit completed baseline, intended change,
execution membership and editable interpretation. New alternatives never move
the project or experiment baseline. Specification/sample/calibration metadata
keeps declared versus inferred fields and their sources, with revision history.

The existing `research-results-v1` reader remains. Import reads a selected
adopted JSON output and replaces any self-supplied execution/locator fields with
host-owned identities. Version 2 adds a bounded impulse-response series:
variable, units, shock normalization, horizon unit, strictly increasing finite
horizons, finite-or-missing values, specification and sample IDs. No implicit
interpolation or unit conversion is applied. See the fixture exporters below.

Comparisons show estimates, uncertainty, sample sizes and specification/sample
metadata beside signed/absolute changes. Relative change uses the absolute
nonzero baseline as denominator only when the user declares it meaningful.
Alternative specifications require a rationale. Unit changes require a recorded
positive affine conversion; estimand/transformation mismatches remain blockers.
Missing/nonfinite values never become zero. Convergence, numerical agreement
and economic interpretation remain separate assessments.

Numeric bindings retain an exact result, manuscript selection, reported value,
precision, units, role and manual/proposed origin. Confirmation requires one
unambiguous printed occurrence. Coverage reports current/stale/unknown/unavailable
states, rounding/sign/unit discrepancies and changed declared dependencies.
Historical links keep their original result and manuscript. Unlinked research
claims have unknown numeric coverage. A selected TeX value macro can be staged
in a task through the ordinary change-set service; it is never silently applied.

## Literature and Zotero

BibTeX parsing preserves raw entries, file revision, keys, unknown fields and
macro expressions. Duplicate keys/versions stay distinct. Directives are retained
in the original file; the parser does not expand BibTeX macros or rename keys.
Literal TeX citation navigation links metadata and passage assessments.
Literature notes organize comparisons by question and retain exact source
version/quote/byte spans, identity checks, access level, support assessment and
method. Metadata-only records cannot be labeled passage-supported; abstract
access stays abstract. Related versions retain separate passages and judgments.

The [official Zotero local API](https://www.zotero.org/support/dev/web_api/v3/local_api)
supports GET reads under loopback port 23119 when local communication is enabled.
Pipeline uses only bounded GET requests, disables proxies and redirects, and
imports selected collection metadata after preview. Instance IDs and item
versions are retained; older clients without an instance ID also use content
hashes to distinguish imports. No write authorization, key storage, attachment
fetch or library write-back is implemented. The endpoint was unavailable on the
qualification machine; actual Zotero collection import remains a live gate.
Online source discovery/acquisition remains disabled pending capability
qualification. Local bibliography and source work needs neither service.

## Theory notes, checks and research directions

Theory notes are structured working records, not a proof assistant. Kinds are
assumption, conjecture, proposition, derivation, proof sketch, unresolved step,
counterexample and rejected approach. A note keeps a statement, optional
derivation prose, inline assumptions, links to assumption notes, saved passage
or equation selections, related notes and retained unresolved steps. Status
(open, supported, refuted, abandoned) is the researcher's disposition. A
derivation, proof sketch, proposition or conjecture with unresolved steps cannot
be recorded as supported; a rejected approach is always abandoned and requires a
reason; a proposed note stays open until reviewed. Abandoned approaches, their
assumptions and reasons enter the bounded automatic project context (at most 20,
reason truncated to 500 characters) so a later session can find them without
treating working notes as accepted decisions. A proposition linked to a proof
note opens both saved selections side by side in the reader.

Check receipts record method, outcome, summary, domain, tolerance, precision,
an optional completed execution, an optional recipe run and passages. Methods
are analytical argument, symbolic identity, numerical verification, numerical
counterexample, heuristic and model assessment. The host derives the evidence
scope: numerical methods must state the tested domain and are always
instances-only or a refuting instance; heuristics and model assessments never
establish a statement; only an analytical argument or a symbolic identity may
claim generality, and the analytical label states that it is researcher-recorded
rather than machine-verified. The overview label per note reports unresolved
steps first, then counterexamples, failed numerical checks, analytical
arguments, symbolic identities, instance-only numerical passes, or assessments.

Promotion inserts a note's derivation prose into a `.tex` or `.md` file inside
an isolated task copy, after a chosen line or at the end, wrapped in comment
markers that name the note, its revision, its referenced assumptions and any
unresolved steps. It uses the same staged-write and change-capture path as
manuscript saves, touches no other file, and records the checkpoint on the note.
Rejected approaches and unresolved steps are never promoted. Acceptance still
happens in Changes.

Research directions use one template: question, mechanism, closest known work,
minimal model or data, first discriminating test, likely failure mode and next
action, plus status (idea, active, converted, dropped), related theory notes and
a required drop reason. Unknown fields such as scores are rejected. Converting a
direction creates an ordinary task whose objective quotes the question and first
discriminating test and whose expected check is that test; it requires a stated
test and links the task to the direction. Five theory-check recipes (limiting
cases, dimensional check, accounting identities, comparative statics, numerical
counterexample search) are built in beside the earlier four; the last requires a
tested execution profile, and its instructions state that a search without a
counterexample supports the statement only on the instances tested.

## Fixtures and limits

`fixtures/research-studio/export_results.py` provides explicit OLS results and
an explicitly illustrative IRF. `export_results.do` provides Stata OLS results;
on this Mac invoke only through `oldstata`. Declare the script/input files and
`results.json` as profile inputs/outputs; also declare the Stata log.

The editor/BibTeX import limit is 2 MiB; finding/result exchanges are 4 MiB;
source passages are 64 KiB; a result export has at most 1,000 quantities/series,
a series at most 1,000 horizons, and record lists at most 500 objects. Theory
statements are 16,000 bytes, derivation prose 128 KiB, and each link list holds
at most 50 entries; promotion targets are files through 2 MiB. These are
bounded research tools, not an unrestricted dataset viewer or citation-manager
replacement. Qualification is recorded separately in
[release-qualification.md](release-qualification.md).
