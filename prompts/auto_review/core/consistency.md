# Claims and Internal Consistency

Audit whether the paper's claims, results, references, notation, tables, and figures agree with one another. Do not independently re-prove theorems or reassess identification; specialist passes handle those questions.

Check the abstract and introduction against the actual main results. Verify numbered cross-references and every important numerical, directional, or significance claim against the referenced equation, theorem, table, or figure. Check whether notation changes meaning and whether conclusions silently exceed the maintained assumptions, sample, model, or experiment.

Before reporting a missing item, inspect the surrounding discussion, footnotes, appendix, and supplied supplementary material. If extraction makes a table or equation unreliable, identify the extraction limitation instead of reconstructing content from guesswork.

## Output

Return only a concise Markdown report with at most six numbered comments. Use this structure for every comment so the report viewer can index it:

**#1. Specific descriptive title**

- **Severity:** Critical, major, or moderate.
- **In the paper:** Quote the claim and identify its precise location.
- **The problem:** Show the conflicting equation, theorem, table, figure, reference, definition, or passage, including the actual number, sign, expression, or wording when available.
- **Consequence:** Explain what a reader would incorrectly conclude.
- **What would help:** State the smallest correction or qualification needed to make both locations agree.
- **Location:** Give both the source claim and the conflicting target location.

Increment `N` sequentially from 1. Every comment must show both sides of the inconsistency. Treat six as a ceiling, not a target. If no material issue survives checking, return only `No material issues identified.` Do not report cosmetic cross-reference or copyediting issues, summarize the paper, list strengths, or discuss the review process.
