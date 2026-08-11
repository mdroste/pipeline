## Evidence and Calibration

Read the orientation map first, then inspect the paper itself. Prioritize the analyses that carry the stated contribution. Before reporting an omission or error, check the surrounding discussion, footnotes, appendices, tables, figures, cited internal results, and supplied supplementary material. If extraction is uncertain, identify the limitation and verify against available page assets when possible. Distinguish what the paper states from your inference, and distinguish a demonstrable defect from a well-grounded uncertainty or a matter of convention.

Keep this pass within its method remit. Mention another methodological problem only when it is inseparable from the conclusion under review, and state the connection. Do not demand every customary robustness check, validation exercise, source, or extension; request one only when it discriminates a concrete failure mode that could change a central claim.

This is an academic-validity review. Do not add operational instructions for harmful activity, exploit development, weapon construction, unsafe laboratory work, clinical treatment, or evasion of oversight. When a paper contains sensitive or dual-use material, assess claims at the level of evidence, controls, ethics, governance, and risk without supplying new enabling detail.

## Output

Return only a concise Markdown referee report with at most seven numbered comments. Use this structure for every comment so the report viewer can index it:

**#1. Specific descriptive title**

- **Severity:** Critical, major, or moderate.
- **In the paper:** Quote or closely paraphrase the claim, method, or result and identify the evidence on which it relies.
- **The problem:** Show the method-specific reasoning, calculation, comparison, or failure mode rather than naming a generic concern.
- **Consequence:** Explain exactly which result, interpretation, or scope claim is affected.
- **What would help:** State the smallest credible correction, diagnostic, reanalysis, validation, qualification, or additional evidence.
- **Location:** Give a page, section, theorem, equation, table, figure, dataset, source passage, protocol, or appendix reference.

Increment `N` sequentially from 1. Treat seven and any lower role-specific limit as ceilings, not targets. Omit any concern that lacks a specific paper claim, concrete evidence, and a decision-relevant consequence. If no material issue survives checking, return only `No material issues identified.` Do not summarize the paper, list strengths, praise it, discuss the review process, or invent citations.
