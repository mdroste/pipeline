# Code–Paper Consistency

Audit whether the code in this replication package implements the methods the paper describes. Do not evaluate completeness, portability, or code style — other passes cover those. This pass is about silent divergence between what the paper says and what the code does.

You have a survey (JSON) that inventories the package. Locate the manuscript (or README method descriptions) and the estimation/analysis scripts. Use the Read tool to open the actual code before commenting — never infer implementation from file names.

## What to check

### Specification
For each main estimating equation: do the regressors, fixed effects, and functional form in the code match the equation in the paper? Flag controls present in one but not the other.

### Inference
Standard error type and clustering level in the code vs. what table notes claim. This is the single most common divergence — check every main table.

### Sample construction
Sample restrictions, outlier trimming, and winsorization in the code vs. the paper's data section; observation counts produced by the code's filters vs. the N reported in tables.

### Variable construction
Key variables (treatments, instruments, outcomes) built the way the text defines them — units, timing, aggregation level, deflators.

### Randomness and tuning
Seeds set where results involve simulation, bootstrap, or ML; tuning parameters and convergence criteria in code vs. those stated in the paper.

## Severity threshold

CRITICAL: divergence that would change reported results or their interpretation (wrong clustering, different sample, different specification). MODERATE: undocumented choices a referee would want disclosed (trimming thresholds, imputation rules). Quote both sides — the paper's words and the code — for every finding. If extraction garbled the relevant paper passage, say so instead of guessing.

## Output

Produce up to 10 numbered findings, most severe first. For each:

- **Title**: Specific header (e.g., "Paper claims state-level clustering; main_results.do clusters by county")
- **The paper says**: Quoted, with section/page.
- **The code does**: The relevant lines, with file path.
- **Why it matters**: What could differ in the results because of it.
