# Changelog

Changes in each Pipeline release, newest first. Development before the first
public release was not tracked as a stable release series.

## Unreleased

- Installing the PaddleOCR-VL Full Parser works again. In 1.2.5 and 1.2.6 the
  install stopped with `HTTP 302 Found`.
- Engine installs show each step, a progress bar, the download size, speed, and
  time left. The installer log moved behind **Show details**.
- Engine downloads survive dropped connections: they reconnect and continue from
  the last byte received. If one source for Python tools fails, they use the
  other.
- PDF Reviews keep reference lists as structured entries. The PaddleOCR-VL Full
  Parser previously lost them during layout analysis, so reference pages fell
  back to plain page text, and entries on the page where the list began could
  be dropped without warning. Reviews extract these PDFs again automatically.
- PDF Reviews no longer read made-up numbers from plots. When the PaddleOCR-VL
  Full Parser mistook a plot for a picture, it could write out a table of values
  the plot does not print, and Reviews treated that table as text from the paper.
  That output now stays out of the document, and quotes from it no longer verify.
  The figure keeps its image, with a note to read it from there. Text in real
  pictures, such as a scanned press release, is unchanged, though a picture of a
  real table is occasionally treated the same way. Reviews extract these PDFs
  again automatically.
- Pages that hold only figures keep their chart images instead of being replaced
  with rearranged label text.
- Text between math inequalities such as `ρ < 0` and `ρ > 0` is no longer removed
  from extracted blocks.
- Reviews now warn when a page without figures recognizes less than 80 percent of
  the text in the PDF.
- **Quality and limitations** lists each extraction note once.
- Evidence marked **Not checkable** no longer makes a Review Degraded, and
  **Quality and limitations** counts it separately.
- A quote from an outside source is marked **Not checkable** instead of **Quote
  not found** when its evidence names a URL or DOI.

## 1.2.6 — 2026-10-30

- Conversation command and browser setup uses a managed runtime, with automatic
  downloads, progress, cancellation, and retry. A separate Node.js installation
  is not required.
- Edit conversations prepare working copies automatically and collect changes
  for **Review edits**. Accept, discard, conflict checks, and undo remain
  available. **Direct edits** supports configured research commands and projects
  that are too large to copy.
- Keep the README title unversioned while checking the version badge against
  the application version.
- Clarify conversation file editing, internet settings, and command setup in
  the README and connection guide.

## 1.2.5 — 2026-10-30

- ChatGPT keeps recent conversation history when settings, project moves, or
  recovery require a new native thread. Research automations recognize final
  structured answers from Claude Code and API connections after tool updates.
- Automations require refreshed context after a connection change and stop
  before sending a prompt when cancelled during setup.
- Web page reads preserve paragraphs, lists, and table columns. Release
  qualification binds evidence to the candidate version and each platform's
  installer name and checksum.

- Connection names and model information are shared across reviews and
  conversations. Gemini review steps apply the selected thinking level,
  and Anthropic review steps use the model's listed thinking options.

- Declaude reviews formulaic prose with one to three topic reviewers and
  produces a Markdown report with original passages, suggested replacements,
  and explanations suited to the document's audience and style.

- Reviews support web search and public page reads through OpenAI API and
  local/custom servers with tool-capable models. Search follows each step's
  access setting, time limit, and cancellation.

- Conversations support OpenAI, Anthropic, and Gemini API keys, plus local or
  custom OpenAI-compatible servers. Choose the connection and model in the
  conversation picker; enabled project and research tools use the same access
  settings as subscription conversations.
- API conversations stream replies, offer model-specific thinking controls,
  and support web search and an optional sandboxed command engine on macOS
  and Linux. Automations keep saved model choices, and discovery authors can
  edit their isolated paper folders through project file tools.

## 1.2.2 — 2026-10-29

- Correct Windows release verification for installation paths with mixed separators.

## 1.2.1 — 2026-10-29

- Windows release builds sign the application and installer with Azure Artifact
  Signing and verify the installed files before uploading the release.

## 1.2.0 — 2026-10-29

### Added

- ChatGPT and Claude conversations share recoverable file creation, public page
  reads and downloads, and optional browser interaction. Assistant settings
  separately control web search, command internet access, allowed command sites,
  and browser interaction. Commands can write inside isolated working copies.
- Claude can use the command sandbox inside a configured WSL2 distribution on
  Windows. Usage reports for both providers are saved per account and show
  their reporting time and separate usage windows.

