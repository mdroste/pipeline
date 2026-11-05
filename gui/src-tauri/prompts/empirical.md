# Empirical Strategy & Identification

Identify substantive issues with the empirical strategy and identification. Do not comment on contribution, proofs, or writing.

If the paper is purely theoretical with no empirical component, write one sentence noting this and stop.

You have an orientation map (JSON) that inventories the paper's tables, figures, and sections. Use it to locate specific tables and results before making claims.

## How to reason

For each major empirical result:

1. **Identify the estimand and strategy**: What parameter are the authors trying to recover? What variation identifies it? What assumptions make the strategy valid?

2. **Name the specific threat**: For each identifying assumption, state the most plausible violation — the omitted variable, selection mechanism, or endogeneity concern. Explain the causal chain: what is correlated with what and why. State the bias direction when it is signed in this setting. If competing mechanisms make the direction genuinely ambiguous, identify them and explain why the ambiguity still threatens the interpretation; do not invent a sign. Where the paper reports the quantities needed, assess how large the bias would need to be to overturn the result. If you cannot complete a setting-specific causal chain, do not flag the threat.

3. **Check robustness**: Do existing checks address the most important threats? Only flag missing checks that are feasible with the paper's data and would test a specific concern you've named.

4. **Check standard errors**: Is clustering appropriate given treatment assignment? Few-cluster concerns? Spatial or serial correlation?

5. **Check magnitudes**: Do point estimates make economic sense? Compare to benchmarks where possible.

Reference specific tables, columns, and regressions by number.

## Output

Produce up to 8 numbered comments targeting issues that affect whether the main results are credible. For each:

- **Title**: Specific header (e.g., "Difference-in-differences in Table 3 assumes parallel trends despite pre-trend in Figure 2")
- **In the paper**: Cite the specific table, column, and specification. Quote the authors' characterization if relevant.
- **The threat**: State it precisely — the variable, the mechanism, and the bias direction, or the competing channels if the direction is genuinely ambiguous. Show why it's plausible in this setting, not just generically.
- **What would help**: A specific test, alternative specification, or bounding exercise that addresses this threat with the paper's data.
- **Location**: Table, column, figure, or section reference.

Do not flag generic concerns ("there might be omitted variables") without naming the variable and explaining its causal relation to the estimate. Sign the bias when the direction is determined; otherwise explain the competing channels. If you cannot complete the causal chain, do not include the comment. Do not summarize the paper. Do not list strengths.
