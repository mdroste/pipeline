# Security Review

Audit the codebase for security vulnerabilities. Do not comment on general bugs, style, or architecture — only weaknesses an attacker or malformed input could exploit.

You have a survey (JSON) that maps the codebase's structure and key elements. Use it to locate trust boundaries (input parsing, file access, subprocess calls, network handlers), and use the Read tool to open files before commenting on them. Trace data from where it enters to where it is used.

## What to check

### Input handling
Untrusted input reaching file paths (traversal), shell commands (injection), SQL/queries, deserializers, or format strings without validation.

### File and process safety
Paths built by concatenation instead of safe joins, missing canonicalization before access checks, temp files with predictable names or loose permissions, subprocesses inheriting more environment or privileges than needed.

### Secrets
Credentials, tokens, or keys in source, logs, error messages, or files written with world-readable permissions.

### Trust boundaries
Data from the network, disk, or another process treated as trusted without validation; security checks performed on a different value than the one later used (TOCTOU).

### Dependencies and configuration
Known-dangerous API usage (e.g. disabled TLS verification, weak crypto primitives), overly permissive CORS/CSP/permissions declarations.

## Severity threshold

Report only weaknesses with a plausible attack path or realistic malformed-input scenario (CRITICAL when exploitable for data access or code execution, MODERATE otherwise). Do not report theoretical hardening ideas with no concrete path.

## Output

Produce up to 10 numbered findings, most severe first. For each:

- **Title**: Specific header (e.g., "Archive extraction writes entries to attacker-controlled relative paths")
- **Location**: File path and line or function name.
- **The weakness**: What the code does with untrusted data, traced from entry point to sink.
- **Attack scenario**: A concrete input or action that exploits it, and what it yields.
- **Fix**: The narrowest change that closes it.
