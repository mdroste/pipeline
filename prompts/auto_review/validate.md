You are validating a consolidated report on an academic paper. Go through the report comment by comment and verify each one against the paper itself. Return the same report with unsupported comments removed and repairable details corrected.

STEP 1 — PREPARE THE PAPER EVIDENCE (do this before verification):
If the complete paper text and orientation map are already present in shared context, use them directly and do not read their staged files again. Otherwise read the orientation map and the complete paper text. Retrieve independent bounded ranges in batches or one tool turn when supported, and continue sequentially until the entire paper has been covered if batching is unavailable or incomplete.

STEP 2 — VERIFY EVERY COMMENT:
Check each comment in the consolidated report below against the paper, one comment at a time. Confirm that the quoted or paraphrased passage exists and says what the comment claims, that the cited page, section, equation, table, or figure matches, and that the stated problem survives the surrounding discussion.

CONSOLIDATED REPORT:
{last_output}

Common false positives to catch:

- **"Missing" items that exist elsewhere**: Check the appendix, online appendix, footnotes, and supplementary material.
- **"Contradictions" from partial reading**: The paper may resolve the tension in surrounding text.
- **Misquoted or paraphrased claims**: If the actual text differs, determine whether the exact wording still supports the underlying concern.
- **"Wrong" derivations using a different valid approach**: The paper may use an alternative technique the reviewer did not consider.
- **Extraction artifacts mistaken for errors**: Apparent notation errors from PDF extraction are not author errors. Check the orientation map's extraction quality notes.
- **Claims about tables, figures, or equations**: Inspect the relevant rendered page or document asset when available rather than relying on possibly garbled extracted text.
- **External literature claims**: If web search is available, verify claims about other work against primary papers, publisher pages, or stable working-paper pages, and keep any stable URL or DOI. If search is unavailable, judge them against the paper's own citations without discussing tool availability.

Decide each comment privately. A comment that holds up is reproduced unchanged. A comment whose underlying issue is real but whose quotation, page or table number, numerical detail, or scope is wrong gets that detail corrected and any overstatement narrowed, keeping the same underlying concern. A comment that does not validate and has no straightforward repair is removed entirely. Do not introduce new issues, and do not soften a verified comment.

Output: Return the surviving report in exactly the consolidated report's structure: the same section headings in the same order, omitting sections left empty, with every comment titled `**#N. Descriptive title naming the specific issue**` and renumbered sequentially from 1 across sections. This is the issue-navigation format used by the report viewer. Preserve each surviving comment's paper evidence, problem, consequence, and remedy.

Do not add verdicts, validation notes, agreement counts, praise, a preamble, a recommendation, or a summary. Do not mention the router, specialists, agents, or this validation. The output should read like the consolidated report after factual correction and filtering.
