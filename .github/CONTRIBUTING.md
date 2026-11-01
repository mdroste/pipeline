# Contributing to Pipeline

Bug reports, documentation fixes, and focused code changes are welcome. For a
large feature, open an issue first so we can agree on the problem and scope.

## Issues and pull requests

Search existing issues before you open one. Describe the research task or bug
concretely, with a small example you can share. Remove private research and
credentials from attachments. Report security problems privately, as
described in [Security](SECURITY.md).

Keep each pull request to one change. Explain what users will notice, why it's
needed, and how you checked it. For changes users will notice, update the
guides and add an entry to the newest section of the
[changelog](../CHANGELOG.md).

## Set up a development build

Use Node.js 24.18.0 (from [`gui/.nvmrc`](../gui/.nvmrc)) and Rust 1.97.1 (from
[`rust-toolchain.toml`](../rust-toolchain.toml)). Then, from a checkout:

```bash
cd gui
npm ci
npm run tauri dev
```

`npm run tauri dev` prepares the bundled notices before it compiles. Before you
run `cargo` directly, run `npm ci` and `npm run prepare:release-resources` in
`gui/`. The Rust build needs those notices and the KaTeX files in `node_modules`.

Install your platform's native build tools. Linux also needs the WebKitGTK,
application indicator, and SVG development libraries listed in
[the CI workflow](workflows/build.yml). `npm run tauri build` puts
installers in `gui/src-tauri/target/release/bundle/`.

Release tests need Python 3.11 or later. The test launcher tries `python3`,
then `py -3` on Windows, then `python`. Set `PYTHON` to an interpreter path to
choose one. The app itself doesn't need Python.

The documentation and size checks call `rg`, so install
[ripgrep](https://github.com/BurntSushi/ripgrep) and keep it on your `PATH`.

A build from source doesn't include Poppler. It uses the `pdftotext` and
`pdftoppm` on your `PATH`, unless you first fill
`gui/src-tauri/resources/poppler/` with
[bundle-poppler.sh](../scripts/release/bundle-poppler.sh).

To keep test projects and conversations away from your own, set
`PIPELINE_WORKBENCH_DEV_ROOT` to the absolute path of a scratch folder before
`npm run tauri dev`. It works in debug builds only. Reviews still use your
research data folder, so choose a scratch folder for them in Settings. The
browser fixtures in `gui/e2e/` load sample projects with `?project=rich`,
`?project=thin`, or `?project=bare`.

## Find your way around the code

Rust handles native services and saved data, and React with TypeScript
handles the interface. Pipeline has three parts that keep separate state:
projects and conversations, reviews, and automations. They share the ChatGPT
sign-in and some presentation code. Read [Architecture](../docs/architecture.md)
before you change a feature, and the header comment of each module you touch.

Saved settings, workflows, and reports must stay readable after an upgrade.
Give new saved fields defaults, and test them against older records. Add a new
numbered database migration instead of editing an applied one. Keep dependency
manifests and lockfiles in step.

## Check your changes

For documentation changes, run from the repository root:

```bash
node scripts/check-markdown-links.mjs
node scripts/check-doc-style.mjs --warnings
```

For code changes, run the tests and formatting checks for what you touched.
Before you ask for review of a release-related change, run everything:

```bash
cd gui
npm run test:release
npm test
npm run build
npm run format:check
npm run check:source-size
npm run check:links
npm run check:docs
cd src-tauri
cargo test --locked --all-targets
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
```

On Windows, set `PIPELINE_LINK_TEST_MANIFEST=1` before `cargo test` so the
library test program can start.

`npm run check:source-size` fails when a large source file grows. Split the
file, or reduce it, rather than refreshing the baseline in
`scripts/source-size-baseline.json` to hide the growth. It also fails when a
baseline entry names a file that's deleted or back within the limit, so remove
that entry. Size limits per file
type are in `scripts/source-size-policy.json`. To see the full report, run
`node scripts/source-size-report.mjs` from the repository root.

Add a regression test for each bug fix. Use made-up documents and test
credentials. Changes to installers, signing, native dependencies, or release
workflows also need the checks in [Releasing](../docs/releasing.md). If a check
fails, find out why before you change it.

## Writing documentation

The guides in `docs/` are for researchers using the app.
[Architecture](../docs/architecture.md) is for contributors, and
[Releasing](../docs/releasing.md) is for the maintainer. Put details about one module
in that module's header comment, not in a guide.

- **Describe what the reader does and what happens.** Name controls in bold,
  exactly as the app shows them, and check the label in `gui/src`.
- **Say what something does, not what it doesn't.** Mention a limit when a
  reader would expect otherwise and it affects their money, privacy, or data.
  Say it once, where they meet it.
- **Keep sentences short.** Aim for 25 words or fewer and one idea each. Use a
  list or table instead of a chain of semicolons.
- **Use the product's words.** Write project, conversation, review,
  automation, and assistant instructions, in lowercase unless they're a
  label. Keep implementation terms out of user guides. These include
  workbench, harness, mission, adapter, owner, boundary, explicit, bounded,
  durable, immutable, and provenance.
- **Describe the current app.** Leave out dates, test counts, validation
  results, and internal codes such as milestone or audit numbers. Leave out
  history ("now", "no longer", "previously") except in the changelog. Test
  records belong in your own notes, not in tracked files.
- **Give limits a reader can hit**, in MB or GB, with what to do about them.
- **Write code comments that explain why.** A good comment explains a reason
  or a rule the code can't show. Don't repeat a constant whose name already
  says it.

## Clean up build output

From `gui/`, preview what can be removed, then stop builds, tests, and
development servers and apply it:

```bash
npm run clean
npm run clean -- --apply
```

This removes the Rust `target/` folder, the frontend `dist/` folder, Vite
caches, TypeScript build-info files, and generated Tauri schemas. It keeps dependencies, source,
lockfiles, local PDFs, bundled tools, credentials, and app data. The next
build takes longer because Rust rebuilds everything. A custom
`CARGO_TARGET_DIR` isn't cleaned. Add `--worktrees` to clean the same
folders in checkouts under `.claude/worktrees/`.

Avoid `git clean -fdx`. Ignored files can include research inputs and local
settings.

## Files to keep out of commits

Don't commit `node_modules`, Rust `target` output, release bundles, generated
notices, bundled Poppler binaries, `.env` files, machine-specific agent
settings, confidential documents, or real credentials.

Contributions are licensed under the repository's [MIT License](../LICENSE).
