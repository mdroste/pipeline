# Releasing

A release is built by starting [release.yml](../.github/workflows/release.yml) by
hand on a signed, annotated version tag. Pushing a tag doesn't start the
build. The workflow creates a draft release, which you review and publish
yourself.

## Repository setup

- Create a `release` GitHub environment with a required reviewer, limited to
  protected `v*` tags. Protect those tags from being replaced.
- Add these secrets to the environment for macOS signing:
  `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`,
  `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, and
  `APPLE_TEAM_ID`.
- Register your tag signing key with GitHub.

Windows installers aren't signed, so they need no secret. They include the
WebView2 bootstrapper, which downloads WebView2 during installation if it's
missing. Tauri blocks installing an older version over a newer one. macOS
builds require macOS 15 or later. Linux builds run on Ubuntu 22.04 and
produce an AppImage.

## Prepare and build

1. Choose a reviewed commit and a stable `X.Y.Z` version newer than every
   earlier stable tag. Update the version in `gui/package.json`,
   `gui/package-lock.json`, `gui/src-tauri/Cargo.toml`, and
   `gui/src-tauri/Cargo.lock` together. Update the README heading and badge,
   issue templates, release metadata, and current-version fixtures and browser
   shims. Verify that About, Settings, the built app, and the CLI show that
   version. Keep Tauri's version sourced from `gui/package.json` and the app
   identifier `com.pipeline.report`. Preserve historical release notes,
   compatibility statements, dependency versions, and deliberately different
   versions in update tests.
2. Update the [changelog](../CHANGELOG.md), the guides, and the notices. Run
   every check in [Contributing](../.github/CONTRIBUTING.md), and review dependency
   advisories.
3. Create and push a signed, annotated tag for that commit:

   ```bash
   git tag -s vX.Y.Z -m "Pipeline vX.Y.Z"
   git push origin vX.Y.Z
   ```

4. Start **Release** on that tag. Choose `all` for a full release, or one
   platform to rebuild a missing installer. The default is macOS on Apple
   Silicon.

The workflow first runs the same quality checks as every pull request:
frontend tests, formatting, size and link checks, a production build,
release tests, Clippy, and native tests on Linux, macOS, and Windows.

It then checks that the version matches, that GitHub verifies the tag
signature, and that the tag points at the workflow's commit. It creates a
draft, or reuses one, and stops if that release is already published.

Each platform job bundles Poppler, builds the installer, signs and notarizes
it on macOS, and uploads it with a SHA-256 checksum file. An asset that's
already uploaded is never replaced. A retry with identical bytes passes, and
different bytes need a new version. A final job downloads all four
installers and checks their checksums, so a single-platform run fails this
step until every platform is built.

## Packaged resources

`npm run prepare:release-resources` fills two ignored folders before Tauri
builds:

- `gui/src-tauri/resources/notices/` gets Pipeline's license, the
  [third-party notice](../THIRD_PARTY_LICENSES.md), and a short versioned
  notice. Tauri bundles only these three files.
- `gui/src-tauri/resources/poppler/` gets `pdftoppm`, `pdftotext`, the
  libraries they load, Poppler's data, and its license files.
  - macOS uses Homebrew's Poppler, moves its library paths into the app
    bundle, and signs the binaries before Tauri builds and notarizes the DMG.
  - Linux uses the runner's Ubuntu packages and the shared libraries the
    AppImage needs.
  - Windows downloads the archive pinned in
    `scripts/release/poppler-lock.json` and checks its hash.

`resources/poppler/BUNDLE_INVENTORY.json` records the Poppler version and the
hash of every bundled file. The release cache reuses a bundled Poppler folder
only when every hash matches. See
[bundle-poppler.sh](../scripts/release/bundle-poppler.sh) and the
[Tauri configuration](../gui/src-tauri/tauri.conf.json) for the exact inputs.

The release produces four installers and four checksum files. It doesn't
produce a software bill of materials, build attestations, or Windows
signatures. Runner images and signing services change over time, so
rebuilding the same source can produce different bytes.

## Manual checks before publishing

Automated tests and no-model probes cover only part of what a release needs.
Before you publish, test the candidate by hand:

- Install it on clean machines for each platform. Check upgrading from the
  previous release with existing data, WebView2 setup on Windows, the
  AppImage on a Linux machine with FUSE 2, and that macOS accepts the
  signature and notarization.
- Sign in to each provider and run real conversations and reviews, including
  tool calls and approvals.
- For both conversation providers, create and undo files, edit in a working
  copy, interrupt commands, and inspect the captured changes before accepting
  them. Check that concurrent changes are preserved.
- Check web search, downloads, and each command internet setting. Verify that
  allowed sites work, denied sites fail, and commands cannot bypass the site
  restriction. Install the optional browser, sign in directly, and confirm
  that stopping the turn closes it. Check private-address and redirect denial.
- On Windows, test Claude inside WSL2 with paths containing spaces and Unicode,
  a separate Linux sign-in, and cancellation that stops Linux child processes.
  Check native ChatGPT commands and native research jobs separately.
- Restart the app and switch accounts to check saved usage reports. Check both
  usage windows, their reporting times, and the display after a window resets.
- Run the 20 tasks in
  [`research-evaluation-v1.json`](../gui/src-tauri/tests/fixtures/research-evaluation-v1.json)
  with the same files, model, and effort in each conversation mode. Have a
  researcher score correctness and traceability.
- Measure startup, first message, streaming, long conversations,
  cancellation, and idle CPU and memory on each platform.
- Stop a running Stata job and confirm its processes are cleaned up.
- Quit the packaged app at each point in the crash and recovery table in
  [Architecture](architecture.md#crash-and-recovery), and confirm it
  recovers.
- Check that the third-party notices cover what's actually bundled, and that
  GPL source obligations for Poppler are met. The Windows Poppler archive
  doesn't include a license file for every library, so add any missing
  notices.

Record the results in an evidence file based on
[`docs/releases/1.1.0-qualification.json`](releases/1.1.0-qualification.json),
with the final commit, tool and provider versions, platforms, outcomes, and
the hashes of the evidence and installers. Then run:

```bash
node scripts/release/qualify-release.mjs /absolute/path/evidence.json <candidate-commit>
```

It fails if a scenario is missing or pending, the commit differs, or a file
changed. The evidence must cover all four platforms. Dropping a platform also
means updating [Support](../.github/SUPPORT.md) and the release checks.

## Publish

Before you publish the draft, confirm its tag and commit, the quality and
completeness results, the checksums, the release notes, and the platforms.
If a candidate fails, leave it unpublished, bump the version, and tag again.
Published tags and assets never change.

## Updating tools

Keep `gui/.nvmrc`, `rust-toolchain.toml`, and the toolchain versions in both
workflows in step. Third-party Actions are pinned to commit SHAs. Update a pin
only after reviewing the upstream change, then rerun the release tests.
Changes to Poppler packaging need a careful review of the bundling script and
its pinned inputs.

## Local unsigned macOS builds

When `APPLE_SIGNING_IDENTITY` isn't set, the pre-bundle step skips signing the
CLI, so local builds work without a certificate. Official builds set
`PIPELINE_OFFICIAL_RELEASE=1` and require the identity. A signed build signs
`PIPELINE_CLI_BINARY` if set, otherwise the CLI for the configured target, or
`target/release/pipeline-cli`. It respects `CARGO_TARGET_DIR`, and fails if the
selected binary is missing.
