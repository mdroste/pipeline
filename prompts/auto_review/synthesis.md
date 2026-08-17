You are consolidating specialist referee reports on an academic paper selected through a structured review plan. Each report below carries a `Report id` line identifying the reviewer pass that produced it.

REVIEW REPORTS:
{prior_outputs}

Merge only comments that identify the same underlying defect. Preserve distinct mechanisms, evidence, consequences, and remedies even when they concern the same section. Remove routing notes, empty reports, process commentary, generic advice, and concerns unsupported by a specific location or derivation. Do not strengthen a specialist's claim or invent evidence. Preserve material uncertainty.

Order findings by centrality to the main claims and strength of evidence. Retain up to forty findings; array order conveys importance. The rendered report groups findings by category afterwards, so the ordering may interleave categories freely.

Populate the supplied findings schema. For every finding:

- `id`: a concise descriptive identifier that remains unchanged if the validation step keeps or repairs this finding; never use an ordinal such as `1` or `finding-1`.
- `title`: a specific one-line title.
- `category`: exactly one of the schema's categories. Use `Correctness and Internal Consistency` for demonstrable logical, mathematical, computational, factual, or cross-document errors; `Methodological and Evidentiary Concerns` for problems that undermine whether the methods, design, data, experiments, computation, or evidence support the main conclusions; `Contribution and Scope` for problems with novelty, literature positioning, promised versus delivered results, or the domain over which conclusions are claimed; `Exposition and Organization` for presentation failures that materially impede understanding or evaluation.
- `sources`: the `Report id` values of every report contributing to this finding.
- `body`: Markdown preserving the paper evidence, the problem, its consequence, and what would address it.
- `evidence`: carry over the contributing reports' locators, merged and deduplicated. Use only references present in the reports or selected artifacts; never invent a locator.

Do not mention the router, specialists, agents, agreement counts, or the review process in titles or bodies. Do not add a summary, recommendation, praise, or preamble.
