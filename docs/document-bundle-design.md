# DocumentBundle architecture

Status: implemented schema v1.0, 2026-07-23.

## Decision

Pipeline's durable document interchange format is a versioned, source-neutral
`DocumentBundle`, not Markdown. Markdown remains a compatibility and
human/LLM-readable projection.

Economics papers are compound documents. Text order, mathematical source,
table structure, page geometry, and visual evidence are distinct kinds of
information. Flattening all of them into one string loses information that
cannot be reconstructed reliably downstream. Conversely, passing a raw PDF or
Word file to every model makes behavior provider-dependent and difficult to
audit.

The bundle therefore separates:

- `origins`: primary source and optional visual companion (for example,
  LaTeX plus its compiled PDF);
- `pages`: page identity and rendered-page asset links;
- `nodes`: ordered semantic blocks such as page text, sections, equations,
  tables, figures, and formal results;
- `assets`: immutable run-relative visual files with media type, page, and
  dimensions when available;
- `representations`: parallel lossless or useful forms such as LaTeX, OMML,
  a table grid, or an orientation summary;
- `provenance`: origin, extraction method, and confidence;
- `quality`: explicit extraction warnings.

The Rust definition in `gui/src-tauri/src/document_bundle.rs` is authoritative.
Every bundle carries `schema_version: "1.0"` and validates IDs, references,
and artifact paths before it is persisted or reused.

## Run artifacts

Each new document run records:

| Artifact | Purpose |
|---|---|
| `context/document_bundle.json` | Canonical machine-readable bundle |
| `context/document.md` | Full readable document with stable node markers |
| `context/blocks.jsonl` | One semantic node per line for streaming/indexing |
| `context/extracted_text.md` | Legacy extraction view for compatibility |
| `artifacts/pages/*` | Compact 120-DPI JPEG page renders, capped at 300 pages |
| `artifacts/figures/*` | Figures emitted by PDF extraction |
| `artifacts/document/figures/*` | Original TeX/DOCX figure media |

Completed runs keep the page files for agent inspection and re-runs but replace
their individual manifest entries with one compact page index. The artifact
viewer renders one direct page-number navigator and reads a page only after
selection. Opening the Sources tab loads the manifest alone; no
artifact body is fetched until the reader chooses one. Interrupted and older
manifests retain ordinary page entries and remain compatible.

Re-runs preserve the parent's canonical bundle and copy its visual assets into
the new run. Runs created before bundle v1 remain resumable: Pipeline builds a
compatibility bundle from their cached extraction.

## Source adapters

### PDF

- Text comes from the configured verified LLM, PaddleOCR-VL Full Parser, or
  pdftotext extractor. Marker and PaddleOCR-VL Fast retain passive decoding
  only for historical run artifacts.
- Page markers are retained when available.
- Every page is rendered independently of the text extraction method.
- Full Parser preserves official layout labels, reading order, boxes/polygons,
  layout confidence, parser settings, title-parent relationships, equations,
  tables/captions, and extracted image associations in `paddle_block`
  representations. Its cross-page table and title reconstruction runs before
  bundle normalization.
- Historical Marker images already stored in runs remain figure assets.
- Figure/table nodes that lack a dedicated crop fall back to their rendered
  page asset. This is explicit in the asset link rather than silently
  pretending that a crop exists.

### LaTeX

- Recursive `\input` and `\include` resolution remains the text source.
- Sections, display-math environments, figure/table environments, captions,
  labels, and `\includegraphics` references become semantic nodes.
- Referenced image files are copied into the run with traversal checks.
- A same-stem or conventional compiled PDF (`paper.pdf`, `main.pdf`, or
  `manuscript.pdf`) is treated as a visual companion and rendered into pages.
  LaTeX remains the semantic primary source.

### DOCX

- `.docx` is accepted by the picker, batch/watch scanners, and CLI scanner.
- `word/document.xml` is read from the ZIP container under XML and archive
  size caps.
- Paragraphs/headings become Markdown; Word tables retain a grid
  representation; Word equations retain their raw OMML representation.
- `word/media/*` images are copied into the run under file-count, per-file,
  and cumulative byte limits.
- Legacy binary `.doc` is intentionally unsupported. It requires an external
  conversion process and does not provide the safety or fidelity guarantees
  of OOXML.

## Enrichment boundary

Deterministic adapters build the first bundle. The orientation call may enrich
it with paper-specific labels, page references, captions, and summaries. It
must not replace source representations or invent visual assets. Matching is
currently by normalized kind and document number, with page-asset fallback.

This boundary is important:

- deterministic extraction establishes evidence;
- the LLM adds semantic interpretation;
- provenance makes the distinction visible to downstream consumers.

## Model access

Primary document access is selected per step. A profile may expose any subset
of four independently useful representations:

- `primary.text`: a private staged copy of readable `document.md`;
- `primary.structure`: a private staged semantic index through
  `{document_bundle}`. The index retains equations, results, captions, tables,
  figures, assets, provenance, and their useful representations, but omits
  page-text, paragraph-block, and footer nodes already duplicated in
  `document.md`;
- `primary.visuals`: the run's page, figure, and document-media roots plus the
  artifact-root hint needed to resolve bundle `rel_path` values;
- `primary.source`: the original file or exact source folder selected by the
  user.

The canonical `context/document_bundle.json` remains unchanged for persistence,
the Artifact Explorer, and re-runs. The resolver constructs the smaller index
only for model access, and prompts tell agents to consult it selectively rather
than return the whole JSON through a text tool. The resolver constructs a
private artifact view for every call. Unselected
representations are absent from its prompt placeholders, generated artifact
manifest, filesystem read roots, and shared-context cache. `Read` is derived
from the resulting view, while `ReadDocumentAsset` is derived specifically from
`primary.visuals`; neither is a profile-level tool permission. On direct API
transports those logical capabilities expose both the compatible single-item
tools and bounded batch variants:

- `Read` and `ReadTextBatch` for UTF-8 text, with batch requests selecting
  1-based line ranges or byte offsets;
- `ReadDocumentAsset` and `ReadDocumentAssetsBatch` for real multimodal image
  content.

Exact duplicates are returned once, aggregate responses stay under independent
text/image budgets, and a truncated or failed item can be retried with the
single-item tool. Batch names are an API implementation detail, not profile
permissions.

For direct APIs, the asset readers validate every requested path against the
run's allowed roots and return provider-native multimodal image blocks:

- Anthropic: base64 image source;
- OpenAI: `image_url` data URL in a user content part after the tool result;
- Google: `inlineData` after the function response.

Claude Code maps the capability to its native multimodal `Read` tool. Codex
and Gemini CLI receive the same selected roots through their native workspace
controls. Prompts tell CLI agents to group independent bounded reads and image
inspections into one tool turn when supported. All transports must fall back
to sequential reads for missing, truncated, or failed items; batching never
relaxes an evidence requirement. Text, image, PDF, call-count, and cumulative
byte limits remain enforced.

## Visual QA

The Artifact Explorer exposes Document, Pages, and Figures as first-class
groups. Selecting `document_bundle.json` opens a schema-aware inspection view
with:

- page/node/figure/table/equation/asset counts;
- source and extraction provenance;
- quality warnings;
- every visual asset, with page and dimensions when known;
- filterable semantic nodes and their alternate representations;
- direct navigation from a node or asset card to the rendered image.

The readable Markdown artifact remains the best way to inspect the full text;
the bundle view is the best way to inspect structure, provenance, and
text-to-visual links.

## Compatibility and evolution

- `ExtractionResult` is retained as the adapter boundary and old-profile
  compatibility view.
- Existing prompt tokens keep their meaning. `{document_bundle}` points to the
  compact structural index for model calls; the canonical bundle remains a run
  artifact.
- Existing runs without a bundle still open and resume.
- New optional fields can be added within schema 1.x using Serde defaults.
- A breaking semantic change requires a new major schema and an explicit
  migrator. Consumers must reject unknown major versions rather than guess.
- Asset paths are always run-relative. Absolute source paths belong only in
  origin/provenance records.

## Next improvements

The v1 implementation deliberately establishes the contract before adding
more extractors. The next work should preserve that contract:

1. Add page geometry and bounding boxes, then crop figure/table regions from
   the rendered page when a source-native asset is unavailable.
2. Add relationship-aware DOCX parsing (`document.xml.rels`) so captions and
   drawing IDs link to exact media rather than relying on document order.
3. Retain TeX include-file/line provenance during recursive expansion.
4. Add a table normalizer that emits typed cells, spans, footnotes, and a
   CSV/Arrow projection without discarding the original TeX/OOXML.
5. Add extractor conformance fixtures: native PDF, scanned PDF, TeX with
   companion PDF, DOCX with OMML, rotated pages, multi-panel figures, and
   appendix numbering.
6. Add measured quality metrics (page coverage, unmatched captions, orphan
   assets, OCR confidence) and surface them in the same viewer.

The core rule is that future extraction engines produce or enrich the same
bundle. Downstream review steps and the Artifact Explorer must not acquire
engine-specific branches.
