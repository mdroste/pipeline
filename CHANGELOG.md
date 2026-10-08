# Changelog

This file records user-visible and release-integrity changes. Development before
the first public release was not maintained as a stable release series.

## 1.0.0 — 2026-10-07

First stable release. Installers are published for macOS (Apple Silicon and
Intel), Windows x86-64, and Linux x86-64.

- Patch rustls and affected frontend test-tool dependency versions. Remaining
  upstream WebdriverIO advisories and release qualification gaps are tracked in
  [the October 2 audit](ASTRA_PIPELINE_OCT2.md).

- Research backups now include app-owned conversation working files in `.pwrx`
  format 2. This build can still restore format-1 backups; older builds cannot
  read format 2. External project folders, Review/Automation stores, Trash and
  credentials are excluded. Imported trash journals are discarded and unfinished
  conversation turns are retained as interrupted history.
- Background conversation titles use their own native process. Workflow drafts
  survive recovery-cache failures, and same-page project/automation navigation
  saves or guards unsaved work and rejects superseded loads. Malformed route
  encoding and unreadable saved navigation fall back without breaking startup.
- Automation control evaluation and retained context are bounded independently
  of model actions. Settled failed/cancelled Review pins are reconciled. Host
  writes finish before their invocation releases ownership.
- Local macOS source builds may be unsigned. Official builds still require
  signing; bundled Poppler inventories and candidate qualification checks make
  release evidence explicit.

- The conversation composer is rebuilt. Model and thinking are one picker that
  shows your ChatGPT account, remaining usage, what each model is for, and the
  thinking levels it supports. The message field grows as you type, and Send
  and Stop are one button. The "+" menu now only adds things to a message:
  files, a project document, or an automation. Files can be dropped onto the
  conversation. Attached sources appear as chips with plain-language roles
  such as "Main paper" and "Referee report". Menus, the project switcher, and
  tooltips share one themed style, and dark mode uses one background across
  the project and chat panes.

- Project overviews now report the state of the paper instead of listing
  conversations and notes: the current version, last build, outstanding and
  regressed review findings, numbers and claims that no longer match their
  results, progress through review comments, and what changed since your last
  visit. Sections with nothing to report are hidden and every row has one
  action. The resume card recalls your last request and any unsent draft, a
  brief can be drafted from the paper, and Project settings has one optional
  target date. Projects with only conversations open directly on the latest
  conversation.

- Projects whose folder is a Git repository show its branch, uncommitted work,
  recent commits, and commits waiting on the remote, with links for GitHub
  repositories. Checking the remote runs `git fetch` with your own Git
  credentials only when you ask; Pipeline never commits, merges, or pushes.

- Home now starts directly with its action cards and current review, with
  introductory headings and helper copy removed.

- Settings connection cards now omit redundant provider descriptions, account
  explanations, and scope badges, and the save-status note is shorter.

- Conversations and Reviews now share one ChatGPT sign-in, model catalog, and
  account controls. Existing managed sign-ins migrate automatically when
  unambiguous; conflicting accounts can be selected in Settings. Account changes
  wait for active work in both modes while histories and permissions stay separate.

- Projects now opens a searchable index with pins, recent research activity, and
  direct conversation resume links. Project overviews show editable research
  briefs, pending decisions, and recent work; setup controls move to Project
  settings. New projects can begin with a conversation before adding a paper or
  folder, and leaving Workspace saves the current conversation draft.

- Removed the deprecated Antigravity CLI adapter, model discovery, authentication
  probes and installation hints. Google remains API-only and is labeled Google API
  in readiness checks; older provider IDs and saved settings still load.

- Settings now has seven searchable categories, compact Review model stages,
  visible separate conversation and Review connections, explicit API-key saving,
  and validated retention inputs. New device preferences control interface and
  report text size, source-editor text and wrapping, spacing, message shortcuts,
  startup restoration, and completion/failure/attention notifications with optional
  desktop delivery and sound. Backups and cleanup are under Data & Storage;
  diagnostics and version information are under Advanced & About.

