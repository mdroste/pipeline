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
schedules, and finished runs. Assistant steps use the conversation's
provider, tools, and file permissions.

### Review and revise

Enter an idea under **Idea or instructions**, and the assistant drafts a paper
in Markdown. To start from a file instead, open **Start from an existing
paper**.

Pipeline reviews the paper with the **Review workflow** you choose, asks the
assistant to revise it, and reviews it again. It stops when a complete review
has no high-priority or unclassified findings, or after **Maximum reviews**.
The final paper is always the last version reviewed. **Save paper…** saves
it.

If a revision leaves the paper unchanged, the automation asks for your
attention. The revision step can include the manuscript text only up to
about 20,000 words. For a longer paper, open **Definition (advanced)** and
have the step revise the file in the conversation's folder instead.

## Set a schedule

Under **When**, choose **Now**, **Later** for a single date and time, **At an
interval** to repeat every so many minutes, or **On a schedule** for a time of
day on chosen weekdays and time zone. The form shows the next three run
times. To change a schedule later, use **Pause** or **Edit timing** on the
**Scheduled** tab.

- A time skipped by the spring daylight-saving change doesn't run. A time
  that occurs twice in autumn runs once, the first time.
- Intervals stay tied to the first run time you chose.
- Missed runs are combined into one, and runs from one schedule never
  overlap.
- Each scheduled run gets its own conversation, with the original
  conversation's settings.

### Keep Pipeline running

Automations run only while Pipeline is open and the computer is awake.
Pipeline doesn't wake a sleeping computer. Turn on **Keep running when the
window closes** on the Automations page to keep Pipeline in the system tray
after you close its window. Quitting stops all automations until you reopen
Pipeline.

## Wait, pause, or stop

When an automation waits for input, its question appears on the automation.
Type your answer and choose **Provide input**. Waiting uses no model calls.

**Pause** lets the current step finish, then holds the next one. **Stop**
cancels the current work. Saved results stay available.

If Pipeline loses track of whether a step finished, for example because the
app quit, the automation asks for attention:

- **Check saved results and update status** looks for a saved reply, review,
  or check result. Use it before retrying.
- **Use current conversation context** accepts changes made to the
  conversation since the automation started.
- **Retry unfinished actions** runs the unfinished steps again.

**Activity log** keeps every earlier attempt and its output.

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

**Export brief…** saves the brief as Markdown. **Export complete automation
record…** on the Activity tab saves everything as JSON.

**Pause research automation** lets the current action finish and holds the
next. Answering a question doesn't resume a paused run; choose **Resume
research automation**. **Stop research automation** cancels the current
action.

The automation runs only the checks and analyses you chose. If their scripts,
inputs, or permissions change, it asks you to approve them again. The
investigator can edit files only if you turn on **Allow edits in the working
copy**. The source conversation must work in a working copy, with **Allow
edits** on. Edits stay in the working copy until you accept them in
**Review edits**.

## Research discovery

Research discovery turns a topic into proposals, investigations, and reviewed
paper drafts. It uses your ChatGPT connection. Open **Automations → Research**
and choose:

- **Research discovery — choose projects yourself** pauses at a shortlist.
  Pick proposals under **Choose projects to develop**, then choose **Develop
  selected projects**. It waits as long as you need.
- **Research discovery — let Pipeline choose projects** picks proposals itself
  and continues without questions.

Describe your topic under **Research topics and questions**. Under **Project
context**, choose a project conversation or **Create a project for this
research**. Then choose **Start research discovery**.

Each chosen proposal gets its own author conversation and folder. Referees
see the research record and manuscript, not the author's chat.

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

Each manuscript includes Markdown, LaTeX, BibTeX, evidence references, and a
response to reviewers. **Export papers and research** saves a ZIP. **Export
portfolio report** saves a Markdown summary.

## In a project

A project's **Automate** area has three views.

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
| Actions per automation | 64 | Up to 64 |
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
