// Structural outline editor for automation chains. Steps are sentences with
// inline editors; add, reorder, nest, and delete without touching JSON. The
// exact definition remains visible in the builder's advanced view.

import { useState } from "react";
import type { Binding, Chain, Step } from "../../lib/taskClient";
import {
  allStepIds,
  blankStep,
  describeStep,
  STEP_KIND_LABELS,
} from "./language";
import { BindingPicker, ConditionEditor } from "./ConditionEditor";

const ADD_GROUPS: [string, Step["kind"][]][] = [
  [
    "Do",
    ["workspace", "review", "snapshot", "check", "literatureLookup", "deliver"],
  ],
  ["Wait", ["input", "delay", "until"]],
  ["Logic", ["if", "repeat", "while", "parallel", "forEach", "chain"]],
];

export interface ProfileChoice {
  id: string;
  name: string;
}

interface Shared {
  chain: Chain;
  profiles: ProfileChoice[];
  expanded: Set<string>;
  toggle: (id: string, open?: boolean) => void;
}

function AddStepMenu({
  onAdd,
  label,
}: {
  onAdd: (kind: Step["kind"]) => void;
  label: string;
}) {
  return (
    <select
      className="task-add-step"
      aria-label={label}
      value=""
      onChange={(event) => {
        if (event.target.value) onAdd(event.target.value as Step["kind"]);
        event.target.value = "";
      }}
    >
      <option value="">+ Add step…</option>
      {ADD_GROUPS.map(([group, kinds]) => (
        <optgroup key={group} label={group}>
          {kinds.map((kind) => (
            <option key={kind} value={kind}>
              {STEP_KIND_LABELS[kind]}
            </option>
          ))}
        </optgroup>
      ))}
    </select>
  );
}

function BindingField({
  shared,
  step,
  label,
  value,
  onChange,
}: {
  shared: Shared;
  step: Step;
  label: string;
  value: Binding;
  onChange: (binding: Binding) => void;
}) {
  return (
    <label className="task-field">
      {label}
      <BindingPicker
        chain={shared.chain}
        targetStepId={step.id}
        value={value}
        onChange={onChange}
        label={`${label} for ${step.label}`}
      />
    </label>
  );
}

