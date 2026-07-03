# Concurrency & Resource Safety Review

Audit the codebase for concurrency defects and resource leaks. Do not comment on general logic bugs, style, or architecture — other passes cover those.

You have a survey (JSON) that maps the codebase's structure. Use it to locate the concurrent parts — threads, async tasks, locks, channels, signal handlers, shared caches — and use the Read tool to open files before commenting on them. Trace shared state to every place it is touched.

## What to check

### Data races and unsynchronized sharing
Shared mutable state reached from more than one thread or task without a lock or atomic; check-then-act sequences on shared values; iteration over collections another path mutates.

### Lock discipline
Inconsistent lock ordering across call paths (deadlock), locks held across await points or blocking I/O, missing unlocks on early-return or error paths, double-locking a non-reentrant lock.

### Async hazards
Futures or promises dropped without being awaited or cancelled, cancellation leaving state half-updated, blocking calls on an async executor, unbounded task spawning.

### Resource lifecycle
File handles, sockets, connections, or child processes opened without a guaranteed close path (especially on error), pools that never reclaim, listeners registered but never removed.

### Ordering assumptions
Code that assumes events, messages, or callbacks arrive in a particular order when nothing enforces it; time-of-check/time-of-use gaps on the filesystem.

## Severity threshold

Report only defects with a realistic interleaving or input that triggers them (CRITICAL for corruption, deadlock, or crash; MODERATE for leaks and degradation over time). For each finding you must describe the interleaving — "two threads could race" without the concrete sequence is not a finding.

## Output

Produce up to 10 numbered findings, most severe first. For each:

- **Title**: Specific header (e.g., "PIPELINE_RUNNING flag cleared before child PIDs are reaped, allowing overlapped runs")
- **Location**: File path and line or function name.
- **The hazard**: The shared state or resource, and every path that touches it.
- **Interleaving / trigger**: The concrete sequence of events that causes the failure.
- **Fix**: The narrowest change that removes the hazard.
