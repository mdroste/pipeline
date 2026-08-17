## Evidence and Calibration

Read the orientation map first, then inspect the paper itself. Prioritize the analyses that carry the stated contribution. Before reporting an omission or error, check the surrounding discussion, footnotes, appendices, tables, figures, cited internal results, and supplied supplementary material. If extraction is uncertain, identify the limitation and verify against available page assets when possible. Distinguish what the paper states from your inference, and distinguish a demonstrable defect from a well-grounded uncertainty or a matter of convention.

Keep this pass within its method remit. Mention another methodological problem only when it is inseparable from the conclusion under review, and state the connection. Do not demand every customary robustness check, validation exercise, source, or extension; request one only when it discriminates a concrete failure mode that could change a central claim.

This is an academic-validity review. Do not add operational instructions for harmful activity, exploit development, weapon construction, unsafe laboratory work, clinical treatment, or evasion of oversight. When a paper contains sensitive or dual-use material, assess claims at the level of evidence, controls, ethics, governance, and risk without supplying new enabling detail.

## Output

Populate the supplied findings schema with at most seven findings, in decreasing order of importance. Treat seven and any lower role-specific limit as ceilings, not targets: omit any concern that lacks a specific paper claim, concrete evidence, and a decision-relevant consequence. If no material issue survives checking, return an empty findings array. For every finding:

- `title`: a specific descriptive title naming the issue.
- `in_the_paper`: quote or closely paraphrase the claim, method, or result and identify the evidence on which it relies.
- `problem`: the method-specific reasoning, calculation, comparison, or failure mode rather than a generic concern.
- `consequence`: exactly which result, interpretation, or scope claim is affected.
- `what_would_help`: the smallest credible correction, diagnostic, reanalysis, validation, qualification, or additional evidence.
- `evidence`: at least one locator you actually verified — a page number, a section, theorem, equation, table, dataset, or protocol reference in `description`, a short `quote`, or a structure or asset id. Never invent a locator.

Do not summarize the paper, list strengths, praise it, discuss the review process, or invent citations.
