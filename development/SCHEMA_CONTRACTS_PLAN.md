# Structured-artifact contracts: evaluation and plan

Reviewed at 0.9.4 pre (`be70540`). Scope: the JSON artifact system (`structured.rs`,
terminal artifact contract, merge/schema inheritance), the schemas the default
Automatic Paper Review actually uses (`auto_review::orientation_schema()`,
`findings::output_schema()`, the managed `{content}` envelope), the prompts that
populate them, and the schema workspace UI (`SchemaEditorPanel.tsx`).

## What the current system gets right

Worth stating because the plan below builds on it rather than replacing it:

- The fail-closed portable dialect is the correct call. Rejecting unknown
  keywords instead of ignoring them means the editor never promises an
  unenforced contract.
- Host-owned canonicalization (`structured::canonicalize`) with provider-native
  constraints as an assist, not the authority.
- Merge inherits the producing step's schema, and a failed merge on a
  schema-carrying step falls back to the *first agent's validated output*
  rather than the non-JSON banner concatenation (`executor.rs` ~2297). This
  closed the sharpest edge in the design.
- Catalog references (`x-pipeline-catalog`) keep saved profiles small while
  giving providers real enums; semantic validation layered above shape
  validation (`validate_review_plan`) covers what JSON Schema cannot.
- Response journaling distinguishes schema rejection from unusable content.

The critique is that the arc is incomplete: structure appears only at the last
two steps of Auto Review, several fields in the contracts are dead or
under-typed, host-verifiable invariants are enforced only by prompt text, and
the editor asks economists to hand-write raw JSON Schema.

---

## Findings

### F1. Rendering defect: centrality ordering + category-grouped rendering conflict

`synthesis.md` orders findings **by centrality**, not grouped by category.
`findings::render_markdown` emits a `## <category>` header every time the
category *changes between adjacent findings*. Interleaved categories (the
expected result of centrality ordering) produce duplicated section headers:
`## Correctness … ## Methodological … ## Correctness` again. Either the
renderer should group by category (first-appearance order, preserving
within-category array order) or the prompt should instruct grouped output.
Renderer-side grouping is better: it keeps "array order = importance" as the
single semantic and never depends on model discipline.

### F2. The parallel → synthesis hand-off is unstructured, and it is the lossy joint

Core reviewers and specialists return Markdown through the `{content}`
envelope, in a rigid prompt-enforced comment format (`**#N. …**` +
Severity/In the paper/Problem/Consequence/What would help/Location bullets).
That format *is* a schema, encoded in prose and duplicated with slight drift
across five prompt files. Consequences:

- Locators exist upstream only as free text ("Location: p. 12, Table 3").
  Synthesis must convert prose to typed `evidence` objects *without access to
  the paper* (its context is deliberately reports-only). "Never invent a
  locator" is hardest to obey exactly where the conversion happens. Validation
  repairs some of this, one step later, at full-paper-read cost.
- Severity has three inconsistent treatments: specialists must produce it,
  synthesis is told to drop it, the findings schema half-supports it
  (`priority`, optional, untyped), and the Issues table renders it if present.
- Attribution is lost: after synthesis nothing records which reviewer(s)
  surfaced a finding or how many agreed — exactly the information an editor
  wants and the calibration/ledger features could use.
- The empty-report signal is a magic sentinel string
  (`No material issues identified.`) in five prompts that synthesis must
  recognize as prose.

### F3. Dead and drifting fields in the two Auto contracts

- `source_key` is required-preserved by `validate.md` but **never mentioned in
  `synthesis.md`** — nothing populates it, so the default flow carries a dead
  field whose preservation is nonetheless demanded downstream.
- `review_plan.primary_domain`, `subject`, and the free-string `methods` array
  are populated but consumed by nothing in materialization, rendering, or the
  ledger. They cost tokens and routing attention on every run. (The
  `routing_uncertainty` field is at least displayed with the survey.)
