// Plain-language vocabulary for automation chains. Every engine step kind has
// one user-facing name and one sentence form; the outline editor, previews,
// and run detail all render from here so internal kind ids never reach copy.

import type { Binding, Chain, Condition, Step } from "../../lib/taskClient";

export const STEP_KIND_LABELS: Record<Step["kind"], string> = {
  workspace: "Ask the assistant",
  review: "Run a review",
  snapshot: "Save a version",
  check: "Run a check",
  capturedCheck: "Run a captured check",
  literatureLookup: "Look up literature",
  deliver: "Deliver a result",
  input: "Wait for me",
  delay: "Wait",
  until: "Wait until a time",
  if: "Only if…",
  repeat: "Repeat until…",
  while: "Repeat while…",
  parallel: "Side by side",
  forEach: "For each item",
  chain: "Group of steps",
};

/** Documented output fields a later step can bind to, per producing kind. */
export const OUTPUT_FIELDS: Partial<
  Record<Step["kind"], { pointer: string; label: string }[]>
> = {
  review: [
    { pointer: "/complete", label: "review finished" },
    { pointer: "/highPriorityCount", label: "high-priority findings" },
    { pointer: "/unknownPriorityCount", label: "unclassified findings" },
    { pointer: "/paperText", label: "reviewed paper text" },
    { pointer: "/report", label: "review report" },
    { pointer: "/reviewedArtifact", label: "reviewed file" },
    { pointer: "/runId", label: "review run id" },
  ],
  workspace: [{ pointer: "/text", label: "assistant reply text" }],
  snapshot: [{ pointer: "", label: "saved file" }],
  input: [{ pointer: "", label: "provided input" }],
  literatureLookup: [{ pointer: "", label: "lookup results" }],
  check: [{ pointer: "", label: "check result" }],
  capturedCheck: [{ pointer: "", label: "check result" }],
};

/** Steps visible to a binding at `path` (all steps before it, at any level). */
export function stepsBefore(chain: Chain, targetId: string): Step[] {
  const out: Step[] = [];
  let found = false;
  const walk = (steps: Step[]) => {
    for (const step of steps) {
      if (step.id === targetId) {
        found = true;
        return;
      }
      out.push(step);
      for (const nested of childLists(step)) {
        walk(nested);
        if (found) return;
      }
      if (found) return;
    }
  };
  walk(chain.steps);
  return found ? out : out;
}

export function childLists(step: Step): Step[][] {
  switch (step.kind) {
    case "if":
      return step.elseSteps?.length
        ? [step.thenSteps, step.elseSteps]
        : [step.thenSteps];
    case "repeat":
    case "while":
    case "forEach":
      return [step.steps];
    case "parallel":
      return step.branches;
    case "chain":
      return [step.chain.steps];
    default:
      return [];
  }
}

export function findStep(steps: Step[], id: string): Step | null {
  for (const step of steps) {
    if (step.id === id) return step;
    for (const list of childLists(step)) {
      const nested = findStep(list, id);
      if (nested) return nested;
    }
  }
  return null;
}

function fieldLabel(steps: Step[], stepId: string, pointer?: string): string {
  const producer = findStep(steps, stepId);
  const label = producer?.label ?? stepId;
  if (!pointer) return `the result of “${label}”`;
  const known = producer
    ? OUTPUT_FIELDS[producer.kind]?.find((f) => f.pointer === pointer)
    : undefined;
  return known
    ? `the ${known.label} from “${label}”`
    : `“${label}” → ${pointer}`;
}

export function describeBinding(chain: Chain, binding: Binding): string {
  switch (binding.kind) {
    case "literal": {
      const value = binding.value;
      if (value && typeof value === "object" && "path" in value)
        return String((value as { path: unknown }).path);
      return JSON.stringify(value);
    }
    case "output":
      return fieldLabel(chain.steps, binding.step, binding.pointer);
    case "input":
      return `the input “${binding.key}”`;
    case "firstAvailable":
      return binding.values
        .map((value) => describeBinding(chain, value))
        .join(", or else ");
  }
}

export function describeCondition(chain: Chain, condition: Condition): string {
  switch (condition.op) {
    case "equals": {
      const left = describeBinding(chain, condition.left);
      if (condition.right === true) return left;
      if (condition.right === false) return `${left} is false`;
      return `${left} is ${JSON.stringify(condition.right)}`;
    }
    case "lessThan":
      return `${describeBinding(chain, condition.left)} is below ${condition.right}`;
    case "exists":
      return `${describeBinding(chain, condition.value)} exists`;
    case "all":
      return condition.conditions
        .map((inner) => describeCondition(chain, inner))
        .join(", and ");
    case "any":
      return condition.conditions
        .map((inner) => describeCondition(chain, inner))
        .join(", or ");
    case "not":
      return `not (${describeCondition(chain, condition.condition)})`;
  }
}

