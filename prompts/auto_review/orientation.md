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
2. Prefer the most specific fitting subfield. Use a discipline-level fallback only when no listed subfield fits. Never select both a discipline fallback and one of its own subfields. Some subfield entries are themselves marked or described as cross-field or general fallbacks — for example the economics General roles, Machine Learning & AI (General) in computer science, and Astrophysics & Cosmology (General) in physics: do not select one alongside a narrower sibling that already covers the contribution.
3. Select between one and four method specialists. Select a method only when it materially supports or tests a central claim; a technique mentioned in passing is not enough.
4. Subject and method roles are complementary. A mathematics subject reviewer does not replace formal-proof review when proofs carry the result; a clinical subject reviewer does not replace clinical-study review when patient evidence carries the conclusion.
5. Within a method family, prefer the most specific fitting role. Select the family-fallback role only for an approach its specific siblings do not list, for a contribution that genuinely spans several of them, or when a separate general issue in that family is central beyond a selected specific sibling. Never select a family fallback merely to duplicate a narrower reviewer.
6. Pure theory and pure mathematics papers must not receive causal, randomized, clinical, measurement, survey, qualitative, archival, or laboratory reviewers unless the paper truly contains that component.
7. Descriptive empirical work without a causal claim should generally receive measurement/data or a fitting statistical role, not causal review. Randomized interventions need randomized-experiment review; structural counterfactuals need structural-estimation review; observational causal work routes to the family role matching its identifying design.
8. Distinguish physical laboratory work, field experiments, observational instrumentation, scientific imaging, genomic or omics assays, sensor signals, materials characterization, observational epidemiology, clinical patient research, surveys, archival sources, and computational simulation. Select the roles matching the evidence actually used; do not route every paper using a familiar instrument to a specialized reviewer.
9. Formal-proofs is for results whose validity depends on proofs or nontrivial derivations. Economic-model-logic is for incentives, equilibrium, mechanisms, incidence, welfare, or comparative statics. Conceptual-argument is for conceptual, synthetic, or normative reasoning. These roles may coexist when each is central.
10. Use network-analysis only when relational construction or network dependence is substantively important. Use computational-text-analysis when automated text measurement supports a substantive conclusion, not when the paper contributes an NLP system. Use bibliometric-scientometric only for claims about publication, citation, collaboration, patent, or knowledge-system records. Use administrative-data/record-linkage when database coverage, entity resolution, linkage, or changing definitions materially determine the evidence, not for every administrative dataset.
11. Use systematic-review/meta-analysis only when evidence synthesis itself is a central method. Use reproducibility/software when central conclusions depend on a nontrivial computational implementation, pipeline, or released research artifact.
12. Use design-based/practice research when iterative functional, educational, or service design is the evidentiary strategy. Use creative/practice-led research when artistic or performative practice generates the scholarly claim. Use participatory/community research when shared authority or co-production is methodologically material. Use research-ethics/governance only when consent, welfare, data authority, conflicts, responsible release, or dual-use risk materially affects validity or permissible scope; routine approval alone is not enough.
13. Classify `genre` by what the manuscript is, not by what it cites: use a catalog genre only when the document genuinely is that kind of document — a replication, a comment or reply, a survey or review article, a data descriptor, a methods or tool paper, a registered report, a null-result paper, a case report, or a software paper — and `research_article` otherwise. When in doubt, prefer `research_article`; record the paper's own framing in `paper_forms`.
14. The catalog contains academic-review roles only. Classify sensitive or dual-use work at a high level from the paper's stated discipline and evidence. Do not reproduce, extend, or infer operationally harmful procedures in the routing notes.
15. If classification is genuinely ambiguous, record the ambiguity. Do not compensate by indiscriminately selecting reviewers; choose the smallest set that covers the material uncertainty.
16. `selection_notes` must contain exactly one entry for every selected subject and method ID, and no unselected IDs. Give paper-specific reasons rather than restating catalog labels; each reason is shown to its reviewer as the starting point for their pass.

Inventory rules:

- List every proposition, theorem, lemma, corollary, definition, and explicit assumption in `formal_results`.
- List every explicitly defined symbol in `notation`; do not invent definitions.
- Record the paper's own contribution claim in `stated_contribution`.
- Flag garbled or incomplete extraction rather than treating it as an error in the paper.

<paper>
{paper_text}
</paper>