- Reviews check every quoted passage against your document or the cited source
  file and label it **Quote found**, **Partial match**, **Quote not found**, or
  **Not checkable**. The quality summary counts each label. **Quote found**
  means the passage is where the comment says; whether the comment is right is
  still your call.
- The review designer can add **Pipeline checks**, steps Pipeline runs itself
  without a model. **Reference check** reports undefined citation keys,
  duplicates, and malformed DOIs. With lookups on, it checks each reference in
  Crossref for missing records, mismatched details, other versions,
  retractions, expressions of concern, and corrections. **Replication package
  check** reads a package folder against data-editor requirements without
  running it. The gallery templates **Reference Check** and **Replication
  Package Readiness** use these checks, and older versions of Pipeline refuse
  workflow files that contain them.
- The review preview estimates model calls, tokens, and cost at API list
  prices. **Settings → Reviews → Review limits** caps cost, calls, or time per
  review. At a limit, running steps finish, later steps are marked stopped, and
  the partial report is saved. The CLI's `run` and `batch` accept `--max-cost`,
  `--max-calls`, and `--max-minutes`, and `check` prints the estimate.
- Export a review as an offline HTML file or a Word document. You can also
  export a copy of the reviewed `.docx` or PDF with each finding added as a
  comment or highlight on the passage it quotes. The original file is
  unchanged, and findings that can't be placed are listed separately.
- Local checks can run Stata do-files with an installed Stata on macOS,
  Windows, and Linux, and a shell alias such as `stata` still works. New check
  types for R, Python, Julia, Jupyter notebooks, and Quarto or R Markdown
  record the runtime version and lockfile hashes and leave notebooks unchanged.
  **Detect installed tools** finds Stata, R, Python, Julia, Quarto, Jupyter,
  and MATLAB without installing anything or starting licensed software.
- **Data & samples** imports Stata `.dta` files (Stata 8 and later) and Excel
  `.xlsx` worksheets with their variable labels, value labels, and formats.
  Stata extended missing values stay separate codes, and Stata files up to 2 GB
  can be catalogued without keeping a copy.
- New downloads from DBnomics (IMF, OECD, Eurostat, ECB, central banks, and
  more) and the World Bank need no key and record each dataset's license,
  required notice, and suggested citation. FRED data shows the notice its API
  terms require, with a link to the terms.
- A **Get started** guide opens on a new installation. It shows which model
  connections are ready, checks this computer for Stata, R, Python, Jupyter,
  Julia, Quarto, and MATLAB, and suggests a first step for your kind of work.
  It changes no settings; reopen it from Home or **Settings → General → Guided
  setup**.
- **See an example Review** shows sample findings, evidence, and quote checks
  on a fictional paper, without calling a model or adding to review history.
- An optional **Crossref contact email** in the **Reading inbox** sends your
  own address with literature lookups, never a shared one.
- New built-in **Slide Review** (`slide-review`) uses Paper Review's reviewers
  on a slide deck exported as PDF and cites slides by page number. **Paper
  Review** also recognizes a deck and applies the same guidance.
- Project conversations can use **Claude Code** with your own Claude
  subscription, chosen per conversation in the **Model and thinking** picker.
  Existing conversations stay on ChatGPT. The picker shows your latest Claude
  usage, and new Claude conversations are titled automatically.
- Claude can use the project context, the research tools you turned on, and
  the project's files. Edits need the **Allow edits** access mode. On macOS and Linux
  it can run shell commands in a sandbox that reads the project but can't
  change it or reach the network.
- On Windows, projects can use working copies, and you can accept edits in
  **Review edits**.

### Changed

- Updated dependencies used for document rendering, downloads, encrypted
  connections, and archive handling to address security advisories.

- Automatic Paper Review (Full) is now **Paper Review** (`auto-paper-review`),
  and Automatic Paper Review (Quick) is now **Paper Review (Fast)**
  (`auto-paper-review-fast`). Your copies and edits move over on first launch
  and keep any custom name. Automations and scripts that use `auto-review` or
  `auto-review-quick` keep working.
- Reviews queue instead of blocking. **Settings → Reviews → Simultaneous
  Reviews** sets how many run at once (default 2, at most 4). The sidebar lists
  running, queued, and recent reviews with their stage and elapsed time, and a
  footer indicator shows active reviews from any page.
