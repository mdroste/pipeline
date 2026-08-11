type AdaptiveSlotKind = "subject" | "method";

const SLOT_COPY: Record<AdaptiveSlotKind, {
  title: string;
  count: string;
  description: string;
  selection: string;
}> = {
  subject: {
    title: "Subject specialists",
    count: "1–2 reviewers",
    description: "Discipline and subfield expertise matched to the paper's central contribution.",
    selection: "The first role is the primary subject specialist. A second is added only for genuinely interdisciplinary work.",
  },
  method: {
    title: "Method specialists",
    count: "1–4 reviewers",
    description: "Method-specific scrutiny matched to the evidence and argument the paper actually uses.",
    selection: "The orientation map chooses only methods that carry material claims, such as proofs, causal identification, experiments, computation, qualitative evidence, or archival work.",
  },
};

export function AdaptiveSlotRow({
  kind,
  selected,
  onSelect,
}: {
  kind: AdaptiveSlotKind;
  selected: boolean;
  onSelect: () => void;
}) {
  const copy = SLOT_COPY[kind];
  return (
    <button
      type="button"
      aria-label={`${copy.title} — Auto-filled from orientation`}
      onClick={onSelect}
      className={`w-full border-b border-blue-100 px-4 py-2.5 text-left dark:border-blue-950/60 ${
        selected
          ? "bg-blue-100/80 dark:bg-blue-950/60"
          : "bg-blue-50/40 hover:bg-blue-50 dark:bg-blue-950/15 dark:hover:bg-blue-950/35"
      }`}
    >
      <span className="flex items-center justify-between gap-2">
        <span className="text-sm font-medium text-gray-800 dark:text-gray-200">{copy.title}</span>
        <span className="shrink-0 rounded-full bg-blue-100 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide text-blue-700 dark:bg-blue-900/60 dark:text-blue-200">
          Adaptive
        </span>
      </span>
      <span className="mt-0.5 block text-[11px] text-blue-700/80 dark:text-blue-300/80">
        Auto-filled from orientation · {copy.count}
      </span>
    </button>
  );
}

export function AdaptiveSlotEditorPanel({
  kind,
  onBrowse,
}: {
  kind: AdaptiveSlotKind;
  onBrowse: (kind: AdaptiveSlotKind) => void;
}) {
  const copy = SLOT_COPY[kind];
  return (
    <div className="flex-1 overflow-y-auto">
      <div className="mx-auto max-w-3xl space-y-5 p-5">
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-base font-semibold text-gray-900 dark:text-gray-100">{copy.title}</h3>
            <span className="rounded-full bg-blue-50 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-blue-700 dark:bg-blue-950/50 dark:text-blue-300">
              Auto-filled
            </span>
            <span className="text-xs text-gray-500 dark:text-gray-400">{copy.count}</span>
          </div>
          <p className="mt-2 text-sm leading-6 text-gray-600 dark:text-gray-300">{copy.description}</p>
        </div>

        <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-800 dark:bg-gray-900">
          <h4 className="text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400">How it is filled</h4>
          <p className="mt-2 text-sm leading-6 text-gray-700 dark:text-gray-300">
            Orientation and classification happen in one LLM call. Its validated <span className="font-mono text-xs">review_plan</span> selects stable role IDs from the host-owned catalog. {copy.selection}
          </p>
        </section>

        <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-800 dark:bg-gray-900">
          <h4 className="text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400">At run time</h4>
          <p className="mt-2 text-sm leading-6 text-gray-700 dark:text-gray-300">
            Pipeline materializes the selected roles as ordinary parallel review steps. They receive the same paper and orientation context as the three core reviews; unselected catalog roles never become workflow steps.
          </p>
        </section>

        <section className="rounded-xl border border-green-200 bg-green-50/60 p-4 dark:border-green-950 dark:bg-green-950/20">
          <h4 className="text-xs font-semibold uppercase tracking-wide text-green-700 dark:text-green-300">Synthesis input</h4>
          <p className="mt-2 text-sm leading-6 text-green-900 dark:text-green-100">
            Every materialized specialist report feeds directly into <span className="font-medium">Consolidate</span>, alongside Contribution &amp; Literature, Claims &amp; Consistency, and Exposition &amp; Architecture.
          </p>
        </section>

        <button
          type="button"
          onClick={() => onBrowse(kind)}
          className="rounded-lg border border-blue-300 bg-white px-3 py-2 text-sm font-medium text-blue-700 hover:bg-blue-50 dark:border-blue-800 dark:bg-gray-900 dark:text-blue-300 dark:hover:bg-blue-950/30"
        >
          Browse {kind === "subject" ? "subject" : "method"} specialist catalog
        </button>
      </div>
    </div>
  );
}

export type { AdaptiveSlotKind };
