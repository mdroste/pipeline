# Release process

Pipeline releases are intentionally manual at the final publication step. A
tag starts the build, but CI creates a **draft** release. Do not publish it
unless the `Verify complete draft release` job and the human checks below pass.

## One-time repository setup

1. Create a GitHub environment named `release`.
2. Require a reviewer for that environment and restrict deployments to protected
   version tags.
3. Protect `v*` tags so they cannot be moved or deleted casually.
4. Configure these environment secrets for macOS:
   `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`,
   `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, and `APPLE_TEAM_ID`.
5. Configure these environment values for Windows:
   - secret `WINDOWS_CERTIFICATE`: base64-encoded PFX/PKCS#12 signing certificate;
   - secret `WINDOWS_CERTIFICATE_PASSWORD`; and
   - variable `WINDOWS_TIMESTAMP_URL`: the timestamp service approved by the
     certificate issuer.
6. Enable GitHub private vulnerability reporting and artifact attestations.
   The release job downloads the pinned Grype scanner and its current advisory
   database; outbound access to Anchore's official release/database endpoints
   is therefore a release prerequisite.

The workflow deliberately fails rather than emitting an unsigned Windows
release when any Windows signing value is missing, invalid, expired, or not
valid for code signing. Never commit certificate material or a thumbprint. The
Windows package embeds Microsoft's small WebView2 bootstrapper. This keeps the
package compact, but a machine without a current Evergreen WebView2 runtime
needs internet access during installation so the bootstrapper can download it.

## Prepare a release candidate

1. Start from a clean reviewed commit on `main`.
2. Choose a version newer than every existing stable tag. Never move or reuse a
   published tag. The public channel accepts only stable `X.Y.Z` versions;
   prerelease and build metadata require a separately designed publishing and
   update channel.
3. Update `gui/package.json`, its lockfile, `gui/src-tauri/Cargo.toml`, and its
   lockfile together. `npm run test:release` verifies their identity.
4. Update [CHANGELOG.md](CHANGELOG.md), user-facing documentation, privacy/data
   flow, supported platform baselines, and third-party notices.
5. Run:

   ```bash
   cd gui
   npm ci
   npm run test:release
   npm test
   npm run build
   cd src-tauri
   cargo test --locked --all-targets
   cargo fmt --check
   cargo clippy --locked --all-targets --all-features -- -D warnings
   ```

6. Review `cargo audit` and the full `npm audit`, including build dependencies.
   Release CI also scans every platform build-input SBOM with pinned Grype
   v0.110.0 and fails for known high or critical vulnerabilities. Its pinned
   Syft v1.42.3 decoder must preserve the exact Homebrew, Debian, and Conda
   package names and versions before scanning starts. Review lower-severity
   findings and scanner coverage gaps. For Windows, confirm that the input lock
   still attributes every shipped PE file to an exact conda package
   version/build and source-package hash; an unattributed or versionless DLL is
   release-blocking.
7. Confirm that `.nvmrc`, `rust-toolchain.toml`, and the exact versions declared
   in both workflows still agree. Toolchain upgrades require the ordinary
   cross-platform test matrix.
8. Confirm that the Tauri identifier remains `com.pipeline.report`, as used by
   v1.0.0. Treat any proposed identifier change as a migration project with
   signed upgrade, settings/data continuity, and operating-system permission
   tests—not as routine metadata cleanup.
9. Complete the third-party legal checklist below.

## Third-party legal checklist

Before publication, confirm with qualified counsel that the distribution method
for bundled Poppler and its native dependency closure satisfies all applicable
license obligations. At minimum:

- every binary component and exact version is identified;
- required copyright, NOTICE, and license texts are available offline;
- exact corresponding source, downstream patches, and build/packaging recipes
  are attached to or durably offered with the release where required; and
- `LICENSE_INVENTORY.json`, the platform build-input SBOM, Poppler input lock,
  native package inventory, bundled native license evidence, and Poppler
  provenance are present and internally consistent.

Source URLs and hashes are valuable provenance, but this project does not assert
that links alone discharge a particular license obligation.

## Tag, build, and review

Create a signed annotated tag and push it:

```bash
git tag -s vX.Y.Z -m "Pipeline vX.Y.Z"
git push origin vX.Y.Z
```

The signing key must be associated with the GitHub account that creates the
tag. The first quality-gate step resolves the Git ref through GitHub's API,
rejects lightweight tags, and requires GitHub to report the annotated tag's
signature as verified.

The release workflow then:

1. verifies the signed annotated tag, then reruns tests, audits, formatting,
   strict Clippy, and the declared Rust minimum-version check on the tagged
   commit;
2. builds four platform artifacts in the protected environment;
3. signs/notarizes macOS and Authenticode-signs Windows;
4. verifies installed/extracted payloads and bundled Poppler;
5. publishes separate platform artifact and build-input SBOMs plus Poppler
   provenance; the artifact SBOM binds the exact installer, build-input SBOM,
   and provenance hashes;
6. publishes a `.sha256` record for every installer;
7. downloads and verifies the complete 20-asset draft, scans all four
   build-input SBOMs under the native vulnerability policy, and attests the
   entire downloaded evidence set; and
8. leaves the release as a draft.

Before publishing the draft, a reviewer should independently:

- verify the signed tag points to the intended commit;
- inspect all workflow conclusions, especially signing and completeness;
- compare each installer with its `.sha256` record;
- verify GitHub attestations;
- install on representative clean machines, complete a synthetic end-to-end
  run, render math, and export a report;
- on a clean, currently serviced Windows 11 x86-64 VM with no Evergreen
  WebView2 runtime and a working network connection, verify that installation
  provisions WebView2 and that Pipeline launches afterward;
- run a compatibility installation on Windows 10 22H2 x86-64 when that system
  remains in the supported matrix, using a device that still receives
  Microsoft security updates;
- upgrade from the previous public Windows release, confirm data preservation,
  then attempt to install the previous version over the candidate and confirm
  that the downgrade is rejected;
- run the Linux AppImage directly (without `APPIMAGE_EXTRACT_AND_RUN`) on a
  clean Ubuntu 22.04 x86-64 VM with FUSE 2; and
- read the final release notes and download instructions.

If anything is wrong, leave the draft unpublished. Fix the problem, bump the
version, and create a new signed tag; do not replace artifacts under a published
version.

Hosted Windows runners already include WebView2 and are not a substitute for
the clean-VM checks above. Likewise, GitHub's Ubuntu runner image and external
signing/notarization services are not bit-for-bit reproducible inputs. Linux
APT resolution is constrained to the timestamp in
`scripts/release/poppler-lock.json`, and every copied file is hashed in
provenance, but the final reviewer must still treat runner-image, certificate,
timestamp, notarization, advisory-database, and package-host availability as
external release gates.

When intentionally updating Linux packages, choose and test a new Ubuntu
snapshot ID, update `linux.aptSnapshot` and any exact package versions together,
regenerate provenance, and review the complete native package/license diff.

When intentionally updating the Windows Poppler bundle, do not infer its
dependency versions from DLL names or the provider tag. Download the provider
ZIP and each candidate conda-forge archive, identify every shipped executable
and DLL by its payload SHA-256, then update the exact package version, build,
URL, archive hash, declared license, source payload path, and bundled-file hash
in `scripts/release/poppler-lock.json`. The release must independently
download and hash-check every locked archive, extract its payload, and prove
the complete one-owner PE-file mapping. Review the native package and license
diff and rerun `npm run test:release`.

## Updating pinned Actions

All `uses:` entries are pinned to immutable commit SHAs. To update one, resolve
the intended official tag from the Action's upstream Git repository, use the
peeled commit for an annotated tag, review the diff between old and new commits,
retain the human-readable version comment, and run the release-script tests.
When updating `anchore/scan-action`, also pin the intended Grype version, verify
the Syft library version embedded by that Grype release, update the independently
hash-checked Syft decoder binary, and retain the four-platform native identity
round-trip gate.
