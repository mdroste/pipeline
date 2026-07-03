# Consolidate Replication Audit

You are consolidating the outputs of several independent replication-package audit passes (completeness, code–paper consistency, portability, data provenance) into one report, structured the way a journal data editor would write it.

## Instructions

1. **Deduplicate.** Where passes flagged the same underlying gap (e.g. a missing file surfaced by both completeness and portability), merge into one finding noting both consequences.
2. **Verify plausibility.** Drop findings that contradict the package's own evidence as reported by another pass; where two passes disagree, state the disagreement rather than picking silently.
3. **Order by what blocks reproduction.** First: anything that makes exhibits unreproducible. Second: divergences between paper and code. Third: friction a replicator can work around. Fourth: documentation polish.
4. **Keep locations.** Every finding retains its file/line or exhibit reference.
5. **Do not add new findings.**

## Output format

Begin with a verdict paragraph in data-editor style: would this package plausibly pass a reproducibility check as shipped — and if not, what class of problem blocks it?

Then:

**Required changes** — numbered findings that block reproduction or accuracy, each with severity (CRITICAL/MODERATE), title, location, one-paragraph description, and source pass(es).

**Recommended changes** — the remainder, same format, MINOR allowed.

**Exhibit status table** — from the completeness pass: each exhibit → generating script → any blocking findings by number.

**Not addressed** — what the audit could not check (restricted data not shipped, code too large to read fully, garbled manuscript extraction), so the limits of the audit are explicit.

## Prior step outputs

{prior_outputs}
