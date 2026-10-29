# Internal Consistency Audit

Audit the paper for internal inconsistencies. Do not comment on contribution, identification strategy, or writing style.

You have an orientation map (JSON) that inventories sections, formal results, tables, figures, and notation. **Before flagging anything as missing or misreferenced, check the orientation map** — it may exist in an appendix, footnote, or supplement.

## What to check

### Cross-references
For every reference to a Table, Figure, Equation, Section, or formal result by number: verify the referent exists and the claim matches. Pay particular attention to coefficients, magnitudes, signs, and significance levels.

### Text vs. tables
For each table discussed in the text: verify the table number, column/row, sign, magnitude, significance level, and qualitative characterization against the actual table. If table formatting was garbled by extraction, note this and do not fabricate table contents.

### Abstract vs. body
For each factual claim in the abstract, locate the supporting result. Flag claims that are unsupported, overstated, or contradicted by the body.

### Notation
Flag symbols that change meaning, are used but never defined, or are defined differently in two places. Ignore standard notation (β for coefficients, etc.).

## Severity threshold

Report only issues where a reader would draw a wrong conclusion (CRITICAL) or be confused in a way that matters (MODERATE). Do not report cosmetic issues.

## Output

Produce up to 8 numbered comments targeting the most important inconsistencies. For each:

- **Title**: Specific header (e.g., "Text claims Table 2 Column 3 coefficient is negative, but table shows 0.034 (positive, p < 0.01)")
- **The paper says**: Quote the passage making the claim, with page/section reference.
- **The paper shows**: Quote or describe what actually appears at the referenced location — give the number, the expression, the actual content.
- **Consequence**: What a reader would incorrectly conclude.
- **Location**: Both the source reference and the target it points to.

Every comment must show both sides of the inconsistency — the claim and the evidence it conflicts with. If you cannot find both sides, do not include the comment. Do not summarize the paper. Do not list strengths.
