# Tasks, chains, and schedules

Status: design reference, September 7, 2026. The durable desktop coordinator,
chain actions, schedules, waits, and task interface are implemented. See
[tasks.md](tasks.md) for the exact shipped-code boundaries and qualification.
This original proposal also discusses future deployment and authoring options;
those sections are design intent rather than a release claim.

The proposed feature gives Pipeline a durable task coordinator. A task can
continue a Workspace conversation, run a saved Review profile, pass the result
back, and repeat subject to explicit conditions. The same coordinator supports
delayed starts, recurring schedules, and waiting for input.

The central design decision is to put orchestration above Workspace and
Workflows. Each retains its runtime, credentials, storage, and cancellation.
The coordinator owns the sequence and its durable progress; the existing
services own the actual work. An ordinary conversation remains lightweight.

## Product model

| Concept | Meaning | Example |
|---|---|---|
| Task | One execution with a goal, inputs, progress, and outcome | Review and revise this paper |
| Chain | A versioned, reusable definition of steps and control flow | Draft → review → revise → review |
| Schedule | A rule that creates task occurrences | Every Monday at 9 a.m. |
| Step | One action or control operation within a task | Run Automatic Paper Review (Full) |
| Wait | Durable suspension of an existing task | Wait for the updated data |

A one-step task needs no chain editor. A recurring schedule creates distinct
task runs; it does not keep one conversation turn alive forever. A wait resumes
the same task. Existing Workflow profiles remain deterministic review graphs;
Workspace recipes remain instruction/check modules used within a turn.

Workspace already has research task records with objectives and working copies.
Preserve their identities and semantics. A coordinator task can reference one
of those records through `researchTaskId`; manual research tasks need not become
executable. Use **Tasks** for the suite's execution destination and **Run task**
as an optional action on an existing research task. Do not merge the tables or
rename persisted formats to make the labels agree.

Keep execution state separate from research outcome. A run can be queued,
running, waiting, paused, cancelling, cancelled, failed, or finished, with an
explicit unknown-outcome recovery state. A finished run separately records
whether criteria were met, issues remain, or a configured limit was reached.
Steps and attempts retain their own states; a failed attempt need not mean
the task has failed while an authorized retry is pending.

## The first complete experience

From a Workspace conversation, the researcher can ask:

> Develop this idea into a paper, run Automatic Paper Review (Full), and revise
> it until no high-priority issues remain. Use at most three reviews, then leave
> the paper and any unresolved issues here.

Pipeline resolves this into a compact, editable task proposal. It identifies
the conversation, working copy, output paper, exact Review profile version,
stop rule, maximum rounds, and execution limits. **Start task** authorizes that
concrete scope once. If the same scope was already authorized through a saved
task, invocation can proceed directly. A missing input becomes a named input
request in the task, rather than another generic confirmation dialog.

Prompt-only tasks can remain unfiled. If the selected research actions need
project-owned papers or a working copy, resolve that binding in the proposal
using the existing project creation/selection flow. An app-owned working
directory can support a new paper without reorganizing an existing folder.
Include these effects in the same Start action; a later timer must not create
an unexpected project or silently attach a different folder.

The task then:

1. Runs a Workspace turn to produce the draft in an isolated working copy.
   If a paper already exists, start from a selected revision and omit drafting.
2. Captures the declared paper output as an immutable artifact and runs any
   required initial checks against that exact snapshot. For a TeX
   project, record the entry file, included-file manifest, and any compiled PDF
   with its source/build identity. A chat paragraph is not implicitly a paper;
   a text-only result can become a host-created Markdown document when that is
   the declared output format.
3. Invokes the pinned Review profile on those exact bytes.
4. Copies a bounded Review result into Workspace and continues the same
   app-owned conversation. The continuation receives the report, findings,
   review limitations, exact reviewed revision, and prior issue dispositions.
5. If the review meets the configured stop rule, finishes with the reviewed
   paper and report. Otherwise, if another review is allowed, revises, runs
   the selected checks, snapshots the new paper, and reviews that revision.
6. If the limit is reached, finishes with unresolved issues and the last
   reviewed revision. It never performs a final unreviewed edit and presents
   that edit as having passed review.

