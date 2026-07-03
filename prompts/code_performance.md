# Performance Review

Audit the codebase for performance problems. Do not comment on correctness, style, or architecture — other passes cover those. Only report costs that plausibly matter at this codebase's realistic scale; do not micro-optimize.

You have a survey (JSON) that maps the codebase's structure. Use it to find the hot paths — request handlers, loops over data, startup, anything called per-item or per-frame — and use the Read tool to open files before commenting on them. Establish the realistic input scale before judging anything.

## What to check

### Complexity cliffs
Nested loops or repeated lookups that make an operation quadratic in input size, linear scans where the data is repeatedly searched, per-item work that could be batched.

### Redundant work
The same file read, query run, or value computed multiple times per operation; caches that exist but are bypassed; work performed eagerly that is usually discarded.

### I/O patterns
Synchronous I/O on interactive paths, per-item round trips that could be one bulk call, unbuffered reads/writes in loops, chatty subprocess or network usage.

### Memory
Whole files or result sets loaded when streaming would do, unbounded caches and queues, large values cloned when a reference would serve.

### Startup and UI responsiveness
Blocking work on the UI or main thread, sequential initialization that could overlap, work done at startup that belongs on first use.

## Severity threshold

Report only issues where you can articulate the realistic scale at which they hurt (CRITICAL for hangs or unbounded growth; MODERATE for user-noticeable latency or waste). Every finding must state its estimated cost — "O(n²) over profile steps where n is at most 10" is a reason NOT to report. Do not report theoretical inefficiencies with no path to user impact.

## Output

Produce up to 10 numbered findings, most impactful first. For each:

- **Title**: Specific header (e.g., "Report list re-parses every history file on each keystroke of the search box")
- **Location**: File path and line or function name.
- **The cost**: What work is done, how often, and at what realistic scale it becomes noticeable.
- **Fix**: The narrowest change that removes the cost, and what it saves.
