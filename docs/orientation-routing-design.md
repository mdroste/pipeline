# Orientation and specialist routing redesign

Status: proposal, 2026-10-28. Not implemented. The current contract is
recorded in [auto-review-catalog.md](auto-review-catalog.md) and
[document-bundle-design.md](document-bundle-design.md); this record proposes
changes to both and does not supersede them until it is accepted.

## Summary

Automatic Paper Review currently uses one model call to do two different
jobs: build an exhaustive factual inventory of the paper, and choose the
specialist panel from the complete catalog. The call receives every catalog
entry, about 32K tokens, before the paper text. Because the schema is
serialized with alphabetically sorted keys, the model also writes its
specialist choices before the reasons and context fields that were meant to
precede them.

The proposal splits orientation into two schema-validated calls:

1. **Paper profile.** Read the paper, record its central claims and what
   supports each one, annotate the deterministic document structure, and
   classify the paper coarsely by discipline, method family, and document
   type. No specialist descriptions are supplied.
2. **Panel selection.** From the profile and a deterministic shortlist of
   catalog entries in the chosen groups, assign specialists to specific
   claims. The host derives the existing `review_plan` object from this
   assignment, so materialization, resumption, and re-runs are unchanged.

Separately, the generation-order defect affects every structured schema in
the application, not only orientation, and should be fixed as its own
compatibility change.

## Current design

### Call sequence

[`commands/run.rs`](../gui/src-tauri/src/commands/run.rs) runs, in order:

1. extraction and the deterministic `DocumentBundle` build;
2. one orientation call through
   [`pipeline/orient.rs`](../gui/src-tauri/src/pipeline/orient.rs), retried up
   to twice on decode or validation failure;
3. persistence of `context/orientation.json` and
   `context/orientation_sampling.json`;
4. `auto_review::materialize_config`, which turns the validated
   `review_plan` into concrete specialist steps;
5. `DocumentBundle::enrich_from_orientation`, then persistence of the bundle;
6. the workflow steps, each given the orientation file path.

[`commands/rerun.rs`](../gui/src-tauri/src/commands/rerun.rs) reuses the
parent's `orientation.json` and calls `materialize_config` on it again.

### Prompt and schema

[`prompts/auto_review/orientation.md`](../prompts/auto_review/orientation.md)
contains `{subject_catalog}`, `{method_catalog}`, and `{genre_catalog}`.
`auto_review::expand_orientation_prompt` replaces them with every manifest's
ID, routing description, and exclusion. As of this date:

| Catalog | Entries | Groups | Largest group | Expanded size |
|---|---:|---:|---|---:|
| Subjects | 287 | 29 disciplines | Economics, 29 | ~90K chars, ~22K tokens |
| Methods | 115 | 15 families | Causal inference, 16 | ~36K chars, ~9K tokens |
| Genres | 10 | — | — | ~2.6K chars, ~0.7K tokens |

The genre count includes `genre_slide_deck`, which Slide Review uses. Slide
Review shares this router behind a short preamble
([`orientation_slides.md`](../prompts/auto_review/orientation_slides.md)) and
pins `genre` with an explicit enum.

The schema is the stock paper inventory from
[`orientation_contract.rs`](../gui/src-tauri/src/orientation_contract.rs) plus
a `review_plan` object. `resolve_schema_catalogs` expands the catalog
references into enums of the same size before the provider call. Host
validation then checks note coverage, fallback conflicts, and count bounds.

### Consumers of orientation fields

| Field | Consumer |
|---|---|
| `metadata.paper_type` | `{paper_type}`, step conditions on `/metadata/paper_type`, report header |
| `tables_figures`, `formal_results` | `enrich_from_orientation`, matched by kind and number |
| `review_plan` IDs, `selection_notes`, `genre` | `materialize_config` (run and re-run) |
| whole object | every step, by file path and survey hint; part of the shared context-cache prefix |
| `primary_domain`, `subject`, `paper_forms`, `routing_uncertainty` | no code consumer; the core contribution prompt refers to "the detected subject" |

## Problems

Evidence comes from the code and from the three local Auto Review runs that
have a `review_plan` (August–September 2026). Three runs show behavior, not
rates.