- `validate.md` lists `priority` among fields to preserve; synthesis is told
  not to produce it.

### F4. Under-typed findings schema

- `category` is a free string while the prompt fixes exactly four values. A
  typo silently creates a fifth report section and fragments the Projects
  ledger across runs. The dialect supports `enum`; use it.
- `priority`, if kept, should be `enum: ["high","medium","low"]`
  (`normalize_priority` exists only because the field is untyped).
- Evidence property schemas carry **no descriptions**. The per-key guidance
  ("relative to the selected source root", "never invent a locator") lives
  only in the synthesis prompt, so any *other* profile that adopts the
  findings schema gets no guidance at all. Descriptions are projected to
  providers — move the guidance into the schema, shrink the prompt.
- Evidence arrays have no `maxItems` (Rust truncates silently at 50);
  `MAX_EVIDENCE` should appear in the schema so the constraint is provider-
  visible and the truncation ceases to be silent behavior.

### F5. Host-verifiable invariants enforced only by prompt text

`validate.md` demands: same order, exact input `id`s, no new findings. All
three are cheap deterministic set/sequence checks against the synthesis
artifact the host already holds — but nothing verifies them, so a
non-compliant validator silently corrupts id lineage (which the ledger and
annotations depend on). These belong in the step-retry loop exactly like
schema violations.

### F6. Strict-mode asymmetry across providers

The orientation contracts deliberately keep every property required
("empty = unavailable") so strict constrained decoding works everywhere. The
findings schema does not (`source_key`/`priority` optional, evidence
all-optional) — so on OpenAI it drops to non-strict `json_schema` mode
(`api_openai.rs::openai_strict_schema` returns `None`). Neither the editor
nor the docs surface which mode a schema will get.

### F7. The editor's starter template steers users to the legacy shape

`ISSUES_SCHEMA` ("Use issues schema") uses `issues`/`severity`/`section` and
an evidence object missing `source_path`/`line_start`/`line_end`/
`source_hash`. The adapters normalize it, but new workflows get steered to
the legacy dialect while Auto uses the canonical one, and source-tree
evidence locators are simply unavailable from the template.

### F8. The schema workspace is a raw-JSON cliff

For the stated audience, a 340-line orientation contract shown as a JSON
blob in a textarea is read-only in practice. Specific gaps:

- No structural (tree/form) rendering of any schema.
- The provider-projection preview ("View resolved provider schema") exists
  only for the catalog-backed Auto orientation target; for ordinary schemas
  users never see what providers actually receive (`uniqueItems` stripped,
  `x-` keys removed), nor the 20 KiB projected-size budget.
- The dialect's fail-closed errors name what is *not* supported but the
  editor never lists what *is*.
- Semantic constraints that live in Rust (selection-notes coverage, agent-
  count bounds, findings-step-requires-schema) are invisible until a failed
  save or run.
- No cross-step interface checking: `run_if.survey_path` pointers,
  `output_matches` step references, and `{step:id}` prompt tokens are never
  linted against the referenced step's *declared schema*, although the
  editor holds both sides. This is the headline capability a typed artifact
  system enables and does not yet deliver.

### F9. Schemas are frozen copies inside saved profiles

`findings::output_schema()` is embedded byte-for-byte in every saved Auto
profile. Any future change to the canonical findings shape needs a profile
migration (the Auto contract explicitly reserves the right to break, but the
mechanism generalizes badly to user profiles that adopted the schema). The
catalog-reference pattern already solves this class of problem: a root
marker the host resolves to the live contract at dispatch.

### F10. Orientation call size risk

One constrained response must carry the full inventory (every formal result,
every symbol, every table) *plus* routing. For long papers the inventory can
dominate the output budget; a truncated response burns one of only two
retries, and orientation failure is terminal. `formal_results` and
`notation` have no `maxItems` bounds.

---

## Plan

