# Automations

An automation runs a sequence of steps: conversation turns, reviews, checks,
and pauses for your input. Use one for repeated work, such as drafting a
paper, reviewing it, and revising it a set number of times.

## Start an automation

1. Choose **Automations** in the sidebar. From a conversation, you can instead
   choose **+** (**Add to message**), then **Run an automation**.
2. Choose **+ New automation**, then a template:
   - **Review and revise** drafts or loads a paper, reviews it, and revises it.
   - **Follow up** sends a message to a conversation now or later.
   - **Wait for input** waits for your answer, then continues the
     conversation with it.
   - **Start empty** lets you add every step yourself.
3. Choose the **Conversation**, fill in the template's fields, and choose
   when it runs under **When**.
4. Choose **Preview automation →**. Check the conversation, folder, reviews,
   limits, and start time, then choose **Start automation**.

The **Active**, **Scheduled**, and **History** tabs list current work,
schedules, and finished runs. Finished runs leave **History** 30 days after they
end, once you've opened them, or if they saved no result. Their scheduled
conversations move to the archive. Assistant steps use the conversation's
provider, saved model, thinking level, tools, and file permissions. This
applies to subscription, API, and local connections. A step can choose another
model; in that case, set its thinking level for that model.

Changing the conversation's connection requires refreshing the automation's
context before it continues. Prepare a new schedule if its connection changes.
Older automations without a saved connection need the same refresh.

### Review and revise

Enter an idea under **Idea or instructions**, and the assistant drafts a paper
in Markdown. To start from a file instead, open **Start from an existing
paper**.

Pipeline reviews the paper with the **Review workflow** you choose, asks the
assistant to revise it, and reviews it again. It stops when a complete review
has no high-priority or unclassified findings, or after **Maximum reviews**.
A review is complete when every step finished, the whole document was read, and
no comment was left for you to review by hand. A quote that needs checking
doesn't count against it, so check any flagged quotes in the final report. The
final paper is always the last version reviewed. **Save paper…** saves it.

If a revision leaves the paper unchanged, the automation asks for your
attention. Enter the revised text under **Corrected paper** and choose
**Retry unfinished actions** to capture that version and continue. The revision step can include the manuscript text only up to
about 20,000 words. For a longer paper, open **Definition (advanced)** and
have the step revise the file in the conversation's folder instead.

## Set a schedule

Under **When**, choose **Now**, **Later** for a single date and time, **At an
interval** to repeat every so many minutes, or **On a schedule** for a time of
day on chosen weekdays and time zone. The form shows the next three run
times. To change a schedule later, use **Pause** or **Edit timing** on the
**Scheduled** tab. **Delete schedule** stops future runs. Past runs stay in
**History**, and a run in progress keeps going.

- A time skipped by the spring daylight-saving change doesn't run. A time
  that occurs twice in autumn runs once, the first time.
- Intervals stay tied to the first run time you chose.
- Missed runs are combined into one, and runs from one schedule never
  overlap. While an earlier run is unresolved, the schedule shows **Waiting
  for the previous run** and **Open previous run**.
- Each scheduled run gets its own conversation, with the original
  conversation's settings and selected sources.
- Project activity, such as a new note or a delivered result, doesn't stop a
  schedule. If the conversation's permissions, research profile, or folder
  change, the next run asks for attention. Choose **Use current conversation
  settings** on the **Scheduled** tab to accept the change, then **Stop** that
  run so the next one can start. A changed connection needs a new schedule.

### Keep Pipeline running

Automations run only while Pipeline is open and the computer is awake.
Pipeline doesn't wake a sleeping computer. Turn on **Keep running when the
window closes** on the Automations page to keep Pipeline in the system tray
after you close its window. Quitting stops all automations until you reopen
Pipeline.

## Wait, pause, or stop

When an automation waits for input, its question appears on the automation.
Type your answer and choose **Provide input**. Waiting uses no model calls.

**Pause** lets the current step finish, then holds the next one. **Resume**
keeps a **Later** automation's date and time. **Stop** cancels the current
work, including setup before a message is sent. Saved results stay available.

If Pipeline loses track of whether a step finished, for example because the
app quit, the automation asks for attention. If the app was closed at the time,
the notice appears the next time you open Pipeline:

- **Check saved results and update status** looks for a saved reply, review,
  or check result. Use it before retrying.
- **Use current conversation context** accepts changes made to the
  conversation since the automation started.
