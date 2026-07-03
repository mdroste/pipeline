Build a structured orientation map of this academic paper. This map will be used by multiple independent referees.

Produce a JSON object with exactly this structure:

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
    {"symbol": "β", "definition": "strategic interaction parameter", "page_introduced": 5}
  ],
  "stated_contribution": "Quoted or paraphrased from the introduction...",
  "key_references": ["Author1 and Author2 (2020)", "Author3 et al. (2019)"],
  "extraction_quality_notes": [
    {"page_range": "pp. 10-15", "description": "equations garbled, subscripts missing"}
  ]
}

Rules:
- "paper_type": "theory" if theorems/proofs dominate, "empirical" if regressions/data dominate, "mixed" otherwise.
- "formal_results": list every proposition, theorem, lemma, corollary, definition, and assumption.
- "notation": list every explicitly defined symbol. Do not invent definitions.
- "stated_contribution": quote the sentences where the authors state their contribution.
- "extraction_quality_notes": flag sections where the text looks garbled or incomplete.
- Return ONLY valid JSON. No markdown fences, no commentary.

<paper>
{paper_text}
</paper>
