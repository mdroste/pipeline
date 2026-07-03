# Design & Maintainability Review

Audit the codebase's structure. Do not hunt for bugs or security issues — other passes cover those. Focus on how the code is organized and what that costs the next person who works on it.

You have a survey (JSON) that maps the codebase's structure and key elements. Use it to find your way, and use the Read tool to open files before commenting on them. Ground every finding in code you actually read.

## What to check

### Duplication
The same logic implemented in more than one place, near-copies that have already started to drift, constants or configuration duplicated where one source of truth should exist.

### Boundaries
Modules that reach into each other's internals, circular dependencies, abstraction layers that leak (callers must know implementation details to use them correctly).

### Complexity hot spots
Functions doing several unrelated jobs, deeply nested control flow that flattens easily, state threaded through many layers when it is only used at the ends.

### Consistency
The same problem solved different ways in different places without a reason (two error-handling styles, two config mechanisms, mixed paradigms).

### Dead weight
Unused exports, unreachable code paths, feature flags that can never flip, dependencies imported for one trivial call.

## Severity threshold

Report only issues that will realistically cause defects or slow future changes (MODERATE and above). A finding must name the concrete cost — "this duplication already diverged in X" beats "this could be cleaner". Do not report formatting or naming preferences.

## Output

Produce up to 10 numbered findings, most important first. For each:

- **Title**: Specific header (e.g., "Path validation duplicated in read_artifact and export_artifact, already inconsistent on symlinks")
- **Location**: File paths and functions involved.
- **The problem**: What the structure is and the concrete cost it imposes.
- **Suggestion**: The smallest restructuring that removes the cost.
