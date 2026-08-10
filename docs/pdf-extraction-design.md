# PDF extraction: bounded verification and optimized local engines

Original design notes, 2026-07-03; reliability revision, 2026-07-24; managed
Full Parser revision, 2026-08-07; single-parser revision, 2026-08-09. Goal:
faithful PDF → Markdown + page images with explicit user control, bounded
latency, resumable local work, and enough provenance to audit the result.

## Single-parser revision (2026-08-09)

PaddleOCR-VL Full Parser is the only supported local OCR extractor. The former
direct Q8/Fast method is retired as an extraction choice. Existing settings,
profiles, and import bundles that contain `paddleocr-vl` are migrated to
`paddleocr-vl-full` during deserialization. Historical Fast structures remain
readable.

The Full Parser is installed and reported as one managed engine even though it
contains two implementation components: the native Q8/llama.cpp recognition
server and the isolated PaddleOCR/PP-DocLayoutV3 client. A single install action
provisions both, and uninstall removes both app-owned roots.

## Managed Full Parser revision (2026-08-07; historical context)

Pipeline initially exposed two local PaddleOCR-VL methods. **Fast** retained the
native Q8 page-transcription path. **Full Parser** added the official
`PaddleOCRVL` client and PP-DocLayoutV3 while reusing the same managed,
authenticated llama.cpp recognition server. It calls `restructure_pages`
before normalization and preserves official block labels, reading order,
coordinates/polygons, matched layout confidence, title hierarchy, formula
numbers, cross-page tables, and extracted images in the DocumentBundle.

The Full Parser is an explicit optional install. A checksum-pinned uv
bootstrap owns CPython 3.12 and a versioned private environment under
`~/.pipeline/native/paddleocr-parser/`; no system Python, pip, Conda, Docker,
or user-installed LlamaCPP is consulted. The installed sidecar is verified
against the app's bundled digest, and sidecar-only updates reuse a verified
pinned runtime without reinstalling Python or Paddle. Supported release
targets are macOS arm64, Windows x64, and Linux x64/arm64. Full Parser is
unavailable on Intel macOS because PaddlePaddle 3.2.1 has no matching wheel.

## Security retirement (2026-07-27)

Marker installation and execution were removed for Pipeline 1.0.1. The
Marker release compatible with Pipeline's integration requires a Python
dependency closure with known security vulnerabilities, and the current
upstream release still constrains at least one affected dependency below its
fixed version. Pipeline therefore does not discover, install, or execute
managed or system Marker binaries. A legacy `marker` setting remains readable
only so the UI can explain the retirement and let the user choose
PaddleOCR-VL, LLM extraction, or pdftotext. Historical Marker structures in
saved runs remain readable. The sections below labeled “historical” document
the retired implementation and are not current product behavior.

## Reliability revision (2026-07-24)

Production traces exposed failure modes that the initial single-call design
did not cover:

| Failure | Consequence | Current behavior |
|---|---|---|
| One LLM call for a long paper under a 32K output cap | A 99-page paper returned only 11–15 pages | The paper is proactively divided into ranges of at most 8 pages and roughly 50K text-layer characters; two ranges may run concurrently. |
| Three repairs, each with its own long timeout | Extraction could exceed 20–40 minutes | The whole extraction stage has a configurable wall-clock budget (30 minutes by default). |
| Missing markers or failed repairs became warnings | Orientation and review could run on an incomplete paper | Page markers are mandatory. Missing or suspicious pages get one smaller-range retry; any remaining gap fails before orientation. |
| Paddle used `--ctx-size 16384 --parallel 2` | llama.cpp divided 16K across two slots, leaving only 8K per page | Context is now 16K **per slot** (`ctx-size = 16384 × slots`). |
| Paddle ignored `finish_reason` and aborted on the first page error | Truncation could look successful; one failure discarded completed work | Non-`stop` responses fail, pages retry individually, and verified pages are checkpointed. A length failure switches the retry to adaptive layout regions instead of repeating the same request. |
| Extractor failure silently changed methods | A local-engine request could unexpectedly incur an LLM call, or an LLM failure could degrade to plain text | Extractors no longer fall back across methods. `pdftotext` is never used as extraction output unless explicitly configured; when available, per-page text-layer character counts serve only as a conservative completeness signal. |
| The run log started after extraction | The slowest and most failure-prone stage had no durable transcript | Preprocessing logs begin before extraction. Successful logs are adopted by the run; failed logs remain under `~/.pipeline/logs/preprocessing/`. |
| Fresh runs ignored prior extraction work | Re-running the same paper repeated the expensive stage | Exact source/settings matches reuse verified LLM output; Paddle resumes verified page checkpoints. |

