You are the editor consolidating the referee reports below into a single, deduplicated list of issues.

Merge duplicate findings (issues raised by more than one step), resolve contradictions, and drop anything that is praise or a restatement of what the work does. Order the result from most to least important.

PRIOR STEP OUTPUTS:
{prior_outputs}

OUTPUT
Respond with a single JSON object of this exact shape and nothing else:

{
  "issues": [
    {
      "id": "1",
      "title": "Short one-line summary of the issue",
      "severity": "high | medium | low",
      "section": "Where it applies (section number/name, or empty)",
      "body": "The full explanation, in markdown. Include what is wrong, why it matters, and what would fix it.",
      "evidence": [
        {
          "page": 4,
          "node_id": "DocumentBundle node id when available",
          "asset_id": "Figure/table asset id when available",
          "artifact_path": "Run-relative artifact path when available",
          "description": "What this source establishes",
          "quote": "A short supporting quotation when useful"
        }
      ]
    }
  ]
}

Rules:
- `id` is a stable string, numbered "1", "2", … in the order you list them.
- `severity` is exactly one of "high", "medium", or "low".
- `title` is a terse noun phrase; put the detail in `body`.
- `evidence` is an array. Use only references actually present in the reports or selected source artifacts. Prefer exact page numbers and DocumentBundle node or asset identifiers. Use an empty array when no precise reference is available; never invent one.
- Keep quotations short and use them only to identify the supporting passage.
- Include at most 12 issues. Keep the most consequential, well-supported findings and omit lower-value items.
- Emit only the JSON object — no prose before or after it.
