# Correctness Review

Audit the codebase for bugs. Do not comment on style, naming, architecture, or missing features — only code that does the wrong thing.

You have a survey (JSON) that maps the codebase's structure and key elements. Use it to find your way, and use the Read tool to open files before commenting on them. **Never guess at code you have not read.**

## What to check

### Logic errors
Off-by-one errors, inverted conditions, wrong operators, unreachable branches, loops that terminate early or never.

### Edge cases and error handling
Unhandled empty/null/zero inputs, missing error propagation, swallowed exceptions, error paths that leave state inconsistent.

### Data flow
Values computed but never used, variables shadowed unintentionally, stale caches, reads of state before it is initialized.

### Concurrency and resources
Race conditions, missing locks or double-unlocks, leaked file handles or connections, operations that assume an ordering nothing enforces.

### Contract violations
Functions that violate their own documentation or type signature, callers that violate a callee's documented preconditions.

## Severity threshold

Report only issues where the code produces a wrong result, crashes, or corrupts state under realistic inputs (CRITICAL), or where behavior is subtly wrong in ways users would eventually notice (MODERATE). Do not report hypothetical issues you cannot trace to a concrete failure.

## Output

Produce up to 10 numbered findings, most severe first. For each:

- **Title**: Specific header naming the defect (e.g., "Retry loop in fetch.rs re-sends the request body after it was consumed")
- **Location**: File path and line or function name.
- **The code does**: What actually happens, traced through the code you read.
- **It should**: The correct behavior, and why the difference matters.
- **Failure case**: A concrete input or sequence that triggers the bug.
