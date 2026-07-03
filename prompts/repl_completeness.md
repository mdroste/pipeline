# Replication Package Completeness

Audit whether this replication package contains everything needed to reproduce the paper's exhibits. Do not evaluate the code's quality or the paper's methods — other passes cover those.

You have a survey (JSON) that inventories the package. Locate the manuscript (PDF or source) if it is included, the README, and the code and data directories. Use the Read tool to open files before commenting. If the manuscript is not in the package, work from the README's list of exhibits and say so.

## What to check

### Exhibit-to-script mapping
For every table and figure in the paper (or README exhibit list): identify the script that produces it. Flag exhibits with no generating code, and scripts whose output corresponds to no exhibit (possible stale or renamed results).

### In-text numbers
Numbers cited in the text but not in any table (elasticities, p-values, sample counts in the abstract) also need a generating location. Flag ones you cannot trace.

### README coverage
The README must state: what to run, in what order, expected runtime, software and version requirements, and which exhibits each step produces. Flag each missing element specifically.

### File inventory
Data files referenced by code but absent from the package (distinguish deliberately-excluded restricted data — which must be documented — from silent omissions); code files referenced (imported, sourced, called) but missing; orphaned files the README never mentions.

## Severity threshold

CRITICAL: an exhibit cannot be reproduced from what ships in the package. MODERATE: reproduction is possible but requires guesswork the README should remove. Do not flag stylistic README preferences.

## Output

Produce up to 12 numbered findings, most severe first. For each:

- **Title**: Specific header (e.g., "Table 4 has no generating script; closest candidate writes different column headers")
- **Evidence**: The exhibit or claim, and where you looked (files, README sections).
- **What's missing**: Exactly what a replicator would need that isn't there.

End with a short mapping table: each exhibit → generating script (or "NOT FOUND").
