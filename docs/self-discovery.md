# Full self-discovery

Two research automations appear under **Automations → Research**:

- **Full self-discovery (supervised)** pauses at a reviewed project shortlist.
  The researcher chooses the projects, then research, drafting, review and
  ranking continue automatically.
- **Full self-discovery (unsupervised)** accepts the topic prompt, selects
  projects automatically and returns the papers and retained research. It does
  not ask follow-up research questions.

Both start directly with the displayed settings. An existing Project supplies
optional context and input files; otherwise Start creates a Project. Workspace
must have a usable configured model and account. The app must remain running
and the computer awake to execute work. Restart recovers saved state; ambiguous
native submissions are reconciled or blocked rather than replayed.

## Field adaptation

An orientation step identifies the fields, research genres, contribution forms,
available evidence, prerequisites and relevant standards. It selects validated
subject and method IDs from the existing automatic Paper Review catalog. A
specialty outside that catalog uses explicit standards with empty catalog IDs;
interdisciplinary research can combine several fields.

Prompts distinguish formal proof, computation, observation, qualitative evidence,
archival interpretation and synthesis. Quantitative or experimental requirements
are applied only when relevant. The same engine handles history, literary
studies, mathematics, sciences, engineering, medicine and social sciences.
This is field adaptation, not evidence that autonomous research quality has been
validated in every discipline.

## Pipeline and defaults

| Step or setting | Default and supported limits |
| --- | --- |
| Candidate generation | 75 proposals in batches of five; 50–100 configurable |
| Initial screening | Every proposal; eligibility, contribution, feasibility, overlap and uncertainty |
| Detailed assessment | Up to min(C, max(20, 2N)) eligible candidates; two independent reviewers, configurable 0–4 |
| Proposal revision | One revision followed by fresh assessment; configurable 0–2, requires detailed reviewers |
| Shortlist | N = 10, configurable 1–25; fewer if insufficient proposals qualify |
| Paper target | K = 5, configurable 1–9 and K ≤ N |
| Research | Up to eight investigation rounds, configurable 1–16; optional independent challenge after each |
| Paper review | Two fresh referees per manuscript version, configurable 0–4 |
| Paper revision | Up to two revisions, configurable 0–5; each changed version is reviewed again |
| Extra Review | Optional pinned structured Review profile, applied to the exact frozen Markdown artifact |
| Replacements | Unsupervised only: up to two eligible reserve projects, configurable 0–5 |
| Limits | 500 managed actions; 48 active hours; 30 minutes per action; seven elapsed days |

Proposal revisions retain the previous design and its objections. Shortlisting
uses the current assessed version. The supervised selection carries the exact
shortlist hash and an idempotent operation identity. It cannot exceed K, include
an ineligible ID, or advance automatically on timeout. Researchers can select
fewer projects. The original paper target remains visible in the final report.

Each paper has its own persistent author conversation and folder. Each referee
gets a fresh conversation with the research dossier and frozen manuscript;
proposal scores and the author transcript are omitted. Final ranking compares
assessed papers, and sound papers must rank above incomplete papers. Outstanding
major/fatal findings or incomplete/unknown/high-priority structured Review
coverage prevent the final assessor from marking a paper complete.

Counts are targets, not guaranteed scientific yield. Failed investigations,
negative results, unsupported claims and unavailable evidence remain explicit.
No step can manufacture data, laboratory observations, interviews, primary
source access or proof verification to meet a requested paper count.

## Sources, computation and outputs

Public literature acquisition is enabled by default for a newly created
Project. It uses the Workspace-owned Crossref service: up to eight oriented
queries with bounded metadata and abstract excerpts. Existing projects retain
their Library acquisition policy. This implementation does not discover and
download arbitrary datasets, paywalled papers or archives automatically.

Researchers can supply up to 20 individual files relative to an existing
Project root. Start captures their bytes and hashes. Each author receives those
snapshots in its isolated directory; reviewers receive immutable evidence and
the reviewed manuscript. Relative input paths cannot traverse outside the
Project, and each action's artifact capture is bounded at 64 MiB.

Computation is enabled by default in the isolated paper folders. Native commands
can use installed tools within the declared sandbox, with command networking
disabled. There are no new grants for legacy host execution profiles. The native
approval policy is `never` only for host-created discovery roles. Research
questions receive empty answers and requests to expand authority are declined;
interactive Workspace conversations retain their existing approval behavior.
Execution grants expire with the run and are revoked when it terminates or when
a Workspace archive is restored.

Each frozen manuscript retains Markdown, standalone LaTeX, BibTeX, evidence IDs,
limitations and the response to reviewers. A model may also produce a PDF and
other attachments using an available toolchain. Listed manuscript sources must
match returned sources; attachments are captured artifacts, not a host-certified
compilation or scientific validation. Markdown remains readable if compilation
is unavailable.

The paper reader displays manuscripts and reviews. ZIP export contains the
portfolio, all proposal versions, input snapshots, research records, manuscript
version history, latest paper files and captured evidence. Exports are bounded
at 256 MiB and verify stored artifact hashes. Native credentials and writable
runtime grants are excluded. A separate Markdown portfolio report is available.

## Ownership and implementation

- `orchestration/discovery/`: typed state machine, validation, prompts, durable
  child ownership, budgets, selection, result adoption and exports.
- `workbench/discovery.rs`: recoverable role/source setup, isolated roots,
  scoped execution grants, literature acquisition and autonomous request policy.
- `components/self-discovery/` and `lib/discoveryClient.ts`: lazy research UI,
  configuration, progress, selection and paper reading.

Coordinator schema 3 adds normalized portfolio records, owned child actions,
events and selection operations. Workspace migration 15 adds role grants. Child
actions use the existing Workspace, Review, snapshot and delivery adapters.
Roles stay out of normal conversation and automation-source lists. Existing
runtime/store/credential ownership remains as described in [Tasks](tasks.md)
and the [Workspace guide](workbench/README.md).

The broader [design record](self-discovery-design.md) also proposes features
not yet implemented: user editing of shortlisted briefs, replacement-shortlist
feedback, quality presets, automatic dataset/full-text acquisition and a
cross-field scientific evaluation program.

## Qualification

Deterministic tests cover the complete 50/75/100-candidate lifecycle, field
orientation, proposal revision, frozen-version re-review, soundness gates,
supervised selection identity, malformed output, durable adoption, interruption,
evidence IDs, isolated role roots, revocation and partial-setup recovery.

On September 9, 2026, the no-model probe passed on macOS arm64 with Codex CLI
0.153.4. It activated both production discovery profiles, retained `never`
approval through native thread start/resume, permitted scoped reads and author
writes, and denied referee writes and reads of private/sibling directories.
The existing transport, isolation and cleanup probe also passed. This does not
qualify authenticated model tool calls or research quality. See the
[release qualification requirements](workbench/release-qualification.md).

For reproducible native development tests, compile with `e2e` and set disposable
`PIPELINE_WORKBENCH_DEV_ROOT` and `PIPELINE_E2E_TASK_ROOT`, plus
`PIPELINE_E2E_DISCOVERY=synthetic-history-fixture`. The provider seam is absent
from release builds and labels responses as synthetic. It exercises real IPC,
coordinator persistence, artifact capture and UI without making model calls.
`gui/e2e/discovery-native-check.mjs` passed on September 9, 2026: both modes
completed 50-candidate portfolios with two reviewed papers; the supervised run
paused for its selection and the unsupervised run proceeded automatically.
The native paper reader and captured artifact hashes were also checked.
