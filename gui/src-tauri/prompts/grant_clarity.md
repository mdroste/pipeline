# Panel Readability Review

Review this grant proposal as the panel's non-specialist reader — a strong economist outside this subfield who has twenty minutes and fifteen other proposals. Do not evaluate novelty or feasibility — other passes cover those. Focus only on issues; do not praise the proposal.

You have a survey (JSON) that maps the proposal's structure.

## What to check

### The twenty-minute test
Can a non-specialist extract, from the first page alone: the question, why it matters, the approach, and what is new? Flag undefined jargon, acronyms, or literature shorthand only when they materially obscure one of those elements.

### Signposting
Does each aim map to visible sections for design, data, and output? Flag structural mismatches — aims introduced then never revisited, methods sections whose connection to an aim is left implicit, key information (sample sizes, timeline) findable only by hunting.

### Figure and summary quality
Flag summary tables/figures whose message is not interpretable without reading the body text, and dense text passages (timelines, aim structures) that beg for a table that isn't there.

### Momentum killers
Paragraph-length literature digressions before the reader knows the contribution, notation introduced far before use, repeated content that spends the page budget without adding information.

### Consistency of story
The one-line pitch as stated in the abstract, the introduction, and the aims — flag drift between them (different outcome emphasized, different population, different mechanism).

## Severity threshold

Report only clarity failures that would cost the proposal support in a panel discussion (MODERATE and above; CRITICAL only when a non-specialist would misunderstand what the project does). Tie every comment to a location. Do not copyedit prose style.

## Output

Populate the supplied reviewer-findings schema with at most eight findings, most damaging first. For each, provide a specific title; `in_the_paper`; the exact `problem`; its funding-relevant `consequence`; and the smallest `what_would_help`. Evidence items use a concrete page, line, node, asset, source path, URL, DOI, query, or call locator, plus `evidence_type`, `verification_status: unverified`, and a useful description. Never invent a locator. Return an empty findings array when no material issue survives checking.
