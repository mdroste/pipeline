# Error Handling & Edge Case Review

Audit how the codebase behaves when things go wrong. Do not comment on happy-path logic, style, or architecture — other passes cover those. Your subject is exclusively the failure paths.

You have a survey (JSON) that maps the codebase's structure. Use it to find the fallible operations — I/O, parsing, network calls, subprocess spawns, arithmetic on external values — and use the Read tool to open files before commenting on them. For each fallible operation, trace what actually happens when it fails.

## What to check

### Swallowed and flattened errors
Errors caught and ignored, logged and dropped when the caller needed to know, or collapsed into a generic message that destroys the information needed to act on them.

### Partial-state failures
Multi-step operations that fail midway and leave files, in-memory state, or external systems half-updated with no rollback or recovery; cleanup code that itself can fail and mask the original error.

### Edge inputs
Empty collections, zero and negative values, extremely large inputs, unicode boundaries, paths with spaces or non-ASCII characters, missing optional fields — anywhere the code indexes, slices, divides, or assumes non-emptiness.

### Error-path correctness
Retry logic that retries non-retryable failures or duplicates side effects, timeouts that fire but leave work running, fallbacks that silently produce lower-quality results with no signal to the user.

### User-facing failure quality
Failures that surface as hangs, blank output, or misleading messages rather than an actionable error.

## Severity threshold

Report only failure paths reachable under realistic conditions (CRITICAL when they corrupt state or mislead the user about success; MODERATE when they degrade or confuse). Every finding needs the concrete failing condition. Do not report "could add more validation" style hardening.

## Output

Produce up to 10 numbered findings, most severe first. For each:

- **Title**: Specific header (e.g., "Profile save failure after temp-file write leaves .tmp files accumulating silently")
- **Location**: File path and line or function name.
- **Failing condition**: What goes wrong and how it realistically occurs.
- **Observed behavior**: What the code does then, traced through the error path.
- **Expected behavior**: What it should do instead.
