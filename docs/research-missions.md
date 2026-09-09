# Autonomous research automations

The first desktop implementation is available in **Automations → Research**. This
document describes the implemented contract and qualification limits.

## Product contract

A mission delegates a research question, completion criteria and bounded access.
Pipeline develops subgoals, compares candidate investigations, executes a selected
investigation, challenges its result in a separate conversation, and updates its
agenda. Refutations and well-supported negative results are useful outcomes.
The original research remit and authorization remain researcher-owned.

Research automations appear under Automations. Preparation shows the exact project, conversations,
permissions, available checks, experiment variants and limits. Starting that
prepared mission authorizes its child actions within those limits. Pause lets
active work settle; Stop cancels only the mission's child. Inputs and observed project
changes can wake a waiting mission. A brief retains findings, source references,
limitations, unresolved questions, activity and proposed reusable methods.

## Starting a research automation

1. Open **Automations → Research → New research automation** and choose a project conversation.
2. Choose a theory, empirical, quantitative, literature, discovery or maintenance
   starting point. Set the remit, current understanding and completion criteria.
3. Set limits on rounds, managed actions, active execution time, each action's
   duration, elapsed deadline and consecutive rounds without substantive progress.
4. Select existing authorized checks, prepared experiment variants, project
   monitors, an optional additional Review and any retained methods.
5. **Prepare research automation** creates a durable preview and scoped role conversations.
   **Start research automation** delegates the displayed work within its recorded limits.

Agenda shows the original criteria, evolving goals and selected investigation.
Findings retains alternatives, reasoning, negative outcomes and evidence. Decisions
collects researcher questions; answering one does not resume a paused mission.
Methods makes proposals available for explicit reuse in another mission in the
same project. Activity opens role conversations and child receipts. Brief renders
an evidence-indexed report; Markdown and complete JSON exports use native save
dialogs. Full retained evidence is inspected on demand rather than copied into
every list response. Model findings remain distinct from researcher acceptance.

## Ownership and execution

`orchestration/missions/` owns the typed agenda, pure planning/result validation,
mission tables, admission and child-task association. It extends the existing
coordinator rather than adding a second executor. `workbench/missions.rs` is the
Workspace-owned adapter for scoped role conversations, exact reference validation,
captured plans and app-owned change signals. Workflow reviews use the existing
adapter and pinned profiles. Neither adapter shares writable runtime state.

Planner and challenger conversations use inspect mode. Investigator editing is
available only when the source conversation already belongs to an isolated project
task and uses edit mode. Role presets omit host-execution tools; computation is
admitted as an explicit coordinator action. Native command/network permissions
remain enforced by the existing Workspace permission boundary. Mission controls
never accept manuscript edits or promote model assessments to confirmed evidence.

Captured experiments select from a finite set of prepared parameter variants.
Starting a mission authorizes only those exact captures. Changed launch identity,
inputs or grants require attention. A fresh execution directory still has the
existing `host_access` and `declared_only` semantics. Arbitrary new host scripts
need a separately reviewed capture; they are not authorized by mission prose.

The coordinator atomically associates each child with a mission and reserves its
action allowance before dispatch. Active execution time is accounted separately
from elapsed deadlines and intentional waits. All descendants use the same
mission budget. A recorded response is adopted once. Unknown outcomes are
reconciled through existing journals, never blindly replayed.

Monitor observations are scoped to the selected project and monitor IDs. A durable
attention cursor and latest-state snapshots detect observed changes, including a
return to a state seen before. Unchanged observations stay quiet. Missed checks
and intervening states may coalesce; this is not a filesystem event log. Disabled
monitors and acquisition permissions retain their owning service's behavior.

## Scientific state

Goal IDs and dependencies are validated, bounded and acyclic. Every proposed
investigation names its goal, uncertainty, possible outcomes, method and selection
rationale. The planner cannot mark a goal resolved. Challenge records separately
describe the outcome, tested domain, limitations, remaining checks and coverage of
the researcher's original completion criteria. Completion is an assessed mission
outcome, not a claim of machine-verified scientific truth.

Exact references are validated by their owning Workspace services; generated file
outputs are captured through the existing confined reader. Receipts remain
available for failed and inconclusive attempts. Repeated investigations and
stagnation have explicit bounds. Reusable methods are proposals with applicability
and limitations; they enter another mission only by explicit selection.

## Delivery ledger

- [x] Durable mission preparation, start/pause/stop/recovery and cumulative budgets.
- [x] Goal graph, next-investigation planner, isolated challenge and stopping rules.
- [x] Authorized checks and adaptive selection of captured parameter experiments.
- [x] Change-triggered maintenance, batched researcher questions and method memory.
- [x] Lazy mission interface, evidence-linked brief and exports.
- [x] Deterministic lifecycle/scientific-contract tests and UI tests.
- [x] Native development interface walkthrough with scripted model responses, plus real captured Python execution fixtures.

The first implementation uses the existing single native owner, including
close-to-tray. A per-user service with authenticated desktop IPC, remote execution,
OS wake support and concurrent native Workspace turns require distinct lifecycle
and protocol qualification. They are subsequent deployment capabilities; the
mission UI must state the actual awake-computer/desktop-process requirement.

## Qualification and remaining deployment work

The full current Rust run passed 903 tests (10 environment/qualification tests
ignored); the frontend run passed 587 tests. Production frontend build and Clippy
with warnings denied passed. Mission Rust files pass formatting checks; the full
repository formatting check still flags existing and concurrent edits outside the
mission files. These counts record this working-tree check, not a release gate.

Validation on macOS uses temporary stores and includes strict response contracts,
acyclic goals and resolved prerequisites, evidence-backed completion criteria,
failed-process classification, atomic child admission, budget exhaustion,
pause/adoption, deadlines while awaiting input, restart recovery, once-only
answers, schema migration backup, permission changes, exact source validation,
monitor reversions, and retained method scoping. The captured experiment fixture
executes two real Python parameter variants from their approved snapshot after the
live source script is changed. No Stata invocation is part of these tests.

`gui/e2e/mission-native-check.mjs` drives the actual Tauri development interface,
IPC, task journals, mission admission, evidence capture and completion. It uses
scripted provider responses for an analytical counterexample, completes three
child actions and checks method retention. The script does not make model calls.
Its provider seam is compiled only with both `e2e` and debug assertions and also
requires both disposable store overrides. Release binaries cannot enable it.

```sh
cd gui
PIPELINE_WORKBENCH_DEV_ROOT=/private/tmp/pipeline-mission-check/workbench \
PIPELINE_E2E_TASK_ROOT=/private/tmp/pipeline-mission-check/tasks \
PIPELINE_E2E_MISSION_SCRIPT="$PWD/e2e/fixtures/mission-cycle.json" \
TAURI_WEBDRIVER_PORT=4457 npm run tauri dev -- --features e2e
# With that development app running, in another terminal:
TAURI_WEBDRIVER_PORT=4457 node e2e/mission-native-check.mjs
```

Authenticated multi-round model behavior, substantive research quality, paid
provider limits, packaged restart/close-to-tray behavior, and Windows/Linux runtime
qualification have not been established by these fixtures. Apply the existing
[release qualification policy](workbench/release-qualification.md) before making
those release claims. Workspace phase extraction is additive: a single known
`final_answer` message is available as `finalText`; unknown/missing/ambiguous phases
fall back to the original concatenated `text`, which must independently satisfy
strict JSON parsing. No new native wire method or concurrent-turn permission is
introduced.
