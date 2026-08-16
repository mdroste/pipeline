---
name: pipeline-workflow
description: Create, validate, inspect, and run portable Pipeline CLI workflows from natural-language requests. Use when a user wants an on-the-fly multi-step document pipeline, parallel Claude/Codex review, a reusable Pipeline workflow JSON file, workflow validation or installation, or to process a document with a generated workflow.
---

# Pipeline Workflow

Turn the user's objective into a bounded portable workflow, make the Pipeline CLI validate and plan it, and run it only when the request includes execution.

## Locate the CLI

Prefer `pipeline-cli` when it is on PATH. In a Pipeline source checkout, use:

```bash
cargo run --locked --manifest-path gui/src-tauri/Cargo.toml --bin pipeline-cli -- <COMMAND>
```

Use one form consistently. Do not infer the wire format from memory when the CLI is available.

## Authoring procedure

1. Ask the CLI for the current starting point with `workflow template`. Ask for `workflow schema` only when a field or enum is uncertain.
2. Save a modified copy as JSON. Preserve `type: "profile"` and the current `schema_version` emitted by the CLI.
3. Translate the request into distinct analytical responsibilities. Make independent work Parallel; make aggregation, adjudication, and final writing Sequential.
4. Give each step only the artifacts it needs through `context.include`. Use `after` for order without data access and a `step` selector for upstream report access.
5. Select explicit `agents` when the user names providers. For Claude and ChatGPT/Codex comparison, use `agents: ["claude", "codex"]` on the relevant Parallel steps. Leave agents empty only when inheriting the user's Pipeline defaults is intended.
6. Ensure at least one enabled Sequential step exists and set `outputs.primary_step` to the terminal report step. Set `outputs.findings_step` only when that step emits Pipeline's findings shape.
7. Run `workflow validate <FILE> --json`. Repair every reported problem and repeat until it succeeds.
8. Before execution, run `check --workflow <FILE> --input <DOCUMENT> --json` (plus variables or named inputs). Read the normalized fingerprint, scheduler stages, work upper bounds, and dependency readiness. Do not proceed past a non-ready check.
9. If the user asked to process the document, run `run --workflow <FILE> --input <DOCUMENT>` and choose `--out` when a durable Markdown result is useful. If the user asked only to design a workflow, stop after validation and present the file.

Read [authoring.md](references/authoring.md) when designing nontrivial dependency graphs, fan-out, conditions, named inputs, or variables.

## Safety and persistence

- Treat `run --workflow`, `check --workflow`, and `batch --workflow` as ephemeral. They do not install or change the active desktop workflow.
- Use `workflow install <FILE>` only when the user explicitly asks to save/install the workflow. Installation creates a new profile rather than overwriting one.
- Never bypass `workflow validate`, even if the JSON looks correct. The CLI rejects unknown keys and checks graph, output, provider, tool, schema, and resource constraints.
- Do not add shell execution to a step. The portable format currently permits only the declared Pipeline tools; consult the emitted schema.
- Keep fan-out maxima and the number of provider passes proportionate to the request. Treat the preflight work bounds as a cost/risk signal.
- Preserve the generated JSON used for a run when the user asks for reproducibility. Pipeline also retains a normalized copy and its SHA-256 fingerprint in the run artifacts.

## Reporting

Briefly state the workflow name, step count, parallel/sequential structure, selected agents, validation fingerprint, and preflight readiness. After a run, provide the report location and run identifier. Surface validation or dependency failures verbatim enough to be actionable.
