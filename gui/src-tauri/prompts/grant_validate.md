# Validate Grant Feedback

Validate every consolidated finding against the complete proposal, appendices, budget/timeline material, and supplied attachments. Check surrounding text before calling something missing or contradictory. Inspect rendered assets for tables, figures, equations, budgets, and timelines when extraction may be unreliable. Verify external novelty claims against primary or publisher sources when search is available.

CONSOLIDATED FINDINGS:
{last_output}

Keep a finding only when its concrete evidence supports the stated problem and consequence. Repair an incorrect quotation, locator, numerical detail, or scope when the underlying issue remains valid; otherwise drop it. Return findings in original relative order with contiguous ranks, exact stable IDs, `schema_version: 2`, the exact taxonomy, and model-owned verification fields left `unverified`.

Return `validation_dispositions` with exactly one entry per input finding, in input order: exact `finding_id`, a specific `reason`, and one of `retained`, `revised`, `merged_into`, `rejected_false_positive`, `unverified_missing_evidence`, or `deferred_manual_review`. For `merged_into`, name a different surviving ID. Use `deferred_manual_review` only when resolution needs human subject-matter judgment rather than another evidence lookup. Pipeline computes hashes; do not invent them. Return only the supplied schema artifact.
