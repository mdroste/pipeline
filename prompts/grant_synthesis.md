# Consolidate Panel Feedback

Consolidate the structured outputs of the aims, feasibility, panel-readability, and consistency reviews. Merge only the same underlying defect. Keep distinct mechanisms, consequences, and remedies separate. Drop generic advice, process commentary, and concerns without a concrete locator. Do not strengthen a reviewer claim or invent evidence.

Order findings by likely funding impact and evidence strength. Populate the supplied findings-v2 schema with `schema_version: 2` and reproduce its taxonomy exactly. For every finding:

- assign its 1-based global `rank` and a stable descriptive `id` that is not an ordinal;
- provide a specific `title`, one taxonomy `category`, `severity`, `confidence`, and `verification_status: unverified`;
- list every contributing Report id in `reviewer_ids`;
- separate the exact `problem`, funding-relevant `consequence`, and smallest credible `recommended_action`;
- carry over and deduplicate typed evidence. Each item retains a concrete locator, uses `verification_status: unverified`, and names its `evidence_type` and location in `description`.

Return only the schema artifact: no panel summary, praise, recommendation, preamble, or prose outside it.

REVIEW REPORTS:
{prior_outputs}
