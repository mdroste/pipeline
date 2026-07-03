# Data Provenance & Documentation

Audit the data documentation in this replication package. Do not evaluate code or methods — other passes cover those. The question: could a stranger identify, obtain, and legally use every dataset, and does the shipped data match its description?

You have a survey (JSON) that inventories the package. Use the Read tool to open the README's data section, any data availability statements, and the data files' headers (first rows) before commenting.

## What to check

### Source identification
Every dataset used by the code has a stated source: provider, dataset name, vintage/version or access date, and a URL/DOI or access procedure. Flag vague citations ("data from the Census") that don't pin down the file.

### Access and restrictions
For data not shipped in the package: the documented access path (application procedure, cost, expected wait), and confirmation that the code shows how the restricted input is used even when the input itself is absent. Flag restricted data that the README does not distinguish from missing data.

### Licenses and terms
Redistribution: shipped third-party data whose terms plausibly forbid redistribution (flag for the author to verify — do not make legal determinations), missing license statements for the package's own data.

### Description accuracy
Variables in the data files vs. any codebook or README description: names, units, coverage (years, regions), observation counts. Flag files whose contents do not match their description.

### PII
Columns that look like direct identifiers (names, precise addresses, national ID numbers) in shipped data — flag for review, do not quote example values.

## Severity threshold

CRITICAL: a dataset cannot be identified or obtained from the documentation, or shipped data contradicts its description. MODERATE: obtainable but with avoidable friction, or documentation gaps a data editor would return the package over.

## Output

Produce up to 10 numbered findings, most severe first. For each:

- **Title**: Specific header (e.g., "panel_main.dta has no stated source; README's data section lists only survey waves 1–3 of 5")
- **The dataset**: File name and how the code uses it.
- **The gap**: What a stranger cannot determine or obtain.
- **Fix**: The documentation or file change that closes it.

End with a data inventory table: each data file → source stated? access documented? shipped or restricted?
