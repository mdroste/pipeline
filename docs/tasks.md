# Automations

Automations run a sequence of conversation turns, Reviews, local checks, and
waits. Use one for repeated work such as drafting a paper, reviewing it, and
revising it a limited number of times.

## Start an automation

1. Open **Automations** under Tools, or choose **Run an automation** in a
   conversation's menu.
2. Choose **Review and revise**, **Follow up**, or **Wait for input**.
3. Choose the conversation, folder, timing, review settings, and limits.
4. Prepare the automation and inspect the work it will run.
5. Choose **Start automation**.

The **Active**, **Scheduled**, and **History** tabs show ongoing work,
schedules, and previous runs. Project **Action items** are separate research
to-dos; creating one does not start an automation.

## Review and revise

This template drafts a complete Markdown paper or captures an existing
document, reviews it, and revises it when another review round is available.
It stops when a complete review has no high-priority or unclassified findings,
or when it reaches the review limit. The final document is always a version
that was reviewed.

If a revision leaves the document unchanged, Pipeline asks for attention
instead of reviewing the same content again. The result includes the findings
and remaining limitations. Inspect these before treating the revision as ready.

## Set a schedule

Choose **Now**, **Later**, an interval, or a calendar schedule. Calendar
schedules let you choose weekdays and a time zone; the form previews the next
three occurrences.

Pipeline skips times that do not exist during the spring daylight-saving
change. A repeated autumn time runs once, at its first occurrence. Intervals
stay anchored to the first start you chose. If several occurrences are missed,
Pipeline combines them into one pending run. Runs from the same schedule do
not overlap.

Each occurrence gets its own conversation with the source conversation's
research settings. Changes to the project, folder, or assistant configuration
can require you to review the setup again.

**Pipeline must remain running and the computer must be awake.** Enable
**Keep running when the window closes** to continue through the tray menu.
Quitting stops execution. On reopening, Pipeline restores saved schedules,
waits, and progress; it checks interrupted work before deciding what can
continue. It does not wake the computer or install a separate background service.

## Wait for input, pause, or stop

An input step waits for your answer on the automation card. It uses no model
turn while waiting. Custom definitions can set a timeout and choose what
happens if no answer arrives.

**Pause** lets active work finish and prevents the next action from starting.
**Stop** cancels that automation's active work. Saved results remain available.

If Pipeline loses track of whether an action finished, it asks for attention.
Use **Reconcile recorded results** to look for a saved conversation response,
Review, or command result before retrying. The Activity log keeps earlier
attempts and their outputs.

A changed conversation may require **Use current conversation context** before
work resumes. This refreshes the conversation context; it does not change the
Review definitions saved with the automation.

## Research automations

Under **Automations → Research**, you can start a longer investigation with
separate planner, investigator, and challenger conversations. Set the research
question, completion criteria, tools, and limits. See
[Research automations](research-missions.md).

**Research discovery** starts from a topic, develops proposals, and produces
reviewed drafts. Choose projects yourself from a shortlist or let Pipeline
choose them. See [Research discovery](self-discovery.md) for settings and limits.

## Advanced definitions

The sections below describe custom automation JSON and its execution rules.
The format uses `task` and `chain` as technical names for an automation and its
steps.

### Defining chains

**Edit definition** exposes the versioned JSON format. Import and export chain
files, or save content-addressed versions in **Saved chains**. The authoritative
schema is [schema.json](../gui/src-tauri/src/orchestration/schema.json).

