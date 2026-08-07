# Changelog

This file records user-visible and release-integrity changes. Development before
the first public release was not maintained as a stable release series.

## Unreleased

## 1.0.1 — 2026-07-27

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
