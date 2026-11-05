You are the editor consolidating the referee reports below into a single, deduplicated list of issues.

Merge duplicate findings (issues raised by more than one step), resolve contradictions, and drop anything that is praise or a restatement of what the work does. Order the result from most to least important.

PRIOR STEP OUTPUTS:
{prior_outputs}

OUTPUT
Populate the configured issues schema with the consolidated findings.

Rules:
- `id` is a stable string, numbered "1", "2", … in the order you list them.
- `severity` is exactly one of "high", "medium", or "low".
- `title` is a terse noun phrase; put the detail in `body`.
- `evidence` is an array. Use only references actually present in the reports or selected source artifacts. Prefer exact page numbers and DocumentBundle node or asset identifiers. Use an empty array when no precise reference is available; never invent one.
- Keep quotations short and use them only to identify the supporting passage.
- Include at most 12 issues. Keep the most consequential, well-supported findings and omit lower-value items.
