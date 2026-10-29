# Research desk

The research desk covers reading and conversation context, local search, decisions and change impact, source acquisition, captured data and samples, and captured execution. This is the implementation and usage guide for those features. It supplements [project-surface.md](project-surface.md), [research-studio.md](research-studio.md), and [release-qualification.md](release-qualification.md). It is not a packaged-release qualification record.

## Find the features

Open a Workspace project, then **Open project**. The project stays beside its conversation. A rootless project can keep documents, sources, decisions, datasets and samples while signed out; execution requires an attached, validated folder and an installed toolchain.

| Feature | Project destination | What is available |
|---|---|---|
| Reading and context | Every desk destination | Persistent document/object pane, assistant, exact-source context tray, pinned comparison, remembered revision and layout |
| Search and collections | Library → Search & collections | Local project search, kind filters, exact excerpts, pagination, saved reading collections, rebuild status |
| Decisions and impact | Activity → Decisions & impact | Decisions with rationale/alternatives/supersession, explicit dependencies, explainable impact, session handoff draft |
| Acquisition | Library → Acquisition inbox | Crossref DOI/bibliographic discovery, selected metadata/abstract import, explicit PDF acquisition, local PDF import, reading status |
| Data and samples | Analyze → Data & samples | Captured CSV/TSV, Stata `.dta` and Excel `.xlsx` with variable and value labels, dictionary and missingness, versioned samples, exporter metadata, per-project data access, FRED/ALFRED vintages, DBnomics series and World Bank indicators with their required notices |
| Captured execution | Analyze → Captured execution | Capture configured inputs, inspect and authorize exact plan, test, replay in a fresh directory, retained execution receipts |

Analyze also contains Experiments, Result links, and Theory. Write contains Manuscript, Responses, and Edits & acceptance. Automate exposes action items, recurring checks, and revision automations. Activity contains research notes, claims and evidence, and decisions. Literature notes retains the existing BibTeX, source assessments, and Zotero metadata adapter.

Workspace and Workflows continue to own separate stores, credentials, process lifecycles, cancellation and writable runtime state. None of these features turns a Workspace recipe or captured execution plan into a Workflow. Existing immutable Review handoffs remain explicit.

## Durable reading and explicit conversation context

`WorkspaceConversationView.tsx` extracts transcript/composer rendering from `WorkspacePage.tsx`. The page still owns streaming, server requests, Stop, draft persistence and the bounded 200-message transcript window. Research panels remain lazy loaded.

`WorkspaceProjectSurface.tsx` has a center object route and an optional pinned comparison. Paper references open the existing reader at their exact revision; other research references open `research-desk/ObjectPane.tsx`. A saved object is never replaced with the latest revision. Missing or excluded references produce an explicit unavailable/omitted result.

Select a passage to ask, save a note, create an action item, or propose a revision using the existing selection actions. **Add to conversation** retains an exact reference in the source tray. If the project has no conversation, the action creates a research conversation beside the object. Adding a source does not send a model turn.

The tray supports at most 12 references and at most one `main` role. Other roles are `source`, `data_dictionary`, `prior_draft`, `referee_report`, `result`, and `supporting`. Saving uses an optimistic selection revision and refuses changes during an active turn. Late saves and reads from a conversation that has been left cannot populate the newly selected tray.

The host includes selected references and bounded excerpts in the ordinary per-turn context snapshot. Explicit excerpts get priority within the existing context budget. Source content is escaped and marked as source material. Changing model-visible context or dataset access changes the harness fingerprint; the existing successor-binding mechanism applies. Resizing a pane does not change that fingerprint.

Layout is optional local UI state under `pipeline.desk.<workspaceId>`: subpage, exact revision, pinned revision, open object, comparison and assistant width. Reader mode/page/passage and scroll position use the reader's revision-specific preferences. Drafts remain authoritative in the Workspace store, with a local pending-draft recovery entry written immediately on edit and removed after successful persistence. Leaving the mode flushes the draft. The container-aware desk offers Project, Assistant, or Show both; narrow windows focus one pane while retaining both mounted. File/file and source/preview splits independently support pointer/keyboard/numeric sizing, saved ratios, automatic stacking and single-pane focus. Local reader outlines collapse according to their own width.

Markdown is registered with the document reader's highlighter. Unknown grammars fall back to plain text. Reader error boundaries contain failures to the affected pane and provide Retry.

## Local search and reading collections