Extraction is evidence-preserving. Prompts explicitly preserve spelling and
typographical errors verbatim; later review steps may flag them, but
preprocessing never silently corrects the paper.

## Where extraction stands

The configured method in `extract.rs` is authoritative. LaTeX source is always
preferred when present; PDF users explicitly select LLM, PaddleOCR-VL Full
Parser, or pdftotext. There is no cross-extractor cascade.
Historical
weaknesses were:

1. **LLM extraction is one giant call** — "Read the PDF and transcribe it."
   On a 40-page paper this invites truncation and silent summarization, and
   it produces no page images.
2. **Provider-gated** — OpenAI/Google direct APIs can't return PDF bytes
   through the Read tool (`pdf_read_supported = false`), so those users are
   forced down to native extraction.
3. **Marker was an afterthought** — the historical integration later retained
   structured blocks and figures, but its executable path is now retired.
4. **Plain-text PDF extraction garbles equations** — it remains an explicit
   basic option rather than an automatic fallback.

## What the research says (July 2026)

Two findings drive this design (full notes: olmOCR-bench, OmniDocBench, the
ICPR 2026 math-formula benchmark, socOCRbench):

- **Frontier LLMs used as extractors are now at or above dedicated OCR models
  for math on clean academic PDFs** (Gemini 3 Pro 9.75/10, Qwen3-VL 9.76 on the
  ICPR math benchmark — above Mathpix at 9.64). Our users already have LLM
  credentials because the app requires them. The best extractor is one we
  already dispatch to.
- **PaddleOCR-VL Full Parser is the supported local path.** It combines the
  managed Q8 GGUF model and pinned native llama.cpp runtime with an isolated,
  app-owned Python package closure for the official layout pipeline.
  Other candidates either require NVIDIA hardware uncommon among the target
  users or introduce licensing/deployment constraints that require separate
  evaluation.

Benchmark caveat: scores are largely vendor-self-reported and the two major
benchmarks disagree wildly (MinerU: 95.7 on OmniDocBench vs 75.2 on
olmOCR-bench). Nothing here bets the architecture on a leaderboard position.

## Design principles

- **User control, existing shape.** The extractor stays a visible choice:
  global `pdf_extractor` setting + per-profile `ExtractionConfig.method`
  override, exactly as today. No hidden auto-selection beyond the existing
  "auto" value. Nothing installs without an explicit click.
- **Lightweight by default.** No Python or model weights are in the app bundle.
  The Paddle stack is opt-in, lives entirely under `~/.pipeline/`, reports its
  disk usage, and uninstalls cleanly.
- **Modular.** Installable local extractors go through a small engine
  registry. Historical Marker artifact parsing is isolated from executable
  discovery and provisioning.
- **Rust does structure, LLMs do judgment.** Page rendering, chunking,
  stitching, validation, and provisioning are deterministic Rust. The LLM
  only transcribes what it sees.

## Move 1: native PDF input + verified extraction (the "llm" method, upgraded)

All three providers accept PDFs natively now; the extraction call should hand
the model the PDF directly rather than hoping a tool loop fetches it. The
extraction uses bounded page-range calls, and completeness is a hard
precondition rather than a warning. The settings value stays `"llm"` — no
migration.

Transport per path:

