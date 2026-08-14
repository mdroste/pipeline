# Generalizing Pipeline: from paper reviews to arbitrary LLM workflows

Original design notes, 2026-07-02. Generalization is now implemented. The
sections below retain the original motivation; `CLAUDE.md` is authoritative
for the current engine.

## Current artifact-access contract

The implementation keeps the flat step list and uses an explicit dependency
graph rather than an implicit wave schedule:

- `after` is order-only and never exposes a producer's data.
- `context.include` is an exact artifact allowlist. Sequential steps may select
  an upstream step's report or supporting files, which automatically adds a
  dataflow edge. Parallel steps cannot select any step output.
- Primary input representations are independently selectable as readable text,
  canonical structure, visuals, and original source.
- Survey data, named-input text/source, upstream reports, and upstream files
  are separate selectors. Supporting files may be filtered by a producer-rooted
  glob.
- Every call receives a private staged artifact view and per-call filesystem
  authority. Every agent/fan-out unit writes to its own producer directory.
- An empty context is intentional isolation. There is no adjacency fallback.

Profiles are schema v6 and all built-in/new profiles serialize this explicit
format. `Read` and `Write` are derived capabilities, not profile tool flags.

## The core insight: most of the architecture is already general

The unified step model was the right call and needs almost no change:

| Already general | Why |
|---|---|
| `StepConfig` (id, label, prompt, phase, tools, agents, model, effort) | Nothing paper-specific in it |
| Executor (waves, semaphore, retries, cancel, failed_steps) | Operates on opaque prompts and text |
| Multi-agent + merge | Domain-agnostic synthesis |
| Profiles + export/import envelopes | Already a workflow-definition format |
| Template placeholders + lint | Extensible token catalog |
| Read-tool sandbox (api_common) | Path-validated, dir-scoped — extendable to Write |
| Settings/provider/dispatch layer | Fully generic |

What is actually paper-specific is a thin layer:

1. **Ingest** — `extract.rs` assumes one PDF/LaTeX file.
2. **Orientation** — the serde schema (sections, formal results, notation) is
   hard-coded for papers; the *mechanism* (one LLM call → validated JSON →
   shared context) is general.
3. **Prompts** — referee content, `{paper_path}`/`{paper_type}` tokens,
   figure hints in the parallel-context template.
4. **Output assumption** — `StepOutput.raw_text` is one markdown string; the
   viewer renders everything as markdown.
5. **Naming** — "paper", "referee passes", `SidebarReferees`, etc.

Generalization = replacing those five assumptions, not the engine.

## Design principles

- **A workflow is a profile.** No new concept. "Paper review" becomes one
  built-in profile among several (e.g. "Codebase review", "Grant proposal",
  "Data-analysis writeup"). The existing export/import bundle is already the
  sharing format.
- **Keep the flat list + phases model.** The editor presents a readable list
  and wave diagram while explicit order/dataflow edges determine readiness.
- **Artifact roles are logical; files are the runtime representation.** Profiles
  select producer/role pairs rather than run-directory paths. The resolver maps
  those roles to a private per-call view.
- **The viewer renders by file kind, in the webview, with zero native deps.**
  That is what makes it robust on macOS/Windows/Linux.

## The five moves

### 1. Run directory + artifact manifest (foundation)

Replace the scattered `history/{hash}.json` + `cache/papers/{hash}.txt` with a
run-centric layout. Current builds use only the run store; old directories are
left untouched but are no longer read:

```
~/.pipeline/runs/{run_id}/
├── manifest.json          # inputs, profile snapshot, timings, failed_steps
├── report.md              # final rendered report (as today)
├── inputs/                # copy or reference of what was ingested
├── context/               # document.md, orientation.json
└── artifacts/{step_id}/   # anything a step wrote (code, CSV, images, …)
```

`save_all_artifacts` already writes 80% of this shape on demand; the change is
to make it the *default* home of a run instead of an export. The manifest
records every artifact (relative path, byte size, sha256, detected kind) so
the frontend never has to walk the filesystem blindly.

### 2. Write-enabled steps (sandboxed)

Add `Write` to the tool vocabulary (the `tools` field was designed for this):

- **CLI mode**: pass `--allowedTools Read,Write`, set the subprocess cwd to
  the step's artifact dir, and `--add-dir` it. The prompt template gains an
  `{output_dir}` token: "Write each refactored file into {output_dir}".