function StepEditor({
  shared,
  step,
  onChange,
}: {
  shared: Shared;
  step: Step;
  onChange: (step: Step) => void;
}) {
  const set = (patch: Partial<Step>) => onChange({ ...step, ...patch } as Step);
  return (
    <div className="task-step-editor">
      <label className="task-field">
        Step name
        <input
          value={step.label}
          onChange={(event) => set({ label: event.target.value })}
        />
      </label>
      {step.kind === "workspace" && (
        <label className="task-field">
          Prompt
          <textarea
            rows={3}
            value={step.prompt}
            onChange={(event) => set({ prompt: event.target.value })}
            placeholder="What should the assistant do? Reference earlier results with the pickers below or {{output:stepId#/pointer}}."
          />
        </label>
      )}
      {step.kind === "review" && (
        <>
          <BindingField
            shared={shared}
            step={step}
            label="Review this"
            value={step.input}
            onChange={(input) => set({ input })}
          />
          <label className="task-field">
            Review workflow
            <select
              value={step.profileId}
              onChange={(event) => set({ profileId: event.target.value })}
            >
              <option value="">Select a review workflow</option>
              {shared.profiles.map((profile) => (
                <option key={profile.id} value={profile.id}>
                  {profile.name}
                </option>
              ))}
            </select>
          </label>
        </>
      )}
      {step.kind === "snapshot" && (
        <>
          <BindingField
            shared={shared}
            step={step}
            label="Save this"
            value={step.input}
            onChange={(input) => set({ input })}
          />
          <div className="task-form-row">
            <label className="task-field">
              File name
              <input
                value={step.filename}
                onChange={(event) => set({ filename: event.target.value })}
              />
            </label>
            <label className="task-check">
              <input
                type="checkbox"
                checked={step.requireChange ?? false}
                onChange={(event) =>
                  set({ requireChange: event.target.checked })
                }
              />
              Only if it changed
            </label>
          </div>
        </>
      )}
      {step.kind === "deliver" && (
        <BindingField
          shared={shared}
          step={step}
          label="Deliver this"
          value={step.input}
          onChange={(input) => set({ input })}
        />
      )}
      {step.kind === "check" && (
        <label className="task-field">
          Check id
          <input
            value={step.profileId}
            onChange={(event) => set({ profileId: event.target.value })}
          />
        </label>
      )}
      {step.kind === "capturedCheck" && (
        <label className="task-field">
          Captured plan id
          <input
            value={step.planId}
            onChange={(event) => set({ planId: event.target.value })}
          />
        </label>
      )}
      {step.kind === "literatureLookup" && (
        <label className="task-field">
          Search for
          <input
            value={step.query}
            onChange={(event) => set({ query: event.target.value })}
          />
        </label>
      )}
      {step.kind === "input" && (
        <label className="task-field">
          Question for you
          <textarea
            rows={2}
            value={step.prompt}
            onChange={(event) => set({ prompt: event.target.value })}
            placeholder="What input should Pipeline wait for?"
          />
        </label>
      )}
      {step.kind === "delay" && (
        <label className="task-field">
          Wait (minutes)
          <input
            type="number"
            min={1}
            value={Math.round(step.seconds / 60)}
            onChange={(event) =>
              set({ seconds: Math.max(1, Number(event.target.value)) * 60 })
            }
          />
        </label>
      )}
      {step.kind === "until" && (
        <label className="task-field">
          Wait until
          <input
            type="datetime-local"
            value={new Date(
              step.at * 1000 - new Date().getTimezoneOffset() * 60000,
            )
              .toISOString()
              .slice(0, 16)}
            onChange={(event) => {
              const at = Math.floor(
                new Date(event.target.value).getTime() / 1000,
              );
              if (Number.isFinite(at)) set({ at });
            }}
          />
        </label>
      )}
      {step.kind === "if" && (
        <ConditionEditor
          chain={shared.chain}
          targetStepId={step.id}
          value={step.condition}
          onChange={(condition) => set({ condition })}
          label={`Condition for ${step.label}`}
        />
      )}
      {(step.kind === "repeat" || step.kind === "while") && (
        <>
          <label className="task-field">
            Maximum repetitions
            <input
              type="number"
              min={1}
              max={32}
              value={step.maxIterations}
              onChange={(event) =>
                set({
                  maxIterations: Math.min(
                    32,
                    Math.max(1, Number(event.target.value)),
                  ),
                })
              }
            />
          </label>
          <ConditionEditor
            chain={shared.chain}
            targetStepId={step.id}
            value={step.kind === "repeat" ? step.until : step.condition}
            onChange={(condition) =>
              step.kind === "repeat"
                ? set({ until: condition })
                : set({ condition })
            }
            label={
              step.kind === "repeat"
                ? `Stop when (for ${step.label})`
                : `Keep going while (for ${step.label})`
            }
          />
        </>
      )}
      {step.kind === "forEach" && (
        <>
          <BindingField
            shared={shared}
            step={step}
            label="For each item in"
            value={step.input}
            onChange={(input) => set({ input })}
          />
          <label className="task-field">
            Maximum items
            <input
              type="number"
              min={1}
              max={64}
              value={step.maxItems}
              onChange={(event) =>
                set({
                  maxItems: Math.min(
                    64,
                    Math.max(1, Number(event.target.value)),
                  ),
                })
              }
            />
          </label>
        </>
      )}
      {step.kind === "chain" && (
        <label className="task-field">
          Group name
          <input
            value={step.chain.name}
            onChange={(event) =>
              set({ chain: { ...step.chain, name: event.target.value } })
            }
          />
        </label>
      )}
    </div>
  );
}

function NestedLists({
  shared,
  step,
  onChange,
}: {
  shared: Shared;
  step: Step;
  onChange: (step: Step) => void;
}) {
  if (step.kind === "if")
    return (
      <>
        <StepList
          shared={shared}
          steps={step.thenSteps}
          onChange={(thenSteps) => onChange({ ...step, thenSteps })}
          label={`Steps when ${step.label} holds`}
        />
        {step.elseSteps ? (
          <>
            <p className="task-branch-label">Otherwise</p>
            <StepList
              shared={shared}
              steps={step.elseSteps}
              onChange={(elseSteps) => onChange({ ...step, elseSteps })}
              label={`Steps otherwise for ${step.label}`}
            />
          </>
        ) : (
          <button
            type="button"
            className="task-condition-add"
            onClick={() => onChange({ ...step, elseSteps: [] })}
          >
            + Otherwise branch
          </button>
        )}
      </>
    );
  if (
    step.kind === "repeat" ||
    step.kind === "while" ||
    step.kind === "forEach"
  )
    return (
      <StepList
        shared={shared}
        steps={step.steps}
        onChange={(steps) => onChange({ ...step, steps })}
        label={`Steps inside ${step.label}`}
      />
    );
  if (step.kind === "parallel")
    return (
      <>
        {step.branches.map((branch, index) => (
          <div key={index} className="task-branch">
            <p className="task-branch-label">
              Branch {index + 1}
              {step.branches.length > 1 && (
                <button
                  type="button"
                  aria-label={`Remove branch ${index + 1}`}
                  onClick={() =>
                    onChange({
                      ...step,
                      branches: step.branches.filter((_, i) => i !== index),
                    })
                  }
                >
                  ×
                </button>
              )}
            </p>
            <StepList
              shared={shared}
              steps={branch}
              onChange={(next) =>
                onChange({
                  ...step,
                  branches: step.branches.map((b, i) =>
                    i === index ? next : b,
                  ),
                })
              }
              label={`Steps in branch ${index + 1} of ${step.label}`}
            />
          </div>
        ))}
        {step.branches.length < 8 && (
          <button
            type="button"
            className="task-condition-add"
            onClick={() =>
              onChange({ ...step, branches: [...step.branches, []] })
            }
          >
            + Branch
          </button>
        )}
      </>
    );
  if (step.kind === "chain")
    return (
      <StepList
        shared={shared}
        steps={step.chain.steps}
        onChange={(steps) =>
          onChange({ ...step, chain: { ...step.chain, steps } })
        }
        label={`Steps inside ${step.chain.name}`}
      />
    );
  return null;
}

