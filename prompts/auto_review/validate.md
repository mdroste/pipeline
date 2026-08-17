You are validating a consolidated findings product on an academic paper. Verify every finding against the paper and return the filtered product containing only findings that hold up.

STEP 1 — PREPARE THE PAPER EVIDENCE (do this before verification):
If the complete paper text and orientation map are already present in shared context, use them directly and do not read their staged files again. Otherwise read the orientation map and the complete paper text. Retrieve independent bounded ranges in batches or one tool turn when supported, and continue sequentially until the entire paper has been covered if batching is unavailable or incomplete.

STEP 2 — VERIFY ALL FINDINGS:
Check every finding in the consolidated product below against the complete paper evidence.

CONSOLIDATED FINDINGS:
{last_output}

Common false positives to catch:

- **"Missing" items that exist elsewhere**: Check the appendix, online appendix, footnotes, and supplementary material.
- **"Contradictions" from partial reading**: The paper may resolve tensions in surrounding text.
- **Misquoted or paraphrased claims**: If the actual text differs, determine whether the exact wording still supports the underlying concern. Correct an immaterial error; drop the finding if its substance depends on the misattribution.
- **"Wrong" derivations using a different valid approach**: The paper may use an alternative technique the reviewer did not consider.
- **Extraction artifacts mistaken for errors**: Apparent notation errors from PDF extraction are not author errors. Check the orientation map's extraction quality notes.
- **Claims about tables, figures, or equations**: Inspect the relevant rendered page or document asset when available. Do not rely on possibly garbled extracted text when the visual evidence can resolve the claim.
- **External literature claims**: If web search is available, verify claims about other work against primary papers, publisher pages, or stable working-paper pages, and keep any stable URL or DOI. If search is unavailable, judge them against the paper's own citations without discussing tool availability.

Classify each finding privately as verified, repairable, or unsupported. Keep verified findings. When the underlying issue is valid but a quotation, page number, table entry, numerical detail, locator, or scope is wrong, correct that detail and narrow any overstatement rather than dropping the finding. Drop unsupported findings.

Populate the supplied findings schema in the same order after removals. Every surviving finding must retain its exact input `id`; do not renumber or replace identifiers, and do not add new findings. Preserve each finding's `title`, `category`, `sources`, `body`, and `evidence`, repairing those fields only when the paper evidence requires it. Do not add verdicts, validation notes, praise, a preamble, a recommendation, or a summary.
