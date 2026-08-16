# Portable workflow authoring reference

The CLI-generated template and JSON Schema are authoritative. This reference explains the intended composition rules rather than duplicating the entire schema.

## Shape

A portable workflow is the GUI's profile export envelope:

```json
{
  "type": "profile",
  "schema_version": 8,
  "name": "Detailed theory review",
  "steps": [],
  "merge": {},
  "outputs": {},
  "extraction": {}
}
```

Always start from `pipeline-cli workflow template`; defaults and the current schema version may evolve.

## Step graph

- `phase: "parallel"` allows several ready steps to run in the same scheduler wave. A Parallel step with multiple `agents` produces independent provider reports; `merge.enabled` controls cross-agent consolidation.
- `phase: "sequential"` runs one ready synthesis step at a time and supports at most one explicit agent.
- `after: ["step_id"]` is order-only. It does not expose the earlier step's content.
- A context selector such as `{"kind":"step","step":"proofs","parts":["report"]}` both creates the dependency and exposes the report.
- Every runnable workflow needs an enabled Sequential step. Usually the final Sequential step is `outputs.primary_step`.

For a five-to-seven-step paper review, a useful decomposition is: orientation (host stage); independent proof audit; independent game-theory/mechanism audit; assumptions and counterexamples; economic interpretation; cross-review reconciliation; final prioritized synthesis. Several independent audits can occupy one scheduler wave even though they are separate logical steps.

## Context selectors

- Primary document: `{"kind":"primary","parts":["text","visuals"]}`. Add `structure` for layout-aware or block-aware work and `source` only when original files are needed.
- Orientation map: `{"kind":"survey"}`.
- Named input: `{"kind":"named_input","key":"appendix","parts":["text"]}`.
- Prior step: `{"kind":"step","step":"proof_audit","parts":["report"]}`. Supporting files may use `"files"` and an optional `glob`.

An empty `context.include` intentionally isolates the step.

## Providers, models, and tools

Portable provider names are `claude`, `codex`, `antigravity`, and `local`. Multiple agents are supported only on Parallel steps. Prefer provider defaults unless the user requests a model policy; query `workflow schema` for the current model-selection shape.

The only currently authorable optional tool is `WebSearch`. Add it only when external verification is part of the user's request. Reading selected artifacts and writing step-owned files are derived capabilities, not entries in `tools`.

## Variables, named inputs, fan-out, and conditions

- Declare prompt variables under `variables` and reference them as `{var:key}`. Pass values with repeated `--var key=value` options.
- Declare additional inputs under `extraction.extra_inputs`, then pass them with `--extra-input key=path`.
- `for_each` is Parallel-only and maps a bounded glob over folder inputs. Keep `max` small and review preflight work bounds.
- `run_if` supports deterministic `output_matches` and `survey_path` guards. It is routing, not a general expression language.

## Validation and execution sequence

```bash
pipeline-cli workflow validate workflow.json --json
pipeline-cli check --workflow workflow.json --input paper.pdf --json
pipeline-cli run --workflow workflow.json --input paper.pdf --out review.md
```

Use `--interpret-as latex-project` for a LaTeX project folder or `--interpret-as source-tree` for a browsable source tree. Omit `--input` only when `extraction.input_mode` is `none`.
