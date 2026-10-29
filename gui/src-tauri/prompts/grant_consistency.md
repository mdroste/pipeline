# Internal Consistency Review

Audit this grant proposal for internal contradictions. Do not evaluate the quality of the aims, design, or writing — other passes cover those. Your subject is whether the proposal's parts agree with each other.

You have a survey (JSON) that maps the proposal's structure. Check every cross-referenceable quantity against its other appearances.

## What to check

### Numbers
Compare sample sizes, site counts, survey waves, effect sizes, and durations across the abstract, body, tables, and timeline. Flag disagreements that change the apparent design or would materially undermine panel confidence.

### Budget vs. plan
Line items with no corresponding activity in the narrative; activities in the narrative with no visible budget line (data purchases, incentives, travel implied by fieldwork); personnel months inconsistent between budget justification and project description; totals that don't sum.

### Timeline vs. design
Activities in the design absent from the timeline and vice versa; orderings that contradict stated dependencies; deliverables promised for dates before the activity that produces them completes.

### Team vs. tasks
Named responsibilities that don't match the personnel descriptions; the same person allocated beyond plausible total effort across aims.

### References and pointers
Citations in the text missing from the bibliography; internal pointers ("see Section 4", "Appendix B") whose target doesn't exist or doesn't contain the claimed content.

## Severity threshold

CRITICAL: contradictions that change the project's apparent scope or cost (budget/narrative mismatches, sample-size disagreements). MODERATE: inconsistencies a careful panelist would catch and hold against the proposal. Do not report cosmetic issues.

## Output

Populate the supplied reviewer-findings schema with at most ten findings, most severe first. For each, provide a specific title; both conflicting statements in `in_the_paper`; the exact `problem`; its funding-relevant `consequence`; and the smallest `what_would_help`, including which version is likely intended when inferable. Evidence items use concrete locators for both statements, plus `evidence_type`, `verification_status: unverified`, and a useful description. Never invent a locator. Return an empty findings array when no material issue survives checking.
