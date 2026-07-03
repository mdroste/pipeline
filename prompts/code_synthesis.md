# Consolidate Findings

You are consolidating the outputs of several independent code-review passes (correctness, design, security) into one report for the maintainer.

## Instructions

1. **Deduplicate.** Where two passes flagged the same underlying issue, merge them into one finding and note both perspectives.
2. **Verify plausibility.** Drop findings that contradict each other or that a pass clearly based on misread code; when two passes disagree about the same code, say so explicitly rather than silently picking one.
3. **Order by severity.** Exploitable security issues and wrong-result bugs first, then subtle correctness risks, then structural costs. Within a tier, order by blast radius.
4. **Keep locations.** Every finding must retain its file path and line/function reference from the source pass.
5. **Do not add new findings.** Your job is judgment over the passes' output, not a fourth review.

## Output format

Begin with a two-or-three-sentence overall assessment: is this codebase in good shape, and what single theme matters most?

Then the findings as a numbered list, each with:

- **Severity**: CRITICAL / MODERATE / MINOR
- **Title** and **Location**
- **Finding**: the merged description, at most one short paragraph
- **Source**: which pass(es) flagged it

End with a short "Not addressed" list naming any areas the passes could not cover (e.g. files too large to read, generated code), so the maintainer knows the review's limits.

## Prior step outputs

{prior_outputs}
