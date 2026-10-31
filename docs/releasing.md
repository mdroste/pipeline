# Releasing

A release is built by starting [release.yml](../.github/workflows/release.yml) by
hand on a signed, annotated version tag. Pushing a tag doesn't start the
build. The workflow creates a draft release, which you review and publish
yourself.

Build progress and history are available in **Actions**, and installers are
listed in **Releases**. Windows packaging uses the `windows-signing` environment;
other release jobs use `release`. Build jobs don't create deployment records.

## Repository setup

- Create a `release` GitHub environment with a required reviewer, limited to
  protected `v*` tags. Protect those tags from being replaced.
- Add these secrets to the environment for macOS signing:
  `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`,
  `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, and
  `APPLE_TEAM_ID`.
- Register your tag signing key with GitHub.
- Add repository secrets `TAURI_SIGNING_PRIVATE_KEY` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Every platform build signs its update
  package with them, so they can't be limited to one environment. See
  [In-app updates](#in-app-updates).
- Create a `windows-signing` environment limited to `v*` tags. Add environment
  secrets `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, and `AZURE_SUBSCRIPTION_ID`,
  and variables `AZURE_SIGNING_ACCOUNT`, `AZURE_SIGNING_PROFILE`, and
  `AZURE_SIGNING_ENDPOINT`.
- In Azure, give the Windows signing app a federated credential matching
  GitHub's exact subject for this repository and the `windows-signing`
  environment. Use issuer `https://token.actions.githubusercontent.com` and
  audience `api://AzureADTokenExchange`. Grant the app **Artifact Signing
  Certificate Profile Signer** access to the signing account or profile.
  The workflow uses OpenID Connect; it doesn't require a client secret.

Windows releases use Azure Artifact Signing with publisher **Michael Droste**.
They include the WebView2 bootstrapper, which downloads WebView2 during installation if it's
missing. Tauri blocks installing an older version over a newer one. macOS
builds require macOS 15 or later. Linux builds run on Ubuntu 22.04 and
produce an AppImage.

## Prepare and build

1. Choose a reviewed commit and a stable `X.Y.Z` version newer than every
   earlier stable tag. Update the version in `gui/package.json`,
   `gui/package-lock.json`, `gui/src-tauri/Cargo.toml`, and
   `gui/src-tauri/Cargo.lock` together. Update the README version badge,
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

The workflow first checks that the version matches, that GitHub verifies the
tag signature, and that the tag points at the workflow's commit. It creates a
draft, or reuses one, and stops if that release is already published.

The same quality checks as every pull request run alongside the platform jobs:
frontend tests, formatting, size and link checks, a production build,
release tests, Clippy, and native tests on Linux, macOS, and Windows. A failed
check doesn't stop the platform jobs, but the final job won't run. Leave that
draft unpublished.

