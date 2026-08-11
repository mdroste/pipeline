Build a structured orientation map and a bounded review plan for this academic paper. Multiple independent specialists will use this object. Your routing choices determine which specialists run, so classify the paper from its actual claims and methods rather than from keywords or departmental labels.

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
    "primary_domain": "A broad discipline such as economics, mathematics, statistics, computer science, biomedical science, or natural science",
    "subject": "A concise, specific field or subject description",
    "paper_forms": ["formal_theory", "causal_empirical", "quantitative_model", "descriptive", "experimental", "algorithmic", "qualitative"],
    "methods": ["Concise names of methods that actually support central claims"],
    "field_specialist_id": "exactly one field ID chosen only from the catalog below",
    "method_specialist_ids": ["one to four method IDs chosen only from the catalog below"],
    "selection_notes": [
      {"id": "one selected ID", "reason": "Concrete reason tied to a central claim, method, theorem, dataset, or experiment"}
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

SPECIALIST CATALOG

{specialist_catalog}

Routing rules:

1. Select exactly one field specialist in `field_specialist_id` and between one and four method specialists in `method_specialist_ids`. Use `field_general_academic` when no more specific field specialist fits.
2. Select only method specialists whose subject is present and material to a central claim. A technique mentioned in passing is not enough.
3. Pure theory and pure mathematics papers must not receive causal-identification, randomized-experiment, structural-estimation, measurement/data, or qualitative reviewers unless the paper truly contains that component.
4. Descriptive empirical work without a causal claim should generally receive measurement/data and statistical-validity review, not causal-identification review.
5. A randomized intervention may need both randomized-experiment and measurement/data review. Observational causal work generally needs causal-identification. Structural estimation needs structural-estimation even when it also uses causal evidence.
6. Quantitative or computational models should receive quantitative-computation. Use simulation-numerics when numerical approximation, Monte Carlo evidence, or simulation accuracy is itself central.
7. Formal-proofs is for results whose validity depends on proofs or nontrivial derivations. Economic-model-logic is for assumptions, equilibrium, mechanisms, welfare, or comparative statics; many economic theory papers need both.
8. Algorithmic-ML is for claims about algorithms, prediction systems, benchmarks, learning procedures, or computational complexity—not merely for using a standard classifier as a control.
9. Use conceptual-argument when the paper's central work is conceptual, interpretive, synthetic, normative, or argumentative rather than formal, empirical, experimental, computational, or qualitative-case evidence.
10. If classification is genuinely ambiguous, record the ambiguity and select the pair of method specialists that safely covers it while staying within the four-method cap.
11. `selection_notes` must contain one entry for the field specialist and every method specialist, and no unselected IDs.

Inventory rules:

- List every proposition, theorem, lemma, corollary, definition, and explicit assumption in `formal_results`.
- List every explicitly defined symbol in `notation`; do not invent definitions.
- Record the paper's own contribution claim in `stated_contribution`.
- Flag garbled or incomplete extraction rather than treating it as an error in the paper.
- Return ONLY valid JSON. No markdown fences or commentary.

<paper>
{paper_text}
</paper>
