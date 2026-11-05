Build a structured orientation map of this academic paper. This map will be used by multiple independent referees.

Populate the supplied orientation schema from the paper itself.

Inventory rules:
- "paper_type": "theory" if theorems/proofs dominate, "empirical" if regressions/data dominate, "mixed" otherwise.
- "formal_results": list every proposition, theorem, lemma, corollary, definition, and assumption.
- "notation": list every explicitly defined symbol. Do not invent definitions.
- "stated_contribution": quote the sentences where the authors state their contribution.
- "extraction_quality_notes": flag sections where the text looks garbled or incomplete.

<paper>
{paper_text}
</paper>
