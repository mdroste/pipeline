# Analyze and write

This guide covers the **Analyze** and **Write** areas of a project. Running
your own code needs an attached project folder. See
[Projects and conversations](projects.md) for creating projects and the other
areas.

## Analyze

Analyze runs your code on your computer and keeps the results with the
project. A typical order:

1. Set up a command in **Command settings**.
2. Save its inputs in **Saved analysis plans** and run it.
3. Import the results file into **Experiments** and compare it with your
   baseline.

### Results

**Results** lists every run of your commands under **Run history**, with its
outcome, command, exit status, log, and outputs. **Stop execution** stops a
queued or running command.

Up to two commands run at once, and up to 16 can wait. Commands keep running
if you stop the assistant. Quitting Pipeline stops them, and they don't
restart on their own.

### Experiments

**Experiments** compares a baseline with alternatives.

- **Experiment notebook** records the research question, the baseline, the
  intended change, the runs involved, and your interpretation.
- **Saved results** imports a results file from a completed run with
  **Import research-results-v1 / v2**. Pipeline records which run and file
  each value came from. Write the file with the
  [result exporters](../examples/research-exporters/README.md).
- **Compare results** shows estimates, uncertainty, and sample sizes side by
  side, with signed and absolute changes. An alternative specification needs a
  **Comparison rationale**. Relative change appears only when you mark the
  denominator as meaningful. Different units need a conversion you record.
- **Specification, sample and calibration metadata** records controls,
  weights, inference, and convergence. Each field shows whether you supplied
  it or Pipeline inferred it.
- **Impulse responses** compares two series when their variable, units, shock
  normalization, and horizons match.

Missing values stay missing in every comparison.

### Result links

**Result links** ties numbers in your manuscript to saved results. Select the
number in **Documents**, then use **Bind a reported number** to link it to a
result, with its decimal places and units. Choose **Save numeric binding**.

**Headline-result coverage** marks each link as current, stale, unknown, or
unavailable, and flags rounding, sign, and unit mismatches. After inputs
change, choose **Refresh dependent results and passages**.

**Create a TeX macro from this value** writes the number as a macro in a
working copy. Accept it in **Review edits**.

### Data & samples

**Data & samples** catalogs datasets and samples. Choose **Import data file**
to add one:

| Format | What Pipeline reads |
| --- | --- |
| CSV, TSV | Up to 32 MB, one million rows, and 2,000 columns |
| Excel `.xlsx` | One sheet, the first or the one you name, up to 32 MB. Pipeline keeps saved cell values and doesn't recalculate formulas. Dates stay Excel serial numbers, with their formats recorded. |
| Stata `.dta` | Stata 8 and later, with variable labels, value labels, and formats, up to 2,000 variables. Files up to 32 MB are copied and can be previewed. Files up to 2 GB are read once for the dictionary and missing-value counts, with no copy or preview. |