- **Retry unfinished actions** runs the unfinished steps again. Past the
  automation's deadline, it continues with a new deadline, as does answering a
  **Wait for input** step. A check whose inputs changed runs only after you
  approve it again in the project's **Results** view.

**Activity log** keeps every earlier attempt and its output. If a step times out
while its result is being saved, the automation stops advancing. **Check saved
results and update status** can adopt the original result. An action that is
still running cannot be submitted a second time.

A group’s deadline starts when the run enters that group. Pausing the run or
closing Pipeline does not stop the clock. Each step uses the shortest active
timeout and the time left in its enclosing groups. A retry uses another action
and does not extend a group’s deadline; start a new run if more time is needed.

**Budget preview** in Research discovery gives the minimum actions needed for
the requested shortlist and ranked papers. It also shows how many actions
extra research rounds, reviews and replacements could take. You can start a
smaller budget and retain partial work. Reaching the minimum does not
guarantee eligible proposals, sound papers, or completion by the deadline.

Set **API allowance (USD)** to limit estimated list-price spending across the
run, including model calls inside Reviews and retries. Leave it blank for no
dollar limit, or enter zero to prevent paid calls. With an allowance, a model
without a known price cannot start a paid request. **Recorded usage** separates
priced requests, unpriced requests, subscription work and local calls. A
request with no confirmed usage keeps its reservation. The allowance uses
estimated tokens before dispatch and reported tokens afterward; the provider’s
invoice can differ.

### Retrieve delivered files

When a step delivers a file to a conversation, the conversation keeps its own
copy. Open the conversation’s actions menu in the chat list and choose **Files
from automations…**, then choose the file to save. A delivered folder saves as
a ZIP. These copies stay available after the automation leaves **History** and
travel with a research backup. Earlier deliveries can be copied when their source files and original delivery
records are still available. The file list explains when a copy could not be recovered.

## Research automations

A research automation investigates a question over several rounds, with
separate conversations to plan, investigate, and challenge the findings. Use
one when the next step depends on what the investigation finds.

1. Open **Automations → Research** and choose **+ New research automation**.
2. Choose a **Source conversation** from a project.
3. Choose a **Starting point**: **Theory investigation**, **Empirical
   investigation**, **Quantitative model**, **Literature investigation**,
   **Research discovery**, or **Project maintenance**.
4. Edit the **Research question and scope**, **Current understanding**, and
   **Completion criteria**, one per line.
