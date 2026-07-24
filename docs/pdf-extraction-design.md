# PDF extraction redesign: verified native-PDF LLM default, uv-managed local engines

Design notes, 2026-07-03. Written against the codebase after the runs/artifacts
generalization. Goal: state-of-the-art PDF → Markdown + page PNGs with faithful
equations, while keeping the app small by default, modular, and user-controlled.

## Where extraction stands

The cascade in `extract.rs` is: LaTeX source → LLM extraction (default) →
marker_single (if the user pip-installed it) → bundled pdftotext. Weaknesses:

1. **LLM extraction is one giant call** — "Read the PDF and transcribe it."
   On a 40-page paper this invites truncation and silent summarization, and
   it produces no page images.
2. **Provider-gated** — OpenAI/Google direct APIs can't return PDF bytes
   through the Read tool (`pdf_read_supported = false`), so those users are
   forced down to native extraction.
3. **marker was an afterthought** — this was subsequently addressed by the
   managed engine, retained figure artifacts, and the DocumentBundle layer.
4. **pdftotext garbles equations** — that's why `scan_math_quality` exists.

## What the research says (July 2026)

Two findings drive this design (full notes: olmOCR-bench, OmniDocBench, the
ICPR 2026 math-formula benchmark, socOCRbench):

- **Frontier LLMs used as extractors are now at or above dedicated OCR models
  for math on clean academic PDFs** (Gemini 3 Pro 9.75/10, Qwen3-VL 9.76 on the
  ICPR math benchmark — above Mathpix at 9.64). Our users already have LLM
  credentials because the app requires them. The best extractor is one we
  already dispatch to.
- **The local models above marker either need NVIDIA hardware our users don't
  have** (olmOCR 2, Chandra 2) **or carry revenue-capped/AGPL licenses**
  (Chandra, Surya: OpenRAIL $2M cap; MinerU: AGPL). marker (GPL code, invoked
  as a subprocess — fine) remains the pragmatic cross-platform CPU/MPS local
  option; MinerU's MLX engine is the quality upgrade path on Apple Silicon.

Benchmark caveat: scores are largely vendor-self-reported and the two major
benchmarks disagree wildly (MinerU: 95.7 on OmniDocBench vs 75.2 on
olmOCR-bench). Nothing here bets the architecture on a leaderboard position.

## Design principles

- **User control, existing shape.** The extractor stays a visible choice:
  global `pdf_extractor` setting + per-profile `ExtractionConfig.method`
  override, exactly as today. No hidden auto-selection beyond the existing
  "auto" value. Nothing installs without an explicit click.
- **Lightweight by default.** The app binary stays ~20 MB. No Python, no
  model weights, no uv in the bundle. The heavy stack is opt-in, lives
  entirely under `~/.pipeline/`, reports its disk usage, and uninstalls
  cleanly.
- **Modular.** Local extractors go through a small engine registry, not
  marker-shaped special cases. Adding MinerU later is a registry entry plus
  an invocation adapter, not new plumbing.
- **Rust does structure, LLMs do judgment.** Page rendering, chunking,
  stitching, validation, and provisioning are deterministic Rust. The LLM
  only transcribes what it sees.

## Move 1: native PDF input + verified extraction (the "llm" method, upgraded)

All three providers accept PDFs natively now; the extraction call should hand
the model the PDF directly rather than hoping a tool loop fetches it. The
extraction stays a single call by default. What changes is the transport and
that completeness stops being taken on faith. The settings value stays
`"llm"` — no migration.

Transport per path:

| Path | How the PDF reaches the model |
|---|---|
| Claude CLI / Gemini CLI | Unchanged — prompt references the path, the CLI's Read tool is multimodal. |
| Codex CLI | Verify PDF-read support during implementation; if absent, fall back as today. |
| Anthropic direct API | Attach as a base64 document block up front (the plumbing exists in `api_anthropic.rs` for the Read tool; attaching in the first request skips the tool round-trip). ~100-page / 32 MB request limit. |
| OpenAI direct API | Attach as a file input (base64 `input_file` content part). ~100-page / 32 MB limit. |
| Google direct API | Attach as `inline_data` (application/pdf), or the File API for large documents (up to ~1000 pages). |

This **removes the `pdf_read_supported` limitation** — all five paths can
extract PDFs, and the OpenAI/Google forced-fallback notice goes away.

Verification (the actual fix for silent truncation/summarization):

1. The prompt asks for `<!-- PAGE n -->` markers before each page's content —
   cheap for the model, and it turns completeness into something Rust can
   check.
