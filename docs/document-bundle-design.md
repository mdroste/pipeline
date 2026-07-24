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
| `artifacts/pages/*` | 150-DPI page renders, capped at 300 pages |
| `artifacts/figures/*` | Figures emitted by PDF extraction |
| `artifacts/document/figures/*` | Original TeX/DOCX figure media |

Re-runs preserve the parent's canonical bundle and copy its visual assets into
the new run. Runs created before bundle v1 remain resumable: Pipeline builds a
compatibility bundle from their cached extraction.

## Source adapters

### PDF

- Text comes from the configured verified LLM, PaddleOCR-VL, marker, or
  pdftotext extractor.
- Page markers are retained when available.
- Every page is rendered independently of the text extraction method.
- Marker images are retained as figure assets when marker emits them.
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

Primary document access is a pipeline capability, not a profile option.
Every step receives:

- the readable `document.md` path as its normal input;
- the canonical bundle path through `{document_bundle}`;
- an artifact-root hint for resolving each asset's `rel_path`;
- `Read` plus `ReadDocumentAsset`, regardless of the profile's optional tool
  list.

For direct APIs, `ReadDocumentAsset` validates the requested path against the
run's allowed roots and returns a real multimodal image block:

- Anthropic: base64 image source;
- OpenAI: `image_url` data URL in a user content part after the tool result;
- Google: `inlineData` after the function response.

Claude Code maps the capability to its native multimodal `Read` tool. Codex
and Gemini CLI retain their native workspace read behavior. Text, image, PDF,
call-count, and cumulative byte limits remain enforced.

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
- Existing prompt tokens keep their meaning. `{document_bundle}` is additive.
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
