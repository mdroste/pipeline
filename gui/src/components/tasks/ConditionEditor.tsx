// Typed pickers for chain bindings and conditions. Conditions read back as a
// sentence; the raw definition stays visible in the builder's advanced view.

import { useMemo } from "react";
import type {
  Binding,
  Chain,
  Condition,
  Json,
  Step,
} from "../../lib/taskClient";
import { describeCondition, OUTPUT_FIELDS, stepsBefore } from "./language";

interface OutputOption {
  value: string;
  label: string;
  binding: Binding;
}

export function outputOptions(chain: Chain, targetStepId: string | null) {
  const prior = targetStepId
    ? stepsBefore(chain, targetStepId)
    : allSteps(chain.steps);
  const options: OutputOption[] = [];
  for (const step of prior) {
    const fields = OUTPUT_FIELDS[step.kind];
    if (!fields) continue;
    for (const field of fields) {
      options.push({
        value: `output:${step.id}:${field.pointer}`,
        label: `${step.label} — ${field.label}`,
        binding: {
          kind: "output",
          step: step.id,
          ...(field.pointer ? { pointer: field.pointer } : {}),
        },
      });
    }
  }
  return options;
}

function allSteps(steps: Step[], into: Step[] = []): Step[] {
  for (const step of steps) {
    into.push(step);
    if (step.kind === "if") {
      allSteps(step.thenSteps, into);
      allSteps(step.elseSteps ?? [], into);
    } else if (
      step.kind === "repeat" ||
      step.kind === "while" ||
      step.kind === "forEach"
    )
      allSteps(step.steps, into);
    else if (step.kind === "parallel")
      step.branches.forEach((branch) => allSteps(branch, into));
    else if (step.kind === "chain") allSteps(step.chain.steps, into);
  }
  return into;
}

function bindingKey(binding: Binding): string {
  if (binding.kind === "output")
    return `output:${binding.step}:${binding.pointer ?? ""}`;
  if (binding.kind === "input") return "input";
  if (binding.kind === "firstAvailable") return "first";
  return "literal";
}

export function BindingPicker({
  chain,
  targetStepId,
  value,
  onChange,
  label,
}: {
  chain: Chain;
  targetStepId: string | null;
  value: Binding;
  onChange: (binding: Binding) => void;
  label: string;
}) {
  const options = useMemo(
    () => outputOptions(chain, targetStepId),
    [chain, targetStepId],
  );
  const key = bindingKey(value);
  const known = options.some((option) => option.value === key);
  return (
    <span className="task-binding">
      <select
        aria-label={label}
        value={known ? key : key.startsWith("output:") ? "output-custom" : key}
        onChange={(event) => {
          const next = event.target.value;
          if (next === "input") onChange({ kind: "input", key: "value" });
          else if (next === "literal")
            onChange({ kind: "literal", value: { path: "" } });
          else if (next === "first")
            onChange({
              kind: "firstAvailable",
              values: [
                options[0]?.binding ?? { kind: "input", key: "value" },
                options[1]?.binding ?? { kind: "input", key: "value" },
              ],
            });
          else {
            const option = options.find((o) => o.value === next);
            if (option) onChange(option.binding);
          }
        }}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
        {!known && key.startsWith("output:") && (
          <option value="output-custom">
            Exact reference ({key.slice(7)})
          </option>
        )}
        <option value="input">A named input…</option>
        <option value="literal">A file path…</option>
        <option value="first">First available of…</option>
      </select>
      {value.kind === "input" && (
        <input
          aria-label={`${label} input key`}
          value={value.key}
          onChange={(event) =>
            onChange({ kind: "input", key: event.target.value })
          }
          placeholder="input key"
        />
      )}
      {value.kind === "literal" && (
        <input
          aria-label={`${label} file path`}
          value={
            value.value &&
            typeof value.value === "object" &&
            "path" in value.value
              ? String((value.value as { path: unknown }).path)
              : JSON.stringify(value.value)
          }
          onChange={(event) =>
            onChange({ kind: "literal", value: { path: event.target.value } })
          }
          placeholder="/path/to/file"
        />
      )}
      {value.kind === "firstAvailable" && (
        <span className="task-binding-list">
          {value.values.map((inner, index) => (
            <span key={index} className="task-binding-list-row">
              <BindingPicker
                chain={chain}
                targetStepId={targetStepId}
                value={inner}
                onChange={(next) => {
                  const values = [...value.values];
                  values[index] = next;
                  onChange({ kind: "firstAvailable", values });
                }}
                label={`${label} choice ${index + 1}`}
              />
              {value.values.length > 2 && (
                <button
                  type="button"
                  aria-label={`Remove ${label} choice ${index + 1}`}
                  onClick={() =>
                    onChange({
                      kind: "firstAvailable",
                      values: value.values.filter((_, i) => i !== index),
                    })
                  }
                >
                  ×
                </button>
              )}
            </span>
          ))}
          <button
            type="button"
            onClick={() =>
              onChange({
                kind: "firstAvailable",
                values: [
                  ...value.values,
                  options[0]?.binding ?? { kind: "input", key: "value" },
                ],
              })
            }
          >
            + Fallback
          </button>
        </span>
      )}
    </span>
  );
}

// --- Condition model -------------------------------------------------------

type Comparison = "isTrue" | "isFalse" | "equals" | "below" | "exists";

interface Leaf {
  negate: boolean;
  binding: Binding;
  comparison: Comparison;
  value: string;
}

interface EditableModel {
  combine: "all" | "any";
  leaves: Leaf[];
}

