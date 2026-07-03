# Test Coverage & Quality Review

Audit the test suite. Do not review the production code itself — other passes cover that. Your subject is whether the tests would actually catch the bugs that matter.

You have a survey (JSON) that maps the codebase's structure. Use it to pair up production modules with their tests, and use the Read tool to open both before commenting. A module's risk — not its size — determines how much coverage it deserves.

## What to check

### Coverage gaps that matter
High-risk logic with no tests at all: parsing of external input, money/date/unit arithmetic, permission checks, state machines, error recovery. Name the specific untested behaviors, not just the module.

### Tests that can't fail
Assertions that are tautologically true, tests that mock the very behavior they claim to verify, snapshot tests nobody could review, tests that pass even when the underlying logic is inverted (check by reading the assertion against the code).

### Missing failure-path tests
Suites that only exercise success: no tests for invalid input, error propagation, boundary values, or concurrent access on code that handles them.

### Brittleness
Tests coupled to incidental details (exact strings, ordering, timing/sleeps) that will break on harmless changes and train people to ignore failures; hidden inter-test dependencies via shared state.

### Fixture honesty
Mocks and fixtures that drift from the real data shapes they imitate; test doubles for external services that accept inputs the real service would reject.

## Severity threshold

Report only gaps and defects where a realistic regression would slip through (CRITICAL for untested high-risk logic, MODERATE for weak or brittle tests on important paths). For each gap, name the concrete bug the missing test would have caught. Do not report coverage percentages or demand tests for trivial code.

## Output

Produce up to 10 numbered findings, most important first. For each:

- **Title**: Specific header (e.g., "Path-validation logic in runs.rs has no test for symlink escape, its main threat")
- **Location**: The production code at risk and the test file (or its absence).
- **The gap or defect**: What is untested or what the existing test fails to verify.
- **Regression scenario**: A realistic code change that would ship broken because of this gap.
- **Suggested test**: One or two sentences describing the test that closes it.
