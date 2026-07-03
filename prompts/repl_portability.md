# Replication Portability

Audit whether this replication package would actually run on a fresh machine. Do not evaluate methods or completeness of exhibits — other passes cover those. Imagine a data editor with a clean OS install and the README: where do they get stuck?

You have a survey (JSON) that inventories the package. Use the Read tool to open the master script, README, and any environment files before commenting.

## What to check

### Paths
Absolute paths ("C:/Users/...", "/home/..."), paths assuming a specific working directory that no script sets, OS-specific separators or shell commands that break cross-platform.

### Dependencies
Packages/libraries used in code but not listed in the README or an environment file; version pins missing where results are version-sensitive; user-written commands (Stata ado files, custom modules) neither shipped nor install-scripted.

### Execution order
A master script that runs everything, or a numbered order in the README; intermediate files consumed before the step that creates them; manual steps ("then open the file and…") that must be documented as such.

### Determinism
Missing seeds before random operations, parallelism that changes results, timestamps or locale-dependent formatting baked into outputs.

### Resources and access
Undocumented runtime or memory requirements likely to matter (flag only when the code suggests hours/large data); API keys, licenses, or credentials required but not mentioned; internet access required at runtime without notice.

## Severity threshold

CRITICAL: the run halts or produces different results on a fresh machine. MODERATE: the run succeeds only after undocumented fiddling. Cite the exact file and line for every path/dependency/seed finding.

## Output

Produce up to 12 numbered findings, most severe first. For each:

- **Title**: Specific header (e.g., "clean_data.R line 12 reads /Users/author/Dropbox/raw.csv")
- **Location**: File and line.
- **What breaks**: What happens on a fresh machine.
- **Fix**: The change or documentation line that resolves it.