### P1. Fields are generated in alphabetical order

The runtime `serde_json` dependency does not enable `preserve_order`, so
every schema, whether built with `json!` or parsed from a saved profile, is a
sorted map when it is serialized for a provider.
Provider documentation, checked 2026-10-28:

- [Anthropic](https://platform.claude.com/docs/en/build-with-claude/structured-outputs):
  object properties "maintain their defined ordering from your schema", with
  required properties first. Every orientation property is required, so the
  order is the serialized order.
- [OpenAI](https://developers.openai.com/api/docs/guides/structured-outputs):
  "outputs will be produced in the same order as the ordering of keys in the
  schema."
- Gemini: earlier documentation stated that properties are ordered
  alphabetically unless `propertyOrdering` is supplied; the current
  [structured-output page](https://ai.google.dev/gemini-api/docs/structured-output)
  does not address ordering. Because Pipeline's schema is already
  alphabetical, the result is the same either way.

The orientation model therefore writes `review_plan` as `genre`,
`method_specialist_ids`, `paper_forms`, `primary_domain`,
`routing_uncertainty`, `selection_notes`, `subject`, and
`subject_specialist_ids`. The method panel is chosen before the subject is
described, and every reason follows its choice. At the top level, the whole
plan is written before `sections`, `stated_contribution`, and
`tables_figures`.

The same applies to findings. The intended item order in
`findings::output_schema_for_taxonomy` starts with `rank`, `id`, `title`, but
generation begins `category`, `confidence`, `consequence`, `evidence`, so
confidence is assigned before the problem is stated.

This is derived from the code and the provider documentation. Orientation
requests are not journaled with their schema, so it has not been observed in
a recorded request body.

### P2. One call does two incompatible jobs

The inventory asks for exhaustive, neutral extraction: every symbol (up to
200), every formal result (up to 150), "report what is there". Routing asks
for a short judgment about which claims are central. The two share a prompt,
a model and effort setting, and one retry loop. A single invalid selection
note causes the entire inventory to be regenerated; one local orientation
output is 47 KB.

### P3. The catalog precedes the paper and grows with every role

The model reads about 400 entries before it knows the paper's subject, and
then has to find the relevant entries again. Adding roles increases the
prompt for every run, including runs in unrelated fields.

### P4. Seven partially redundant classification fields

`paper_type`, `paper_forms`, `primary_domain`, `subject`, `genre`, subject
IDs, and method IDs overlap. Three have no consumer. If they came first they
would act as useful intermediate reasoning, but P1 places them after the
decisions they were meant to inform.

### P5. Genre affects every step but carries no reason

A non-article genre inserts its context paragraph into every enabled step.
The field has no reason, no evidence, and no confidence. In one local run, a
paper reporting new wealth-inequality estimates was classified
`genre_methods_tool`, so every reviewer was told to judge it "as a method
offered for adoption".

### P6. "Smallest set" is not binding

Two of the three local runs used the maximum of four method specialists. In
all three, `routing_uncertainty` is empty. Nothing in the output structure
attaches a cost to an additional specialist.

### P7. The prompt is written in the system's vocabulary

The model is addressed as a component: "the host application assembles",
"catalog-backed specialist IDs", "panel-size bounds", "reviewers see these
notes as pointers". Editorial judgment and validator rules are mixed
together. Rule 6 is economics-specific and names `formal-proofs`,
`economic-model-logic`, and `conceptual-argument`; the catalog IDs use
underscores.

### P8. Every reviewer sees the whole routing plan

Steps receive the complete `orientation.json`, including the reasons each
other specialist was chosen. This weakens the independence of the panel and
invites reviewers to defer to another seat's remit.

### P9. Retries defeat prompt caching

The retry prompt places the validation error before the base prompt
(`orient.rs`), so the cached prefix changes on every retry.

### P10. The inventory duplicates deterministic extraction

When PaddleOCR-VL Full Parser or LaTeX ingestion has already produced
numbered tables, figures, equations, and sections with pages, the model
re-enumerates them from text, and `enrich_from_orientation` joins the two by
kind and number. When the input exceeds the 250 KB orientation sample, the
instruction to list "every" item cannot be satisfied.

## Goals and non-goals

Goals:

- Routing decisions follow written evidence about the paper's claims.
- Catalog growth does not increase the size of the prompt that reads the paper.
- A routing error can be corrected without regenerating the inventory.
- The model still chooses stable IDs only; prompts, tools, artifact access,
  providers, and model policy remain host-owned.
- `review_plan` keeps its shape, so saved runs remain resumable.
- The routing intermediate is inspectable in the run record.

Non-goals: changing specialist prompts, the consolidation and validation
passes, or Workspace. The findings-order issue in P1 is noted here but should
be handled in its own change.

## Proposed design

### Design rule: alphabetical order equals reasoning order

Until P1 is fixed globally, and on Gemini by default, generation follows
sorted key order. Every new field name below is chosen so that sorted order
is also the intended reasoning order. The design does not depend on the
serialization change.

### Stage 1: paper profile

Input:

- the paper text or deterministic sample, as today;
- a bounded list of the bundle's structural nodes (sections, numbered
  tables, figures, equations, and formal results, with node ID, number, page,
  and caption excerpt);
- a labels-only catalog index: discipline labels, and method-family labels
  with their member labels. Measured size: about 2K tokens for subjects and
  1K for methods, compared with about 32K today. No routing descriptions or
  exclusions are included.

New fields, which sort before the existing inventory fields:

```json
{
  "central_claims": [
    {
      "claim": "Pass-through tax cuts had small real effects in Kansas.",
      "depends_on": ["table:3", "proposition:2"],
      "id": "C1",
      "location": "Section 5, pp. 21-24",
      "support_kind": "causal_empirical",
      "technique": "stacked difference-in-differences across eight reforms"
    }
  ],
  "classification": {
    "disciplines": [{ "claim_ids": ["C1", "C2"], "group_id": "economics" }],
    "document_type": {
      "evidence_quote": "",
      "location": "",
      "type_id": "research_article"
    },
    "method_families": [{ "claim_ids": ["C1"], "family_id": "causal_inference" }],
    "secondary_candidates": ["statistical_inference"]
  }
}
```

- `central_claims`: three to eight claims on which the conclusions depend.
  `support_kind` uses today's `paper_forms` values. `technique` records the
  design or method in the paper's own terms.
- `disciplines`: one to three; `method_families`: one to four. Each must cite
  claim IDs.
- `document_type`: `research_article` unless the title, abstract, or
  introduction presents the manuscript as another type. A non-article type
  requires a quotation and its location; host validation rejects one without
  them.
- `secondary_candidates`: plausible groups that were not chosen. They widen
  the stage-2 shortlist, which limits the cost of a stage-1 omission.

Existing inventory fields keep their names and top-level positions, so
`models::paper_view`, `enrich_from_orientation`, and step conditions are
unchanged. Inventory items gain a `node_id`, empty when no deterministic
node matches. The model annotates supplied nodes and adds only items the
extraction missed. Where the text was sampled, the instruction is "every item
in the supplied text".

The host derives `metadata.paper_type` from the claim support kinds: theory
when every central claim is formal or conceptual, empirical when none is,
mixed otherwise. The field remains in the stored metadata for `{paper_type}`
and conditions.

### Stage 2: panel selection

Input:

- the profile's `central_claims`, `classification`, `stated_contribution`,
  and title;
- the opening of the document (abstract and introduction, about 12 KB) for
  disambiguation;
- a deterministic shortlist: full routing descriptions and exclusions for
  every entry in the chosen and secondary groups, plus each group's fallback.
  Typical size is 20–60 entries, about 1.5K–4K tokens.

The schema's ID enum is built per run from the shortlist, so an ID outside
it cannot be returned. This also keeps enums far from provider limits as the
catalog grows. OpenAI caps a single enum of more than 250 values at 15,000
characters in total; today's subject enum has 287 values and about 9,900
characters.

```json
{
  "seats": [
    {
      "claim_ids": ["C1"],
      "need": "Assess parallel trends and stacking weights for the eight-state design.",
      "role_id": "difference_in_differences"
    }
  ],
  "uncertainty": []
}
```

Host validation, beyond today's checks:

- `claim_ids` is non-empty and refers to stage-1 claims;
- subject seats come first; there are one or two subject seats and one to
  four method seats, or the configured count (`apply_agent_count_instruction`
  moves to this stage);
- the existing fallback-with-own-sibling rule.

The host records `uncovered_claim_ids` (central claims with no seat) as
provenance rather than as a failure. The benchmark below tracks it.

The host then writes the existing `review_plan`:

| `review_plan` field | Derived from |
|---|---|
| `subject_specialist_ids`, `method_specialist_ids` | seat order, split by catalog kind |
| `selection_notes` | `{ id: role_id, reason: need }` |
| `genre` | stage-1 `document_type.type_id` |
| `paper_forms` | distinct claim `support_kind` values |
| `primary_domain`, `subject` | label of the first discipline; first claim's text |
| `routing_uncertainty` | stage-2 `uncertainty` |

### Prompt language

Both prompts address the model as an editor, not as a part of the software.
Rules that the host already enforces appear once, briefly, or not at all; a
violation returns as the retry message. Draft opening for stage 2:

> You are choosing specialist referees for the manuscript profiled below.
> Three general referees already cover contribution and literature, internal
> consistency, and exposition. Choose one subject referee whose field
> knowledge is needed to judge whether the main result is correct and
> important; add a second only when the main claim depends on two fields and
> either referee alone would miss a material problem. Choose method referees
> for the techniques that carry the central claims, not techniques used in
> passing, for robustness, or for background. Most papers need one or two.
> For each referee, name the claims it covers and what checking them
> requires.

Vocabulary to remove from model-facing text: host, routing, catalog,
allowlisted, materialize, panel-size bounds. The safety paragraph remains in
the reviewer contracts, where it already appears.

### What reviewers receive

The shared orientation projection omits `review_plan` and the stage-2 seats.
It stays identical across steps, which keeps the context-cache prefix
([`context_cache.rs`](../gui/src-tauri/src/pipeline/context_cache.rs))
shared. Each specialist's own seat (its `need` plus the text and location of
the claims it covers) extends the existing quoted "Routing Context" section
in its step prompt. That section is already framed as untrusted
model-authored context.

### Retries

Each stage retries independently. The correction note is appended after the
base prompt so the cached prefix survives. A stage-2 failure never re-runs
stage 1.

### Persistence

- `context/orientation.json` remains the merged object: inventory, profile
  fields, and the derived `review_plan`. Older runs remain readable and
  resumable.
- `context/routing_shortlist.json` records the shortlist and its sources,
  like `orientation_sampling.json` records sampling.
- `context/panel_selection.json` records the raw stage-2 output, including
  seats and uncovered claims.
- New manifest fields are optional with Serde defaults.

### Contract selection and migration

- A new contract marker, `auto-review-v3`, selects the two-stage path.
  Profiles carrying `auto-review-v2` keep the single-call path.
- The built-in Paper Review, Paper Review (Fast), and Slide Review profiles
  move to v3 through the existing digest-matched migration pattern in
  [`pipeline_config/migrations.rs`](../gui/src-tauri/src/pipeline_config/migrations.rs),
  with a new marker. A user-customized orientation prompt is left unchanged.
- Slide Review's preamble applies to stage 1, and its genre pin becomes a pin
  on stage 1's `document_type.type_id`. Stage 2 is shared unchanged.
- Whether to retain the v2 path for customized prompts, or to migrate them
  with a notice, is an open question below.
- `CLAUDE.md` currently states "Orient with one required, schema-validated
  model call". Acceptance requires updating it, together with
  [auto-review-catalog.md](auto-review-catalog.md).
- The Workflow Editor's "Orientation & Classification" stage
  (`pipeline-editor/PreprocessingEditors.tsx`) shows the two prompts and
  schemas; the "Auto-filled from orientation" slots are unchanged.
- The launch fingerprint already includes the catalog revision and the saved
  configuration, so new templates change it automatically.

### Estimated cost

| | Today | Proposed |
|---|---|---|
| Catalog text in the paper-reading call | ~32K tokens | ~3K tokens (labels index) |
| Additional call | — | one short call: profile, ~12 KB excerpt, 1.5K–4K-token shortlist |
| Cost of a routing retry | full inventory regenerated | stage 2 only |

Overall input tokens fall. Latency rises by one short sequential call. These
are estimates from the catalog sizes, not measurements.

## Generation-order fix (separate change)

Enabling `serde_json/preserve_order` makes serialized schemas follow
declaration order everywhere. Two of the fingerprint paths already sort keys
explicitly: `stable_snapshot_fingerprint` in
[`commands/run_context.rs`](../gui/src-tauri/src/commands/run_context.rs) and
`canonical_pretty_json` in
[`pipeline_config/workflow.rs`](../gui/src-tauri/src/pipeline_config/workflow.rs).
Before enabling it, audit every other module that computes SHA-256 digests,
which as of this date includes `engines/integrity.rs`, `engines/runtime.rs`,
`workbench/release/exchange.rs`, `workbench/release/retention.rs`,
`workbench/research/papers.rs`, `projects/ledger.rs`, `runs.rs`,
`model_catalog.rs`, `commands/export.rs`, and `orchestration/adapters.rs`. Each must either sort keys or be shown not to
hash a `Value`. Then review findings field order (P1) on its own merits.

## Evaluation before changing the default

No routing benchmark exists; M-14 in
[SOL_AUG23_FULLSEND_FEATURES.md](../notes/SOL_AUG23_FULLSEND_FEATURES.md)
planned one. Before either router becomes the default:

- **Fixtures.** Twenty to twenty-five papers spanning disciplines, paper
  forms, and at least four non-article genres, including long papers that
  trigger sampling. Each fixture lists required, acceptable, and forbidden
  specialist IDs and the expected genre. Labels come from someone with domain
  knowledge, not from either router.
- **Metrics.** Primary-subject accuracy; recall of required methods; rate of
  forbidden selections; panel size; genre false-positive rate; uncovered
  central claims (v3 only); stability across three repeated runs (Jaccard
  overlap of panels); input tokens and wall time.
- **Comparison.** Run v2 and v3 on the same fixtures with the same models.
  v3 replaces v2 when required recall is at least equal and the
  forbidden-selection rate, panel size, and genre false-positive rate are
  lower.
- **Location.** A developer-run harness outside CI, since it needs model
  calls. Fixture papers stay local unless their licenses permit inclusion.

## Rollout

1. **Small fixes in the current design.** Correct rule 6's IDs and remove its
   economics-specific wording; append retry corrections after the base
   prompt. Each stock-prompt edit needs a digest-matched migration for the
   built-in profiles.
2. **Benchmark and v2 baseline.**
3. **Generation-order fix**, with the fingerprint audit above, as its own
   reviewable change.
4. **v3 two-stage router** behind the new contract marker, compared against
   the baseline.
5. **Reviewer projection and node-ID enrichment**, after v3 is the default.

## Alternatives considered

- **Keep one call and fix only field order.** Cheapest, but the catalog size,
  the mixed jobs, and the full-inventory retries remain.
- **A catalog-search tool during orientation.** Tool-use behavior differs by
  provider and transport and is harder to audit, which conflicts with
  deterministic Reviews.
- **Embedding or keyword pre-ranking.** The routing instructions already warn
  against keyword routing. Lexical or embedding scores could at most serve as
  a recall check on stage 1's group choices.
- **Model-written specialist focus.** Breaks the rule that the model chooses
  IDs only and that reviewer prompts are host-owned.
- **Stage 1 choosing specific roles from labels alone.** Labels do not carry
  the exclusions that separate adjacent roles, which are the main defense
  against attractive but irrelevant reviewers.

## Open questions

1. Retain the single-call v2 path for customized orientation prompts, or
   migrate them with a notice?
2. Should stage 2 use the orientation model setting, or a separate, cheaper
   default?
3. Should stage 2 receive Read access to the document instead of a fixed
   excerpt?
4. Should the claims ledger become a bundle-level record that findings can
   reference as locators?
5. Should the stock non-Auto paper orientation adopt the stage-1 profile, so
   every paper workflow gains the claims ledger?