Phases are ordered by value-per-risk. Phase 1 is contract tightening with no
architectural change; Phase 2 is the one real design change; Phases 3–4 are
editor and engine capability work that pays off for non-Auto workflows.

### Phase 1 — Tighten the existing contracts (small, safe, do first)

1. **Fix category-grouped rendering** (`findings::render_markdown`): group by
   category in order of first appearance, preserving within-category array
   order. Prompt unchanged. *(F1)*
2. **Type the findings schema** (`findings::output_schema()`):
   - `category`: `enum` of the four canonical categories. The four strings
     live today only in `synthesis.md`; make the schema the source of truth
     and have the prompt reference them (`the categories defined in the
     findings schema`), or generate the prompt list from the same constant.
   - Decide severity once. Recommended: **drop severity from the core/
     specialist Output format entirely** (order already conveys importance,
     and severity is currently produced then discarded), and remove
     `priority` from the Auto prompts' vocabulary. Keep `priority` in the
     schema as an optional enum for non-Auto profiles that want it.
   - Add `description` to every evidence property (locator semantics,
     "relative to the selected source root", page = rendered-PDF page).
     Shrink the matching prose in `synthesis.md` to the behavioral rule
     ("never invent a locator; prefer quote+page").
   - `evidence`: `maxItems: 50` to match `MAX_EVIDENCE`.
   - `findings.items.title`/`id`: add `minLength: 1` once the dialect
     supports it (see Phase 4.4); until then unchanged.
   *(F4)*
3. **Make attribution real.** Add an optional `sources` array
   (`items: {type: "string"}`, `maxItems: 8`) to the findings schema —
   contributing reviewer step ids — and instruct synthesis to populate it
   from the report headers it already receives (`## <step label>` /
   selectors). Have validate preserve it. Render it subtly (e.g. in the
   Issues table detail row), feed it to the Projects ledger. Retire the
   never-populated `source_key` from the Auto prompts (keep it in the schema
   for legacy adapters). Alternatively repurpose `source_key` itself as a
   comma-joined list — but a typed array is cleaner and the contract is
   explicitly pre-release. *(F2-attribution, F3)*
4. **Enforce validate's invariants in Rust.** After `auto_validate` (any
   findings-schema step consuming another findings-schema step declared via
   `outputs.findings_step` lineage — in practice: compare against the
   selected upstream artifact when both carry the findings contract):
   ids ⊆ input ids, no duplicates, relative order preserved. Violation =
   schema-rejection path: journal as rejected, retry, then fail the step.
   *(F5)*
5. **Prompt drift cleanup:** remove `priority`/`source_key` from
   `validate.md`'s preserve-list (per the decisions above); ensure both
   prompts and the schema state the same field set. Delete `synthesis_v1.md`
   or move it under a clearly-archival name. *(F3)*
6. **Trim `review_plan`:** drop `primary_domain`, `subject`, and `methods`
   from the schema and prompt (nothing consumes them), or give them a
   consumer (run header + ledger metadata) in the same change. Fewer routing
   tokens, less attention spent on dead output. *(F3)*
