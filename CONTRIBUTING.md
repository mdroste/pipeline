# Contributing to Pipeline

Bug reports, documentation improvements, and focused code changes are welcome.
For a substantial feature, open an issue first so we can agree on the problem
and scope.

## Issues and pull requests

Search existing issues before opening one. Describe the research task or bug
in concrete terms, and use a small example that you can share. Remove private
research and credentials from attachments. Report vulnerabilities through
[the private reporting process](SECURITY.md).

Keep each pull request focused on one change. Explain what users will notice,
why it is needed, and how you checked it. Update the documentation and add an
[Unreleased changelog entry](CHANGELOG.md) for user-visible changes that are
not already covered by a prepared release entry.

## Development setup

Use Node.js 24.18.0 from [`gui/.nvmrc`](gui/.nvmrc) and Rust 1.97.1 from
[`rust-toolchain.toml`](rust-toolchain.toml). From a source checkout:

```bash
cd gui
npm ci
npm run tauri dev
```

Install the native build tools for your platform. Linux also needs WebKitGTK,
application-indicator, and SVG development libraries; the package list is in
[the CI workflow](.github/workflows/build.yml). `npm run tauri build` creates
installers under `gui/src-tauri/target/release/bundle/` when using the default
Cargo target directory.

Release tests need Python 3.11 or later. The test launcher tries `python3`, the
Windows `py -3` launcher, and then a verified `python`. Set `PYTHON` to an
interpreter path to override discovery; put no arguments in that value. The
packaged app does not require a system Python installation.

## Understand the code

Rust owns native services and saved data; React and TypeScript own the
interface. Start with the [feature module map](docs/refactoring/module-map.md)
and read the relevant implementation guide before changing a feature.

- [Workspace](docs/workbench/README.md) owns projects, conversations, and
  interactive research tools.
- Reviews execute saved workflows and keep their own run history.
- [Automations](docs/tasks.md) coordinate conversation turns, Reviews, and
  local checks through each feature's interface.

Conversations and Reviews share a managed ChatGPT account, but keep their
execution state, storage, cancellation, and file access separate. Preserve
those boundaries. Saved settings, workflow definitions, and reports must
remain readable after an upgrade; new persisted fields need defaults and
tests using older records. Keep dependency manifests and lockfiles aligned.

## Check your changes

For documentation-only edits, run this from the repository root:

```bash
node scripts/check-markdown-links.mjs
```

For code changes, run the relevant tests and formatting checks. Before
requesting review for a release-related change, run the full checks:

```bash
cd gui
npm run test:release
npm test
npm run build
npm run format:check
npm run check:source-size
npm run check:links
cd src-tauri
cargo test --locked --all-targets
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
```

On Windows, set `PIPELINE_LINK_TEST_MANIFEST=1` before `cargo test` so the
library test executable gets the manifest it needs to start.

Add regression coverage for bug fixes. Use synthetic documents and test
credentials. Changes to installers, signing, native dependencies, or release
workflows also need the checks in [Releasing](RELEASING.md). Diagnose a failed
check before changing its requirements.

## Clean up build output

From `gui/`, preview generated files that can be removed:

```bash
npm run clean
```

Stop builds, tests, and development servers, then apply the cleanup:

```bash
npm run clean -- --apply
```

Cleanup removes the local Rust `target/` directory, frontend `dist/`, Vite
caches, and generated Tauri schemas. It keeps installed dependencies, source,
lockfiles, local PDFs, bundled tools, credentials, and application data. The
next build recreates these outputs and requires a full Rust rebuild. External
`CARGO_TARGET_DIR` locations are not included.

Add `--worktrees` to include the same generated paths in checkouts under
`.claude/worktrees/`. Source files and Git registrations remain intact.
Use `npm run test:clean` to check the cleanup safeguards.

Development and release Cargo profiles disable incremental compilation, but
compiled dependencies and other output still accumulate. Cleanup is useful
after toolchain changes or when disk use grows. Avoid `git clean -fdx`: ignored
files can include research inputs and local settings.

## Files to keep out of commits

Do not commit `node_modules`, Rust `target` output, release bundles, generated
notices, bundled Poppler binaries, `.env` files, machine-specific agent
settings, confidential documents, or live credentials.

Contributions use the repository's [MIT License](LICENSE).
