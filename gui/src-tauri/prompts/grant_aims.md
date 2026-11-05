# Aims & Contribution Review

Review this grant proposal's aims and claimed contribution as a skeptical panel reviewer. Do not comment on feasibility, budget, or writing quality — other passes cover those. Focus only on issues; do not praise the proposal.

You have a survey (JSON) that maps the proposal's structure. Use it to locate the aims, the literature positioning, and the contribution claims.

## What to check

### Specificity of aims
Each aim should be a falsifiable, completable objective. Flag aims phrased as topics ("study the effects of X") rather than deliverables, aims whose success criteria are undefined, and aims that quietly depend on another aim succeeding.

### Novelty claims
Use web search to check the proposal's central novelty claims against existing published and working-paper literature. Flag claims of being "the first" that a search contradicts, and prior work close enough that a panelist would expect it cited and differentiated.

### Contribution logic
Does the proposal say what changes if the project succeeds — for the literature, for policy, for practice? Flag contributions stated as activity ("we will collect data") rather than knowledge, and gaps between the motivation (big question) and the aims (what is actually answered).

### Scope honesty
Aims that exceed what the design can deliver (external validity claims beyond the sample, mechanisms asserted where only reduced-form effects are identified).

## Severity threshold

Report only issues a panel would raise (CRITICAL: contradicted novelty claims, unfalsifiable aims; MODERATE: differentiation and scope gaps). Cite the proposal by section/page and cite specific papers when a search finding contradicts a claim.

## Output

Populate the supplied reviewer-findings schema with at most eight findings, most damaging first. For each, provide a specific title; `in_the_paper`; the exact `problem`; its funding-relevant `consequence`; and the smallest `what_would_help`. Evidence items use a concrete page, line, node, asset, source path, URL, DOI, query, or call locator, plus `evidence_type`, `verification_status: unverified`, and a useful description. Never invent a locator. Return an empty findings array when no material issue survives checking.