The conversation shows one expandable task card plus meaningful assistant
updates. Each review and revision remains accessible in the activity history.
The final card links to the paper, diffs, reports, recorded checks, and remaining
issues. Applying changes to the researcher's original files uses the existing
Edits acceptance flow; automated revisions can proceed within the working copy.

Pseudocode illustrates the ordering; it is not a proposed executable format:

```text
paper = draft_or_select_exact_revision()
checks = check_exact_outputs(paper)
for round in 1..=max_reviews:
    review = run_profile(profile_snapshot, paper)
    deliver_review_to_workspace(review, paper)

    if review_is_incomplete_or_condition_is_unknown(review):
        wait_for_action("Review cannot establish the stop condition")
    if stop_rule(review, checks):
        finish("Criteria met", paper, review)
    if round == max_reviews:
        finish("Review limit reached; issues remain", paper, review)

    revised = revise_in_workspace(paper, review)
    paper = snapshot(revised)
    checks = check_exact_outputs(paper)
    if unchanged_or_previously_seen_revision(paper):
        wait_for_action("Revision made no progress or returned to an earlier version")
```

Stopping on model-reviewed findings is a research policy, not a proof of
correctness. The outcome should say **No high-priority issues reported**, not
**Paper verified**. A disputed finding can retain a reasoned disagreement;
neither the writer nor the coordinator can silently turn it into a human
confirmation. Cross-round issue matching is advisory unless an exact lineage
exists; issue IDs are scoped to their producing run.

## What exists, and what must change

| Inspected implementation | Reuse | Required extension |
|---|---|---|
| `workbench/release.rs`: `prepare_review_handoff` | Immutable staging and operation IDs | Select an exact revision; current code resolves the current paper revision and labels directories `source_tree` |
| `App.tsx`: `handleWorkspaceReviewHandoff` | Launch-preview presentation | Move launch preparation into a reusable backend service; orchestration cannot depend on navigating a page |
| `commands/run_context.rs`: `load_run_snapshot_for_profile` | Select a profile without modifying the active profile | Persist/reconstitute a non-secret pinned launch specification and accept an operation identity |
| `commands/run_entry.rs` and `commands/run.rs` | Normal validation, extraction, execution, persistence | Durable launch reservation before extraction; inspectable launch-to-run mapping |
| `commands/lifecycle.rs` | Existing single-run guard and process cleanup | Owner-scoped launch/cancel handle and queue admission |
| `models.rs`: `RunProducts`, `FindingSet`, `ReportQuality` | Canonical findings and host-authored coverage/quality | Versioned Review-result adapter with explicit unknown states |
| `workbench/project/studio/review.rs` | Finding package preview and selected import | Idempotent automated package adoption under the task's authorized selection |
| `workbench/commands.rs`: `workbench_codex_send_turn` | Turn setup, binding, snapshots, global turn permit | Separate interactive draft submission from durable task continuation |
| `workbench/research/jobs.rs` | Bounded local jobs, ownership and adoption journals | Adapter integration; current queue is not a timer or a chain scheduler |
| `workbench/project/` and `project/studio/` | Working copies, snapshots, diffs, checks, acceptance | Exact output contracts and task ownership at orchestration boundaries |
| `emit.rs`: `EventBus`, background sink | Decoupled backend event delivery | Task/step/attempt correlation; background reviews must not take over foreground Review UI |

Two restrictions shape the first implementation. Workspace permits one active
turn process-wide. Review permits one active run, including a batch's held
guard; cancellation, logging, usage and settings state still have shared
ownership. Preserve those limits initially. Concurrent review runs require a
separate conversion to per-run state. A larger semaphore alone is unsafe.

Automatic Paper Review explicitly rejects `source_tree` inputs. Its adapter
must hand over a document or an explicitly resolved `latex_project`, and must
validate that the requested profile accepts that input. Reusing the existing
directory handoff unchanged would break this principal use case.

## Runtime architecture

```text
Conversation task card     Tasks page     Schedule editor
             \                |                /
                    typed task service
                            |
                  durable coordinator
           definitions / runs / waits / dispatch journal
                /              |               \
       Workspace adapter   Workflow adapter   Research-check adapter
              |                  |                    |
       Workspace service   Workflow service     scoped execution service
       isolated App Server existing providers  owned jobs / check receipts
       Workspace store    Workflow run store  Workspace-owned artifacts
```

