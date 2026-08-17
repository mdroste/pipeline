// Structural rendering of Pipeline's portable schema dialect: properties,
// types, required markers, bounds, enums, and descriptions as an indented
// tree instead of raw JSON. Read-only — editing stays in the JSON draft.

const MAX_ENUM_CHIPS = 8;
const MAX_DEPTH = 16;

function typeLabel(schema: Record<string, unknown>): string {
  if (typeof schema.type === "string") return schema.type;
  if (Array.isArray(schema.enum)) return "enum";
  return "any";
}

function boundsLabel(schema: Record<string, unknown>): string | null {
  const parts: string[] = [];
  if (typeof schema.minItems === "number" || typeof schema.maxItems === "number") {
    const min = typeof schema.minItems === "number" ? schema.minItems : 0;
    const max = typeof schema.maxItems === "number" ? String(schema.maxItems) : "∞";
    parts.push(`${min}–${max} items`);
  }
  if (schema.uniqueItems === true) parts.push("unique");
  if (typeof schema.minLength === "number" && schema.minLength > 0) {
    parts.push(`≥${schema.minLength} chars`);
  }
  return parts.length ? parts.join(" · ") : null;
}

function EnumChips({ values }: { values: unknown[] }) {
  const shown = values.slice(0, MAX_ENUM_CHIPS);
  return (
    <span className="ml-1 inline-flex flex-wrap gap-1 align-middle">
      {shown.map((value, index) => (
        <span
          key={`${String(value)}-${index}`}
          className="rounded bg-gray-100 px-1.5 py-0.5 font-mono text-[10px] text-gray-700 dark:bg-gray-800 dark:text-gray-300"
        >
          {typeof value === "string" ? value : JSON.stringify(value)}
        </span>
      ))}
      {values.length > shown.length && (
        <span className="px-1 text-[10px] text-gray-500 dark:text-gray-400">
          +{values.length - shown.length} more
        </span>
      )}
    </span>
  );
}

function SchemaNode({
  name,
  schema,
  required,
  depth,
}: {
  name: string | null;
  schema: unknown;
  required: boolean;
  depth: number;
}) {
  if (depth > MAX_DEPTH || !schema || typeof schema !== "object" || Array.isArray(schema)) {
    return null;
  }
  const object = schema as Record<string, unknown>;
  const description = typeof object.description === "string" ? object.description : null;
  const bounds = boundsLabel(object);
  const properties =
    object.properties && typeof object.properties === "object" && !Array.isArray(object.properties)
      ? (object.properties as Record<string, unknown>)
      : null;
  const requiredKeys = new Set(
    Array.isArray(object.required) ? (object.required as unknown[]).filter((key): key is string => typeof key === "string") : [],
  );

  return (
    <div className={depth > 0 ? "border-l border-gray-200 pl-3 dark:border-gray-700" : ""}>
      <div className="flex flex-wrap items-baseline gap-x-2 py-0.5">
        {name !== null && (
          <span className="font-mono text-xs font-medium text-gray-900 dark:text-gray-100">
            {name}
          </span>
        )}
        <span className="rounded bg-violet-50 px-1.5 py-0.5 text-[10px] font-medium text-violet-700 dark:bg-violet-950/50 dark:text-violet-300">
          {typeLabel(object)}
        </span>
        {required && (
          <span className="text-[10px] font-medium text-amber-700 dark:text-amber-400" title="required">
            required
          </span>
        )}
        {bounds && <span className="text-[10px] text-gray-500 dark:text-gray-400">{bounds}</span>}
        {Array.isArray(object.enum) && <EnumChips values={object.enum as unknown[]} />}
      </div>
      {description && (
        <p className="max-w-2xl pb-0.5 text-[11px] leading-snug text-gray-500 dark:text-gray-400">
          {description}
        </p>
      )}
      {properties && (
        <div className="mt-0.5 space-y-0.5">
          {Object.entries(properties).map(([key, child]) => (
            <SchemaNode
              key={key}
              name={key}
              schema={child}
              required={requiredKeys.has(key)}
              depth={depth + 1}
            />
          ))}
        </div>
      )}
      {object.items !== undefined && (
        <div className="mt-0.5">
          <SchemaNode name="items" schema={object.items} required={false} depth={depth + 1} />
        </div>
      )}
    </div>
  );
}

export default function SchemaTreeView({ schema }: { schema: Record<string, unknown> }) {
  return (
    <div
      aria-label="Schema structure"
      className="overflow-x-auto rounded-xl border border-gray-200 bg-white p-4 text-sm dark:border-gray-700 dark:bg-gray-900"
    >
      <SchemaNode name={null} schema={schema} required={false} depth={0} />
    </div>
  );
}