function leafFromCondition(condition: Condition): Leaf | null {
  let negate = false;
  let inner = condition;
  if (inner.op === "not") {
    negate = true;
    inner = inner.condition;
    if (inner.op === "not") return null;
  }
  if (inner.op === "equals") {
    if (inner.right === true)
      return { negate, binding: inner.left, comparison: "isTrue", value: "" };
    if (inner.right === false)
      return { negate, binding: inner.left, comparison: "isFalse", value: "" };
    return {
      negate,
      binding: inner.left,
      comparison: "equals",
      value: JSON.stringify(inner.right),
    };
  }
  if (inner.op === "lessThan")
    return {
      negate,
      binding: inner.left,
      comparison: "below",
      value: String(inner.right),
    };
  if (inner.op === "exists")
    return { negate, binding: inner.value, comparison: "exists", value: "" };
  return null;
}

export function modelFromCondition(condition: Condition): EditableModel | null {
  const single = leafFromCondition(condition);
  if (single) return { combine: "all", leaves: [single] };
  if (condition.op === "all" || condition.op === "any") {
    const leaves = condition.conditions.map(leafFromCondition);
    if (leaves.every((leaf): leaf is Leaf => leaf !== null))
      return { combine: condition.op, leaves };
  }
  return null;
}

function leafToCondition(leaf: Leaf): Condition {
  let parsed: Json = leaf.value;
  if (leaf.comparison === "equals") {
    try {
      parsed = JSON.parse(leaf.value) as Json;
    } catch {
      parsed = leaf.value;
    }
  }
  const inner: Condition =
    leaf.comparison === "isTrue"
      ? { op: "equals", left: leaf.binding, right: true }
      : leaf.comparison === "isFalse"
        ? { op: "equals", left: leaf.binding, right: false }
        : leaf.comparison === "equals"
          ? { op: "equals", left: leaf.binding, right: parsed }
          : leaf.comparison === "below"
            ? {
                op: "lessThan",
                left: leaf.binding,
                right: Number(leaf.value) || 0,
              }
            : { op: "exists", value: leaf.binding };
  return leaf.negate ? { op: "not", condition: inner } : inner;
}

export function modelToCondition(model: EditableModel): Condition {
  if (model.leaves.length === 1 && model.combine === "all")
    return leafToCondition(model.leaves[0]);
  return {
    op: model.combine,
    conditions: model.leaves.map(leafToCondition),
  };
}

export function ConditionEditor({
  chain,
  targetStepId,
  value,
  onChange,
  label,
}: {
  chain: Chain;
  targetStepId: string | null;
  value: Condition;
  onChange: (condition: Condition) => void;
  label: string;
}) {
  const model = modelFromCondition(value);
  if (!model) {
    // Structures beyond the picker (nested groups) stay intact; edit as JSON.
    return (
      <p className="task-muted">
        {describeCondition(chain, value)} — this condition uses nesting the
        picker does not edit; change it in the definition view.
      </p>
    );
  }
  const update = (next: EditableModel) => onChange(modelToCondition(next));
  return (
    <div className="task-condition" role="group" aria-label={label}>
      <p className="task-condition-sentence">
        {describeCondition(chain, value)}
      </p>
      {model.leaves.map((leaf, index) => (
        <div key={index} className="task-condition-row">
          {index > 0 && (
            <select
              aria-label="Combine conditions"
              value={model.combine}
              onChange={(event) =>
                update({
                  ...model,
                  combine: event.target.value as "all" | "any",
                })
              }
            >
              <option value="all">and</option>
              <option value="any">or</option>
            </select>
          )}
          <BindingPicker
            chain={chain}
            targetStepId={targetStepId}
            value={leaf.binding}
            onChange={(binding) =>
              update({
                ...model,
                leaves: model.leaves.map((l, i) =>
                  i === index ? { ...l, binding } : l,
                ),
              })
            }
            label={`${label} value ${index + 1}`}
          />
          <select
            aria-label={`${label} comparison ${index + 1}`}
            value={leaf.negate ? `not-${leaf.comparison}` : leaf.comparison}
            onChange={(event) => {
              const raw = event.target.value;
              const negate = raw.startsWith("not-");
              const comparison = raw.replace(/^not-/, "") as Comparison;
              update({
                ...model,
                leaves: model.leaves.map((l, i) =>
                  i === index ? { ...l, negate, comparison } : l,
                ),
              });
            }}
          >
            <option value="isTrue">is true</option>
            <option value="isFalse">is false</option>
            <option value="equals">equals</option>
            <option value="below">is below</option>
            <option value="exists">exists</option>
            <option value="not-exists">does not exist</option>
          </select>
          {(leaf.comparison === "equals" || leaf.comparison === "below") && (
            <input
              aria-label={`${label} comparison value ${index + 1}`}
              value={leaf.value}
              onChange={(event) =>
                update({
                  ...model,
                  leaves: model.leaves.map((l, i) =>
                    i === index ? { ...l, value: event.target.value } : l,
                  ),
                })
              }
              placeholder={leaf.comparison === "below" ? "1" : "value"}
            />
          )}
          {model.leaves.length > 1 && (
            <button
              type="button"
              aria-label={`Remove condition ${index + 1}`}
              onClick={() =>
                update({
                  ...model,
                  leaves: model.leaves.filter((_, i) => i !== index),
                })
              }
            >
              ×
            </button>
          )}
        </div>
      ))}
      <button
        type="button"
        className="task-condition-add"
        onClick={() =>
          update({
            ...model,
            leaves: [
              ...model.leaves,
              {
                negate: false,
                binding:
                  outputOptions(chain, targetStepId)[0]?.binding ??
                  ({ kind: "input", key: "value" } as Binding),
                comparison: "exists",
                value: "",
              },
            ],
          })
        }
      >
        + Condition
      </button>
      <p className="task-muted">
        If a value is not known yet, the automation pauses for your attention
        instead of guessing.
      </p>
    </div>
  );
}