Each platform job bundles Poppler and builds the installer. macOS builds are
signed and notarized. Finder arranges the disk image window during the macOS
build, and the job fails if the layout wasn't saved. If Finder times out, run
the job again. Windows builds sign the bundled tools, application,
CLI, uninstaller, and installer. Before upload, the Windows job installs the
package and verifies its signatures, timestamps, publisher, and Poppler
inventory. Each installer is uploaded with a SHA-256 checksum file. An asset that's
already uploaded is never replaced. A retry with identical bytes passes, and
different bytes need a new version. Once the quality checks and platform jobs
pass, a final job downloads all four installers and checks their checksums and
[update signatures](#in-app-updates), so a single-platform run fails this step
until every platform is built.

Platform jobs reuse Rust dependencies compiled by
[release-cache.yml](../.github/workflows/release-cache.yml), because a new tag
can't read another tag's cache. **Release cache** runs when `main` changes Rust
dependencies or the toolchain, and twice a week. If a release changes either,
wait for it to finish on `main` before you start **Release**. Otherwise, the
release still works but compiles every dependency.

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
    `scripts/release/poppler-lock.json`, checks its hash, and signs the
    executables and libraries before recording their bundled hashes.

`resources/poppler/BUNDLE_INVENTORY.json` records the Poppler version and the
hash of every bundled file. The release cache reuses a bundled Poppler folder
only when every hash matches. See
[bundle-poppler.sh](../scripts/release/bundle-poppler.sh) and the
[Tauri configuration](../gui/src-tauri/tauri.conf.json) for the exact inputs.

The release produces four installers and four checksum files. It doesn't
produce a software bill of materials or build attestations.
Runner images and signing services change over time, so
rebuilding the same source can produce different bytes.

## Manual checks before publishing

Automated tests and no-model probes cover only part of what a release needs.
Before you publish, test the candidate by hand:

- Install it on clean machines for each platform. Check upgrading from the
  previous release with existing data, WebView2 setup on Windows, the
  AppImage on a Linux machine with FUSE 2, and that macOS accepts the
  signature and notarization.
- Update from the previous release on each platform. Choose **Install update**,
  then **Restart now**, and check that the new version opens with its data. On
  macOS, repeat this from an account that isn't an administrator. On Windows,
  check that the installer shows only its progress and reopens Pipeline.
- On macOS, open the disk image in light and dark mode. It should open without
  a license prompt and show Pipeline, an arrow, and Applications, with both
  labels easy to read.
- On Windows, go through the installer at 100% and 150% display scale. The logo
  should look sharp, no text should be cut off, and the installer should move
  from the progress bar to the finish page without a click.
- On Windows, confirm the installer and installed application show **Michael
  Droste** as the verified publisher. Signing doesn't guarantee that every
  SmartScreen reputation warning disappears immediately.
- On Windows, check the window with the sidebar full and folded to icons: drag it from the
  top of the window, double-click there to maximize, snap it to a screen edge,
  resize it from every edge, and use the minimize, maximize, and close buttons.
- Set up ChatGPT, Claude Code, OpenAI API, Anthropic API, Gemini API, and a
  local/custom server. Run real conversations and reviews through each,
  including supported tool calls, image inputs, and approvals. Check model
  selection, thinking controls, provider switching, and API key errors.
- Run review steps with **Web search** on OpenAI API and a tool-capable
  local/custom server. Check search results, page reads, citations, and usage.
  Turn search off and confirm web calls are denied. Cancel an active search,
  and check that a server rejecting tools reports an actionable error.
- For every conversation connection, create and undo files, edit in a working
  copy, interrupt commands, and inspect the captured changes before accepting
  them. Check that concurrent changes are preserved. Run an automation with a
  saved model, and confirm discovery authors can edit their paper folders
  while referees remain read-only.
- Let Pipeline prepare command tools on macOS and Linux. Verify read-only and
  working-copy access, protected-file denial, and child-process cleanup after
  interruption and normal completion. On Windows, confirm its unavailable
  state leaves project, research, web, and browser tools usable.
- Check web search, downloads, and each command internet setting. Verify that
  allowed sites work, denied sites fail, and commands cannot bypass the site
  restriction. Enable **Browser interaction** and let setup finish. Sign in
  directly, and confirm that stopping the turn closes it. Check private-address
  and redirect denial.
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
  cancellation, and idle CPU and memory on each platform. Include streamed API
  tool calls and a custom server that returns a complete JSON reply.
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
the hashes of the evidence and installers. First capture the candidate identity
from the complete, verified draft's downloaded installers and checksum files.
Use the commit and version of its signed tag, and keep this manifest separate
from the observations:

```bash
node scripts/release/qualify-release.mjs --candidate <candidate-commit> <candidate-version> /absolute/path/draft-installers > /absolute/path/candidate.json
node scripts/release/qualify-release.mjs /absolute/path/evidence.json /absolute/path/candidate.json
```

It fails if a scenario is missing or pending, the commit or version differs,
an installer belongs to another platform, or a file changed. Installer names
and hashes must match the candidate manifest. The evidence must cover all four platforms. Dropping a platform also
means updating [Support](../.github/SUPPORT.md) and the release checks.

## In-app updates

Each build signs a package for Pipeline's updater: an app archive on macOS, and
the installer itself on Windows and Linux. The final job adds a `latest.json` file
that lists them, and checks every signature against the public key in
`gui/src-tauri/tauri.conf.json`. A release without those files still shows
researchers the update, with **Download** in place of **Install update**.

Copies of Pipeline trust only that public key, and they can't update themselves
past a change to it. Keep the private key and its password in a password manager.
If either is lost or exposed, generate a new pair with `npm run tauri signer
generate`, put the new public key in `tauri.conf.json`, and update both secrets.
Researchers on earlier versions then have to download that release once by hand.

Local builds don't create these packages, so they need no key.

## Publish

Before you publish the draft, confirm its tag and commit, the quality and
completeness results, the checksums, the release notes, and the platforms.
If a candidate fails, leave it unpublished, bump the version, and tag again.
Published tags and assets never change.

## Updating tools

Keep `gui/.nvmrc`, `rust-toolchain.toml`, and the toolchain versions in the
Tests, Release, and Release cache workflows in step. Third-party Actions are
pinned to commit SHAs. Update a pin
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