- **Direct-API mode**: implement a `Write` tool next to `Read` in
  `api_common.rs`, reusing `validate_tool_path`-style checks but against a
  single allowed *write* root (the step's artifact dir). Never allow writes
  outside it — this also bounds the blast radius of prompt injection from
  untrusted inputs.

After the step completes, Rust scans the artifact dir and appends entries to
the manifest. `StepOutput.raw_text` stays what it is — the step's prose
summary — so the report format doesn't change and old reports keep loading.

### 3. Generalized ingest

`extract.rs` becomes one ingester among three input modes, selected per
profile:

- **Document** (today's behavior): PDF/LaTeX → text + hash.
- **Folder**: record a file inventory (paths, sizes, kinds) as the context
  document instead of inlining contents; steps use the Read tool to open files
  on demand. This sidesteps context limits and is exactly how the CLI agents
  already prefer to work.
- **None**: workflow runs from the prompt alone (e.g. "draft X from these
  instructions").

The orientation stage generalizes to a required **survey** step: same
mechanism (one LLM call, JSON output, retry on parse failure), but the schema
becomes per-profile — either free-form JSON (validated as JSON, not against a
struct) or the current paper schema for the built-in review profiles. The
hard-coded serde struct is the only thing to relax.

### 4. Neutral vocabulary + token aliases

`{paper_path}` → `{input_path}`, `{paper_type}` → gone (profile-specific
context instead), "referee passes" → "steps". The codebase already has the
alias pattern for this (`{referee_reports}` is still accepted in sequential
prompts) — old tokens keep working, docs and UI use the new ones.

### 5. Artifact explorer + multi-kind viewer

Replace the single artifact dropdown in `App.tsx` with a two-pane explorer:
a tree (run → context → steps → files, driven by the manifest) and a viewer
pane that picks a renderer by detected kind:

| Kind | Renderer | Notes |
|---|---|---|
| markdown | existing ReportViewer (KaTeX, TOC, comment cards) | unchanged |
| code (rs, py, ts, tex, do, R, …) | read-only CodeMirror 6 **or** highlight.js `<pre>` | pure JS, no native deps; start with highlight.js (~40 kB core + languages), upgrade to CodeMirror if search/folding is wanted |
| json | pretty-printed via the code renderer + collapsible tree later | orientation.json benefits immediately |
| csv/tsv | simple virtualized table (first N rows + "open externally") | |
| image (png/svg/jpg) | `<img>` via a backend `read_artifact` command returning base64 | avoids Tauri asset-scope/path-scheme differences across WKWebView/WebView2/WebKitGTK — the single most robust cross-platform choice |
| pdf / binary / oversized | metadata card + "Open in system viewer" (opener plugin) | don't embed PDF renderers |

Cross-platform robustness rules:

- **All file bytes flow through one Tauri command** (`read_artifact(run_id,
  rel_path) -> {kind, base64|text, truncated}`) with path validation against
  the run dir — no `file://` URLs, no asset-protocol scope tuning per OS.
- **Size caps with explicit truncation banners** (reuse the pattern from the
  log-truncation fix): text >1 MB renders the head with a notice; binaries
  never inline.
- **Kind detection in Rust** (extension + content sniff), recorded in the
  manifest, so the frontend never guesses.
- **Pure-JS renderers only.** No native previewers, no shelling out except
  the explicit "open externally" action.

## Phasing (each phase ships something useful on its own)

1. **Viewer first.** Artifact explorer + kind-based renderers over what runs
   already produce (report, extracted text, orientation.json, per-step
   outputs). Immediately improves the current paper workflow (orientation as
   pretty JSON, extracted text as plain text, not fake-markdown). No backend
   schema changes beyond a `read_artifact` command.
2. **Run directory + manifest.** Move run outputs to `runs/{id}/` and switch
   the viewer to the manifest-driven tree.
3. **Write-enabled steps.** `{output_dir}` + sandboxed Write tool in both
   dispatch modes. This is the moment "generate code files" workflows work.
4. **Generalized ingest + survey + vocabulary.** Folder/none input modes,
   per-profile survey schema, neutral tokens, 2–3 non-paper built-in
   profiles to prove the generality (e.g. "Codebase review" is nearly free:
   folder ingest + Read tool + review prompts).

## Non-goals (the simplicity budget)

- No DAG/graph editor — waves are enough, and the diagram stays legible.
- No plugin system or embedded scripting — steps are prompts + tools, period.
- No Bash tool by default. If ever added (test-running workflows), it must be
  per-step opt-in, CLI-mode only, with a loud permission surface.
- No embedded PDF viewer, no WYSIWYG editing of artifacts.
- No server/multi-user anything.

## Risks and open questions

- **Prompt injection × Write tool**: an untrusted input document can tell the
  LLM to write files. Sandboxing writes to the run's artifact dir (and never
  executing anything) keeps this contained; the report should list written
  artifacts so nothing lands silently.
- **`claude -p` write behavior**: `--permission-mode acceptEdits` is already
  set; verify Write stays confined with cwd + `--add-dir` on all three OSes
  (the temp-dir canonicalization bug is the cautionary tale — test the
  sandbox empirically per platform).
- **Folder ingest context economics**: inventory-only context means steps
  must be good at choosing what to Read; the survey step likely matters more
  here than for papers. Needs prompt iteration.
- **Windows paths in prompts**: keep the existing forward-slash normalization
  for every new token (`{output_dir}` included).
- **Back-compat**: old reports (`referee_reports`/`editor`) already load via
  `all_outputs()`; keep that path working. Profiles gain fields with
  `#[serde(default)]` as usual.