Implement a Rust module `orchestration/` with no Tauri or provider dependency
in its core. Inject a clock, durable store, event sink, and action adapters.
The transition function consumes a persisted state and a recorded event and
produces the next state plus dispatch intents. It never calls a model, reads
arbitrary project files, or inspects another domain's database itself.

The product adapters expose a small contract:

```text
describe_capabilities() -> action schemas, limits, recovery support
prepare(request)       -> resolved inputs, effects, fingerprint, readiness
submit(operation_key, prepared_request) -> durable child handle
inspect(child_handle or operation_key)  -> authoritative state and outputs
cancel(child_handle, expected_owner)    -> cancellation acknowledgement
```

Add actions through this registry. Version their input/output schemas and
required capabilities. The initial actions are `workspace.turn`,
`workflow.run`, `artifact.snapshot`, `workspace.deliver`, and `research.check`.
Some are narrow compositions of existing services; none is claimed to exist
yet as a coordinator API. The research-check adapter must return host-recorded
results and dependency hashes, not parse an assistant's claim that tests passed.

A deterministic interpreter handles `sequence`, `if`, bounded `repeat`,
`wait.until`, and `wait.input`. Later releases can add bounded `parallel`,
`forEach`, and calls to versioned subchains through the same contracts. Declare
outputs at branch joins, reject unbound references, and give iterations stable
addresses such as `review-cycle/2/review`. Recursive chain calls and arbitrary
back edges are initially invalid. Native plugins or an embedded scripting
language are unnecessary for these semantics.