| Path | How the PDF reaches the model |
|---|---|
| Claude CLI / Gemini CLI | Unchanged — prompt references the path, the CLI's Read tool is multimodal. |
| Codex CLI | Prompt references the path and the CLI reads it from the granted source directory. |
| Anthropic direct API | Attach as a base64 document block up front (the plumbing exists in `api_anthropic.rs` for the Read tool; attaching in the first request skips the tool round-trip). ~100-page / 32 MB request limit. |
| OpenAI direct API | Attach as a file input (base64 `input_file` content part). ~100-page / 32 MB limit. |
| Google direct API | Attach as `inline_data` (application/pdf), or the File API for large documents (up to ~1000 pages). |

This **removes the `pdf_read_supported` limitation** — all five paths can
extract PDFs, and the OpenAI/Google forced-fallback notice goes away.

Verification (the actual fix for silent truncation/summarization):

1. The prompt asks for `<!-- PAGE n -->` markers before each page's content —
   cheap for the model, and it turns completeness into something Rust can
   check.
2. A local text-layer map supplies page counts and conservative length checks.
   Initial requests cover no more than 8 pages / roughly 50K baseline
   characters. Missing or suspicious pages get one targeted retry in ranges
   of at most two pages. A missing marker, truncated response, or page that
   remains implausibly short fails the extraction before orientation.
3. `scan_math_quality` runs on the result as today; final markdown lands in
   `cache/papers/{hash}.txt` and the run's `context/` as usual.

Chunking is proactive and bounded. The same PDF is attached/read for each
range because splitting PDF object graphs while preserving shared resources is
error-prone. Two ranges may run concurrently, and a document-level time budget
stops cost and latency from growing without bound. There is no automatic
native or plain-text fallback.

Page renders are decoupled from extraction: bundled `pdftoppm` renders compact
120-DPI JPEGs under `runs/{id}/artifacts/pages/` for the ArtifactExplorer
regardless of which extraction method ran. JPEG avoids the CPU and disk cost
of lossless full-page PNGs while retaining enough detail for figures and
visual extraction checks.

Touches: `extract.rs` (`extract_llm` transport + verification loop),
`api_anthropic.rs`/`api_openai.rs`/`api_google.rs` (up-front PDF attachment),
`runs.rs` (`RunWriter::register_existing` for binary artifacts — today only
`add_text` exists). No new dependencies.

## Historical Move 2: managed Marker via uv (retired)

The original implementation used this pattern:
download the `uv` binary on demand, then let uv own an app-scoped Python
toolchain. Nothing touched system Python; this same app-owned-runtime boundary
is reused by the 2026-08-07 Paddle Full Parser, without reviving Marker.

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

The original registry shipped one Python entry — `marker` (`pip_spec:
"marker-pdf"`, entry point `marker_single`, ~500 MB packages + ~2–3 GB
models). Pipeline 1.0.1 removed the Python engine kind, uv provisioning, and
this registry entry. At that release the registry contained only native
PaddleOCR-VL; the later Full Parser add-on reintroduces a narrowly scoped
managed Python engine, not generic Python-tool discovery.

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

The retired implementation checked `~/.pipeline/bin/` before PATH and
redirected managed Marker model weights to `~/.pipeline/hf`. Pipeline 1.0.1
does neither: dependency checks ignore Marker and the extraction guard rejects
the legacy value before hashing or command resolution.

The historical licensing assessment treated uv as MIT/Apache dual and Marker
as GPL-3 code with separately licensed model weights. Neither component is
downloaded or bundled by Pipeline 1.0.1.

## Historical Move 3: Marker invocation upgrade (retired)

