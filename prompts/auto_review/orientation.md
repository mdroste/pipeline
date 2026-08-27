Build a structured orientation map and a bounded review plan for this academic paper. Multiple independent reviewers will use this object. Your routing choices determine which specialist reviewers the host application assembles for this run, so classify the paper from its actual claims, evidence, and methods rather than keywords, author affiliations, or departmental labels.

Populate the supplied orientation schema from the paper itself. The schema defines the inventory fields, routing fields, allowed paper forms, catalog-backed specialist IDs, document genres, and panel-size bounds.

SUBJECT SPECIALIST CATALOG

The catalog is grouped by discipline. Within each group, the broad discipline entry is a fallback; prefer one more-specific subfield entry whenever it fits the paper's central contribution.

{subject_catalog}

METHOD SPECIALIST CATALOG

The catalog is grouped into method families. Within each family, a role marked "family fallback" is that family's broad entry; prefer a more specific sibling whenever it covers the central approach.

{method_catalog}

DOCUMENT GENRE CATALOG

`genre` classifies the manuscript as the kind of document it presents itself as. It is not a reviewer selection: the host hands the matching genre context to every reviewer. Most papers are ordinary original research articles — classify them as `research_article`.

{genre_catalog}

Routing rules:

1. Select one primary subject specialist. Select a second subject specialist only when the central contribution genuinely crosses two bodies of subject-matter expertise and one reviewer would predictably miss a material issue. The array is ordered: primary first, secondary second.
2. Prefer the most specific fitting role. Use a discipline or method-family fallback only when no sibling fits or the central contribution genuinely spans several siblings. Never select a fallback alongside a narrower sibling merely to duplicate coverage.
3. Select between one and four method specialists, limited to methods that materially support or test central claims. Subject and method roles are complementary: subject expertise does not replace review of the proof, design, data, experiment, or computation carrying the result.
4. Route causal claims by their actual design. Descriptive evidence is not causal; randomized interventions, structural counterfactuals, and observational causal designs require their matching roles. Do not assign empirical roles to pure theory or mathematics unless the paper contains that component.
5. Match specialists to the evidence actually used, not to a familiar instrument or keyword. Apply each catalog entry's inclusion and exclusion boundaries, especially where adjacent roles distinguish data collection, measurement, analysis, and substantive contribution.
6. Use formal-proofs when validity depends on proofs or nontrivial derivations; economic-model-logic for incentives, equilibrium, mechanisms, incidence, welfare, or comparative statics; and conceptual-argument for conceptual, synthetic, or normative reasoning. Select more than one only when each is central.
7. Classify `genre` by what the manuscript is, not by what it cites. Use `research_article` unless a catalog genre genuinely describes the document; record the paper's own framing separately in `paper_forms`.
8. For sensitive or dual-use work, classify at a high level from the stated discipline and evidence. Do not reproduce, extend, or infer operationally harmful procedures in routing notes.
9. If classification is genuinely ambiguous, record the ambiguity and choose the smallest set covering it. `selection_notes` must contain exactly one paper-specific reason for every selected subject and method ID, and no unselected IDs; reviewers see these notes as pointers, not established facts.

Inventory rules:

- List every proposition, theorem, lemma, corollary, definition, and explicit assumption in `formal_results`.
- List every explicitly defined symbol in `notation`; do not invent definitions.
- Record the paper's own contribution claim in `stated_contribution`.
- Flag garbled or incomplete extraction rather than treating it as an error in the paper.
- If the paper contains more entries than an inventory array's schema bound allows, keep the entries most relevant to the main claims and record the overflow in `extraction_quality_notes`.

<paper>
{paper_text}
</paper>
