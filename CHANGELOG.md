# Changelog

This file records user-visible and release-integrity changes. Development before
the first public release was not maintained as a stable release series.

## 0.9.0 — 2026-08-10

- Added Paper Review (Auto), which validates one orientation/router call and
  selects one or two subject/subfield reviewers plus one to four method
  reviewers from a host-owned catalog covering 28 disciplines, 191 subject
  roles, and 32 method roles. Its saved workflow remains a four-step skeleton;
  Rust assembles only the selected specialists for each six-to-ten-step run.
  The plan is visible in progress and saved orientation artifacts, and
  unselected specialists never become workflow steps or consume model calls.
  The New Run workflow summary links to a searchable catalog browser grouped
  by discipline, with separate subject and method views. The Workflow Editor
  now shows one combined Orientation & Classification stage followed by two
  read-only adaptive slots instead of expanding the specialist catalog into
  saved steps. Every selected subject and method report is a direct input to
  synthesis alongside the three universal reviews. Untouched 28- and 29-step
  Auto Review development profiles migrate to the compact skeleton, including
  installs where an earlier migration marker was recorded before compaction.
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
- Retired Marker installation, discovery, and execution because its compatible
  Python dependency closure contains known vulnerabilities. Legacy selections
  now fail closed with replacement guidance; historical run artifacts remain
  readable, and Settings can remove an old app-managed Marker environment.
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
  large structured bundle, and stopped duplicating native Marker/Paddle blocks
  with inferred Markdown structure.
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