7. **Bound the inventory:** `maxItems` on `formal_results` (~150),
   `notation` (~200), `sections`, `tables_figures` (~100), plus prompt
   language for prioritizing when a paper exceeds them ("keep the entries
   most relevant to the main claims; note the overflow in
   `extraction_quality_notes`"). *(F10)*
8. **Replace the editor's `ISSUES_SCHEMA` starter** with the canonical
   findings schema (server-provided via a small command so it can't drift
   from `findings::output_schema()`), labeled "Findings schema (canonical)".
   Keep the legacy issues shape available but labeled legacy. *(F7)*

### Phase 2 — Structure at the source: specialist findings

The one architectural change. Core reviewers and specialists return a small
schema instead of formatted Markdown:

```json
{
  "type": "object",
  "required": ["findings"],
  "properties": {
    "findings": {
      "type": "array",
      "maxItems": 7,
      "items": {
        "type": "object",
        "required": ["title", "in_the_paper", "problem", "consequence",
                     "what_would_help", "evidence"],
        "properties": {
          "title":          {"type": "string"},
          "in_the_paper":   {"type": "string", "description": "Quote or close paraphrase of the claim and the evidence it relies on."},
          "problem":        {"type": "string", "description": "The field-specific analysis, in Markdown."},
          "consequence":    {"type": "string"},
          "what_would_help": {"type": "string"},
          "evidence":       { /* same evidence item schema as findings */ }
        }
      }
    }
  }
}
```

The item fields are exactly today's bullet structure, so review quality
guidance carries over verbatim; the model still writes prose, just in typed
fields. What this buys:

- **Typed locators at the source.** The specialist has the page open (visual
  assets, structure index with `node_id`s) when it writes the comment; the
  evidence object is captured there instead of re-derived from prose by a
  synthesis step that cannot see the paper. Validation's repair burden drops.
- **`No material issues identified.` sentinel → empty array**, checked by
  schema, across all five prompts. Synthesis's "remove empty reports"
  instruction becomes host behavior (skip empty artifacts when building
  synthesis context).
- **Multi-agent specialist merges become schema merges** (merge already
  inherits producing schemas), with the first-agent fallback instead of a
  prose banner.
- **Synthesis becomes dedup/classify over typed items** — its prompt sheds
  the format-parsing burden, and `sources` attribution is mechanical.
- **Per-specialist calibration**: `draft_calibration` and the ledger can
  attribute accepted/rejected findings to the specific catalog role that
  produced them, which is the feedback loop the catalog needs.

Costs and mitigations:

- *Readability of intermediate artifacts.* Keep the current readable report:
  render specialist JSON to exactly today's Markdown deterministically (the
  findings renderer pattern). The workspace's Provenance/step views show the
  rendered form by default, raw JSON on the existing output toggle.
- *Constrained decoding tax on review quality.* Real but bounded: the fields
  are free prose; the constraint is only the envelope, one level deeper than
  today's `{content}` envelope. Worth an A/B on 2–3 papers before commit
  (run Full twice, compare finding quality/count by hand).
- *Synthesis context format.* `{prior_outputs}` for JSON upstream steps
  currently inlines raw JSON. Either keep that (models read JSON fine) or
  render-to-Markdown when building sequential context and pass JSON only via
  explicit `{step:id}`. Recommend: inline a host-rendered compact form with
  ids, so synthesis references stable item ids (`auto_exposition/3`) in
  `sources`.
- The Auto contract is explicitly pre-release with no migration path — this
  is the window to do it. Bump `AUTO_REVIEW_CONTRACT` to `auto-review-v2`.

### Phase 3 — Schema workspace and viewer

1. **Structural schema view (read-first).** Render any portable schema as an
   indented property tree: name, type badge, required marker, description,
   enum values, array bounds. Default view for stock/managed contracts;
   "Edit as JSON" switches to today's textarea. The dialect is small enough
   that this is a single recursive component, not a schema-editor product.
