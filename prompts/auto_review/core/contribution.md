# Contribution, Claims, and Literature

Identify material problems with what the paper claims to contribute. Do not audit proofs, estimation, or prose except where those defects directly change the contribution claim.

Start from the paper's stated contribution and the detected subject in the orientation map. Compare the introduction's promise with the paper's actual central results. Determine whether the claimed advance is conceptually distinct from the closest cited work, whether the paper establishes the scope it claims, and whether an omitted reference would materially change the positioning.

If web search is available, verify external literature claims using primary papers, publisher pages, or stable working-paper pages. Never infer novelty from titles or search snippets. Give a stable URL or DOI for every externally verified claim. If search is unavailable, restrict the assessment to the paper's own citations and comparisons without discussing tool availability.

## Output

Populate the supplied findings schema with at most six findings, in decreasing order of importance. Treat six as a ceiling, not a target: omit any concern that cannot be tied to a specific claim and concrete evidence. If no material issue survives checking, return an empty findings array. For every finding:

- `title`: a specific descriptive title naming the issue.
- `in_the_paper`: quote or closely paraphrase the contribution or scope claim and identify the result offered in support.
- `problem`: compare that claim with the delivered result or closest prior work. Give a full citation and stable URL or DOI for any externally verified literature claim.
- `consequence`: why the gap changes the paper's novelty, importance, or defensible scope.
- `what_would_help`: the smallest credible correction, comparison, additional result, or reframing.
- `evidence`: at least one locator you actually verified — the page containing the paper's claim, the section reference in `description`, or a short `quote`. Never invent a locator.

Do not summarize the paper, praise it, compile a generic reading list, discuss the review process, or invent citations.
