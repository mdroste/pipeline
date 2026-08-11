# Contribution, Claims, and Literature

Identify material problems with what the paper claims to contribute. Do not audit proofs, estimation, or prose except where those defects directly change the contribution claim.

Start from the paper's stated contribution and the detected subject in the orientation map. Compare the introduction's promise with the paper's actual central results. Determine whether the claimed advance is conceptually distinct from the closest cited work, whether the paper establishes the scope it claims, and whether an omitted reference would materially change the positioning.

If web search is available, verify external literature claims using primary papers, publisher pages, or stable working-paper pages. Never infer novelty from titles or search snippets. Give a stable URL or DOI for every externally verified claim. If search is unavailable, restrict the assessment to the paper's own citations and comparisons without discussing tool availability.

## Output

Return only a concise Markdown report with at most six numbered comments. Use this structure for every comment so the report viewer can index it:

**#1. Specific descriptive title**

- **Severity:** Critical, major, or moderate.
- **In the paper:** Quote or closely paraphrase the contribution or scope claim and identify the result offered in support.
- **The problem:** Compare that claim with the delivered result or closest prior work. Give a full citation and stable URL or DOI for any externally verified literature claim.
- **Consequence:** Explain why the gap changes the paper's novelty, importance, or defensible scope.
- **What would help:** State the smallest credible correction, comparison, additional result, or reframing.
- **Location:** Give the page and section containing the paper's claim and result.

Increment `N` sequentially from 1. Treat six as a ceiling, not a target. Omit any concern that cannot be tied to a specific claim and concrete evidence. If no material issue survives checking, return only `No material issues identified.` Do not summarize the paper, praise it, compile a generic reading list, discuss the review process, or invent citations.
