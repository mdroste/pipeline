You are consolidating specialist reports on an academic paper selected through a structured review plan.

REVIEW REPORTS:
{prior_outputs}

Merge only comments that identify the same underlying defect. Preserve distinct mechanisms, evidence, consequences, and remedies even when they concern the same section. Remove routing notes, empty reports, process commentary, generic advice, and concerns unsupported by a specific location or derivation. Do not strengthen a specialist's claim or invent evidence. Preserve material uncertainty.

Classify each surviving finding into exactly one of these categories:

- `Correctness and Internal Consistency` — demonstrable logical, mathematical, computational, factual, or cross-document errors.
- `Methodological and Evidentiary Concerns` — problems that undermine whether the methods, design, data, experiments, computation, or evidence support the main conclusions.
- `Contribution and Scope` — problems with novelty, literature positioning, promised versus delivered results, or the domain over which conclusions are claimed.
- `Exposition and Organization` — presentation failures that materially impede understanding or evaluation.

Order findings by centrality to the main claims and strength of evidence. Retain up to forty findings. Array order conveys importance; do not add severity labels merely to restate that order.

Populate the supplied findings schema. For every finding:

- `id`: a concise descriptive identifier that remains unchanged if the validation step keeps or repairs this finding; never use an ordinal such as `1` or `finding-1`.
- `title`: a specific one-line title.
- `category`: exactly one category listed above.
- `body`: Markdown preserving the paper evidence, the problem, its consequence, and what would address it.
- `evidence`: an array of zero or more evidence objects. Use only references present in the reports or selected artifacts. An evidence object may contain `page`, `node_id`, `asset_id`, `artifact_path`, `source_path`, `source_hash`, `line_start`, `line_end`, `description`, and a short `quote`. `source_path` must be relative to the selected source root; `artifact_path` must be relative to the saved run. Never invent a locator.

Do not mention the router, specialists, agents, agreement counts, or review process. Do not add a summary, recommendation, praise, or preamble.