const firstLine = (text: string, max = 70) => {
  const line = text.trim().split("\n")[0] ?? "";
  return line.length > max ? `${line.slice(0, max - 1)}…` : line;
};

/** One sentence per outline row: what this step does, in user language. */
export function describeStep(
  chain: Chain,
  step: Step,
  profileName?: (id: string) => string,
): string {
  switch (step.kind) {
    case "workspace":
      return step.prompt.trim()
        ? `Ask: “${firstLine(step.prompt)}”`
        : "Ask the assistant";
    case "snapshot":
      return `Save ${describeBinding(chain, step.input)} as ${step.filename}${
        step.requireChange ? " (only if it changed)" : ""
      }`;
    case "review":
      return `Review ${describeBinding(chain, step.input)} with ${
        profileName?.(step.profileId) ?? step.profileId
      }`;
    case "check":
      return `Run the check ${profileName?.(step.profileId) ?? step.profileId}`;
    case "capturedCheck":
      return `Run captured check ${step.planId}`;
    case "literatureLookup":
      return `Search literature for “${firstLine(step.query)}”`;
    case "deliver":
      return `Deliver ${describeBinding(chain, step.input)}`;
    case "delay":
      return step.seconds % 3600 === 0
        ? `Wait ${step.seconds / 3600} hour${step.seconds === 3600 ? "" : "s"}`
        : `Wait ${Math.round(step.seconds / 60)} minutes`;
    case "until":
      return `Wait until ${new Date(step.at * 1000).toLocaleString([], {
        month: "short",
        day: "numeric",
        hour: "numeric",
        minute: "2-digit",
      })}`;
    case "input":
      return `Pause and ask me: “${firstLine(step.prompt)}”`;
    case "if":
      return `Only if ${describeCondition(chain, step.condition)}`;
    case "repeat":
      return `Repeat up to ${step.maxIterations} time${
        step.maxIterations === 1 ? "" : "s"
      } until ${describeCondition(chain, step.until)}`;
    case "while":
      return `Repeat while ${describeCondition(chain, step.condition)} (up to ${step.maxIterations})`;
    case "parallel":
      return `Do these ${step.branches.length} branches side by side`;
    case "forEach":
      return `For each item in ${describeBinding(chain, step.input)} (up to ${step.maxItems})`;
    case "chain":
      return `Group: ${step.chain.name}`;
  }
}

let counter = 0;
export function newStepId(kind: string, existing: Set<string>): string {
  let id = "";
  do {
    counter += 1;
    id = `${kind}${counter}`;
  } while (existing.has(id));
  return id;
}

export function allStepIds(
  steps: Step[],
  into = new Set<string>(),
): Set<string> {
  for (const step of steps) {
    into.add(step.id);
    for (const list of childLists(step)) allStepIds(list, into);
  }
  return into;
}

/** A fresh step of the given kind with sensible defaults. */
export function blankStep(kind: Step["kind"], existing: Set<string>): Step {
  const id = newStepId(kind, existing);
  const base = { id, label: STEP_KIND_LABELS[kind] };
  switch (kind) {
    case "workspace":
      return { ...base, kind, prompt: "" };
    case "snapshot":
      return {
        ...base,
        kind,
        input: { kind: "literal", value: { path: "" } },
        filename: "output.md",
      };
    case "review":
      return {
        ...base,
        kind,
        input: { kind: "literal", value: { path: "" } },
        profileId: "",
        interpretation: "document",
      };
    case "check":
      return { ...base, kind, profileId: "" };
    case "capturedCheck":
      return { ...base, kind, planId: "" };
    case "literatureLookup":
      return { ...base, kind, query: "" };
    case "deliver":
      return { ...base, kind, input: { kind: "literal", value: { path: "" } } };
    case "delay":
      return { ...base, kind, seconds: 3600 };
    case "until":
      return { ...base, kind, at: Math.floor(Date.now() / 1000) + 3600 };
    case "input":
      return { ...base, kind, prompt: "" };
    case "if":
      return {
        ...base,
        kind,
        condition: { op: "exists", value: { kind: "input", key: "value" } },
        thenSteps: [],
      };
    case "repeat":
      return {
        ...base,
        kind,
        maxIterations: 3,
        until: { op: "exists", value: { kind: "input", key: "value" } },
        steps: [],
      };
    case "while":
      return {
        ...base,
        kind,
        maxIterations: 3,
        condition: { op: "exists", value: { kind: "input", key: "value" } },
        steps: [],
      };
    case "parallel":
      return { ...base, kind, branches: [[], []] };
    case "forEach":
      return {
        ...base,
        kind,
        input: { kind: "input", key: "items" },
        maxItems: 8,
        steps: [],
      };
    case "chain":
      return {
        ...base,
        kind,
        chain: {
          schemaVersion: 1,
          name: "Group",
          description: "",
          steps: [],
          limits: {
            maxActions: 64,
            deadlineHours: 168,
            actionTimeoutSecs: 7200,
          },
        },
      };
  }
}
