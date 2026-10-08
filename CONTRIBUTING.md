# Contributing to Pipeline

Thanks for helping improve Pipeline. Small, focused changes with tests and
clear user impact are easiest to review.

## Before opening an issue

- Use the bug form for reproducible defects and the feature form for proposals.
- Search existing issues first.
- Remove papers, prompts, API keys, provider tokens, usernames, and local paths
  from screenshots, logs, and run artifacts.
- Report vulnerabilities privately according to [SECURITY.md](SECURITY.md).
  Never use a public issue for an unpatched security problem.

The supported release platforms and maintenance policy are in
[SUPPORT.md](SUPPORT.md).

## Development setup

Use the exact Node.js version in `gui/.nvmrc` and the Rust toolchain in
`rust-toolchain.toml`. From `gui/`:

```bash
npm ci
npm run tauri dev
```

Linux development also needs the WebKitGTK and application-indicator packages
listed in `.github/workflows/build.yml`. Do not commit generated `node_modules`,
Rust `target` output, release bundles, generated notices, bundled Poppler
binaries, `.env` files, or machine-local agent settings.

## Tests

Run checks proportionate to the change. Before requesting review for a
release-relevant change, run:

```bash
cd gui
npm run test:release
npm test
npm run build
cd src-tauri
cargo test --locked --all-targets
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
```

On Windows, set `PIPELINE_LINK_TEST_MANIFEST=1` before `cargo test`. Tauri
embeds the application manifest only in binaries, and the library's test
executable cannot start without it.

Changes to packaged behavior, release metadata, signing, native dependencies,
or workflows must also follow [RELEASING.md](RELEASING.md). Do not weaken a
fail-closed release check merely to make CI pass.

## Pull requests

- Keep one coherent change per pull request.
- Explain the user-visible behavior, risks, and validation performed.
- Add regression tests for bug fixes and update user-facing documentation when
  behavior or requirements change.
- Treat Pipeline as an academic-research agent orchestration suite. Preserve the
  distinction between interactive Workspace harnesses/recipes and deterministic
  Workflows; read `docs/workbench/README.md` before changing Workspace code.
- Keep Workspace runtime, credentials, storage, cancellation, and lifecycle
  independent from Workflow state except at the explicit immutable handoff.
- Preserve backward compatibility for saved settings, profiles, and run
  manifests; new persisted fields need serde defaults and old-file coverage.
- Keep dependency lockfiles synchronized with their manifests.
- Add an entry under `Unreleased` in [CHANGELOG.md](CHANGELOG.md) for
  user-visible changes that are not already part of a prepared release entry.
- Never include confidential source documents or live credentials in tests.

All contributions are provided under the repository's [MIT license](LICENSE).
