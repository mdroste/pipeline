import type {
  ArtifactSelector,
  Phase,
  PipelineConfig,
  PrimaryArtifactPart,
  StepConfig,
  StepContext,
} from "../../lib/types";

/** Mirror the backend's effective graph and return transitive upstreams. */
export function conditionUpstreamIds(steps: StepConfig[], targetId: string): string[] {
  const enabled = steps.filter((step) => step.enabled);
  const ids = new Set(enabled.map((step) => step.id));
  const dependencies = new Map<string, string[]>();
  for (const step of enabled) {
    const selectedProducers = (step.context?.include ?? [])
      .filter((selector): selector is Extract<ArtifactSelector, { kind: "step" }> =>
        selector.kind === "step")
      .map((selector) => selector.step);
    dependencies.set(
      step.id,
      [...new Set([...(step.after ?? []), ...selectedProducers])]
        .filter((id) => ids.has(id)),
    );
  }
  const upstream = new Set<string>();
  const pending = [...(dependencies.get(targetId) ?? [])];
  while (pending.length) {
    const id = pending.pop()!;
    if (upstream.has(id)) continue;
    upstream.add(id);
    pending.push(...(dependencies.get(id) ?? []));
  }
  return enabled.filter((step) => upstream.has(step.id)).map((step) => step.id);
}

export function primaryPartsForMode(inputMode: string): PrimaryArtifactPart[] {
  if (inputMode === "none") return [];
  if (inputMode === "folder") return ["text", "source"];
  return ["text", "structure", "visuals", "source"];
}

export function defaultStepContext(
  phase: Phase,
  inputMode: string,
  useOrientation: boolean,
  priorSteps: Array<Pick<StepConfig, "id" | "enabled">>,
): StepContext {
  const include: ArtifactSelector[] = [];
  const primary = primaryPartsForMode(inputMode);
  if (phase === "parallel" && primary.length) {
    include.push({ kind: "primary", parts: primary });
  }
  if (useOrientation) include.push({ kind: "survey" });
  if (phase === "sequential") {
    include.push(...priorSteps
      .filter((step) => step.enabled)
      .map((step): ArtifactSelector => ({
        kind: "step",
        step: step.id,
        parts: ["report"],
      })));
  }
  return { include };
}

const OUTPUT_SCHEMA_TYPES = new Set([
  "object", "array", "string", "number", "integer", "boolean", "null",
]);

function schemaTypeMatches(type: unknown, value: unknown): boolean {
  switch (type) {
    case "object": return !!value && typeof value === "object" && !Array.isArray(value);
    case "array": return Array.isArray(value);
    case "string": return typeof value === "string";
    case "number": return typeof value === "number" && Number.isFinite(value);
    case "integer": return typeof value === "number" && Number.isSafeInteger(value);
    case "boolean": return typeof value === "boolean";
    case "null": return value === null;
    default: return false;
  }
}

export function outputSchemaError(schema: unknown, path = "$", depth = 0): string | null {
  if (depth > 32) return `${path}: nesting exceeds 32 levels`;
  if (!schema || typeof schema !== "object" || Array.isArray(schema)) {
    return `${path}: schema must be a JSON object`;
  }
  const object = schema as Record<string, unknown>;
  if (object.type !== undefined &&
      (typeof object.type !== "string" || !OUTPUT_SCHEMA_TYPES.has(object.type))) {
    return `${path}.type: unsupported type`;
  }
  if (object.enum !== undefined) {
    if (!Array.isArray(object.enum) || object.enum.length === 0) {
      return `${path}.enum: expected a non-empty array`;
    }
    const encoded = object.enum.map((value) => JSON.stringify(value));
    if (new Set(encoded).size !== encoded.length) {
      return `${path}.enum: entries must be unique`;
    }
    if (object.type !== undefined &&
        object.enum.some((value) => !schemaTypeMatches(object.type, value))) {
      return `${path}.enum: every entry must match the declared type`;
    }
  }
  if (object.required !== undefined) {
    if (!Array.isArray(object.required) || object.required.some((key) => typeof key !== "string")) {
      return `${path}.required: expected an array of strings`;
    }
    if (new Set(object.required).size !== object.required.length) {
      return `${path}.required: entries must be unique`;
    }
    if (object.type !== undefined && object.type !== "object") {
      return `${path}.required: only valid for an object schema`;
    }
  }
  if (object.properties !== undefined) {
    if (!object.properties || typeof object.properties !== "object" ||
        Array.isArray(object.properties)) {
      return `${path}.properties: expected an object`;
    }
    if (object.type !== undefined && object.type !== "object") {
      return `${path}.properties: only valid for an object schema`;
    }
    for (const [key, child] of Object.entries(object.properties)) {
      const error = outputSchemaError(child, `${path}.properties.${key}`, depth + 1);
      if (error) return error;
    }
  }
  if (object.items !== undefined) {
    if (object.type !== undefined && object.type !== "array") {
      return `${path}.items: only valid for an array schema`;
    }
    const error = outputSchemaError(object.items, `${path}.items`, depth + 1);
    if (error) return error;
  }
  for (const keyword of ["minItems", "maxItems"] as const) {
    const value = object[keyword];
    if (value !== undefined &&
        (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0)) {
      return `${path}.${keyword}: expected a non-negative integer`;
    }
    if (value !== undefined && object.type !== undefined && object.type !== "array") {
      return `${path}.${keyword}: only valid for an array schema`;
    }
  }
  if (typeof object.minItems === "number" && typeof object.maxItems === "number" &&
      object.minItems > object.maxItems) {
    return `${path}: minItems cannot exceed maxItems`;
  }
  if (object.uniqueItems !== undefined) {
    if (typeof object.uniqueItems !== "boolean") {
      return `${path}.uniqueItems: expected a boolean`;
    }
    if (object.type !== undefined && object.type !== "array") {
      return `${path}.uniqueItems: only valid for an array schema`;
    }
  }
  return null;
}

export function normalizeConfig(config: PipelineConfig): PipelineConfig {
  return {
    ...config,
    // The field remains in the wire format for compatibility with existing
    // profiles, but orientation is now a required workflow stage.
    use_orientation: true,
    steps: (config.steps ?? []).map((step) => ({
      ...step,
      after: step.after ?? [],
      context: step.context ?? { include: [] },
    })),
    merge: config.merge ?? { enabled: true, prompt: "", agents: [] },
    context_cache: config.context_cache ?? { enabled: false },
    extraction: config.extraction ?? { method: "" },
    orientation_prompt: config.orientation_prompt ?? "",
    orientation_schema: config.orientation_schema ?? null,
    parallel_context_template: config.parallel_context_template ?? "",
    variables: config.variables ?? [],
  };
}