- Workspace Agent profiles now provide searchable built-in and custom profiles,
  blank creation and duplication for one project or all Workspaces, explicit
  system-prompt and tool editing, and Writing, Code review, and Economics research
  starters. Profiles can inherit Codex or replace its native base prompt, with
  supplemental instructions kept separate. Codex default stays read-only, with
  model templates and additional sections viewable from local runtime caches,
  including source/version information and explicit cache limitations.

- Pipeline now presents two complementary agent-orchestration modes for
  academic research. The new Workspace is a persistent ChatGPT client with
  independent workspaces and conversations, isolated managed ChatGPT sign-in,
  visible model/effort and quota controls, streamed responses, approvals,
  durable drafts, and crash reconciliation. Its optional modular research
  harness adds editable presets and recipes, immutable papers and sources,
  bounded research tools, configured local LaTeX/Stata execution, structured
  results, research memory, claims/evidence with explicit human confirmation,
  and performance/evaluation records. Project Research tools add manuscript
  editing/builds, referee responses, experiments, result links, literature, and
  a Theory panel whose typed check receipts keep numerical evidence distinct
  from proofs and whose derivation promotion is a reviewable change set.
  Selective `.pwex` project exchange packages carry chosen research objects
  and their dependencies to a coauthor without conversations, credentials,
  or executable settings, import into a nonempty store with reviewable
  conflicts, and sit beside a storage report with previewed, restorable
  pruning and a Workflow-draft export for the Workflows page.
  Whole-store `.pwrx` archives use bounded
  inspection and explicit root remapping, and an optional immutable paper
  handoff enters the existing Workflow launch preview without sharing runtime,
  credentials, settings, or cancellation. Workflows remain the deterministic
  graph engine for repeatable research and review tasks. Authenticated,
  real-tool, packaged-app, and cross-platform Workspace qualification remains a
  release gate.
- A comprehensive default-workflow audit produced a batch of reliability
  fixes across platforms and input types:
  - GUI launches on macOS/Linux now resolve the login-shell PATH robustly:
    rc-file banners no longer corrupt it (the probe reads `printenv PATH` and
    takes the last output line), fish/csh/tcsh/nushell users get a working
    probe, a transient probe failure is retried instead of being cached for
    the whole session, and system `/opt/...` entries no longer suppress
    resolution on Linux. Command lookup also skips non-executable files that
    would shadow a real CLI later on PATH.
  - A provider CLI killed by a signal Pipeline did not send (out-of-memory
    kills, crashes) is now reported as a retryable provider failure instead
    of "Pipeline cancelled", so normal retry policy applies.
  - Folder inputs: one unreadable or concurrently deleted file no longer
    aborts the run (it is fingerprinted by name/size with a quality note),
    and folders with multi-gigabyte data files now run — the fingerprint
    hashes at most 1 MB per file plus name, size, and mtime instead of
    reading up to 2 GB of content and failing beyond it.
  - PDF extraction: helper processes (pdftotext, pdftoppm, the Paddle
    sidecar) and provider CLIs now read a staged private copy instead of the
    original path, so a previously denied macOS folder permission cannot
    silently break extraction; a failed pdftotext run surfaces its own error
    (an encrypted PDF now says so); one failed chunk call feeds the targeted
    retry pass instead of aborting the run; direct-API extraction fails fast
    with the actual constraint when a PDF exceeds Anthropic/OpenAI attachment
    limits (100 pages / 24 MB); oversized PDFs get a PDF-specific staging
    message and a 256 MB budget instead of the LaTeX 32 MB error; and
    Windows OneDrive Files-On-Demand placeholders are re-opened so they
    hydrate instead of failing the first read.
  - Shared-context reuse degrades to ordinary self-contained calls when the
    shared prefix cannot be assembled (for example an extracted text between
    the 8 MB shared-context cap and the 10 MB LaTeX cap) instead of failing
    the run; a cancelled Codex warm-up no longer disables context reuse for
    the rest of the run.
  - Auto Paper Review: selecting a folder as a browsable source tree is
    rejected up front with guidance (the router and reviewers would otherwise
    treat the file inventory as the paper); the orientation editor now offers
    a "Restore adaptive router prompt" action instead of survey-insert
    buttons that silently broke the workflow's required review plan; changing
    the exact adaptive-agent count no longer breaks resuming earlier runs;
    and disabling all three core reviews is caught before the orientation
    call instead of after it.
  - Orientation surveys: a small JSON object echoed in prose can no longer
    hijack the survey — the largest valid non-empty object wins, and an
    empty-object response now retries instead of producing a blank survey.
  - The finished-run view no longer crashes the app when a paper-shaped
    survey omits its metadata or authors (seen with schema-less profiles and
    older saved runs); the report workspace is also isolated behind its own
    error boundary.
  - Dependency preflight now requires every provider the active workflow
    actually dispatches to, so an unrelated available provider (for example a
    local server) can no longer mask a missing required CLI until mid-run.
  - Completed parallel analyses are checkpointed as each finishes, so a crash
    later in the wave no longer loses them; byte-based run retention can no
    longer delete the run that just completed; merged multi-agent outputs no
    longer double-count against the run output budget (and an over-budget
    merge falls back to the unmerged analyses); a Sequential step listing
    several agents now logs that only the first runs; the one-run Parallel
    agent override is correctly included in the plan/launch fingerprint; and
    Windows builds are long-path aware.
  - LaTeX extraction no longer expands commented-out `\input{}` lines, and an
    include that exists but cannot be read (encoding, size) leaves a quality
    note instead of silently dropping the chapter.
