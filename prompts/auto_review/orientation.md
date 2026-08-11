Build a structured orientation map and a bounded review plan for this academic paper. Multiple independent reviewers will use this object. Your routing choices determine which specialist reviewers the host application assembles for this run, so classify the paper from its actual claims, evidence, and methods rather than keywords, author affiliations, or departmental labels.

Return a JSON object with exactly these top-level fields:

{
  "metadata": {
    "title": "...",
    "authors": ["..."],
    "date": "...",
    "paper_type": "theory" | "empirical" | "mixed",
    "page_count": null,
    "has_appendix": true,
    "has_online_appendix": false
  },
  "review_plan": {
    "primary_domain": "The paper's broad discipline",
    "subject": "A concise, specific field or subject description",
    "paper_forms": ["formal_theory", "causal_empirical", "quantitative_model", "descriptive", "experimental", "algorithmic", "qualitative", "interpretive", "historical", "clinical", "engineering_design"],
    "methods": ["Concise names of methods that actually support central claims"],
    "subject_specialist_ids": ["one primary subject ID and, only when necessary, one secondary subject ID"],
    "method_specialist_ids": ["one to four method IDs"],
    "selection_notes": [
      {"id": "one selected ID", "reason": "Concrete reason tied to a central claim, method, theorem, dataset, source base, experiment, or design"}
    ],
    "routing_uncertainty": ["Material ambiguity in classification, or an empty array"]
  },
  "sections": [
    {"number": "1", "title": "Introduction", "page_start": 1, "page_end": 4}
  ],
  "formal_results": [
    {"kind": "theorem", "number": "1", "page": 10, "summary": "...", "proof_location": "Appendix A, pp. 30-33"}
  ],
  "tables_figures": [
    {"kind": "table", "number": "1", "page": 14, "caption_summary": "...", "what_it_shows": "..."}
  ],
  "notation": [
    {"symbol": "beta", "definition": "...", "page_introduced": 5}
  ],
  "stated_contribution": "Quoted or closely paraphrased from the introduction",
  "key_references": ["Author (year)"],
  "extraction_quality_notes": [
    {"page_range": "pp. 10-15", "description": "equations garbled, subscripts missing"}
  ]
}

SUBJECT SPECIALIST CATALOG

The catalog is grouped by discipline. Within each group, the broad discipline entry is a fallback; prefer one more-specific subfield entry whenever it fits the paper's central contribution.

{subject_catalog}

METHOD SPECIALIST CATALOG

{method_catalog}

Routing rules:

1. Select one primary subject specialist. Select a second subject specialist only when the central contribution genuinely crosses two bodies of subject-matter expertise and one reviewer would predictably miss a material issue. The array is ordered: primary first, secondary second.
2. Prefer the most specific fitting subfield. Use a discipline-level fallback only when no listed subfield fits. Never select both a discipline fallback and one of its own subfields.
3. Select between one and four method specialists. Select a method only when it materially supports or tests a central claim; a technique mentioned in passing is not enough.
4. Subject and method roles are complementary. A mathematics subject reviewer does not replace formal-proof review when proofs carry the result; a clinical subject reviewer does not replace clinical-study review when patient evidence carries the conclusion.
5. Pure theory and pure mathematics papers must not receive causal, randomized, clinical, measurement, survey, qualitative, archival, or laboratory reviewers unless the paper truly contains that component.
6. Descriptive empirical work without a causal claim should generally receive measurement/data and statistical-validity review, not causal-identification review. Randomized interventions need randomized-experiment review; observational causal designs need causal-identification review; structural counterfactuals need structural-estimation review.
7. Distinguish physical laboratory work, observational scientific instrumentation, clinical patient research, survey research, archival sources, qualitative cases, textual interpretation, and computational simulation. Select the role matching the evidence actually used.
8. Formal-proofs is for results whose validity depends on proofs or nontrivial derivations. Economic-model-logic is for incentives, equilibrium, mechanisms, incidence, welfare, or comparative statics. Conceptual-argument is for conceptual, synthetic, or normative reasoning. These roles may coexist when each is central.
9. Quantitative-computation covers calibrated or numerically solved models and counterfactuals. Simulation-numerics covers approximation error, discretization, Monte Carlo behavior, or synthetic experiments. Algorithmic-ML covers claims about algorithms, learning systems, benchmarks, or complexity. Engineering-validation covers performance against requirements, prototypes, tolerances, and failure modes.
10. Use network-analysis only when relational construction or network dependence is substantively important. Use computational-text-analysis when automated text measurement supports a substantive conclusion, not when the paper contributes an NLP system. Use bibliometric-scientometric only for claims about publication, citation, collaboration, patent, or knowledge-system records.
11. Use systematic-review/meta-analysis only when evidence synthesis itself is a central method. Use reproducibility/software when central conclusions depend on a nontrivial computational implementation, pipeline, or released research artifact.
12. Use design-based/practice research when iterative functional, educational, or service design is the evidentiary strategy. Use creative/practice-led research when artistic or performative practice generates the scholarly claim. Use participatory/community research when shared authority or co-production is methodologically material. Use research-ethics/governance only when consent, welfare, data authority, conflicts, responsible release, or dual-use risk materially affects validity or permissible scope; routine approval alone is not enough.
13. The catalog contains academic-review roles only. Classify sensitive or dual-use work at a high level from the paper's stated discipline and evidence. Do not reproduce, extend, or infer operationally harmful procedures in the routing notes.
14. If classification is genuinely ambiguous, record the ambiguity. Do not compensate by indiscriminately selecting reviewers; choose the smallest set that covers the material uncertainty.
15. `selection_notes` must contain exactly one entry for every selected subject and method ID, and no unselected IDs. Give paper-specific reasons rather than restating catalog labels.

Inventory rules:

- List every proposition, theorem, lemma, corollary, definition, and explicit assumption in `formal_results`.
- List every explicitly defined symbol in `notation`; do not invent definitions.
- Record the paper's own contribution claim in `stated_contribution`.
- Flag garbled or incomplete extraction rather than treating it as an error in the paper.
- Return ONLY valid JSON. No markdown fences or commentary.

<paper>
{paper_text}
</paper>