This adopts the useful separation between durable orchestration and individual
actions described in [Temporal's activity model](https://docs.temporal.io/activities).
It does not require running a Temporal service. A local, native interpreter
fits the existing application; a future remote execution adapter can preserve
the same task model if demand warrants it.

### Storage and recovery

Use a separate local `~/.pipeline/orchestration/tasks.sqlite3` database, with
transactional migrations, foreign keys, backup-before-migration, and rejection
of newer schemas. Keep immutable coordinator-owned packages in an adjacent
blob directory. Store references and small summaries in rows; never put
transcripts, PDFs, or full provider payloads into the event log.

Logical records:

- `chain_versions`: immutable definition, schema/adapter versions, fingerprint.
- `task_runs`: definition version, bindings, limits, origin, state and revision.
- `step_instances` and `attempts`: stable address, operation key, input hashes,
  child handle, output references, timestamps and outcome.
- `events` and `dispatch_outbox`: ordered durable transitions and pending effects.
- `waits` and `signal_inbox`: timer/input registrations, cursors and consumption.
- `schedules` and `occurrences`: activation revision, occurrence key, intended
  time, actual dispatch time, coalescing and disposition.
- `authorizations`: local authority references scoped to the selected plan and
  capabilities; no provider credentials.

Keep transactions short and use bounded blocking workers. WAL permits readers
alongside a writer but still has one writer at a time; process waits and model
calls belong outside transactions. See [SQLite isolation](https://www.sqlite.org/isolation.html).
Use a singleton local owner lock plus generation fencing for the coordinator;
deadline expiry alone must never allow a second owner to replay an uncertain
model action after sleep.

Each transition commits its state, event and outbox entry together. Dispatch
can be delivered more than once. The receiving adapter therefore records a
stable operation key before launching work and returns the existing child for
an identical retry. A different payload with that key is an error. There is no
cross-database transaction and no blanket exactly-once claim.

| Interruption point | Required behavior |
|---|---|
| Before adapter submission | Retry the durable dispatch intent with its existing operation key |
| Child reserved, response lost | Query the adapter's operation journal and attach to that child |
| Workspace `turn/start` acknowledgement ambiguous | Reconcile native history; if identity remains uncertain, enter **Outcome unknown** and require an explicit retry decision |
| Review finished before coordinator recorded it | Recover the result from the owned run, copy it once, and continue |
| Result copy interrupted | Verify staged hashes and finish adoption idempotently; never run the review again just to recreate the copy |
| Native process died mid-action | Preserve checkpoints; adapter-specific resume only after the old owner is known inactive |
| App upgrade while waiting | Continue with compatible pinned semantics; unsupported versions remain readable and paused for migration |

The Workflow journal must cover preflight and extraction, because current run
directory creation happens after extraction. Reserve a durable operation/run
identity before any expensive or externally observable work. Preserve existing
partial-run recovery and avoid whole-profile retries over completed steps.

Resource pins belong to the source domain. A Review launch must reserve
retention protection before normal run retention can prune its outputs. Copy
the return package into Workspace before releasing the pin. Journal adoption
and pin release separately so a crash cannot leave a dangling reference;
reconcile abandoned pins during recovery. Task deletion never deletes a
conversation, Review run, or user's working files as a side effect.

Existing `.pwrx` and `.pwex` imports do not restore active scheduling. A future
portable chain file uses its own schema and logical resource bindings, excludes
credentials/local grants, and imports disabled. Workspace restore or deletion
requires the coordinator to quiesce affected dispatch and revalidate bindings;
do not add writable cross-store dependencies to existing archive formats.

### Typed inputs, conditions, and versions

Use references such as `ArtifactRef`, `WorkspaceTurnRef`, `ReviewResultRef`,
and `CheckReceiptRef`, each with owner, identity, version/hash and declared
media/schema type. A step's inputs explicitly select upstream outputs. Bind
files in the owning service and validate size, type, path scope and hashes;
strings emitted by a model are not trusted artifact identities.

The launch preview pins the chain, profile content, effective settings,
specialist catalog revision, harness configuration and selection policies.
Snapshot the referenced catalog content, prompt bodies and expanded schemas
needed to reproduce materialization; a catalog fingerprint alone cannot
recreate an older adaptive profile after an app update. If an adapter cannot
execute that stored version compatibly, pause instead of expanding it against
the new catalog silently.
Provider credentials remain in their existing namespaces and are resolved at
dispatch. Do not serialize `RunSnapshot.settings` wholesale into the
coordinator: the current in-memory type can contain secrets. Preserve exact
model selections; an unavailable pinned model pauses. An explicitly selected
Automatic model policy may resolve per launch and records the actual model.

New task occurrences use the schedule's pinned definition. Updating a saved
profile does not change an active task or existing schedule silently. Offer
**Update future runs** with a version diff. Dynamic paper inputs are different:
**Current paper when the task starts** is an explicit selector, resolved to an
exact immutable revision once for each occurrence. Every loop iteration then
binds the previous iteration's exact output.

Conditions are a small typed expression tree: comparisons, `all`, `any`,
`not`, `exists`, and bounded counts over declared output fields. Evaluate
them in Rust with explicit true/false/unknown semantics. Missing output,
unsupported schema, an unclassified finding priority, or incomplete review
coverage must not become a zero count or a successful condition.

The Review adapter reads `RunProducts.findings` from the selected canonical
producer and `ReportQuality` from the host. Its normalized condition output
includes `complete`, `coverage`, `findingsByPriority`, and `unknownPriorityCount`.
Current `Finding.priority` uses optional high/medium/low labels; do not invent
a numeric severity scale. The default stop rule requires a complete review,
zero high-priority findings, zero unknown priorities, and any required checks
for that same revision. Degraded/partial reports remain useful context but
cannot satisfy that rule automatically.

Custom profiles that return only Markdown still chain successfully. They
cannot support a structured issue-count stop rule without an explicit
schema-producing assessment step or a human decision. Semantic predicates
such as “the argument is convincing” become an explicit model-assessment
step with evidence and an uncertainty outcome; prose does not rewrite the
control graph.

Require `maxIterations` and a task-level deadline for every loop. Track model
calls, active execution time and available token/cost usage across both modes.
Separate active-time budgets from elapsed deadlines so intentional waits do
not consume execution time. Reserve conservative call budgets before parallel
dispatch and retain inner Workflow caps. Incomplete provider usage means an
unknown estimate, not zero spend; do not promise an exact dollar ceiling on
subscription usage. Stop at unchanged or repeated revision hashes; issue
counts alone are not a reliable progress metric.

## Workspace continuation and authority

Scheduled turns must not go through the current composer-draft mutation path.
Introduce a common internal turn submission service with distinct interactive
and task entry points. Both use the existing global permit, snapshots, binding
checks and event reconciliation. Task input is persisted in a separate durable
continuation record and never overwrites or clears the user's draft.
Label generated continuation entries with their task origin in the transcript.
New task conversations can take their titles deterministically from the task
and occurrence, avoiding an extra automatic-title model call.

Bind continuation to the app-owned conversation ID, expected transcript cursor,
project/root identity, selected paper revision, and harness fingerprint. Native
thread IDs are runtime bindings; a qualified successor thread can still belong
to the same user-visible conversation. Do not promise a provider-native fork
without its separate compatibility qualification.

Capture the continuation cursor after the initiating turn finishes; task-owned
turns advance it through recorded completion. Only intervening external turns
or material context changes invalidate it. Otherwise the task would conflict
with its own messages on every round.

Revalidate just before dispatch, under admission control:

- A newer user turn or material context change pauses automatic continuation
  in that conversation. The task offers **Continue with the new context** or
  **Continue in a separate conversation** with a bounded, inspectable handoff.
- A nonempty draft remains untouched. The conservative same-conversation
  policy waits until the user sends or dismisses it; an explicitly separate
  task conversation avoids this wait.
- A busy conversation or global turn slot queues the task. A removed, archived
  or relocated target becomes an actionable unavailable binding.
- Input/checkpoint changes during a queued launch require a fresh snapshot
  and preparation. An output capture cannot race another writer.

For recurrence, default to a separate task conversation per occurrence linked
to the source conversation/project. For the interactive review-and-revise
example, default to continuation in the initiating conversation. Both choices
are visible and editable in the task proposal.

A model-accessible task tool can resolve an available profile, propose a chain,
request an authorized child review and inspect its status. Scope comes from the
active Workspace binding and local grant, never model-supplied workspace IDs.
The request returns promptly with a durable child handle. It does not hold a
tool call open for an hour or keep the Workspace turn semaphore while waiting
for its own continuation. The current turn ends normally, then the coordinator
waits for the child and submits the follow-up. Add these tools as an optional
harness module; ordinary chat does not initialize them.

Child requests inherit task ownership and consume the parent's call, iteration
and execution budgets. Cap pending children and reject recursive invocation;
a model cannot create an unlimited set of detached reviews or schedules.

Native model questions and permission requests require a separate path from
planned input waits. An unattended turn must not monopolize the sole Workspace
slot while a person is away. Record the request as task attention, allow only
a bounded live response window, then decline or interrupt through the existing
runtime and reconcile any partial effects before releasing ownership. The
durable task resumes with the saved answer in a fresh continuation; it never
replays an old epoch-scoped approval into a new connection. If the interrupted
action's outcome is ambiguous, recovery remains explicit.

Authorization attaches to a concrete versioned plan: permitted action kinds,
profile/provider policy, input selector, working-copy root, recurrence and
budgets. Approved deterministic branches can run without per-round prompts.
New capabilities or materially changed targets require a revised proposal.
Review text and imported files are research input and confer no task authority.

Preserve existing research trust boundaries. Automatic revision may edit the
authorized working copy; accepting source-file changes, research claims and
human evidence remains a separate researcher action. Existing host-execution
grants bind exact launch and input identities and task-copy conversations
disable legacy host tools. Chaining must not silently relax those grants.
For unattended checks on evolving working copies, implement and qualify a
narrow sandboxed check adapter, or pause for the exact host authorization
required by the current execution service. That adapter is a real delivery
dependency, not an assumed capability.

## Scheduling and waits

Use one trigger subsystem to create task occurrences and wake suspended steps.
Support three time forms with distinct semantics:

| Form | Semantics |
|---|---|
| In one hour / at a selected time | Resolve and store one UTC due instant at creation; retain the user's local-time display intent |
| Every N hours | Fixed elapsed interval from an explicit anchor; optional advanced delay-after-completion mode |
| Every Monday at 9 a.m. | Calendar recurrence in a saved IANA time zone, independent of travel or machine-zone changes |

Persist intended and actual start times. Show the next three occurrences
before saving calendar schedules. Save explicit daylight-saving policy:
default skip nonexistent local times and run once at the first occurrence of
an ambiguous local time. Do not label a custom scheduling format RFC 5545
compliant without matching its exact rules; its treatment of invalid
recurrences is a useful reference ([RFC 5545](https://www.rfc-editor.org/rfc/rfc5545.html)).
Use a maintained timezone/recurrence library selected against the repository's
toolchain during implementation. Record timezone-rule version changes that
alter future occurrences and show the revised next-run preview.

The coordinator keeps an indexed `next_due_at`, one earliest-deadline wakeup,
and wake signals for new work. Persist UTC deadlines; use a monotonic timer
while awake and re-evaluate on sleep/resume or clock changes. A low-frequency
watchdog may reconcile missed clock notifications; do not scan all tasks every
second. Expired timers enqueue ready work, not model requests directly.

| Situation | Default behavior |
|---|---|
| A one-time task is overdue on reopening | Run once when ready within a visible 24-hour grace window; beyond that, ask whether it is still wanted |
| Multiple recurring occurrences were missed | Coalesce to the latest eligible occurrence; record skipped ones |
| Previous occurrence is still running or waiting | Keep at most one pending occurrence; replace it with the latest and retain the disposition history |
| Task is paused for a person | Suppress duplicate attention notifications and further overlapping execution |
| User pauses a schedule | Stop future dispatch; keep active work unless **Stop current task** is also chosen |
| Schedule is edited | New activation revision governs future occurrences; obsolete undispatched occurrences are retired |
| Login, permission, required file or model unavailable | Named waiting reason and one notification; resume after readiness returns if authority and deadline still permit |
| Known rate-limit reset | Durable retry time after that reset, bounded by task policy; no repeated probing or provider switch outside the pinned policy |

Occurrence identity includes schedule ID, activation revision and logical
occurrence time. Claiming that identity and advancing the schedule cursor must
be one transaction. Overlap and catch-up policy belong on the schedule,
separate from the chain. This separation is also established in
[Temporal's schedule design](https://docs.temporal.io/schedule).

**Wait for input** exposes a small typed form or document slot attached to the
task. The user's reply is stored once with a signal ID and exact attachment
revision. Register a wait and its event cursor atomically; match buffered
signals after registration to prevent a lost wakeup. A timer and an input
racing to resume a step use one compare-and-set transition. Cancellation
retires the wait. Inputs submitted to a paused task are retained and become
eligible on resume.

An input wait can be indefinite, but retains a visible **Waiting for you** state
and can have a timeout branch. **After another task completes** targets its
exact run, not any similarly named task. **After a new paper is imported**
targets an app-owned revision event. Raw filesystem watches and external
webhooks come later: a filesystem notification is only a hint until the host
has captured a stable, validated artifact.

### When Pipeline is closed

The first release runs while Pipeline is open and the computer is awake;
timers and waits survive restart and use the missed-run policy above. State
that clearly beside the schedule controls. Closing a window and quitting the
application must have distinct, tested meanings. Graceful quit checkpoints
the task and asks the owning adapters to stop active work through their normal
cleanup paths; an interrupted action is not automatically known safe to retry.

Add optional **Keep tasks running in the background** after the core is
qualified. A native per-user runner then becomes the sole coordinator and
runtime owner, with the desktop acting as its UI client over authenticated
local IPC. Do not have a helper and GUI independently open competing
Workspace runtimes or shared Review globals. Package and update the helper
with the app, use versioned IPC, and retain the existing isolated credential
homes and product stores. This needs a dedicated process-lifecycle milestone.

Background mode can cover a closed UI while the computer is awake. Running
at an exact time during sleep, logout or power-off is a different requirement;
it needs qualified OS wake support or an explicitly configured always-on
runner. No schedule should claim that capability before it exists.

## Admission, cancellation, and performance

One coordinator can manage many waits with a small bounded set of active
actions. Admit work only when its owning adapter's resources are available:
one Workspace turn, one Workflow run, and the existing research-job limits.
Interactive requests take precedence at action boundaries; add aging so
background tasks eventually run. Never interrupt a person's active turn to
dispatch a timer. Existing batch runs retain their guard until the batch
finishes in the first version; show **Queued behind batch** rather than
promising fairness inside a held batch.

Adapters remain authoritative for resource acquisition and revalidation.
Queued work holds no execution permit. Waits hold no turn, model call, database
worker or OS process. Never hold a coordinator transaction while acquiring a
product lock; acquire product resources in a documented order and release
them before any downstream dispatch. In particular, Workspace's current
project-local-job exclusion must remain enforced across adapter calls.

**Pause task** prevents the next step from starting and allows the current
action to settle. **Stop task** cancels only its active child and prevents new
dispatch immediately. Pass a child handle and ownership generation through
cancellation, even while the Review engine has only one slot, so a stale task
cannot cancel a later manual review. Retain completed outputs. A cancelled
task can be continued through a new run linked to its checkpoints, without
rewriting its old history. Pausing a schedule is distinct from either action.

Retry transient, unambiguously failed actions with bounded backoff and jitter.
Respect the existing Workflow's internal retry budget: do not multiply retries
by automatically restarting the whole profile. Condition failure is a normal
branch, authentication failure is a wait, partial review is an explicit policy
decision, and unknown side effects require reconciliation. These are different
outcomes, not one generic retry state.

Proposed qualification targets, to measure on release builds and recorded
reference hardware:

| Measure | Initial target |
|---|---|
| Warm task list, 10,000 historical runs, first 50 summaries | p95 under 150 ms excluding first lazy module load |
| Coordinator enqueue/transition transaction | p95 under 50 ms excluding artifact staging |
| Due occurrence → ready queue while awake | p95 under 1 second; resource queue time reported separately |
| Stop click → cancellation dispatched to owner | p95 under 250 ms; process cleanup has adapter-specific limits |
| 1,000 waiting tasks | No per-task polling, processes, connections, or model calls |
| Idle scheduler with waiting tasks | Below 0.5% of one core and under 25 MiB incremental memory |
| Ordinary chat with no task module enabled | No Workflow initialization or task-detail hydration; no material regression against its measured baseline |

Page the task list, activity history and artifact views. Stream coalesced
task-state changes rather than provider token deltas into the task card.
Record queue, execution, waiting, and recovery time separately. Reuse verified
extraction by content/settings fingerprint, but never reuse an old review as
if it had reviewed changed paper bytes. Context delivery includes a short
summary and typed artifact handles with bounded reads; it does not paste every
previous report into every round.

## Interface

Keep the normal Workspace layout. Add **Run task…** to the composer menu and
paper/message actions, **Schedule…** to an existing task, and one **Tasks**
destination in the suite navigation. No new permanent inspector column is
needed in chat. The navigation badge counts attention states, not every event.

The task proposal is an ordered outline:

```text
Review and revise
This conversation · Working copy of paper.tex

  Draft paper
  Repeat up to 3 reviews
    Review                 Automatic Paper Review (Full)
    If high-priority issues remain
      Revise paper         This conversation
      Check outputs        Selected checks
  Return paper and remaining issues

Start  Now ▾                         Limits…
                                      Start task
```

Expand a row to edit its profile, input/output binding or prompt. Use plain
condition labels with the structured expression available under **Details**.
Indent branches and loop bodies; preserve reading order. Parallel branches can
later appear as compact adjacent groups when space allows. An optional graph
view may help large plans, but should not be the primary authoring interface.

**Tasks** has **Active**, **Scheduled**, and **History** views. Each row shows
the task name, project/conversation, current action or waiting reason, and next
relevant time. Opening it shows the same step outline, with **Activity** for
attempts, iterations and results. Saved chains are available from **New task →
Saved chains**, keeping template administration out of the primary navigation.

Use precise visible states: **Queued**, **Reviewing · Round 2 of 3**, **Waiting
for updated data**, **Paused**, **Needs sign-in**, **Outcome unknown**,
**Criteria met**, or **Finished with unresolved issues**. A successful process
exit and a satisfied research condition are different facts.

Schedule editing starts with **Once**, **Repeating**, or **After input**. Show a
natural-language time, timezone when relevant, next occurrences, and where the
result goes. Put catch-up, overlap and limits in an expanded section. The
creation action changes to **Schedule task** when appropriate. In-app activity
is always durable; OS notifications are optional and emitted for completion,
failure or required action. Routine steps and unchanged waits remain quiet.

Follow the existing native typography, restrained blue accent in light mode,
grayscale dark mode, slim separators and compact controls. Use icons with text,
no moving graph during execution, and no arbitrary completion percentage when
loop length is unknown. Preserve keyboard access, focus on dialog closure,
screen-reader state announcements, reduced motion, and legible narrow layouts.

## Delivery sequence and acceptance

Each milestone delivers a usable slice. Avoid a large generic editor before
the review round trip works.

| Milestone | Scope | Exit criterion |
|---|---|---|
| T0 — Contracts and durable dispatch | Definition/condition types, SQLite journal, fake clock/adapters, Review and Workspace service seams, owner-scoped cancellation | Crash at each dispatch boundary cannot silently duplicate a known child; unknown sends remain visible |
| T1 — Review and return | Run a selected profile from Workspace, exact-revision handoff, durable result adoption, same-conversation continuation, compact task card | One real authenticated round trip survives navigation and a controlled restart; a user draft is never changed |
| T2 — Revise and repeat | Working-copy outputs, check adapter, structured conditions, bounded repeats, iteration history | Three-round fixture stops on criteria or cap, preserves disputed/unknown findings, and always identifies the exact final reviewed revision |
| T3 — Run later and wait | One-time starts, typed input waits, resume/clock reconciliation, Tasks page | An hour delay releases all execution slots and survives restart; input/timeout/cancel races produce one transition |
| T4 — Recurrence and reuse | Calendar/interval schedules, overlap/catch-up, saved chain versions, disabled portable import/export | DST, missed-run and schedule-edit fixtures produce the documented occurrence set without bursts or duplicates |
| T5 — Closed-window operation | Optional native runner, IPC, single runtime ownership, packaged update/shutdown behavior | Qualified closed-UI runs and process recovery on each advertised platform |
| T6 — Broader composition | Bounded parallelism, subchains, app-owned event triggers, richer outputs | Admission/cancellation isolation and bounded resource use hold under mixed interactive/background load |

T1–T3 are the recommended first product release: the complete review/revise
loop plus delayed starts and input waits. T4 completes recurring intervals;
it can proceed after T3 without waiting for the background runner. Treat T5
as a separate packaging/lifecycle project. Do not advertise unattended checks,
closed-app execution or concurrent Workspace turns before their own gates pass.

Suggested source layout:

```text
gui/src-tauri/src/orchestration/
  mod.rs                  facade and task service
  definition.rs           versioned plan and typed bindings
  conditions.rs           pure bounded condition evaluation
  state.rs                deterministic transitions
  store.rs, migrations/   durable records, outbox, occurrences
  dispatch.rs             admission, reconciliation, ownership
  triggers.rs             time and input semantics
  adapters/               Workspace, Workflow, checks, artifacts
  tests/                  fake-clock, crash and integration fixtures
gui/src-tauri/src/commands/tasks.rs       thin Tauri facade
gui/src/components/tasks/                lazy cards, outline, list, schedule UI
gui/src/lib/taskClient.ts, taskTypes.ts   app-owned DTOs
```

Extract shared launch and turn services without duplicating their validation.
Keep Workspace database access behind its existing bounded worker/archive gate.
Leave the Workflow DAG parser and recipe registry semantically unchanged;
only factor neutral schema/fingerprint helpers where useful.

Required deterministic coverage includes crash injection before/after each
receipt and adoption commit; duplicate events; out-of-order child completion;
ambiguous Workspace send; retention racing completion; archived/restored target;
changed draft, harness, root or paper; missing custom-profile schema; partial
review with zero findings; unknown priorities; branch output typing; max-round
off-by-one; A→B→A revisions; budget exhaustion; Stop versus Pause; late input;
clock movement; DST gaps/overlaps; coalesced occurrences; and GUI/runner owner
contention. Fake clocks replace real sleeps.

Run the ordinary Rust and frontend suites plus focused adapter integration and
UI tests. Shared App Server changes require both Workflow and Workspace
regressions. Follow [Workspace compatibility policy](workbench/protocol/compatibility-policy.md)
for any new tool catalog or wire behavior and
[release qualification](workbench/release-qualification.md) for authenticated,
real-tool and packaged claims. Exercise the Pipeline GUI through `npm run
tauri dev` on this machine. Unit tests do not establish that a real model can
invoke the task tool or that an OS helper survives a packaged update.

The highest-risk work is the durable adapter boundary, especially ambiguous
Workspace sends and Review launch identity before extraction. Prove that
boundary first. The chain editor and recurrence controls then sit on a small,
testable model whose behavior is the same whether work starts from a message,
a button, a timer, or an input signal.