- Auto Paper Review is now the default workflow, and the Paper Review (Full)
  and Paper Review (Quick) built-ins were retired: Auto's adaptive specialist
  routing covers what both provided. On existing installations the retired
  profiles (including any customizations) are archived under
  `~/.pipeline/profiles/.retired-builtins/` rather than deleted, and an active
  selection pointing at one of them switches to Auto Paper Review once. Their
  step prompts (contribution, technical correctness, empirical strategy,
  internal consistency, exposition, consolidation, feedback validation) remain
  shipped defaults available from the workflow editor, and the freed profile
  IDs can be reused by custom workflows. Deleting a custom profile now falls
  back to Auto Paper Review, and "Reset to defaults" restores the matching
  stock definition for the remaining built-ins.
- A PaddleOCR-VL Full Parser install no longer dies silently when a pipeline
  run finishes or is cancelled while the install is downloading or
  provisioning. Installer subprocesses were tracked in the same kill list as
  run subprocesses, so the end of an unrelated run terminated the installer
  mid-flight and rolled the install back; the parser then appeared unusable
  until the app was restarted and the install repeated. Installer processes
  now have engine-scoped tracking (cancel from Settings still works, and
  Windows still terminates the full installer process tree), and a failed
  cleanup of the previous install's backup is logged instead of ignored.
- LaTeX extraction now reads files a project explicitly references just
  outside the selected directory (for example
  `\input{../output/estimates/numbers.tex}` from a `draft/dev` selection).
  Referenced files are accepted only when they are regular files of the
  expected type within three directory levels above the selection; each
  external read is recorded as a quality note, and everything else stays
  blocked. Staged source views copy those files under `_external/` so
  reviewer steps that read the source tree see the same content. Circular
  `\input{}` chains are now skipped precisely (with one note) instead of
  expanding to the depth limit, and repeated identical extraction warnings
  are recorded once instead of once per occurrence.
