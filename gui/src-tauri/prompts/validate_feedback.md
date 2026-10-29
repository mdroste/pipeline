Verify every comment in the consolidated report against the complete paper, including relevant appendices and supplied supplementary material. Return only comments that hold up.

CONSOLIDATED REPORT:
{last_output}

Common LLM referee errors to catch:

- **"Missing" items that exist elsewhere**: Check surrounding text, footnotes, appendices, and supplementary material.
- **"Contradictions" from partial reading**: The paper may resolve tensions in surrounding text.
- **Misquoted or paraphrased claims**: If the actual text differs, determine whether the exact wording still supports the underlying concern. Correct an immaterial error; drop the comment if its substance depends on the misattribution.
- **"Wrong" derivations using a different valid approach**: The paper may use an alternative technique the referee did not consider.
- **Extraction artifacts mistaken for errors**: Apparent notation errors from PDF extraction are not author errors. Check the orientation map's extraction quality notes.
- **Claims about tables, figures, or equations**: Inspect the relevant rendered page or document asset when available. Do not rely on possibly garbled extracted text when the visual evidence can resolve the claim.
- **External literature claims**: If web search is available, verify them against primary papers, publisher pages, or stable working-paper pages and retain any stable URL or DOI. Otherwise judge them against the paper's own citations without discussing tool availability.

Classify each comment privately as verified, repairable, or unsupported. Reproduce verified comments. When the underlying issue is valid but a quotation, page number, table entry, numerical detail, or scope is wrong, correct that detail and narrow any overstatement rather than dropping the comment. Keep the same underlying concern; do not introduce a different issue. Drop comments that are false positives or whose underlying concern you cannot confirm.

Output: Return the surviving comments using the same format and section groupings as the consolidated report. Omit empty sections and renumber sequentially.

Do not add new issues. Do not add verdicts or validation notes. Do not praise the paper. Do not add a preamble, recommendation, or summary. The output should read like the consolidated report after factual corrections and filtering.
