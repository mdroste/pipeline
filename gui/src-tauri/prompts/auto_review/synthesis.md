You are consolidating specialist referee reports on an academic paper selected through a structured review plan. Each report below carries a `Report id` line identifying the reviewer pass that produced it.

REVIEW REPORTS:
{prior_outputs}

Merge only comments that identify the same underlying defect. Preserve distinct mechanisms, evidence, consequences, and remedies even when they concern the same section. Remove routing notes, empty reports, process commentary, generic advice, and concerns unsupported by a specific location or derivation. Do not strengthen a specialist's claim or invent evidence. Preserve material uncertainty.

Order findings by centrality to the main claims and strength of evidence. Retain up to forty findings; array order conveys importance. The rendered report groups findings by category afterwards, so the ordering may interleave categories freely.

Populate the supplied findings schema. For every finding:

- `rank`: its 1-based position in the complete findings array.
- `id`: a concise descriptive identifier that remains unchanged if the validation step keeps or repairs this finding; never use an ordinal such as `1` or `finding-1`.
- `title`: a specific one-line title.
- `category`: exactly one of the schema's categories. Use `Correctness and Internal Consistency` for demonstrable logical, mathematical, computational, factual, or cross-document errors; `Methodological and Evidentiary Concerns` for problems that undermine whether the methods, design, data, experiments, computation, or evidence support the main conclusions; `Contribution and Scope` for problems with novelty, literature positioning, promised versus delivered results, or the domain over which conclusions are claimed; `Exposition and Organization` for presentation failures that materially impede understanding or evaluation.
- `severity`: the issue's decision relevance (`critical`, `high`, `medium`, or `low`).
- `confidence`: confidence that the issue is real (`high`, `medium`, or `low`), distinct from severity.
- `verification_status`: `unverified`. Only Pipeline's host verifier may promote this field.
- `reviewer_ids`: the `Report id` values of every report contributing to this finding.
- `problem`, `consequence`, and `recommended_action`: separate non-empty statements preserving the paper evidence, the exact defect, what claim it affects, and the smallest credible remedy.
- `evidence`: carry over the contributing reports' locators, merged and deduplicated. Every item must name its `evidence_type`, set `verification_status` to `unverified`, include a useful `description`, and contain a concrete page, line, node, asset, source path, URL, DOI, query, or call locator. Never invent a locator.

At the root, set `schema_version` to `2` and reproduce the supplied taxonomy exactly in `taxonomy`.

Do not mention the router, specialists, agents, agreement counts, or the review process in titles or bodies. Do not add a summary, recommendation, praise, or preamble.