- A Claude CLI shared-context run no longer fails outright when the warmed
  session cannot be forked (stderr like "No conversation found with session
  ID"). The first affected step now marks the shared session unavailable and
  every remaining step automatically uses a self-contained call; other fork
  failures retry the one call self-contained without disabling sharing. A
  cancelled warm-up is no longer cached as a permanent session failure.
- Retired the Gemini CLI transport. The provider ID remains `antigravity` in
  workflows and persisted settings for compatibility, but Google dispatch is
  now always through the direct Gemini API and requires `google_api_key`;
  legacy Subscription selections normalize to API mode and dependency checks
  never probe or invoke `agy`. The sandboxed Antigravity CLI implementation
  remains dormant and test-covered rather than user-selectable. Profiles
  naming the older `gemini` agent must switch to `antigravity`, stale `gemini`
  settings are dropped on load, and the legacy `pro`/`flash`/`flash-lite`
  aliases no longer resolve.

## 0.9.0 — 2026-08-10

- A PaddleOCR-VL Full Parser installed by an older app build now keeps working
  after the app updates. Extraction verifies the managed runtime against its
  own install-time manifest digests and integrity inventory instead of the
  current binary's provisioning constants, and refreshes a stale bundled
  sidecar in place when that pinned runtime verifies. Previously a
  provisioning-metadata change (such as the pinned qualification mirror in
  `runtime-lock.json`) made runs fail with "not installed" while Settings
  still reported the parser as installed. The parser release name remains the
  compatibility contract, and tampered or corrupted files still fail closed.
- Added Paper Review (Auto), which validates one orientation/router call and
  selects one or two subject/subfield reviewers plus one to four method
  reviewers from a host-owned catalog covering 29 disciplines, 257 subject
  roles, and 115 method roles in 15 method families. Its saved workflow
  remains a five-step skeleton; Rust assembles only the selected specialists
  for each seven-to-eleven-step run.
  The plan is visible in progress and saved orientation artifacts, and
  unselected specialists never become workflow steps or consume model calls.
  The New Run workflow summary links to a searchable catalog browser grouped
  by discipline and method family, with separate subject and method views.
  The Workflow Editor now shows one combined Orientation & Classification
  stage followed by read-only adaptive slots instead of expanding the
  specialist catalog into saved steps.
  Method families give each group of related roles an explicit broad-fallback
  entry, so the router prefers the most specific fitting reviewer under one
  generic rule. The orientation call also classifies the document's genre —
  replication, comment or reply, survey article, data descriptor, methods or
  tool paper, registered report, null results, case report, or research
  software paper, defaulting to an ordinary research article — and every
  reviewer receives the matching host-owned genre context, so a comment is
  not faulted for lacking a free-standing contribution and a null-result
  paper is not faulted for the null itself. Each materialized specialist
  prompt also begins with the router's validated selection reason, quoted as
  bounded, untrusted context, so every reviewer starts from the claim that
  triggered its selection. The new subject coverage adds a neuroscience
  discipline and deeper AI, astronomy, climate, demography, and metascience
  subfields; new method coverage spans modern causal designs, AI-era
  evaluation, health and laboratory research, and a
  reproducibility-and-integrity family that can recompute reported
  statistics, flag figure inconsistencies, spot-check citations, and audit
  replication packages. The orientation prompt and schema that earlier stock
  installs wrote to disk are pinned verbatim for migration fingerprinting, so
  untouched stock Auto profiles keep upgrading in place while the live
  catalog evolves. Every selected subject and method report is a direct input to
  the Consolidate Feedback step alongside the three universal reviews, which
  merges them into up to forty ordered comments; a Validate Feedback step then
  re-checks each consolidated comment against the paper, repairs details that
  are easy to fix, and removes comments that do not validate, returning the
  same format the report viewer indexes. Web search is enabled by default on
  every Auto review step, including materialized specialists. Untouched
  28- and 29-step Auto Review development profiles and untouched four-step
  skeletons migrate to the validated five-step skeleton, including installs
  where an earlier migration marker was recorded before compaction; a
  configured adaptive-agent count is preserved and customized workflows are
  left exactly as edited.
  The pre-launch execution review now shows the bounded adaptive groups in the
  timeline and artifact-access table, including their direct inputs to Auto
  synthesis, without truncating step and count summaries. During a live run,
  the parallel wave now reserves an Adaptive agents row until orientation
  resolves it into the selected subject and method specialists.
- Added Projects for grouping related saved runs and revisions without moving
  or deleting the underlying run artifacts. Projects now include a persistent,
  report-neutral issue ledger with conservative cross-run matching, lifecycle
  decisions and notes, regression tracking, provenance/evidence history,
  filters, manual duplicate merging, and Markdown export.
- Added an explicit Run preview before launch, showing the execution timeline,
  provider and artifact access, declared tools, and bounded model work units.
- Added evidence references to structured issue cards, with page and artifact
  links into the run's Sources view and citations in accepted-issue exports.
- Added a Workflow Gallery with editable templates for revision responses,
  literature positioning, thesis chapters, and rubric-based review.
- Modernized the source-build CLI with transient workflow selection, dependency
  checks for concrete PDF inputs, and managed Paddle bundle status,
  installation, repair, and confirmed uninstallation commands.
- Consolidated local OCR extraction on PaddleOCR-VL Full Parser. The former
  direct Q8/fast extractor is no longer offered; saved settings and workflows
  that selected it migrate automatically to Full Parser. Installation and
  dependency status now present the parser and its recognition runtime as one
  managed engine.

### Security and privacy

- Restricted model-visible source access to a private view of the selected
  document. LaTeX projects now expose only a bounded in-project dependency
  closure rather than the surrounding folder.
- Re-runs and resumed runs now revalidate saved runtime inputs and the current
  profile's run budget before contacting a model.
- Limited request details sent to the application console to UTF-8-safe 16 KiB
  previews, bounding the amount of prompt or paper content copied into the
  WebView.
- Updated and constrained the frontend build and test dependency tree to
  eliminate all findings from the full npm production-and-development audit.
- Added a pre-run data-sharing notice, detailed privacy documentation, and a
  private vulnerability-reporting policy.

### Reliability and output quality

- Made execution planning and launch validation agree on explicit document,
  folder, and no-input modes; concrete named inputs now fail early when their
  path kind or format is unsupported.
- Made readiness account for PDF named inputs and the exact Poppler or managed
  PaddleOCR-VL components required by the selected extraction method.
- Made folder-watch stop transitions visible immediately while retaining the
  completed-job history.
- Preserved readable document text when provider limits prevent inclusion of a
  large structured bundle, and stopped duplicating native Paddle blocks with
  inferred Markdown structure.
- Made saved-run export complete and atomic, with a unique destination for each
  export. The legacy core-file export is now clearly labeled and refuses to
  overwrite an existing destination.
- Bundled KaTeX styling and fonts locally. PDF print output now embeds its
  rendering resources offline and waits for fonts before printing.
- Added safe Markdown links throughout reports, issues, and comparisons.
- Added unsaved-change protection for settings and workflows, including
  navigation and window-close confirmation.
- Made settings and workflow saves robust to edits that occur while a save is
  still in progress.
- Added exact purge previews and explicit confirmation before deleting stored
  data.

### Release integrity

- Retained and now tests the `com.pipeline.report` application identifier used
  by v1.0.0 so signed upgrades keep one stable application identity.
- Defined a stable-only update channel: prereleases are neither offered by the
  in-app release check nor accepted by the public release workflow.
- Added tested minimum main-window dimensions to keep the desktop layout usable.
- Public Windows builds now require Authenticode signing and timestamping.
- The Windows installer embeds WebView2 for self-contained first installation
  and rejects application downgrades.
- Public releases now require a cryptographically verified annotated Git tag.
- Release Actions are pinned to reviewed commits and run with job-scoped
  permissions in a protected environment.
- Every installer receives a SHA-256 record and GitHub build-provenance
  attestation; a final job verifies the complete draft release.
- Release and pull-request builds now use the same exact Node and Rust
  toolchains, locked Cargo resolution, and pinned runner families.
- Separate artifact and build-input SBOMs bind each installer to its dependency
  and native provenance records; the final draft gate scans all platform inputs
  and attests the complete evidence set.
- Windows Poppler provenance now attributes every shipped DLL/executable hash
  to an exact conda-forge package version, build, and independently verified
  source-archive payload.
- Linux release inputs use a dated Ubuntu archive snapshot, and CI launches the
  AppImage through its public FUSE entry point.
- Offline notices now include a machine-readable license-evidence inventory.
- Pinned Grype/Syft validation now round-trips every native SBOM identity before
  vulnerability scanning, preventing silently unscannable package records.

### Documentation

- Added a repeatable public-release checklist, including signing and
  third-party legal review gates.
- Added explicit supported-platform and maintenance policies, contribution
  guidance, structured issue forms, dependency automation, and repository-wide
  ownership rules.
- Removed machine-local agent permission settings from version control and
  documented the actual copyright holder established by repository history.