The rebuildable projection is SQLite FTS5 with Unicode tokenization. It covers immutable paper/source versions, citation identifiers, retained note revisions, project record revisions, finalized conversation messages, execution receipts, structured results, claims and artifact metadata. Collections, decisions, dataset dictionaries and sample declarations are also searchable. Inventory snapshots, execution-plan internals and raw dataset bytes are excluded.

Search returns the object kind, exact revision/hash, optional UTF-8 byte range, provenance, access level, completeness and a bounded snippet. A message or proposal remains unaccepted source material. Source metadata and abstracts do not become full-text evidence. Citation keys and DOI metadata remain indexed when a source also has an abstract. A small local abbreviation map expands IRF/IRFs, stderr, TFP and IV; search makes no embedding or model call.

Each authoritative mutation coalesces into one dirty generation per workspace. Each indexing call processes at most 12 sources and 256 KiB per source, in 8 KiB chunks. A persisted source/byte cursor resumes interrupted work. The write transaction checks the generation and cursor again before publishing a batch, so overlapping workers cannot publish duplicated or mixed-generation results. Deletion, source exclusion and data-policy changes invalidate the projection.

The UI reports incomplete indexing, advances short batches, and offers **Rebuild index**. Search returns 20 hits by default, at most 50 through the host service. Pagination binds the workspace generation and query identity. A changed index requires a new search; the UI clears stale results. Exact reads resolve authoritative records independently of FTS, so an old reference remains usable after a newer version is imported.

A reading collection retains up to 200 exact references. A new collection version can explicitly supersede an earlier one; saving does not merge source identities. Excluded content cannot be added to context or reopened through the curated object service.

The scoped dynamic tools are:

- `workbench_research_search_v1`: at most 10 results for a tool call, with exact references and completeness.
- `workbench_research_object_v1`: an authoritative exact read, at most 16 KiB.
- `workbench_dataset_rows_v1`: an explicit, separately policy-gated row preview.

These use the existing active Workspace binding, tool catalog, durable tool receipts and per-turn call budget. They expose no network acquisition or host execution grant. Native App Server web search remains independently unavailable under the existing compatibility policy.

## Decisions and change impact

Decisions retain a statement, rationale, alternatives, exact assumption references, state and optional superseded decision. Each also creates an ordinary `ResearchNote`, preserving existing context and note-history behavior. An accepted superseding decision retires the earlier accepted note. Rejected decisions remain searchable with their rationale; creating a proposal does not accept it. Operation reuse with changed content is rejected.

`project/relations.rs` combines explicit researcher links with projections from existing bindings, theory/check records, experiments, response decisions, literature assessments, claim evidence, executions, structured results, datasets and samples. Each relation retains its origin, acceptance and reason. Unconfirmed bindings and unaccepted evidence do not become accepted dependencies.

**Refresh impact** checks exact versions and bounded declared live-input hashes. A changed live script is compared with the original captured plan's working folder; the historical run remains a valid capture while dependent current work is marked for review. A result points to its execution, allowing changes to propagate through the existing result/binding graph.

Impact statuses distinguish `input_changed`, `check_failed`, `passage_moved`, `source_unavailable`, `unknown`, and `review_needed`. The view retains the traversed reference path and reason. A changed assumption is a request to recheck an argument, not a finding that its conclusion is false. Traversal uses visited sets, a 16-step depth limit, a 500-object impact limit, bounded record reads and a two-second/32 MiB live-file hashing budget. Incomplete coverage is displayed rather than called clean.

**Draft session handoff** creates a retained inventory of proposed notes, open work, produced outputs and the context a fresh conversation would receive. The researcher accepts individual proposed notes using existing note acceptance. This is a host-generated draft inventory; it makes no model call and does not silently turn a summary into accepted memory.

## Acquisition with honest source access

Enable acquisition explicitly for the project in **Library → Acquisition inbox**. The default is off. This setting belongs to Workspace and is independent of Workflow networking, command networking and model web search.

