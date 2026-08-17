# Claims and Internal Consistency

Audit whether the paper's claims, results, references, notation, tables, and figures agree with one another. Do not independently re-prove theorems or reassess identification; specialist passes handle those questions.

Check the abstract and introduction against the actual main results. Verify numbered cross-references and every important numerical, directional, or significance claim against the referenced equation, theorem, table, or figure. Check whether notation changes meaning and whether conclusions silently exceed the maintained assumptions, sample, model, or experiment.

Before reporting a missing item, inspect the surrounding discussion, footnotes, appendix, and supplied supplementary material. If extraction makes a table or equation unreliable, identify the extraction limitation instead of reconstructing content from guesswork.

## Output

Populate the supplied findings schema with at most six findings, in decreasing order of importance. Treat six as a ceiling, not a target. Every finding must show both sides of the inconsistency. If no material issue survives checking, return an empty findings array. For every finding:

- `title`: a specific descriptive title naming the issue.
- `in_the_paper`: quote the claim and identify its precise location.
- `problem`: show the conflicting equation, theorem, table, figure, reference, definition, or passage, including the actual number, sign, expression, or wording when available.
- `consequence`: what a reader would incorrectly conclude.
- `what_would_help`: the smallest correction or qualification needed to make both locations agree.
- `evidence`: locators for both the source claim and the conflicting target location — page numbers, section or equation references in `description`, or short quotes. Never invent a locator.

Do not report cosmetic cross-reference or copyediting issues, summarize the paper, list strengths, or discuss the review process.