The retired implementation ran Marker with `--output_dir` pointing into the versioned
`~/.pipeline/cache/marker/{hash}/` workspace instead of scraping stdout. Read the emitted
`.md` as the extraction text; register extracted figure images in the
manifest so the ArtifactExplorer shows them. Keep stdout capture as the
fallback for older marker versions. `marker_disable_images` was initially
retained as true. As of the DocumentBundle work (2026-07-23), new settings
default it to false because figure retention is part of the normal document
contract; users can still disable it when speed or disk use matters more.
Marker's own automatic OCR detection was the default; users could instead
disable or force OCR. Settings also exposed figure extraction, low/high
resolution DPI, machine-aware PDF-text workers, and layout/OCR recognition
batches. Successful Markdown, figures, and an engine/settings manifest are
reused only on an exact match. These controls and the invocation path were
removed in 1.0.1. Only passive readers for previously saved Marker structure
and figure artifacts remain.

### PaddleOCR-VL runtime policy

#### Fast (retired extractor; historical design)

Paddle renders one lossless page image per request (150 DPI by default, with
120/180/200-DPI overrides) and loads the Q8 model once per document. Automatic
settings are platform-aware:

- Apple Silicon: two page slots and a 2,048-token vision batch;
- other platforms: one page slot and a 1,024-token vision batch;
- every slot receives 16K context, so total llama.cpp context scales with
  concurrency;
- Flash Attention remains `auto`;
- page output defaults to 4,096 tokens with one targeted retry;
- a length, degeneration, or empty-output failure recursively bisects the page
  along its long axis, with a small overlap, so the element-oriented model sees
  smaller text or figure regions instead of receiving the same failing page
  twice.

Users may override concurrency (1–4), render DPI, vision batch (512–4,096),
Flash Attention, output cap (2,048/4,096/8,192), page retries (0–3),
extraction budget, and cache reuse. A non-`stop` finish reason, empty output, or output
far shorter than the text-layer map triggers a page retry. Degenerate visual
leaves that remain after bounded subdivision are replaced by an explicit
source-page warning; an incomplete text-heavy page still fails closed. This
prevents one pathological plot from discarding an otherwise usable long-paper
extraction without accepting its looping output. Each verified page is written
atomically to a cache keyed by source hash, managed model/runtime identity,
render profile, and effective tuning settings.

#### Full Parser

The Full Parser launches Pipeline's existing Q8 llama.cpp server on an
authenticated loopback port and passes its URL, model alias, and ephemeral API
key to a narrow private Python sidecar. The sidecar constructs `PaddleOCRVL`
with `vl_rec_backend="llama-cpp-server"`, performs layout detection and VLM
recognition, and then calls `restructure_pages(concatenate_pages=False)`.
Neither a system LlamaCPP installation nor an ambient Python environment is
used.

Structure-affecting controls are explicit, validated, and included in the
cache fingerprint:

- layout detection and reading order;
- layout confidence threshold and NMS;
- contained/overlapping box policy (`large`, `small`, or `union`);
- cross-column/staggered layout-block merging;
- OCR within image blocks and Markdown formatting of block content;
- cross-page table merging and title releveling; and
- formula-number retention.

Output is normalized into schema-v2 `paddle_structure.json`. The sidecar
measures recognition completeness before cross-page restructuring can move
blocks. A suspicious page gets the configured number of layout-aware retries;
if those remain incomplete, the same PaddleOCR-VL pipeline performs one
whole-page recognition pass and records that loss of block-level layout as a
quality note. The normalized page retains its pre-restructure character count,
so page number/order, non-empty content, and conservative text-layer
completeness can be checked before the cache becomes active without mistaking
legal cross-page movement for lost text. The sidecar subprocess has cancellation,
wall-clock, output-capture, process-tree, directory-walk, and 250 MB generated
output bounds. Cache entries are immutable fingerprint directories activated
through a small manifest; run creation copies parser images into
`artifacts/figures/` before building the DocumentBundle.

Text-layer completeness counts exclude whitespace. In particular, alignment
spaces emitted for sparse chart layouts must not make a figure-only page appear
to contain several pages' worth of prose. Recovery never changes extractors:
both the layout-aware and whole-page paths use the managed PaddleOCR-VL parser
and its authenticated private recognition server.

## Move 4: UI