| Step             | Meaning                                                                         |
| ---------------- | ------------------------------------------------------------------------------- |
| `workspace`      | Submit a turn to the bound conversation without changing its draft              |
| `snapshot`       | Capture text, a scoped file, or a source/TeX folder as an immutable artifact    |
| `review`         | Run a pinned Workflow profile against a captured artifact                       |
| `check`          | Run a configured, tested, explicitly authorized Workspace execution profile     |
| `capturedCheck`  | Run an exact authorized captured plan (`planId`) in a fresh execution directory |
| `deliver`        | Copy a bounded immutable result into the conversation's task exchanges          |
| `delay`, `until` | Suspend until an elapsed duration or absolute UTC timestamp                     |
| `input`          | Suspend until a researcher supplies input, optionally with a timeout            |
| `if`             | Select and persist one branch                                                   |
| `repeat`         | Run a bounded body, testing the stop condition after each iteration             |
| `while`          | Test before entering each bounded iteration                                     |
| `parallel`       | Run independent branches and join before the next step                          |
| `forEach`        | Process a captured bounded input array, exposing `item` and `index`             |
| `chain`          | Embed a versioned chain definition                                              |

Step IDs are unique throughout the definition, including embedded chains.
Bindings select a literal, a named input, a step output with a JSON pointer, or
the first available binding. Workspace prompts substitute only explicit
`{{input:key}}` and `{{output:stepId#/pointer}}` references. Conditions support
equality, numeric less-than, existence, all, any, and negation. Missing or
mistyped values evaluate to unknown and need attention; they never silently pass
a stop rule. Conditions do not execute JavaScript, shell commands, or model prose.
Within a loop, an output binding selects that step's latest completed iteration;
every earlier receipt and full output remains in history. Parallel branches must
be independent; put dependent steps in the same sequential branch.

Validation checks output availability in execution order, including references
inside Workspace prompts. It rejects self/forward dependencies and references
between sibling parallel branches. Outputs become eligible after their parallel
join. A repeat's stop condition can read its completed body; optional outputs
from earlier iterations require an existence guard or an available fallback.
The built-in Review-and-revise template uses that guarded pattern. Conditional
or empty-loop outputs still require runtime value checks when their path did
not run.

The default template revises as Markdown. File-based chains can ask Workspace to
edit a declared file in its existing working copy, capture that file or folder,
and use `latex_project` or `source_tree` according to the selected Review profile's
input contract. The coordinator honors existing isolated research-task sessions
and their native roots; it does not apply their proposed edits to original files.
Use the existing Edits acceptance interface for that operation. Legacy host check
grants remain bound to their exact inputs; the task coordinator cannot authorize
a check or relax task-copy restrictions. A `capturedCheck` is pinned by its full
plan hash and exact existing grant, including declared inputs and parameters.
Changed capture identities require attention; a fresh directory retains host
access and declared-only dependency coverage. Mission preparation is the explicit
review surface for granting its finite set of captured variants.

Enable the optional **Task chains** harness module to let Workspace propose
chains in conversation. `workbench_task_catalog` exposes the schema and profile
identities; `workbench_task_propose` records a local proposal. Neither starts
work nor grants permissions. Open the resulting proposal card to resolve its
exact scope and start it. Plain conversations load no task tool catalog.

### Runtime boundaries and recovery

`src-tauri/src/orchestration/` owns the durable coordinator, pure control-flow
interpreter, scheduler, and SQLite store. `commands/orchestration.rs` is the
Workflow-owned adapter. `workbench/tasks.rs` owns conversation binding, turn
inspection, and immutable exchanges. Returned results also enter subsequent
Workspace turns as bounded, escaped source context. Workspace and Review keep their separate
runtime, credentials, storage, and cancellation namespaces.

The native owner holds an OS lock under `~/.pipeline/orchestration/`. SQLite uses
WAL, indexed ready/due queues, optimistic revisions, transactional schedule
activation, occurrence uniqueness, consumed signal identities, and append-only
events. At most four leaf actions execute concurrently, with the existing global
one-Workspace-turn and one-Review-run limits retained. Database work uses bounded
blocking pools. A completed response is written atomically to its action journal
before the coordinator advances. `orchestration/durable.rs` publishes receipts
and snapshots with file sync and Unix directory sync (including created parents);
Windows uses write-through moves. Publication failures propagate before adoption.
Task history loads small previews; full outputs
are fetched on demand. The Tasks UI and Workspace task cards are separate lazy
chunks, with debounced event-driven refreshes and paginated history.