- Selecting several documents in **New review**, or re-running from
  **History**, queues one review each. Each review keeps its own log and token
  totals, and cancelling one never stops another. Batches, automations, and
  history purges still run alone.
- Pipeline checks for updates about once a day while it runs, not only at
  startup, and the banner's **Download** button opens the installer for your
  computer. Turn this off with **Check for updates automatically** in
  **Settings → Advanced & About**. If GitHub limits a shared network, Pipeline
  waits as long as GitHub asks.
- The review designer's workflow graph runs top to bottom, frames parallel
  stages together, and wraps wide stages onto new rows.
- Labels, help text, and messages are clearer throughout the app. Saved
  settings and behavior are unchanged.
- The README and public guides are rewritten in plain language. The privacy
  and security pages cover the shared ChatGPT sign-in, the recoverable Trash,
  and current release checks.

### Fixed

- In the review designer, **Restore adaptive router prompt & schema** restores
  the open built-in's own version. It no longer removes **Paper Review
  (Fast)**'s limit of two method reviewers, and **Slide Review** keeps its
  slide-deck setup.
- The automation builder's **Action limit** goes up to 1,024 actions, the
  limit Pipeline accepts when it checks an automation. It previously stopped
  at 64.

### Security

- Before Claude subscription calls, Pipeline confirms that Claude Code is
  signed in to a Claude subscription: once per review and before each
  conversation turn. These calls ignore API keys inherited from your
  environment and never fall back to a saved API key. API mode stays a separate
  choice for reviews.

## 1.1.1 — 2026-10-22

### Added

- Reviews accept Markdown and plain-text files, including drafts written by
  research automations.

### Changed

- Permission requests in conversations show the full access requested, the
  working folder, and how long the permission lasts.

### Fixed

- Failed approvals refresh pending requests and can't be answered twice.
  Discarding an unreadable API key keeps unsaved settings, and a late new
  conversation no longer overrides where you navigated.
- Feedback validation keeps findings past its call limit. Merges keep every
  original finding and its decision, even after a usage-limit fallback.
- Cancelling an experiment as it starts works reliably, and recovery records
  the exact plan or settings that were tested. Replication packages keep
  executable permissions without letting files run.
- A failed computation can't complete a research automation, and a failed
  automation turn can be retried instead of being marked unknown.
- Conversations no longer hang on shutdown or while answering a permission
  request. `pipeline-cli` exits with an error when it can't write its output,
  but stays quiet when the output pipe closes.
- LaTeX extraction follows `\subfile`, `\import`, and includes with extra
  spaces, and attaches figures only when a label or page matches. Word
  extraction keeps tabs, breaks, special characters, and whole equations, and
  notes any equation too large to keep.
- The installed `pipeline-cli` finds its bundled PDF tools. Research commands
  find programs on your PATH consistently, including Windows `.exe` LaTeX
  engines.
- Project Markdown previews show images and follow absolute links within the
  project folder, including on Windows.
- Automation deadlines use the chosen date's daylight-saving offset and reject
  local times that don't exist.
- Both conversation **Stop** controls and **Export conversation** report
  failures and allow a retry. A finished export shows where it was saved.

### Security

- Prompt placeholders are filled once, so placeholder text quoted in a document
  can't reveal other inputs or variables.
- Excluded sources stay hidden from the assistant's tools, including cached
  reads.
- On Windows, project file reads can't escape the opened folder through
  junctions. Mixed-case names and Windows path aliases can't bypass file-access
  rules or excluded folders.

## 1.1.0 — 2026-10-15

### Changed

- Automation step IDs can't be `done`, `enter`, or a plain number, which caused
  steps to be skipped. Upgraded installations get the current Grant Review
  suite.

### Fixed

- A message rejected after sending returns to the composer instead of being
  erased. Reconnected conversations keep their message order, keep up with
  automation turns, and unlock the composer when the connection drops.
- With a custom data folder, opening History or a project during a review no
  longer marks it interrupted or lets it be deleted. Restoring a review from
  Trash works on Windows default storage and symlinked data folders.
- Re-running or resuming a review whose source is over 50 MB or 500 files no
  longer fails its integrity check.
- Parallel review steps on ChatGPT recover when the connection lags during a
  tool call. Steps that continue a Claude CLI session can write supporting
  files again.
- Quality reports count only real retries, so steps that succeed first time
  aren't labeled degraded.
- An invalid built-in workflow no longer blocks every launch, and resetting it
  repairs it. Settings backups from 0.9.x import again, and an API key that
  can't be decrypted can be discarded in Settings.