- **SettingsPage** — extraction choices are "LLM", "Local engine:
  PaddleOCR-VL Full Parser", and "pdftotext (basic)". Below them, **Local
  Engines** exposes the parser's total size estimate, install progress,
  platform availability, installed version/disk use, cancellation, and
  uninstall. A detected
  app-managed Marker environment left by an older build gets a separate,
  explicit removal control; no removal is automatic.
- **PipelinePage** — the per-profile override uses the same supported labels.
  A loaded legacy Marker value appears as disabled with an actionable
  replacement warning.
- **DepsCheck** — reports Full Parser as one managed dependency and verifies
  both its recognition and layout components; it does not search for Marker
  executables.
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
   fast) and removes model weights only when the last engine goes. This
   Python/uv path was removed in 1.0.1; provisioning now covers only native
   PaddleOCR-VL.*
3. **Engines UI + Marker upgrade** (Moves 3–4; retired in 1.0.1).
   *Implemented 2026-07-03. The Local Engines cards live inside Settings →
   Text Extraction (EnginesPanel.tsx) rather than a separate nav section;
   Marker output went to `~/.pipeline/cache/marker/{hash}/` and figure
   images are copied into the run's artifacts after the run dir exists,
   since extraction runs before the run id is known.*
4. **Managed PaddleOCR-VL Full Parser** — private uv/CPython/Paddle environment,
   official layout client, settings, structured cache, DocumentBundle mapping,
   dependency checks, and UI install card. *Implemented 2026-08-07.*
5. **Later, on demand** — `mineru` registry entry (MLX engine on Apple
   Silicon, richer layout JSON + figure images; AGPL noted in its card), and
   a private ~20-paper eval to settle local-engine ranking if it ever
   matters.

## Non-goals

- No bundling of Python, uv, or model weights in the app installer. Optional
  engines download only after an explicit install action and remain app-owned.
- No CUDA management. (A "CUDA build" opt-in checkbox is possible later;
  CPU/MPS is the supported path.)
- No hosted OCR APIs (Mistral OCR, Datalab, Mathpix) — redundant with
  vision extraction quality and each adds an API-key surface.
- No chasing NVIDIA-only leaderboard toppers (Chandra 2, olmOCR 2).
- No DAG/extractor plugin system — the registry is a static Rust table.

## Compatibility

- Supported `pdf_extractor` values are
  (`llm`/`auto`/`paddleocr-vl-full`/`pdftotext`). The retired
  `paddleocr-vl` value migrates to `paddleocr-vl-full`. Existing profiles and
  export bundles containing `marker` still load for repair, but a document run fails
  before command resolution with an actionable replacement message. `"llm"`
  gets bounded verification; profile `"auto"` inherits the global setting.
- Old saved reports/runs unaffected; new runs gain `pages/` artifacts.
- CI unchanged (no new bundled binaries to sign). Release workflow unchanged.

## Testing

- Pure-function unit tests: page-marker verification, short/missing-page
  detection against a pdftotext baseline, range-split computation, native/uv
  engine artifact checksums, Full Parser platform gating and structure
  verification, DocumentBundle layout/title semantics, and a fail-closed
  legacy-Marker guard.
- Vitest: engines card states (not installed / unavailable / installing with
  progress / installed / failed), Full Parser controls, retired-environment
  removal, and dropdown labels.
- Manual matrix before release: macOS arm64, Windows x64, and Linux x64/arm64;
  install PaddleOCR-VL Full Parser, extract math/table/figure-heavy papers, and
  verify page completeness, title hierarchy, formula numbers, cross-page
  tables, bounding boxes, and figure associations in the explorer. Confirm
  Intel macOS shows Full Parser as unavailable.

## Remaining validation

- Tune the conservative page-length threshold against a larger corpus of
  native-text, scanned, figure-heavy, and appendix-dense economics papers.
- Benchmark Paddle's automatic two-slot/2K-batch Apple Silicon default across
  16 GB, 32 GB, and 64 GB machines; retain the explicit low-memory overrides.
