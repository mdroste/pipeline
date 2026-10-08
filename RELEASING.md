# Release process

Releases are built by manually dispatching [release.yml](.github/workflows/release.yml)
on a signed, annotated version tag. Pushing a tag alone does not start packaging.
The workflow leaves a draft; publication is a separate human action.

## Repository setup

Configure a `release` GitHub environment with a required reviewer and restrictions
to protected `v*` tags. Protect those tags against replacement. macOS builds use
these environment secrets: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`,
`APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, and `APPLE_TEAM_ID`.
The tag signing key must be recognized by GitHub.

Windows installers are intentionally unsigned. No Windows signing secret is
required by the current workflow. Windows bundles embed the silent WebView2
bootstrapper; installation needs a network connection when Evergreen WebView2
is missing. Tauri blocks installer downgrades. macOS requires version 15.0 or
later; Linux packaging uses Ubuntu 22.04 and produces an AppImage.

## Prepare and build

1. Choose a reviewed commit and a stable `X.Y.Z` version newer than every other
   stable tag. Update `gui/package.json`, `gui/package-lock.json`,
   `gui/src-tauri/Cargo.toml`, and `gui/src-tauri/Cargo.lock` together. Preserve
   the application identifier `com.pipeline.report`.
2. Update [CHANGELOG.md](CHANGELOG.md), relevant user documentation and notices.
   Run the checks in [CONTRIBUTING.md](CONTRIBUTING.md), including the production
   frontend build, release tests, Rust all-target tests, formatting and strict
   Clippy. Review dependency advisories separately when preparing a candidate.
3. Create and push a signed annotated tag for that exact commit:

   ```bash
   git tag -s vX.Y.Z -m "Pipeline vX.Y.Z"
   git push origin vX.Y.Z
   ```

   On the maintainer's configured Mac, run Git through an interactive login
   zsh outside the agent sandbox, as required by [AGENTS.md](AGENTS.md).
4. Dispatch **Release** on that tag. Choose `all` for a complete candidate, or
   a specific platform to build a missing installer. The default is macOS ARM.

The reusable quality workflow runs frontend tests, formatting, source-size and
link checks, a production build, release-contract tests, Rust formatting and
strict Clippy, plus native all-target tests on Linux, macOS and Windows. These
checks also run automatically on pushes to `main` and pull requests.

After quality succeeds, one coordinator fetches full tag history, checks version
identity, and asks GitHub to verify the annotated tag signature. The signed tag
must point to the workflow's commit. The coordinator creates or reuses a draft
and rejects an already published release before packaging begins.

The selected matrix jobs bundle Poppler, build installers, and sign/notarize
macOS. Each uploads an installer and its SHA-256 sidecar. Existing assets are
never overwritten: an identical retry is accepted, while different bytes
require a new version. A final job downloads and verifies **all four** installers
and their checksums. A platform-only dispatch therefore leaves an incomplete
draft and fails completeness until the other platforms have been built.

## Packaged resources and evidence

The Poppler packaging policy is unchanged. macOS copies the available Homebrew
Poppler and its dylib closure; Linux copies the runner's installed APT utilities
and selected native libraries. Windows downloads the locked provider archive
and verifies its archive hash. Platform caches may reuse that bundled tree only after `BUNDLE_INVENTORY.json`
hashes match every file. Cache keys include the bundler, lock and inventory
code; no partial-key fallback is used. Notices preparation regenerates the
observed version and file inventory after macOS signing.
The build prepares the three configured notice files and packages them alongside
Poppler resources. See [bundle-poppler.sh](scripts/release/bundle-poppler.sh) and
[Tauri configuration](gui/src-tauri/tauri.conf.json) for the actual inputs.

The active workflow produces four installers and four checksum sidecars. It
does not produce SBOMs, attestations, exhaustive native-library provenance,
Windows Authenticode signatures, automated vulnerability scans, or packaged
application smoke results. Other checked-in release utilities are available for
separate checks; their presence is not evidence that CI ran them. The runner
package sources and signing services also prevent a claim of bit-for-bit
reproducible builds.

## Review and publish

Maintain [the candidate evidence manifest](docs/releases/1.0.0-qualification.json)
with the exact final commit, tool/provider versions, expected/observed outcomes,
platforms, and SHA-256-bound evidence and installer files. Keep sanitized evidence
and installers beside a completed copy of that manifest, then run:

```bash
node scripts/release/qualify-release.mjs /absolute/path/evidence.json <candidate-commit>
```

The command fails on missing/pending scenarios, a different commit, or changed
files. This is a required pre-publication check; the draft build does not certify
human observations. The checked-in manifest intentionally remains pending until
those observations exist. Its current matrix requires all four advertised
platforms; narrowing scope requires an explicit product and gate change.

Before manually publishing the draft, verify its tag/commit, quality and
completeness conclusions, installer checksums, release notes, and intended
platform support. Install and exercise the candidate on representative clean
machines, including upgrade/data preservation and Windows WebView2 setup.
For Linux, exercise the AppImage directly on a machine with FUSE 2. Verify macOS
signature/notarization acceptance on a clean machine.

Complete applicable Workspace live and packaged qualification in
[release-qualification.md](docs/workbench/release-qualification.md). Deterministic
tests and no-model probes do not establish authenticated model/tool behavior,
packaged crash recovery, or platform permission enforcement. Review third-party
notices and distribution obligations against the actual bundled components.

Leave a failing candidate unpublished. For changes to existing installer bytes,
bump the version and create a new signed tag. Published assets and tags remain
immutable. This workflow has no automatic publication step.

## Updating tools

Keep `.nvmrc`, `rust-toolchain.toml`, and both workflow toolchain versions aligned.
Third-party Actions are pinned to immutable commit SHAs; update a pin only after
reviewing the upstream change and rerunning release-contract tests. Changes to
Poppler packaging require explicit review of the active bundling script and
applicable locked inputs; do not assume unused provenance utilities constrain
that script.

## Local unsigned macOS builds

The pre-bundle CLI signing hook skips signing when `APPLE_SIGNING_IDENTITY` is
unset for a local source build. Official CI sets `PIPELINE_OFFICIAL_RELEASE=1`
and requires that identity. Signed builds select the explicit
`PIPELINE_CLI_BINARY`, the configured target triple, or the default
`target/release/pipeline-cli`; `CARGO_TARGET_DIR` is respected. A missing selected
binary fails instead of signing unrelated stale target directories.