Crossref supports normalized DOI lookup and bibliographic queries. Preview returned candidates, supply a citation key if useful, and import selected works. Receipts retain query/provider/retrieval time, candidate identity, canonical URL and access. Different imports retain distinct source-version identities, including duplicate keys; retrying a completed operation returns its existing result. An available abstract is captured and hashed as an abstract. Crossref metadata is not a promise of full-text access. See the [Crossref REST API documentation](https://www.crossref.org/documentation/retrieve-metadata/rest-api/).

Crossref asks API users for a contact address and, since July 2026, applies its limits per address. **Crossref contact email** in the Acquisition inbox stores your own optional address as an app preference on this computer (not per project). When it is set, literature lookups from the inbox and research discovery send it as Crossref's `mailto` parameter; when it is empty, lookups go to Crossref's public pool without a contact. Pipeline never ships or substitutes a shared address, and it refuses example and placeholder addresses. The address stays in the private Workspace store and its `.pwrx` backups; selective `.pwex` exchange does not include preferences.

Explicit remote PDF acquisition requires a public HTTPS hostname without embedded credentials. Requests have a 30-second timeout, no automatic redirects, a 32 MiB byte cap, PDF MIME and magic checks, and a bounded `pdfinfo` inspection allowing 1–300 pages. Page inspection runs outside the database worker. Enter a verified final PDF URL if the server redirects. Landing pages, blocked downloads, invalid page trees and missing tools produce visible failures.

The original PDF is retained as an immutable source artifact. Existing deterministic extraction supplies searchable text; its limitations remain visible. A failed extraction is unavailable text, never full-text evidence. A no-file metadata import cannot label itself full text. Local PDF import and existing source/citation navigation remain available.

The inbox tracks new lookup results, metadata-only imported items, ready-to-read sources, read items, excluded items and failures. State changes create superseding receipts. Excluding a source removes its version from curated search and context; it does not destroy the retained source.

The existing Zotero metadata preview remains under Literature notes. Selected attachments, remote-library credentials, collection refresh and automatic working-paper/published-version reconciliation require separate adapters and qualification; Pipeline does not write Zotero's database or merge versions automatically.

## Captured data, declared samples and sharing policy

CSV/TSV import captures immutable bytes and a SHA-256. It supports UTF-8/BOM, quoted fields, escaped quotes, embedded newlines and CRLF. Imports refuse ragged rows, invalid quoting and oversized inputs rather than silently dropping cells. Limits are 32 MiB, one million data rows, 2,000 columns, two million cells and 1 MiB per field.

Stata `.dta` import reads releases 113–115 (Stata 8–12) and 117–119 (Stata 13 and later), in either byte order, without Stata. The dictionary records each variable's name, storage type (`byte`, `int`, `long`, `float`, `double`, `str#`, `strL`), display format, variable label and attached value-label set, plus the dataset label and save time. Value-label sets attached to variables are catalogued, including labels for extended missing values; at most 1,000 entries per set and 64 KiB of label text are kept, and the dictionary records how many were left out. System missing (`.`) and extended missing values (`.a`–`.z`) are recognised from their stored bit patterns in every numeric type and counted separately under `missingCodes`; they are never read as numbers. Empty strings are missing in string variables. Dates and times stay Stata numbers, with their `%t` display format recorded. Files up to 32 MiB are kept as a copy and can be previewed. Larger files, up to 2 GiB, are read once in a single streaming pass to build the dictionary and missingness over every row, but no copy is kept: the record names the file, holds the SHA-256 of exactly the bytes read, and row previews are unavailable. A Stata file may have at most 2,000 variables. Truncated, reordered or otherwise malformed files are refused.

Excel `.xlsx` import reads one worksheet (the first, or the sheet you name) within the 32 MiB limit. The first non-empty row is the header, and its first to last named cells are the table's columns; empty header cells inside that range, duplicate names and values outside it are refused. Cells keep their saved values: shared and inline text, numbers, booleans and error values. Formulas are not recalculated. Dates and times remain Excel serial numbers; each column records the cell number formats used (for example `yyyy-mm-dd`), the workbook's 1900 or 1904 date system is recorded, and date-formatted columns are listed. Empty cells, error values such as `#N/A`, and the text NA, N/A, NaN, null or dot are missing, each counted under its own code. Empty rows inside the table count as all-missing; trailing empty rows are ignored. Every archive part is read through a decompressed-size limit (16 MiB for workbook, relationship and style parts, 128 MiB for shared text, 512 MiB for the worksheet), DTDs are refused, and the worksheet is streamed so only statistics and the preview window are kept. Password-protected workbooks, `.xls` and `.xlsb` are not supported.

`DatasetVersion` records the artifact or external immutable reference, hash, dimensions, variable definitions, acquisition information and diagnostic coverage. Local type inference and missing counts cover all captured rows. Empty/NA/N/A/NaN/null/dot values remain missing; zero is a value. Local row previews are limited to 25 rows, 100 columns and 500 bytes per cell and disclose that coverage.

`SampleDefinition` references 1–32 exact datasets and records inclusion/exclusion rules, filters, weights, dates, unit of observation and an optional membership hash. These are declarations. Saving the form does not claim to have executed filters. Changing rules or vintage creates a new identity and can supersede an earlier sample/dataset. Exact dataset/sample references can be attached to captured execution plans.

For large/proprietary data, **Import an exporter dictionary** accepts a schema-version-1 `DatasetVersion` JSON object with an external immutable reference, SHA-256, declared/exporter variable definitions and acquisition metadata. It does not read or validate external rows. Parquet, Arrow, SPSS `.sav` and SAS files are not read directly; describe them with an exporter dictionary. The result-export examples in `examples/research-exporters/` document installed-toolchain requirements; they do not install packages or bypass the Stata adapter.

| Project policy | Default | Scope |
|---|---|---|
| Dictionary | On | Dataset dictionary in curated search and context, including variable labels, value labels, formats and source terms |
| Summaries | Off | Missingness (`missing` and `missingCodes`) and other derived diagnostic fields in those services |
| Assistant rows | Off | Explicit bounded model row-preview tool |
| Package data | Off | Eligibility of raw data artifacts in explicit selective sharing |

A local UI row preview is a researcher action and does not grant the assistant row access. Policy changes wait for active project turns to finish and enter the next harness fingerprint. Revoking dictionary access omits a previously selected dictionary with an explanation, allowing conversation to continue. Search is invalidated when policy changes.

Selective `.pwex` still has its existing object-selection contract; there is no new automatic dataset export. Raw dataset and captured-plan input artifacts are ineligible by default, including when reached through evidence links. Enabling package data permits an explicitly selected eligible artifact; it does not include every dataset. `.pwrx` remains a private whole-store backup and contains captured local data. Arbitrary explicitly authorized host commands retain host access; these curated-service policies are not an operating-system sandbox or a filter on arbitrary execution output.

FRED/ALFRED capture accepts a series ID, an exact vintage date and a transient API key. Requests set both `realtime_start` and `realtime_end` to the requested vintage and retain the response's real-time bounds, observation dates, series units/frequency, `lin` transformation and retrieval date. Requests refuse incomplete results beyond 100,000 observations. The key is used in memory for the request and is omitted from records, recorded request URLs and error messages. See the official [series metadata](https://fred.stlouisfed.org/docs/api/fred/series.html), [observations](https://fred.stlouisfed.org/docs/api/fred/series_observations.html), and [real-time period](https://fred.stlouisfed.org/docs/api/fred/realtime_period.html) contracts.

The [FRED® API Terms of Use](https://fred.stlouisfed.org/docs/api/terms_of_use.html) require a notice wherever FRED data appear. The FRED form, every FRED dataset in Data & samples (including captures made before this notice existed) and the dataset pane show: "This product uses the FRED® API but is not endorsed or certified by the Federal Reserve Bank of St. Louis. By using FRED® data, you agree to be bound by the FRED® API Terms of Use," with a link to the terms. Some FRED series are copyrighted by their owners and need permission for uses beyond personal use; the recorded licence says so, and notes when a series' own notes mention copyright.

Two keyless connectors sit beside FRED and use the same project download permission:

- **DBnomics** captures one series by its `provider/dataset/series` ID through the [DBnomics API v22](https://api.db.nomics.world/v22/series). DBnomics gathers series from the IMF, OECD, Eurostat, the ECB, central banks and other providers. Observations are kept as `period`, `period_start_day` and `value`; DBnomics's `NA` stays missing. At most 100,000 observations, and the request must match exactly one series. DBnomics data are distributed under the Open Database License and keep the original provider's terms ([legal terms](https://docs.db.nomics.world/legal-terms)); the notice names the provider and links its terms of use when DBnomics reports them. IMF series add the IMF's source line and its rule that bulk automated download and model training need the IMF's permission, so capture them on demand only.
- **World Bank Indicators** captures one indicator for up to 60 economy codes (or `all`) and an optional `YYYY`, `YYYY:YYYY`, monthly or quarterly date range, through the [Indicators API v2](https://api.worldbank.org/v2). Pipeline reads the indicator's name, source and source organization, then one page of at most 20,000 observations; larger results are refused with a request to narrow the economies or dates rather than paged silently. Missing values (`null`) stay missing. World Bank data are [CC BY 4.0 by default](https://datacatalog.worldbank.org/public-licenses); third-party indicators may carry other terms, so the licence names the indicator's source organization.

Each connector capture is an immutable CSV dataset version with a `connector` record: the connector, the exact secret-free query and request URLs, the source's own last-updated or indexing time, the licence, the required notice, a suggested citation, the terms URL, and the date Pipeline last reviewed the terms (October 28, 2026). The dataset view shows these under **Source terms and citation**, and the notice appears with the dataset. Requests use HTTPS only, a 30-second timeout, no redirects, and response limits of 1 MiB for metadata and 16 MiB for data. Service error messages are passed on (with a FRED key removed); a failed capture is kept as an acquisition receipt. Keyed sources such as BLS, BEA, Census and IPUMS are not yet available.

## Capture, authorize, test and replay

1. Configure an existing execution profile with an attached project folder, installed executable, relative script paths, declared inputs, expected outputs and timeout. Declare the script, helpers, data and package lockfiles.
2. In **Captured execution**, choose the profile and optional exact datasets/sample. Supply JSON parameters, optional seed and a declared toolchain/environment description.
3. **Capture inputs and review plan** snapshots the declared inputs and launch identity. Review the retained plan before granting execution.
4. **Authorize this exact host plan**, then **Test captured plan**. A successful captured-plan test is required for an ordinary run.
5. **Run / replay in a fresh directory** creates a new execution and fresh private directory from the same retained plan. Inspect jobs, logs and outputs through the existing execution service.

Structured adapters (Stata, R, Python, Julia, notebook and Quarto) each accept one argument shape; see [Run local checks](../projects.md#run-local-checks) for the shapes and `workbench/research/execution/adapters.rs` for validation. Their launch identity adds `environmentIdentity`: hashes of the lockfiles and startup files found in the working folder, such as `renv.lock`, `uv.lock`, `Manifest.toml` or Stata's `profile.do`. A changed lockfile therefore requires a new authorization. A staged plan directory holds only declared inputs, so declare lockfiles that a captured plan must carry. Receipts add `runtimeIdentity` (the version the program reported after the run, or Stata's version from its install folder), `stataLaunchMode` (`direct` or `customLauncher`) and, for Stata logs, each `r(###)` code with its preceding message. Command, LaTeX and custom-launcher profiles keep their earlier launch identity, so existing authorizations remain valid. `workbench_detect_toolchains` is read-only: it inspects standard install folders, `PATH` and the project folder, runs bounded `--version` probes for R, Python, Julia, Quarto and Jupyter, and never starts Stata or MATLAB.

A plan captures complete declared inputs up to 256 MiB, relative paths and executable bits, parameters, seed, profile environment overrides, locale/thread settings, platform, launch/executable hashes, relevant shell startup hashes and declared toolchain version. Supplied dataset hashes must agree with captured declared inputs. Directly referenced launch scripts must be declared. `pipeline-parameters.json` is reserved and generated for each run. The script must read `PIPELINE_PARAMETERS_FILE` and apply `PIPELINE_RANDOM_SEED` itself.

Staging copies bytes into `jobs/captured-<executionId>` and verifies hashes. It never hardlinks the original or retained blobs into the mutable run directory. Launch and shell startup identity are rechecked. Every replay gets a new receipt linked to the plan. Existing queue limits, ownership, cancellation, timeout, output validation and adoption-journal behavior remain in force. Authorizing a plan does not authorize a changed profile or another plan.

Receipts and the captured-job inspector keep three separate claims. The legacy aggregate `snapshotConsistency` field retains its compatible `uncertain` value; the captured-input claim is in the input manifest and validation receipt:

- **Input identity:** `captured_inputs_verified` means the run was staged from the verified declared capture. Existing live-profile execution continues to report its live-file consistency.
- **Dependency coverage:** `declared_only`. Bounded TeX `.fls` recorder paths are retained as observed dependencies, not proof of complete closure.
- **Containment:** `host_access`. Fresh cwd and copied inputs do not restrict access to undeclared host files, installed packages or networking. Toolchain versions are researcher declarations plus host executable identity and, for structured adapters, environment-file hashes and the observed runtime version. They do not describe a provisioned environment.

Ordinary result adoption still requires a successful execution and validated expected outputs. Failed exits, cancellation and unknown outcomes keep their existing distinct receipts. The helper examples serialize `research-results-v1` coefficients, uncertainty, sample/specification identity and declared diagnostics; they never infer authoritative coefficients from logs. Python uses the standard library, R requires `jsonlite`, Julia requires `JSON3`, and Stata's scalar helper runs through the Stata launcher configured in the execution profile.

## Storage, recovery and implementation map

Migration `011_research_desk.sql` is additive. It introduces immutable desk records, context selection, data policy, acquisition settings, retained note versions, FTS projection state and captured-plan authorization/test state. It backfills current notes and available earlier note bodies from existing change history. Historical migrations and the compatibility `workbench` namespace are preserved. Later migrations from independent features may follow migration 11.

Typed service constructors validate new concepts; papers, sources, notes, executions and project records remain authoritative in their existing services. Desk IDs are scoped operation identities and immutable bodies are content hashed. `.pwrx` import preserves desk objects and blobs, clears execution-plan grants/test state and invalidates search for rebuilding. Imported plans require fresh local authorization and test. Existing root mappings and host-authorization retirement continue to apply.

| Layer | Implementation |
|---|---|
| Common exact refs, selections, collections | `workbench/desk.rs`, `desk/tests.rs` |
| Local search and authoritative reads | `workbench/search.rs`, `migrations/011_research_desk.sql` |
| Decisions, dependency projections, handoffs | `workbench/project/relations.rs` |
| Literature and economic data acquisition | `workbench/acquisition.rs` |
| Dataset/sample catalog and policies | `workbench/data/mod.rs` |
| Captured plans and existing execution integration | `workbench/research/execution_plan.rs`, `research/execution.rs` |
| Snapshot/tool integration | `workbench/research.rs`, `commands.rs`, Tauri registrations in `lib.rs` |
| Research desk UI | `WorkspaceProjectSurface.tsx`, `WorkspaceContextTray.tsx`, `research-desk/`, `deskClient.ts`, `deskLayout.ts` |
| Export/restore rules | `workbench/release/archive.rs`, `release/exchange.rs` |

## Validation and qualification

Automated coverage includes a 200-document Unicode/repeated-title project, exact older references, index rebuilds and exclusions, identifier/abbreviation search, context scope and optimistic conflicts, note supersession, distinct data vintages, sample impact, missing-value preservation, row policy, immutable acquisition identities, archive authority retirement, draft recovery, layout restoration, and context-save races.

A real numerical Python fixture captures an analysis, replaces its live script, then tests and replays the capture in different directories; both results must equal the expected mean of 2 within `1e-12`. The opt-in `captured_real_tex_build` fixture compiles twice from the capture after a live edit and verifies PDF/recorder output and the PDF page guard. These establish local fixture results, not bitwise reproducibility of arbitrary projects. The opt-in `live_crossref_doi_lookup` test queries the public Crossref API; `live_dbnomics_series` and `live_world_bank_indicator` query DBnomics and the World Bank.

Data-format tests build Stata files byte by byte (releases 113–115 and 117–119, both byte orders, value labels, labelled extended missing values, strL, truncation at many offsets and corrupted maps, types and label tables) and Excel workbooks with the zip writer (shared, rich and inline text, errors, booleans, formulas, custom and built-in date formats, 1904 dates, gaps, DTDs and an oversized part). Connector tests parse responses recorded from FRED, DBnomics and the World Bank on October 28, 2026. Files written by Stata, Excel or LibreOffice themselves have not yet been qualified.

Credentialed FRED/ALFRED acquisition, live DBnomics and World Bank captures in the app, real Zotero attachments, real Stata, R, Julia, Jupyter and Quarto runs (adapter tests use fixture programs), toolchain detection on real Windows and Linux installs, authenticated model use of the new tools, packaged-app behavior and other operating systems remain unqualified. No general release claim should be based on these deterministic tests alone. The separate release checklist remains authoritative.

Useful verification commands from the repository root:

```sh
npm --prefix gui run build
npm --prefix gui test -- --maxWorkers=2
cargo test --locked --manifest-path gui/src-tauri/Cargo.toml --lib
cargo clippy --locked --manifest-path gui/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path gui/src-tauri/Cargo.toml captured_real_tex_build --lib -- --ignored
cargo test --locked --manifest-path gui/src-tauri/Cargo.toml live_crossref_doi_lookup --lib -- --ignored
```

The last two commands are opt-in local-tool/network checks. Use the repository's native-runtime, real-tool and packaging qualification workflow before shipping.