- Staged LaTeX projects keep figures found through `\graphicspath` or in
  chapter files.
- Cancelling PaddleOCR-VL verification no longer blocks the next run, and
  reinstalling a current parser no longer rebuilds it.
- Importing the same Zotero item or findings into a second project no longer
  overwrites the first project's record. Items imported by 1.0.2 and earlier
  aren't duplicated on re-import.
- Commands that start `/bin/zsh`, including Stata, work when `~/.zshrc` or
  `pyvenv.cfg` is a symlink.
- Saving a file in a folder that task capture skips, such as `node_modules`, is
  refused instead of deleting the original on acceptance.
- In Sources, clicking another page after opening a citation no longer jumps
  back.
- Research discovery runs ask for attention and offer **Retry** instead of
  stopping as blocked when a conversation or setting changes or one turn fails.
- Research automation retries work after you steer the planner. Long check
  instructions and `{{input:…}}` text in research content no longer block
  results.
- A review that finished while its automation lost track of it can be
  reconciled instead of re-run. Automations waiting for a slot log the wait
  once, not every two seconds.
- The manuscript reader counts the last PDF page, and plain editor saves no
  longer leave a task and a copy folder behind.
- Project packages (`.pwex`) re-import without false evidence conflicts and
  include every selected note and retired claim.
- Paper and project search handle single-line files and punctuation-only query
  words.
- `pipeline-cli` exits cleanly on a closed pipe, accepts a bare `.tex` name,
  and resolves relative input paths so the app can file those runs under a
  project.

### Security

- Research backups (`.pwrx`) and project packages (`.pwex`) reject entries
  whose stated size doesn't match, so a crafted file can't fill the disk or
  memory. Restores reject references outside the research store.
- Steps that continue a Codex CLI session set their own sandbox and approval
  policy instead of inheriting your global Codex settings.
- The file viewer no longer offers to open programs written during a review,
  such as `.exe` or `.dmg` files.

## 1.0.2 — 2026-10-08

First stable release, with installers for macOS (Apple Silicon and Intel),
Windows x86-64, and Linux x86-64. The `v1.0.0` and `v1.0.1` tags were never
released because a complete set of installers couldn't be built from them.

### Added

- Projects, where you work in ChatGPT conversations alongside your reviews.
  Conversations have streamed replies, approvals, saved drafts, recovery after a crash, and model,
  effort, and usage controls.
- Optional research tools for papers and sources, local LaTeX and Stata runs,
  results, research notes, and claims with evidence you confirm. Project tools
  cover manuscript builds, referee responses, experiments, literature, and a
  **Theory** panel that keeps numerical checks separate from proofs.
- Project packages (`.pwex`) share chosen research items with a coauthor,
  without conversations, credentials, or command settings, and show conflicts
  on import. Research backups (`.pwrx`) restore a whole store.
- A storage report previews cleanup you can undo. A project can export a draft
  workflow or send a fixed copy of a paper to a new review.
- **Agent profiles** are searchable built-in and custom profiles with editable
  system prompts and tools, plus writing, code review, and economics research
  starters. A profile can add to or replace Codex's base prompt.
- Projects in a Git repository show the branch, uncommitted work, recent
  commits, and commits waiting on the remote. Pipeline fetches with your own
  credentials only when you ask, and never commits, merges, or pushes.
- LaTeX extraction reads files the project references up to three folders
  above the selection, such as `\input{../output/numbers.tex}`, and notes each
  one.

### Changed

- Reviews and conversations share one ChatGPT sign-in and model list. Existing
  sign-ins move over when that's unambiguous, you choose between conflicting
  accounts in Settings, and account changes wait for active work.
- The composer is rebuilt. One picker sets model and thinking and shows your
  remaining usage, Send and Stop share a button, and the **+** menu adds files,
  a project document, or an automation. Attached sources appear as chips with
  roles such as "Main paper".
- **Projects** opens a searchable index with pins, recent activity, and links
  to conversations. A new project can start with a conversation, and Home opens
  straight to its action cards.
- Project overviews report the paper's state: version, last build, open and
  regressed findings, numbers and claims that no longer match results, pending
  decisions, and changes since your last visit. Setup controls and an optional
  target date are in **Project settings**.
- Settings has seven searchable categories and separate conversation and
  review connections. New preferences cover text size, editor wrapping,
  spacing, message shortcuts, startup, and notifications with optional sound.