Profile definitions and nonsecret settings are pinned at preparation. Provider
credentials are resolved only inside the Review adapter at dispatch. Declared
secret variable values cannot be embedded in task definitions. Review runs are
pinned against retention while the task adopts their full result. The task keeps
its own raw Review response and immutable paper, so ordinary Review retention can
resume after adoption.

Before every automated turn, Workspace checks the conversation cursor, project,
harness, root identity, and unsent draft while holding its existing turn permit.
A changed scope needs attention. The **Use current conversation context** control
refreshes the conversation binding without silently changing the pinned Review
profiles. Pause lets active work settle; Stop sends cancellation only to that
task's owned children. A process interruption marks uncertain actions explicitly
and does not replay them. **Reconcile recorded results** adopts a durable action
response, completed Workspace submission, Review result, or terminal check receipt before a
researcher decides whether to retry anything unresolved.

Retry saves each removed receipt in the same event-journal transaction that
requeues the task. **Activity log** exposes prior attempts with their operation,
sequence, state, timing, error and child/output references. Opening an attempt's
output or saving its artifact uses that exact operation, so a replacement at the
same step address cannot hide it. A late result from an earlier attempt remains
inspectable without being adopted as the replacement's result. Older textual
retry events remain readable; receipts already discarded by older versions
cannot be reconstructed from those events alone.

Default limits are 64 actions, a seven-day task deadline, and two hours per leaf
action. Definitions are capped at 256 KiB, 128 steps, eight nesting levels, 32 loop
iterations, eight parallel branches, and 64 `forEach` items. Context retained for
dispatch is bounded at 8 MiB. File capture uses the existing confined root reader
with 32 MiB per file and 64 MiB per snapshot; TeX/source folders exclude the
existing project metadata/cache paths. A Review continuation receives bounded
report text and structured findings. Manuscript text above 128 KiB is unavailable
to the default text-revision prompt and requires a file-based chain; the full raw
Review result remains retained. Prompts exceeding 256 KiB need smaller bindings.

### Validation

The Rust suite includes real-store coordinator tests for timer/input suspension,
parallel joins, exactly-once signal consumption, artifact persistence, pause and
resume, uncertain-action recovery, pinned scope checks, schedule coalescing,
atomic activation, and both daylight-saving transitions. Frontend tests cover
template ordering, preparation before dispatch, error handling, and the schedule
view. The native development smoke test drives the actual task interface and IPC
with an isolated task store and no model calls:

```sh
cd gui
PIPELINE_E2E_TASK_ROOT=/private/tmp/pipeline-task-native-check \
TAURI_WEBDRIVER_PORT=4449 npm run tauri dev -- --features e2e
# In another terminal, with the development app running:
node e2e/task-native-check.mjs
```

The `PIPELINE_E2E_TASK_ROOT` override exists only in binaries with the `e2e`
feature. Release binaries cannot enable it. Authenticated Review/revision loops,
real host tools, packaged close/reopen behavior, and Windows/Linux runtime
qualification remain governed by the existing
[release qualification policy](workbench/release-qualification.md).

#### Control evaluation and retained Review failures

Control-only branches and loops have a separate ceiling of 4,096 durable
control decisions and 32,768 interpreter visits per advance. Progress must stay
below 8 MiB before every save. Exhaustion requires attention; it does not dispatch
another action or silently replay an uncertain result. Full action evidence
remains in the adapter's durable journals when the bounded context cannot adopt it.

A task-owned Review keeps its retention pin until the result has been adopted.
An authoritatively failed or cancelled Review has no result to adopt: after its
execution owner settles, its matching pin is removed. Startup reconciles these
settled failures as well. Running, successful-but-unadopted, and uncertain runs
remain protected. Pin ownership must match the run's operation journal.

Explicit Stop and Retry also record abandonment durably. Once the Review runtime
is idle, matching settled pins (including recovered interrupted runs) can be
released without discarding their artifacts or receipt history. Startup replays
this reconciliation after a crash between the Task decision and pin cleanup.
A still-running Review is never unpinned by abandonment.
