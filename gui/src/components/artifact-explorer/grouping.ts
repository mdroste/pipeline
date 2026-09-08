import type { ArtifactEntry } from "../../lib/artifactTypes";
export const GROUPS: { id: string; label: string; description?: string }[] = [
  { id: "report", label: "Report" },
  { id: "document", label: "Document" },
  { id: "pages", label: "Pages" },
  { id: "figures", label: "Figures" },
  { id: "tables", label: "Tables" },
  { id: "equations", label: "Equations" },
  { id: "context", label: "Context" },
  {
    id: "agent_response",
    label: "Agent reports",
    description: "Provider responses, including retries and merge calls.",
  },
  {
    id: "step",
    label: "Step outputs",
    description: "One finalized result per workflow step, after agent merging.",
  },
  { id: "files", label: "Files" },
];

export interface AgentReportGroup {
  key: string;
  label: string;
  order: number;
  items: ArtifactEntry[];
}

export function artifactStem(entry: ArtifactEntry): string {
  return (
    entry.rel_path
      .split("/")
      .pop()
      ?.replace(/\.[^.]+$/, "") ?? ""
  );
}

export function comparableProducer(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function stepOutputIdentity(
  entry: ArtifactEntry,
  fallbackOrder: number,
) {
  const stem = artifactStem(entry);
  const match = stem.match(/^(\d+)_([^]*)$/);
  const order = match ? Number(match[1]) : fallbackOrder;
  const key = comparableProducer(match?.[2] ?? stem);
  // Multi-agent outputs used to retain a synthetic "[Agent]" suffix even
  // after the reports had been merged. It identifies a provider call, not a
  // separate workflow step, so omit it from the group heading.
  const label =
    entry.label.replace(/\s+\[[^\]]+\]\s*$/, "").trim() || entry.label;
  return { key, label, order };
}

export function agentResponseProducer(entry: ArtifactEntry): {
  key: string;
  merge: boolean;
} {
  const stem = artifactStem(entry);
  const producer = stem.split(/--[a-f0-9]{12}--attempt-/i, 1)[0] ?? stem;
  const merge = producer.startsWith("merge-");
  return {
    key: comparableProducer(merge ? producer.slice("merge-".length) : producer),
    merge,
  };
}

/**
 * Agent-response filenames contain a producer-derived key. Finalized
 * step-output filenames contain the corresponding key and a leading output
 * number, so they provide a durable step order without loading report.json
 * (Sources remains manifest-only until an item is read).
 */
export function groupAgentReports(
  items: ArtifactEntry[],
  stepOutputs: ArtifactEntry[],
): AgentReportGroup[] {
  const steps = stepOutputs.map(stepOutputIdentity);
  const groups = new Map<string, AgentReportGroup>();

  items.forEach((item, itemIndex) => {
    const producer = agentResponseProducer(item);
    // Prefer the longest match when one step id prefixes another (for example,
    // "technical" and "technical-appendix"). A suffix denotes an agent or a
    // fan-out unit belonging to that logical step.
    const step = steps
      .filter(
        ({ key }) => producer.key === key || producer.key.startsWith(`${key}-`),
      )
      .sort((a, b) => b.key.length - a.key.length)[0];
    const key = step?.key ?? `unmatched:${producer.key}`;
    const existing = groups.get(key);
    if (existing) {
      existing.items.push(item);
      return;
    }
    groups.set(key, {
      key,
      label: step?.label ?? item.label.split(" · Attempt", 1)[0] ?? item.label,
      order: step?.order ?? Number.MAX_SAFE_INTEGER - items.length + itemIndex,
      items: [item],
    });
  });

  return [...groups.values()]
    .sort((a, b) => a.order - b.order || a.label.localeCompare(b.label))
    .map((group) => ({
      ...group,
      items: [...group.items].sort((a, b) => {
        const producerA = agentResponseProducer(a);
        const producerB = agentResponseProducer(b);
        // Individual agent responses precede the cross-agent merge report.
        if (producerA.merge !== producerB.merge)
          return producerA.merge ? 1 : -1;
        return a.rel_path.localeCompare(b.rel_path, undefined, {
          numeric: true,
        });
      }),
    }));
}