2. **Provider projection everywhere.** Extend "View resolved provider
   schema" to all schema targets (function exists — `provider_schema` behind
   a command), and add a projected-size meter against the 20 KiB budget plus
   a strict-capability badge per provider ("strict on OpenAI/Anthropic/
   Gemini" vs "advisory + host-validated") driven by the same rule
   `openai_strict_schema` uses. *(F6, F8)*
3. **Example-instance preview.** Deterministically generate a minimal valid
   instance from any portable schema (all types are simple; enums pick the
   first value) and show it beside the editor and in RunPreview — "this is
   the JSON the step must return."
4. **Dialect reference in-place.** A collapsible "Supported keywords" panel
   sourced from the same constant list as `SUPPORTED_KEYWORDS`, so
   fail-closed errors have an answer one click away. Surface the semantic
   rules Rust enforces for the Auto contract (selection-note coverage,
   agent-count bounds) as static text under the orientation schema.
5. **Cross-step interface lint.** In the editor (and `workflow validate`):
   - `run_if.survey_path` pointer must resolve within the declared
     orientation schema (when one exists) — warn otherwise.
   - `run_if.output_matches` against a schema-carrying step gets a note that
     it matches canonical JSON text (easy to misuse).
   - `{step:id}` tokens in prompts checked against existing enabled steps
     (the executor already resolves them leniently at run time; the editor
     should catch typos at save time).
6. **Schema-aware JSON artifact rendering.** Generic renderer for
   schema-carrying step artifacts in ReportWorkspace/ArtifactExplorer:
   array-of-objects → table (columns from `properties`, headers from
   `title`), object → key/value list, descriptions as tooltips. This is what
   makes *custom* JSON workflows presentable without bespoke frontend work,
   and it reuses the schema the run manifest already stores.

### Phase 4 — Engine capabilities the JSON system now makes natural

1. **JSON-pointer step substitution:** `{step:<id>#/json/pointer}` resolving
   into the canonical artifact (mirror of `survey_path` semantics; same
   RFC 6901 code). Lets a sequential step consume `#/findings` or one
   table from a big extraction artifact instead of the whole blob.
   Escaping/limits follow the existing single-pass substitution rules.
2. **Artifact fan-out:** extend `for_each` with an artifact source —
   `{ "artifact": {"step": "auto_synthesis", "pointer": "/findings"}, "max": 40 }`
   — binding `{item}` to each array element (canonical JSON). Unit keys
   `step_id/<index>`; existing merge/`{step:id}` machinery applies; budget
   validation (`validate_run_budget`) already counts fan-out.
   - Flagship use: **per-finding validation.** Today `auto_validate` verifies
     up to 40 findings in one pass — attention dilution on exactly the step
     whose diligence matters most, and its instruction list (read whole
     paper, check 7 false-positive classes, per-finding verdicts) is the
     longest in the suite. A fan-out validator gets one finding, clean
     context, targeted reads, returns `keep/repair/drop + repaired finding`;
     the host reassembles by id deterministically (no LLM merge). Shared-
     context caching keeps the paper prefix warm across units. Offer it as a
     profile variant first (cost is materially higher; Quick keeps the
     single-pass validator).
3. **Live contract references for host-owned schemas:** a root
   `x-pipeline-schema: "findings-v1"` marker the host resolves to
   `findings::output_schema()` at dispatch (exact catalog-reference
   pattern: saved profiles stay compact and current, editor shows the
   resolved form read-only with an "unlink to customize" action). Apply to
   the Auto profiles' two findings steps. *(F9)*
4. **Dialect: add `minLength`** (and only that, for now) as a host-checked
   keyword projected to providers that support it — precedent is
   `uniqueItems`, already host-owned. Motivation: non-empty `id`/`title`
   without prose rules. Resist adding more keywords until a concrete
   contract needs them; the dialect's smallness is a feature.
5. **Deferred/rejected for now:** a named schema library shared across steps
   (wait for real demand; pointer-substitution covers most reuse),
   `anyOf`/conditional schemas (breaks strict-mode portability), and
   schema-checked *inputs* (a step declaring the shape it expects — the
   editor lint in 3.5 delivers most of the value without new config).

### Sequencing

Phase 1 items are independent and individually shippable; do 1.1–1.5 in one
change (they touch the same three files: `findings.rs`, `synthesis.md`,
`validate.md`) and 1.6–1.8 in a second. Phase 2 gates on a quality A/B and
bumps the contract id; it supersedes parts of 1.2 (severity removal happens
in the specialist schema instead of prose format) so decide the A/B before
polishing specialist prompt formats further. Phase 3.1–3.3 are frontend-only
and parallel to everything. Phase 4.2 (per-finding validation) is the
highest-value engine item and depends only on Phase 1's id-invariant
enforcement for safe reassembly.