Password-protected workbooks, `.xls`, and `.xlsb` files can't be imported.
For Parquet, SPSS, SAS, or data stored elsewhere, use
**Import a data dictionary** instead. See
[Register a dataset without importing it](../examples/research-exporters/README.md#register-a-dataset-without-importing-it).

Empty cells and the text NA, N/A, NaN, null, and `.` count as missing. In
Stata files, `.` and `.a` to `.z` count as missing, each tallied separately.
Excel errors such as `#N/A` count as missing too. Zero is always a value.

The dataset view shows each variable's label, format, value labels, and
missing count. **Preview local rows** shows up to 25 rows.

**Declare a sample** records which datasets a sample uses, with its inclusion
rules, unit of observation, weights, and dates. Your own code applies the
rules.

**Project sharing policy** controls which dataset details search,
conversations, and packages can use:

| Setting | Default | What it allows |
| --- | --- | --- |
| **Include variable descriptions in search and conversations** | On | Variable names, labels, value labels, formats, and source terms |
| **Include data summaries in search and conversations** | Off | Missing-value counts and other summaries |
| **Allow the assistant to preview rows** | Off | The assistant can read a few rows |
| **Allow raw data in sharing packages** | Off | Selected data files can go into project packages and replication capsules |

#### Download public data

Downloads need **Allow Pipeline to search for literature and download data for this project**,
set in **Reading inbox**.

- **FRED / ALFRED vintage** takes a series ID, a vintage date, and your FRED
  API key. Pipeline uses the key for that request only and doesn't save it.
- **DBnomics series** takes a series ID from db.nomics.world. DBnomics
  gathers series from the IMF, OECD, Eurostat, the ECB, and central banks. No
  account is needed.
- **World Bank indicator** takes an indicator code, up to 60 economy codes or
  `all`, and optional years.

FRED and DBnomics downloads can have up to 100,000 observations, and World
Bank downloads up to 20,000. Narrow a larger request. Each download becomes a
dataset with its query, license, suggested citation, and notice, shown under
**Source terms and citation**.

Some FRED series belong to other copyright holders and need their permission
beyond personal use. The recorded license says so. For IMF series from
DBnomics, bulk automated download and model training need the IMF's
permission.

FRED's terms require this notice wherever FRED data appear. Pipeline shows it
on the FRED form and with every FRED dataset:

> This product uses the FRED® API but is not endorsed or certified by the
> Federal Reserve Bank of St. Louis. By using FRED® data, you agree to be
> bound by the
> [FRED® API Terms of Use](https://fred.stlouisfed.org/docs/api/terms_of_use.html).

### Command settings

**Command settings** sets up the programs Pipeline runs on your computer.

1. Under **New execution profile**, name the command and choose its kind.
2. Enter the arguments as a JSON list, then the input files and expected
   outputs. Give scripts as paths inside the project folder.
3. Choose **Save profile**.
4. Choose **Test**. **Review command before running** shows the exact command,
   folder, inputs, and outputs. Choose **Authorize this command and test**.
5. After a passing test, **Run** works the same way.

| Kind | Runs | Also checks or records |
| --- | --- | --- |
| Stata do-file | Your installed Stata in batch mode: `stata-mp -b do main.do`, or `StataMP-64.exe /e do main.do` on Windows | The log must be an expected output. A log with an error code such as `r(198)` fails. |
| R script | `Rscript analysis.R` | The R version, `renv.lock`, and `.Rprofile` |
| Python script | `python3 analysis.py`, or the project's `.venv` interpreter | The Python version, and files such as `uv.lock`, `requirements.txt`, and `environment.yml` |
| Julia script | `julia --project=. analysis.jl` | The Julia version, `Project.toml`, and `Manifest.toml` |
| Jupyter notebook | `jupyter nbconvert --execute`, into a new file | The executed copy must be an expected output. Your notebook stays as it was. |
| Quarto or R Markdown document | `quarto render report.qmd` | The rendered file must be an expected output |
| LaTeX | latexmk, pdflatex, xelatex, lualatex, or tectonic | Shell escape stays off. The PDF must be an expected output. |
| Command | Any program | Expected outputs only |

Arguments go straight to the program, so shell operators such as `;`, `|`,
and `$` aren't allowed. Your approval covers that exact command. If you change
the program, a script, or a lockfile, Pipeline asks you to approve it again.
Commands run with your computer's file and network permissions.

**Detect installed tools** looks in standard install folders, on your `PATH`,
and in the project folder, such as a `.venv` environment. Choose **Use** next
to a tool to fill in its path.

If you start Stata through a shell alias, choose **Stata do-file** and enter a
launcher such as `/bin/zsh -lic "stata -q -b do main.do"`. When you stop a
run, the launcher gets up to 30 seconds to exit before Pipeline forces it to
stop. An installed Stata stops at once. After a forced stop, check that Stata
has exited and that anything your launcher changes, such as the clock, is
restored.

### Saved analysis plans

**Saved analysis plans** saves a command's scripts and inputs, so each run
starts from the same files in a new working folder.

1. In **Command settings**, list every script, helper, data file, and lockfile
   as an input.
2. Here, **Choose a configured profile**. Optionally pick exact datasets and a
   sample, set **Execution parameters** as JSON, and set a **Random seed**.
3. Choose **Capture inputs and review plan**. Inputs can total up to 256 MB.
4. Check the plan, then choose **Allow this plan to run on this computer**.
5. Choose **Test saved plan**. After a passing test,
   **Run in a new working folder** runs it again from the saved copy.

Each run folder contains `pipeline-parameters.json`. Pipeline sets two
environment variables: `PIPELINE_PARAMETERS_FILE`, the path to that file, and
`PIPELINE_RANDOM_SEED`. Your script must read and apply both.

### Specification grids

**Specification grids** runs a family of specifications from one saved
analysis plan.

1. **Choose a saved analysis plan**.
2. Choose **Add factor** for each thing that varies, and list its values. Use
   up to 8 factors, 32 values per factor, and 64 specifications in all.
3. Add **Exclusion rules** with reasons, and write your interpretation before
   you see outcomes. Set **Maximum specifications** and
   **Total time limit (seconds)**.
4. Choose **Prepare complete grid**, review it, and choose
   **Authorize this family and start**.

Pipeline runs the specifications one at a time, each from a fresh copy of the
saved inputs. Your script reads each specification's values from
`PIPELINE_PARAMETERS_FILE`. The list keeps every outcome in your declared
order, including failures, timeouts, and exclusions. You can pause or cancel
a running grid. If Pipeline restarts, the grid waits for you to resume it.
**Export all run attempts** saves the full record.

### Theory

**Theory** holds your working notes on models and proofs.

- **Theory notes** record assumptions, conjectures, propositions,
  derivations, proof sketches, unresolved steps, counterexamples, and
  rejected approaches. Link each note to assumptions and saved passages or
  equations. The status is your judgment, but a note with unresolved steps
  can't be marked supported.
- A rejected approach needs a reason. Future conversations see it, so the
  assistant can avoid repeating it.
- **Checks for the selected note** records analytical, symbolic, numerical,
  heuristic, or model-assessment checks. A numerical check covers only the
  cases tested. Each note's evidence label comes from its checks.
- **Stage derivation into manuscript** inserts a derivation into a `.tex` or
  `.md` file in a working copy. A comment lists its assumptions and unresolved
  steps. Accept it in **Review edits**.
- **Research directions** records an idea's question, mechanism, first test,
  and next action. **Convert to task** turns one into an action item.

### Symbols & assumptions

**Symbols & assumptions** keeps your notation consistent.

- Record each symbol's notation, aliases, scope, definition, domain, and units
  with **Save notation revision**.
- **Find symbols in the source** suggests symbols from your TeX. Keep the ones
  you want in **Review suggested symbols**.
- **Find notation conflicts** lists symbols defined differently in
  overlapping sections. Check each one against the text.
- **Try a different assumption** copies a theory note into a new branch with
  other assumptions.
- **Record a computational check of an assumption** records a limiting case,
  accounting identity, comparative statics, dimensional check, or
  counterexample search, with the run that tested it.

## Write

### Manuscript

**Manuscript** edits TeX, Markdown, BibTeX, and text files up to 2 MB. Under
**Edit in**, choose the **Project working copy** or an edit's copy. Unsaved
drafts are kept. If the file changed elsewhere, Pipeline stops the save so you
can compare first.

To build the manuscript:

1. Under **Build settings**, choose the engine: pdflatex, xelatex, lualatex,
   or latexmk. Set the root TeX file, the expected PDF, and a timeout.
2. List every section, bibliography, table, and figure the build uses.
   Pipeline tracks changes only in the files you list.
3. Choose **Save build settings**, then **Review test run** and
   **Authorize and start test**. After a passing test, build the same way.

Shell escape is always off. A successful build keeps its PDF, log, and
SyncTeX file. A failed build lists its errors with file and line, and shows no
PDF. Choose an earlier successful build to see one.

**Show build PDF beside source** opens the PDF next to the editor.
Double-click the PDF to find the source line.
**Build history and PDF comparison** compares two builds.

### Reviews & findings

**Reviews & findings** shows reviews of this project's documents. Attach a
folder to connect them. **Start a review** opens a new one.

**Findings across reviews** follows each finding through later reports as
**Open**, **Regressed**, **Addressed**, or **Dismissed**. A finding stays
open when a later report omits it, until you mark it addressed. You can also
combine duplicates. See [Reviews](reviews.md) for reading a single report.

### Responses

**Responses** organizes your replies to referees and editors.

1. Under **Import referee or editor comments**, choose
   **Import local report**, then **Preview paragraph split**. Check the
   numbering and choose **Import selected comments**. You can also import
   findings from a finished review with **Enable importing Workflow findings**.
2. In the **Response matrix**, give each comment a category, severity, and
   disposition: addressed, deferred, or rejected. Draft the reply and link the
   manuscript change, run, or evidence behind it. Choose
   **Save response decision**.
3. Under **Response-letter export**, choose Markdown or LaTeX, then
   **Preview selected responses and check links**.

The preview flags replies that claim an added analysis without linked
evidence, empty drafts, and deferrals or rejections with no reason. A flagged
letter exports as a draft.

**Focused re-review** sends only the passages you changed, and their
context, to a new review. Choose them, then
**Preview review of selected passages**.

### Review edits

**Review edits** lets the assistant change files in a separate copy. Your
folder changes only when you accept.

1. Under **Start an edit**, choose the action item the edit is for and tick
   the files the assistant may change.
2. Choose a **Working copy** method. **Plain copy of the files** copies them.
   **Git branch in a separate worktree** puts them on a new `codex/` branch.
   Then start the edit.
3. Choose **Open conversation** to work with the assistant in the copy.
4. When it's done, choose **Check for changes** and read each file's diff.
5. Tick the files to keep and accept them. Within a text file, you can accept
   only selected changes. **Discard this edit** drops the proposal and keeps
   the copy for reference.

Wait for the assistant to finish before you check for or accept changes. A
file can be accepted only if your original hasn't changed since the copy was
made. Afterward, rebuild the manuscript or rerun your checks.

**Accepted edits** lists what you've accepted. **Undo** reverses an
acceptance while the files still match it. If accepting was interrupted,
**Restore original files** puts the originals back.

Pipeline never commits or pushes. With a Git branch, you decide what to
commit.

### Tables & figures

**Tables & figures** builds tables and figures from saved results.

- Choose **Table: Markdown / TeX / CSV**, **Coefficient and interval plot**,
  or **Impulse-response overlay**.
- Add results in order, set labels and decimal places, and choose
  **Saved confidence interval**, **One standard error**, or
  **No uncertainty displayed**.
- Entries must agree in estimand, units, and transformation. Different
  samples need a written rationale.
- **Enter a value manually** needs a reason. The row is labeled as entered by
  you, and the original estimate is kept.
- A table holds up to 40 entries, and a figure up to 12 series.

Tables come out as Markdown, TeX, and CSV. The unrounded values are saved
too.

Figures run a Matplotlib script on your computer. Choose a Python profile that
has Matplotlib, then **Preview figure setup**, **Review figure execution**,
and **Allow this plotting command to run**.
**Create figure and save files** produces SVG, PDF, and PNG files. Then choose
**Save figure to Tables & figures**.

Each change makes a new version. Export the files, or stage them into an edit
and accept them in **Review edits**.

### Drafts & templates

- **Research starter kits** create an editable outline and two action items
  without a model. Kits cover literature synthesis, empirical research,
  quantitative models, theory, grant proposals, seminars, and teaching. Choose
  **Create this outline and action items**.
- **Build a draft** arranges your text, source excerpts, tables, and figures
  into sections. Pick a template, then **Generate a new draft version** to get
  Markdown and TeX.
- **Export draft with all asset files** saves a ZIP. The TeX is a fragment,
  so include it in a document that loads the `graphicx` package.
- **Main manuscript and supporting documents** gives your documents roles such
  as main, grant, or seminar. A main manuscript is optional.

### Coauthors & replication

**Prepare for coauthor review** writes a brief with questions, a change
summary, open issues, and comments on an exact manuscript version.
**Create review brief** saves it as a note. To send it, export a project
package under **Share selected work and import a coauthor’s changes**. Tick
**Research notes** and the sources the brief mentions.

**Replication package** builds a replication capsule from saved analysis
plans. **Inspect and import replication materials** opens one you received.
See [Replication capsule](projects.md#replication-capsule).

### Send for review

**Send for review** reviews a saved copy of your paper. Choose the
**Paper to review**, then **Review this revision…**. In the preview, choose
the review and models, then start it. The review uses your review settings
and account, and project notes stay out of it. See [Reviews](reviews.md).

### Share project

**Share project** exports and imports project packages. See
[Back up and share](projects.md#back-up-and-share) for what a package holds
and how conflicts work.

**Create a review design from saved research steps** turns completed action
items and theory notes into a draft review. Choose **Draft workflow**, then
**Save draft…**, and open the file in the review designer.