- Auto Paper Review is the default and replaces Paper Review (Full) and Paper
  Review (Quick). Retired profiles and your edits are archived in
  `~/.pipeline/profiles/.retired-builtins/`, and their step prompts stay in
  the workflow editor.
- Research backups (`.pwrx` format 2) include conversation working files, and
  older versions can't read them. Backups leave out external project folders,
  review and automation history, Trash, and credentials.
- Google models run only through the Gemini API with a Google API key.
  Workflows that name `gemini` must switch to `antigravity`, and the `pro`,
  `flash`, and `flash-lite` aliases no longer work.
- macOS builds from source can be unsigned for local use. Official releases
  are signed.

### Fixed

- On Windows, conversations can read project files, and Windows backups pass
  their integrity check.
- Opened from the Dock or a desktop launcher on macOS and Linux, Pipeline finds
  command-line tools more reliably, including with fish, csh, tcsh, nushell,
  and shells that print startup messages.
- A provider CLI that crashes or runs out of memory is reported as a retryable
  failure, not "Pipeline cancelled".
- One unreadable file no longer stops a folder input, and folders with
  multi-gigabyte data files run.
- PDF extraction works despite denied macOS folder permissions, reports
  encrypted PDFs, retries failed chunks, accepts PDFs up to 256 MB, and opens
  OneDrive placeholders on Windows. Provider limits (100 pages or 24 MB) are
  stated up front.
- Steps fall back to ordinary calls when shared context or a shared Claude
  session is unavailable, instead of failing the review.
- Auto Paper Review rejects a folder chosen as a source tree and resumes after
  the reviewer count changes. The results view no longer crashes when a
  paper's authors are missing.
- The dependency check requires every provider a workflow uses.
- Finished parallel steps are saved as each completes, and retention can't
  delete a just-finished review. Windows builds support long paths.
- LaTeX extraction skips commented-out and circular `\input{}` lines and notes
  unreadable includes instead of dropping chapters.
- A PaddleOCR-VL install survives an unrelated review finishing or being
  cancelled.
- Workflow drafts survive a damaged recovery cache, and moving between projects
  or automations protects unsaved work.

### Security

- Updated rustls and several development dependencies to patched versions.

## 0.9.0 — 2026-08-10

### Added

- **Paper Review (Auto)** reads the paper, picks one or two subject reviewers
  and up to four method reviewers from a catalog covering 29 disciplines, and
  checks every combined comment against the paper. It also identifies the kind
  of paper, such as a replication or null result, so reviewers judge it on its
  own terms.
- Projects group related runs and revisions, with an issue list that tracks
  issues, decisions, and regressions across runs and exports to Markdown.
- A run preview shows the timeline, provider and file access, and expected
  model work before launch.
- Issue cards link their evidence to pages and files in the run.
- A Workflow Gallery offers editable templates for revision responses,
  literature positioning, thesis chapters, and rubric-based review.
- Settings and workflows warn about unsaved changes, and deleting stored data
  previews what will be removed.
- Supported-platform and maintenance policies, contribution guidelines, and
  issue forms.

### Changed

- Local OCR uses PaddleOCR-VL Full Parser only, and settings that chose the
  fast Q8 extractor switch automatically.
- The source-build CLI can pick a workflow per run and install, repair, or
  uninstall the PaddleOCR bundle.
- Update checks offer stable releases only. The main window has a minimum
  size, and the Windows installer includes WebView2 and refuses downgrades.

### Fixed

- A PaddleOCR-VL parser installed by an older version keeps working after an
  update.
- Unsupported inputs fail before the run, and readiness checks include the
  exact PDF tools each extraction method needs.
- Stopping a folder watch shows at once. Readable text is kept when a provider
  can't accept a large structured bundle.
- Saved-run export is complete and all-or-nothing, with a new destination each
  time.
- Math renders offline, PDF printing waits for fonts, and saves handle edits
  made during a save.

### Security

- Models see only a private copy of the selected document, and LaTeX projects
  expose only the files the document uses.
- Re-runs recheck saved inputs and the run budget before contacting a model.
  The app console shows at most 16 KB of each request.
- Added a data-sharing notice before each run, privacy documentation, and a
  private vulnerability-reporting policy.
- Updated frontend dependencies to clear every npm audit finding.
- Each installer is built from a verified tag and comes with a SHA-256
  checksum, a GitHub build attestation, and software bills of materials.
