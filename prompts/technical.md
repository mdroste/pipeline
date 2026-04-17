# Technical Correctness

Identify errors and gaps in the formal results. Do not comment on contribution, writing, or empirical strategy.

You have an orientation map (JSON) that inventories all formal results, their proof locations, and the paper's notation. Use it to locate proofs and check dependencies between results.

## How to reason

For each formal result (theorem, proposition, lemma, corollary):

1. Read the statement and its proof in full.
2. Identify the key steps. For each non-trivial step, work through the algebra or logic yourself.
3. Check whether the stated assumptions are sufficient. What happens if an assumption is relaxed? Does the proof rely on something stronger than stated?
4. Look for: sign errors, missing cases, boundary conditions, incorrect applications of cited results, division by zero, unjustified limit interchanges, circular reasoning, steps marked "it can be verified" where the intermediate step is non-trivial.

**Show your work.** When you find an error, reproduce the derivation, identify the step that fails, and show what actually follows. Do not simply assert a step is wrong — demonstrate it.

## Confidence calibration

If a section is flagged as garbled in the extraction quality notes, note this rather than asserting the paper is wrong. For PDF-extracted papers, be cautious with subscripts, superscripts, and equation formatting.

## Output

Produce up to 8 numbered comments targeting issues that affect whether the main results are correct. For each:

- **Title**: Specific header (e.g., "Proof of Proposition 2 assumes nonnegativity not guaranteed by Assumption 1")
- **In the paper**: Quote or cite the specific result and the proof step where the argument breaks.
- **The problem**: Show the derivation — what the paper claims follows, and what actually follows. If a case is missing, construct it. If an assumption is insufficient, show why the step fails without a stronger condition.
- **Consequence**: What goes wrong in the paper's conclusions if this issue is real.
- **Fix**: What it would take to repair — a stronger assumption, an additional case, a corrected step.
- **Location**: Page, equation, or proof reference.

If you cannot demonstrate the error — if you are suspicious but cannot show the failing step — say so explicitly and mark the comment as uncertain. Do not manufacture issues where the formal content is straightforward. Do not summarize the paper. Do not list strengths.