2. After the call, deterministically verify: every page marker present, and
   per-page text length sane against a `pdftotext` baseline (poppler is
   bundled; the baseline is free). A page that's missing or suspiciously
   short gets one targeted follow-up call for just that page range — the
   same PDF is attached again with a "transcribe ONLY pages N–M"
   instruction. (Implemented this way instead of splitting the PDF with a
   library like `lopdf`: page extraction that preserves shared fonts and
   resources is error-prone, and re-sending the input for the rare repair
   call costs pennies.) Pages that still fail become extraction-quality
   notes naming the page numbers, feeding the existing quality-notes path.
3. `scan_math_quality` runs on the result as today; final markdown lands in
   `cache/papers/{hash}.txt` and the run's `context/` as usual.

Repair is therefore **reactive, not proactive** — a paper that extracts
cleanly in one call costs one call. A PDF that exceeds a provider's hard
request limit fails the attachment with a clear message and falls back to
native extraction; range-splitting oversized PDFs is deferred until someone
actually hits it.

Page PNGs (the PDF → PNG requirement) are decoupled from extraction: bundled
`pdftoppm` renders `runs/{id}/artifacts/pages/page-NNN.png` as run artifacts
for the ArtifactExplorer regardless of which extraction method ran. This is
deterministic, fast, and has nothing to do with what the model sees.

Touches: `extract.rs` (`extract_llm` transport + verification loop),
`api_anthropic.rs`/`api_openai.rs`/`api_google.rs` (up-front PDF attachment),
`runs.rs` (`RunWriter::register_existing` for binary artifacts — today only
`add_text` exists). No new dependencies.

## Move 2: managed local engines via uv (new module `engines.rs`)

The 2026-standard pattern (ComfyUI Desktop is the precedent at scale):
download the `uv` binary on demand, then let uv own an app-scoped Python
toolchain. Nothing touches system Python; the CLAUDE.md "no Python, no pip"
promise holds because the app owns the runtime invisibly.

Layout — everything under one deletable root:

```
~/.pipeline/
├── bin/            # uv binary + tool entry-point shims (UV_TOOL_BIN_DIR)
├── tools/          # per-engine venvs (UV_TOOL_DIR)
├── python/         # managed CPython (UV_PYTHON_INSTALL_DIR)
├── uv-cache/       # wheel cache (UV_CACHE_DIR — same volume, hardlinked)
└── hf/             # model weights (HF_HOME)
```

Engine registry (static, in Rust):

```
EngineSpec { id, label, description, pip_spec, entry_point,
             est_download, est_disk, extra_env, platform_index_args }
```

Ships with one entry — `marker` (`pip_spec: "marker-pdf"`, entry point
`marker_single`, ~500 MB packages + ~2–3 GB models) — and is designed so
`mineru` is a later addition, not a rework.

`ensure_uv()`:
- Pinned uv version; per-platform download URL + SHA-256 table baked into the
  binary (updated on our release cadence). ~25 MB download to
  `~/.pipeline/bin/`, verified before chmod +x.
- Files downloaded by our own code carry no quarantine xattr (macOS) or
  Mark-of-the-Web (Windows) — no Gatekeeper/SmartScreen prompts, no
  notarization impact. Verification responsibility is therefore ours: hard
  SHA-256 check, no "latest" URLs.
- Set `UV_SYSTEM_CERTS=1` and pass through `HTTP(S)_PROXY` — corporate/
  university TLS interception is the top real-world failure mode.

`install_engine(id)` — three labeled phases, each streaming to the log panel
via new `engines:phase` / `engines:log` events (same pattern as
`pipeline:log`), child PIDs registered for cancellation:
1. *Runtime* — `ensure_uv()`; disk-space precheck against `est_disk`.
2. *Packages* — `uv tool install <pip_spec> --python 3.12` with the env vars
   above. On Linux/Windows add the PyTorch CPU index
   (`https://download.pytorch.org/whl/cpu`) — the PyPI default on Linux is the
   CUDA build (~3–4 GB download, 6–8 GB installed) that most of our users
   can't use. macOS needs no pin (default wheel is MPS-enabled, 88 MB).
3. *Models* — pre-warm weights with a bundled one-page PDF run
   (`HF_HOME=~/.pipeline/hf`), so the 2–3 GB HuggingFace download happens
   under the progress UI, not silently during the user's first real review.

Also: `engine_status(id)` (installed, version via `uv tool list`, disk usage
of tools/ + hf/), `uninstall_engine(id)` (`uv tool uninstall` + remove hf/),
`cancel_install()`. All exposed as Tauri commands.

Discovery changes: `find_command` in `extract.rs`/`deps.rs` checks
`~/.pipeline/bin/` first, then PATH — a system-installed marker still works,
and `deps.rs` labels the source "managed" vs "system" (same pattern as
bundled-vs-system poppler). Marker invocations gain
`HF_HOME=~/.pipeline/hf` in their env when the managed install is in use.

