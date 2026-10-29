# Research discovery

Research discovery develops a topic into proposals, investigations, and
reviewed paper drafts. Open **Automations → Research** and choose one of two
options:

- **Research discovery — choose projects yourself** pauses at a shortlist so
  you can select the proposals to develop.
- **Research discovery — let Pipeline choose projects** selects the proposals
  automatically and continues without follow-up research questions.

Enter the topic and review the settings before starting. An existing project
can supply context and input files; otherwise Pipeline creates one. A working
conversation model connection is required. Keep Pipeline running and the
computer awake while it works.

## How it works

Pipeline identifies the field, methods, available evidence, and type of
contribution. It generates proposals, screens them, and asks separate reviewers
to assess the strongest candidates. Selected proposals then move through
investigation, drafting, review, and revision.

Each paper has its own author conversation and folder. Referees get the
research record and a saved manuscript version, without the author's chat
history or proposal scores. Revised manuscripts receive fresh reviews. The
final report compares the papers and records incomplete work and unresolved
findings.

Prompts adapt to different fields and forms of evidence, including formal
arguments, computation, observation, archival sources, and synthesis. Research
quality across these fields has not been established by the software tests.
Treat the outputs as drafts and research records to assess.

## Settings and defaults

| Setting | Default | Available range or behavior |
| --- | --- | --- |
| Proposals | 75, generated in groups of five | 50–100 |
| Initial screening | Every proposal | Checks contribution, feasibility, overlap, and uncertainty |
| Detailed assessment | Two reviewers | 0–4 reviewers; assesses up to the smaller of the eligible candidate count and the larger of 20 or twice the shortlist size |
| Proposal revision | One round | 0–2; requires detailed reviewers |
| Shortlist | 10 proposals | 1–25; may be smaller if too few qualify |
| Paper target | 5 | 1–9, no more than the shortlist size |
| Investigation | Up to 8 rounds | 1–16; optional independent challenge after each round |
| Manuscript review | Two fresh referees per version | 0–4 |
| Manuscript revision | Up to 2 rounds | 0–5; each revised version is reviewed again |
| Additional Review | Off | Choose a saved structured Review for each captured Markdown manuscript |
| Replacement projects | Up to 2 | 0–5; available when Pipeline chooses the projects |
| Work limits | 500 actions, 48 active hours, 30 minutes per action, seven elapsed days | Review these limits before starting |

When choosing projects yourself, you can select fewer than the paper target.
Pipeline waits for your selection rather than advancing after a timeout.

These counts are targets. Missing data, inconclusive investigations, and
unresolved objections can leave fewer completed drafts. The report keeps those
outcomes visible.

## Sources and computation

New projects enable public literature search through Crossref by default.
Pipeline uses up to eight topic-specific queries and retrieves limited
metadata and abstract excerpts. Existing projects keep their Library
acquisition settings. It does not automatically obtain arbitrary datasets,
paywalled papers, or archives.

You can provide up to 20 files from an existing project folder. Pipeline saves
copies at the start, so later changes to the originals do not alter the run's
inputs. Each author receives those copies in its paper folder.

Computation is enabled by default in the isolated paper folders and can use
installed tools within the permitted file access. Command networking is
disabled. Discovery roles do not ask for broader permissions; requests outside
their scope are declined. Each action can retain up to 64 MiB of output files.

## Read and export results

The paper reader shows manuscripts and reviews. Each saved manuscript version
includes Markdown, standalone LaTeX, BibTeX, evidence references, limitations,
and a response to reviewers. A model may also generate a PDF if the required
tools are installed. Markdown remains readable when PDF compilation is
unavailable.

Export a Markdown portfolio report or a ZIP containing proposals, input copies,
research records, manuscript history, the latest paper files, and captured
evidence. ZIP exports are limited to 256 MiB and check stored file hashes.
Credentials and execution permissions are excluded.

## Implementation reference

The code and saved format use the name *self-discovery*. The following
sections document the implementation and its tests.

### Ownership and implementation

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

Not yet implemented: user editing of shortlisted briefs, replacement-shortlist
feedback, quality presets, automatic dataset/full-text acquisition and a
cross-field scientific evaluation program.

### Qualification

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
