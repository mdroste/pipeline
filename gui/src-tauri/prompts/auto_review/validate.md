You are validating a consolidated findings product on an academic paper. Check every finding against the complete paper, including relevant appendices and supplied supplementary material, and return only findings that hold up.

CONSOLIDATED FINDINGS:
{last_output}

Check common false positives: allegedly missing material found in surrounding text, footnotes, appendices, or supplements; tensions resolved by context; misquotation; an alternative valid derivation; and extraction artifacts. Inspect rendered pages or assets for claims about tables, figures, equations, or notation. When search is available, verify external literature claims against primary papers, publisher pages, or stable working-paper pages and retain the stable URL or DOI; otherwise use the paper's citations.

Classify each finding privately as verified, repairable, or unsupported. Keep verified findings. Repair an incorrect quotation, locator, numerical detail, or scope when the underlying issue remains valid; otherwise drop it.

Populate the supplied findings schema in the same order after removals. Set `schema_version` to `2`, reproduce the supplied taxonomy exactly, and assign surviving findings contiguous `rank` values starting at 1. Every surviving finding must retain its exact input `id`; do not replace identifiers, and do not add new findings. Preserve each finding's title, category, reviewer and call lineage, problem, consequence, recommended action, and evidence, repairing those fields only when the paper evidence requires it. Model-returned verification fields remain `unverified`; Pipeline records the validation disposition separately. Do not add verdict prose, praise, a preamble, a recommendation, or a summary.

Return `validation_dispositions` with exactly one entry for every input finding, in the original input order. Each entry contains the exact `finding_id`, one disposition, and a specific non-empty `reason`:

- `retained` when the finding is unchanged;
- `revised` when any substantive field or locator was repaired;
- `merged_into` when it was combined into another surviving stable ID (also set `merged_into` to that ID);
- `rejected_false_positive` when paper evidence disproves it;
- `unverified_missing_evidence` when it cannot be supported by a concrete locator.
- `deferred_manual_review` when resolution requires human subject-matter judgment rather than more evidence retrieval.

Pipeline computes before/after hashes and rejects incomplete or inconsistent ledgers. Do not invent hash values.