5. Set the limits. Every round and action draws on this one budget. See
   [Limits](#limits) for the defaults.
6. Under **Computation and project changes**, choose the local checks and
   saved analysis plans it may run. For project maintenance, choose scheduled
   checks that wake it when something changes.
7. Choose **Preview research automation →**, check **Plan and permissions**,
   then choose **Start research automation**.

| Tab | What it shows |
| --- | --- |
| Agenda | Your criteria, current goals, and the next investigation |
| Findings | Arguments, evidence, alternatives, and negative results |
| Decisions | Questions for you; answer each with **Send research input** |
| Methods | Proposed methods you can **Save for reuse** in this project |
| Activity | Role conversations, checks, and other actions |
| Brief | A report with evidence links, limitations, and open questions |

Under **Claims and inspected evidence**, each assessment names the claim,
its scope, the records inspected, and any objections that remain. A proof
needs stated assumptions and a checked derivation. A numerical result needs
the data version, procedure, checked output, and the location and precision
of the reported number. A sourced interpretation needs an inspected passage.
A null result or counterexample can meet a criterion within its tested scope.

**Accept this assessment** records your decision about that round. It does
not accept manuscript edits. Earlier assessments remain available, with no
claim-level support or acceptance inferred from their original status.

**Export brief…** saves the brief as Markdown. **Export complete automation
record…** on the Activity tab saves everything as JSON.

**Pause research automation** lets the current action finish and holds the
next. Answering a question doesn't resume a paused run; choose **Resume
research automation**. A run that was waiting for your answer or for a project
change goes back to waiting, unless the answer or change has arrived. **Stop
research automation** cancels the current action.

The automation runs only the checks and analyses you chose. If a check's
scripts, inputs, or permissions change, you get a notice, the automation shows
the check as waiting for your approval, and it carries on with other work.
Approve the check again in the project's **Results** view, and the next plan
can use it. An action already using the check stops and shows **Needs
attention**. Approve the check, then choose **Retry interrupted action**, or
choose **Reconsider the agenda** to have the planner pick something else. The
investigator can edit files only if you turn on **Allow edits in the working
copy**. The source conversation must work in a working copy, with **Allow
edits** on. Edits stay in the working copy until you accept them in
**Review edits**.

## Research discovery

Research discovery turns a topic into proposals, investigations, and reviewed
paper drafts. It uses the same provider as the project conversation you
choose, or ChatGPT for a new project. Open **Automations → Research** and
choose **Research discovery**. Describe your topic under **Research topics
and questions**. Under **Project context**, choose a project conversation or
**Create a project for this research**.

Under **Who chooses the projects?**, choose one of two options:

- **I’ll choose** pauses at the shortlist. Pick proposals under **Choose
  projects to develop**, then choose **Develop selected projects**. Time
  spent waiting for your selection does not count toward **Finish within
  (hours)**.
- **Let Pipeline choose** selects proposals and continues without questions.

Use **Portfolio plan** to set how many projects to explore, shortlist, and
develop into papers. Under **Optional settings**, expand **Sources & tools**,
**Research & review**, **Models & ranking**, or **Resource limits** to change
the defaults. Choose **Start research discovery** when ready. **Portfolio
history** opens earlier runs; **Return to setup** brings you back to your draft.

For a ChatGPT or Claude Code conversation, **Research & review → Idea
subagents** lets you request 1–8 native subagents for the idea stage. For
example, request four subagents and 100 candidate projects. The lead gives
the four children different angles on the topic, waits for their results,
and combines their sketches into 100 starting points. Pipeline develops and
screens those proposals in batches. The team runs once per portfolio and
uses one action per requested child plus one for the lead. Set the count to
zero to use the usual proposal batches. A provider shortfall appears under
**Needs attention**.

Each chosen proposal gets its own author conversation and folder. Authors
can create and edit drafts there using project file tools with any connection.
Referees have read-only access to the research record and manuscript, without
the author’s chat. The original project folder stays separate.

While a run is open, **Activity feed** shows recent milestones, assistant updates,
and tool activity. **Last activity** shows how recently work was observed.
**Current step** shows elapsed time, and agent status lists each idea agent as
it starts and finishes. Open proposals and papers refresh as results arrive.

| Setting | Default | Range |
| --- | --- | --- |
| Candidate projects | 75 | 50–100 |
| Shortlisted projects | 10 | 1–25 |
| Papers requested | 5 | 1–9, no more than the shortlist |
| Detailed proposal reviewers | 2 | 0–4 |
| Proposal revision passes | 1 | 0–2 |
| Paper reviewers per round | 2 | 0–4 |
| Revision passes | 2 | 0–5 |
| Investigation rounds per paper | 8 | 1–16 |
| Maximum projects Pipeline can replace automatically | 2 | 0–5 |
| Allow code and computation in isolated paper folders | On | Commands have no network access |
| Search public literature records and available abstracts | On for a new project | Up to eight Crossref searches |
| Research input files | None | Up to 20 files, copied at the start |
| Maximum actions | 500 | 20–2,000 |
| Maximum time spent running (hours) | 48 | 1–168 |
| Minutes per action | 30 | 1–120 |
| Finish within (hours) | 168 | 1–720 |

These counts are targets. Missing data or unresolved objections can leave
fewer papers. The final report compares the papers and lists unfinished work.

A delivered draft can still have unresolved claims. **Evidence requirements
met** means the assessor supplied the required inspections for the frozen
version. Open **Claims and inspected evidence** to see what was checked. The
record distinguishes an investigator’s account, a source version, a captured
file and a completed check. Pipeline compares the number read from the cited execution with the value the
assessor read from the table. It does not independently verify a proof or
interpret a table. **Accept this
assessment** records your decision about that paper version. The report and
research export keep delivery, assessment, and acceptance separate.

Each manuscript includes Markdown, LaTeX, BibTeX, evidence references, and a
response to reviewers. **Export papers and research** saves a ZIP. **Export
portfolio report** saves a Markdown summary.

If a step fails, or Pipeline quits during one, the run shows **Needs
attention**. So does a reply that still can't be used after two repair
attempts. **Retry** repeats the step with the conversation as it is now, and
**Stop** ends the run. A run marked **Stopped: needs review** has ended, and its
recorded work stays available to export.

## In a project

Open **Project tools** and look under **Project work** for **Action items**,
**Scheduled checks**, and **Revision automations**.

### Action items

**Action items** is the project's to-do list. Type a task and choose **Add
action item**. **Start an edit** opens **Review edits** for that item.

### Scheduled checks

A scheduled check watches a project record and tells you when it changes. It
can watch an accepted file, a tracked dataset, new local results, a command's
status, or a saved Crossref search. Choose what to watch under **What to
check**, set the **Check interval (minutes)**, and choose **Save and enable
check**.

The first check records a starting point. Later checks report changes, which
you clear with **Mark as seen**. Checks run only while Pipeline is open, and
missed checks are combined into one. A Crossref check needs literature search
turned on for the project. After three failures in a row, Pipeline
tells you and checks less often until one succeeds. Restoring a research
backup turns checks off; see [Back up and share](projects.md#back-up-and-share).

### Revision automations

**Revision rounds** keeps a manuscript version, reviewer comments, your
responses, and evidence together for each submission round. Add comments from
your [response letter](analysis-and-writing.md), link the results each one
needs, and choose **Save round snapshot**.

Each comment then shows whether its task is done, its results are current,
and you marked it addressed. A comment marked addressed is still flagged if
its linked results are missing or out of date.

## Limits

| Limit | Default | Range or maximum |
| --- | --- | --- |
| Actions per automation | 64 | Up to 1,024 |
| Automation deadline | 168 hours | Up to 8,760 hours |
| Time per step | 2 hours | |
| Reviews in Review and revise | 3 | 1–32 |
| Schedule interval | | 1 minute to 1 year |
| Scheduled check interval | | 1 minute to 30 days |
| A file or folder saved by **Save a version** | | 32 MB per file, 64 MB in total |
| Research automation rounds | 8 | 1–32 |
| Research automation actions | 32 | 3–256 |
| Research automation active hours | 6 | Up to 168 |
| Research automation minutes per action | 30 | 0.5–120 |
| Research automation deadline | 24 hours | 1–720 hours |
| Rounds without progress before stopping | 2 | 1–8 |
| Research discovery ZIP export | | 256 MB |

## Automation definitions (advanced)

Every automation is a JSON definition. Open **Definition (advanced)** in the
builder to **Copy definition**, **Import automation** from a file up to
0.25 MB, use **Saved automations…**, or **Save as template**. **Named inputs**
supplies values that steps can use. A started automation offers **Export
definition…** under **Steps and limits**.

**+ Add step…** adds these steps:

| Step | `kind` | What it does |
| --- | --- | --- |
| Ask the assistant | `workspace` | Sends a message to the conversation |
| Run a review | `review` | Runs a saved review on a saved file |
| Save a version | `snapshot` | Saves text, a file, or a folder as a fixed copy |
| Run a check | `check` | Runs a tested command from **Command settings** |
| Run a saved analysis plan | `capturedCheck` | Runs an approved plan from **Saved analysis plans** (JSON only) |
| Look up literature | `literatureLookup` | Searches Crossref; needs project literature search on |
| Deliver a result | `deliver` | Copies a result into the conversation |
| Wait for me | `input` | Waits for your answer, with an optional timeout |
| Wait, Wait until a time | `delay`, `until` | Waits for a duration or until a set time |
| Only if… | `if` | Runs one branch or the other |
| Repeat until… | `repeat` | Checks the stop condition after each pass, up to 32 |
| Repeat while… | `while` | Checks the condition before each pass, up to 32 |
| Side by side | `parallel` | Runs up to 8 independent branches, then continues |
| For each item | `forEach` | Runs once per item, up to 64, with `item` and `index` |
| Group of steps | `chain` | Includes another definition |

A definition can have up to 128 steps, nested up to 8 levels, with unique step
IDs.

### Conditions and references

The condition picker offers **is true**, **is false**, **equals**, **is
below**, **exists**, and **does not exist**, joined by **and** or **or**. In
JSON, conditions use `equals`, `lessThan`, `exists`, `all`, `any`, and `not`.
Each value is a `literal`, an earlier step's `output` with a JSON pointer such
as `/highPriorityCount`, a named `input`, or `firstAvailable` of several
values. A missing or mistyped value makes the condition unknown, and the
automation asks for your attention.

Prompts can include `{{input:key}}` and `{{output:stepId#/pointer}}`. A step
can use outputs only from earlier steps. Inside a loop, it gets the latest
pass. **Side by side** branches can't use each other's outputs, so put
dependent steps in the same branch.

The **Automations** tool in a conversation's assistant instructions lets the
assistant suggest automations as cards that you review and start.