const CONTAINER_KINDS = new Set<Step["kind"]>([
  "if",
  "repeat",
  "while",
  "parallel",
  "forEach",
  "chain",
]);

function StepList({
  shared,
  steps,
  onChange,
  label,
}: {
  shared: Shared;
  steps: Step[];
  onChange: (steps: Step[]) => void;
  label: string;
}) {
  return (
    <ol className="task-outline task-outline--editable" aria-label={label}>
      {steps.map((step, index) => {
        const open = shared.expanded.has(step.id);
        return (
          <li key={step.id}>
            <div className="task-outline-row">
              <span className="task-step-number">{index + 1}</span>
              <button
                type="button"
                className="task-outline-sentence"
                aria-expanded={open}
                onClick={() => shared.toggle(step.id)}
                title={open ? "Collapse step" : "Edit step"}
              >
                <span className="task-outline-label">{step.label}</span>
                <small>
                  {describeStep(
                    shared.chain,
                    step,
                    (id) =>
                      shared.profiles.find((profile) => profile.id === id)
                        ?.name ?? id,
                  )}
                </small>
              </button>
              <span className="task-outline-tools">
                <button
                  type="button"
                  aria-label={`Move ${step.label} up`}
                  disabled={index === 0}
                  onClick={() => {
                    const next = [...steps];
                    [next[index - 1], next[index]] = [
                      next[index],
                      next[index - 1],
                    ];
                    onChange(next);
                  }}
                >
                  ↑
                </button>
                <button
                  type="button"
                  aria-label={`Move ${step.label} down`}
                  disabled={index === steps.length - 1}
                  onClick={() => {
                    const next = [...steps];
                    [next[index], next[index + 1]] = [
                      next[index + 1],
                      next[index],
                    ];
                    onChange(next);
                  }}
                >
                  ↓
                </button>
                <button
                  type="button"
                  aria-label={`Remove ${step.label}`}
                  onClick={() => onChange(steps.filter((_, i) => i !== index))}
                >
                  ×
                </button>
              </span>
            </div>
            {open && (
              <StepEditor
                shared={shared}
                step={step}
                onChange={(next) =>
                  onChange(steps.map((s, i) => (i === index ? next : s)))
                }
              />
            )}
            {CONTAINER_KINDS.has(step.kind) && (
              <NestedLists
                shared={shared}
                step={step}
                onChange={(next) =>
                  onChange(steps.map((s, i) => (i === index ? next : s)))
                }
              />
            )}
          </li>
        );
      })}
      <li className="task-outline-add">
        <AddStepMenu
          label={`Add to ${label}`}
          onAdd={(kind) => {
            const step = blankStep(kind, allStepIds(shared.chain.steps));
            onChange([...steps, step]);
            shared.toggle(step.id, true);
          }}
        />
      </li>
    </ol>
  );
}

export default function OutlineEditor({
  chain,
  onChange,
  profiles,
}: {
  chain: Chain;
  onChange: (chain: Chain) => void;
  profiles: ProfileChoice[];
}) {
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const shared: Shared = {
    chain,
    profiles,
    expanded,
    toggle: (id, open) =>
      setExpanded((current) => {
        const next = new Set(current);
        const shouldOpen = open ?? !next.has(id);
        if (shouldOpen) next.add(id);
        else next.delete(id);
        return next;
      }),
  };
  return (
    <div className="task-outline-editor">
      <StepList
        shared={shared}
        steps={chain.steps}
        onChange={(steps) => onChange({ ...chain, steps })}
        label="Automation steps"
      />
    </div>
  );
}