Licensing: uv is MIT/Apache dual — runtime download needs at most a docs
note (add to THIRD_PARTY_LICENSES.md only if we ever bundle it). marker is
GPL-3 code + revenue-capped OpenRAIL weights: subprocess invocation is fine,
never bundle it; add one plain sentence to the README so commercial users
know the weights' terms are between them and Datalab.

## Move 3: marker invocation upgrade

While in there: run `marker_single` with `--output_dir` pointing into
`runs/{id}/artifacts/marker/` instead of scraping stdout. Read the emitted
`.md` as the extraction text; register extracted figure images in the
manifest so the ArtifactExplorer shows them. Keep stdout capture as the
fallback for older marker versions. `marker_disable_images` was initially
retained as true. As of the DocumentBundle work (2026-07-23), new settings
default it to false because figure retention is part of the normal document
contract; users can still disable it when speed or disk use matters more.

## Move 4: UI

- **SettingsPage** — extraction dropdown relabeled for honesty:
  "LLM (default — uses your configured provider)", "Local engine:
  marker", "pdftotext (basic, equations lost)", "auto". Below it, a **Local
  Engines** section: one card per registry entry — name, one-line
  description, size estimate ("~0.5 GB packages + ~2.5 GB models"), Install
  button → phase-labeled progress bar with streamed log lines → Installed
  state showing version, disk usage, and Uninstall. Cancel supported
  mid-install.
- **PipelinePage** — the per-profile extraction override dropdown gets the
  same labels; no structural change (ExtractionConfig already carries it).
- **DepsCheck** — marker row reflects managed installs.
- `lib/types.ts` mirrors `EngineSpec`/status types; component tests for the
  engines card (install flow states) and the relabeled dropdown.

## Phasing

1. **Native PDF extraction + verification** (Move 1) — removes a provider
   limitation, makes truncation detectable instead of silent, and ships page
   PNGs as artifacts. No new dependencies. Independent of everything else.
   *Implemented 2026-07-03.*
2. **Provisioning backend** (Move 2 minus UI) — `engines.rs`, Tauri
   commands, discovery changes, tested headlessly.
   *Implemented 2026-07-03. Deviations: uv is pinned (version + five SHA-256
   constants in engines.rs) rather than tracking latest; the pre-warm runs
   the engine on a generated one-page PDF and downgrades to a warning on
   failure; uninstall keeps uv and the wheel cache (small, make reinstall
   fast) and removes model weights only when the last engine goes.*
3. **Engines UI + marker upgrade** (Moves 3–4).
   *Implemented 2026-07-03. The Local Engines cards live inside Settings →
   Text Extraction (EnginesPanel.tsx) rather than a separate nav section;
   marker output goes to `~/.pipeline/cache/marker/{hash}/` and figure
   images are copied into the run's artifacts after the run dir exists,
   since extraction runs before the run id is known.*
4. **Later, on demand** — `mineru` registry entry (MLX engine on Apple
   Silicon, richer layout JSON + figure images; AGPL noted in its card), and
   a private ~20-paper eval to settle local-engine ranking if it ever
   matters.

## Non-goals

- No bundling of Python, uv, or model weights in the app.
- No CUDA management. (A "CUDA build" opt-in checkbox is possible later;
  CPU/MPS is the supported path.)
- No hosted OCR APIs (Mistral OCR, Datalab, Mathpix) — redundant with
  vision extraction quality and each adds an API-key surface.
- No chasing NVIDIA-only leaderboard toppers (Chandra 2, olmOCR 2).
- No DAG/extractor plugin system — the registry is a static Rust table.

## Compatibility

- `pdf_extractor` values unchanged (`llm`/`auto`/`marker`/`pdftotext`);
  existing profiles and export bundles load untouched. `"llm"` silently gets
  better; `"auto"` resolves managed-or-system marker → pdftotext as before.
- Old saved reports/runs unaffected; new runs gain `pages/` artifacts.
- CI unchanged (no new bundled binaries to sign). Release workflow unchanged.

## Testing

- Pure-function unit tests: page-marker verification, short/missing-page
  detection against a pdftotext baseline, range-split computation, uv URL/SHA
  table shape, engine path resolution precedence.
- Mocked-subprocess tests for install phase sequencing and cancel (same style
  as existing marker/claude wrappers).
- Vitest: engines card states (not installed / installing with progress /
  installed / failed), dropdown labels.
- Manual matrix before release: macOS arm64 (MPS), Windows x64, Linux x64
  (CPU-pinned torch), each: install marker → extract a math-heavy paper →
  verify pages + figures in the explorer.

## Open questions

- Codex CLI PDF-read support — verify during Move 1; affects only the
  fallback table.
- How aggressive the per-page length check can be before it false-positives
  on figure-heavy pages (a page that is one figure has little pdftotext
  baseline) — tune against a handful of real papers.
- Whether `uv tool list` output is stable enough for version reporting or we
  read the venv's `marker-pdf` dist-info directly.
