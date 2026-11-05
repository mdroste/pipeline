# Feasibility & Design Review

Review whether this grant proposal's plan can actually deliver its aims. Do not evaluate novelty or writing — other passes cover those. Focus only on issues; do not praise the proposal.

You have a survey (JSON) that maps the proposal's structure. Use it to locate the research design, data plan, timeline, and team description.

## What to check

### Design–aim fit
For each aim: does the proposed method actually answer it? Flag identification strategies asserted without addressing their standard threats, and aims with no corresponding design section at all.

### Data realism
Data claimed to exist: is access plausible in the stated timeframe (restricted-access approvals, partner agreements not yet signed)? Data to be collected: is the sample size justified by a power calculation, and is the recruitment plan concrete? Flag power claims with no calculation and samples that seem infeasible at the stated budget or timeline.

### Timeline
Dependencies between stages (approval → collection → analysis) versus the Gantt/timeline as stated; stages whose durations are implausible given the described work; no slack for the steps most likely to slip (approvals, partnerships, fieldwork).

### Risk planning
Single points of failure with no stated contingency: one dataset, one partner, one identification strategy. Flag missing "if this fails, we…" for the riskiest elements.

### Team and resources
Expertise the plan requires (e.g. survey operations, specific methods, languages) that no team member's description covers; personnel allocations obviously mismatched to the described workload.

## Severity threshold

Report only feasibility concerns a panel would weigh (CRITICAL: an aim that the design cannot deliver, or a dependency with no plausible path; MODERATE: missing contingencies and optimistic-but-arguable timelines). Ground every concern in the proposal's own numbers and statements.

## Output

Populate the supplied reviewer-findings schema with at most eight findings, most damaging first. For each, provide a specific title; `in_the_paper`; the exact `problem`; its funding-relevant `consequence`; and the smallest `what_would_help`. Evidence items use a concrete page, line, node, asset, source path, URL, DOI, query, or call locator, plus `evidence_type`, `verification_status: unverified`, and a useful description. Never invent a locator. Return an empty findings array when no material issue survives checking.
