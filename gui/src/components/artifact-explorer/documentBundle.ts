import type { DocumentNode } from "../../lib/artifactTypes";
export function formatRepresentation(value: unknown): string {
  const text =
    typeof value === "string" ? value : JSON.stringify(value, null, 2);
  if (!text) return "";
  return text.length > 4_000 ? `${text.slice(0, 4_000)}\n…` : text;
}

export const STRUCTURAL_KINDS = new Set([
  "section",
  "equation",
  "table",
  "figure",
  "theorem",
  "proposition",
  "lemma",
  "corollary",
]);
export const INITIAL_BUNDLE_NODES = 100;

export function equationNumberText(text: string): string | null {
  let value = text.trim();
  while (value.length >= 2 && value.startsWith("$") && value.endsWith("$")) {
    value = value.slice(1, -1).trim();
  }
  if (value.startsWith("\\(") && value.endsWith("\\)")) {
    value = value.slice(2, -2).trim();
  } else if (value.startsWith("\\[") && value.endsWith("\\]")) {
    value = value.slice(2, -2).trim();
  }
  const tag = /^\\tag\*?\{([^{}]{1,40})\}$/.exec(value);
  if (tag) value = tag[1].trim();
  if (
    ((value.startsWith("(") && value.endsWith(")")) ||
      (value.startsWith("[") && value.endsWith("]"))) &&
    value.length >= 2
  ) {
    value = value.slice(1, -1).trim();
  }
  return /^[a-z0-9]+(?:[.:-][a-z0-9]+)*$/i.test(value) ? value : null;
}

export function sameEquationContext(
  left: DocumentNode,
  right: DocumentNode,
): boolean {
  return (
    left.page === right.page &&
    left.parent_id === right.parent_id &&
    left.provenance?.method === right.provenance?.method
  );
}

export function mergeEquationNumber(
  equation: DocumentNode,
  numberNode: DocumentNode,
  number: string,
): DocumentNode {
  return {
    ...equation,
    number: equation.number || number,
    asset_ids: Array.from(
      new Set([...equation.asset_ids, ...numberNode.asset_ids]),
    ),
    representations: [
      ...equation.representations,
      ...numberNode.representations,
    ],
  };
}

export function coalesceEquationNumberNodes(
  nodes: DocumentNode[],
): DocumentNode[] {
  const result: DocumentNode[] = [];
  for (let index = 0; index < nodes.length; index += 1) {
    const node = nodes[index];
    const number =
      node.kind === "equation" ? equationNumberText(node.text) : null;
    if (number) {
      const previous = result[result.length - 1];
      if (
        previous?.kind === "equation" &&
        !equationNumberText(previous.text) &&
        sameEquationContext(previous, node)
      ) {
        result[result.length - 1] = mergeEquationNumber(previous, node, number);
        continue;
      }
      const next = nodes[index + 1];
      if (
        next?.kind === "equation" &&
        !equationNumberText(next.text) &&
        sameEquationContext(node, next)
      ) {
        result.push(mergeEquationNumber(next, node, number));
        index += 1;
        continue;
      }
    }
    result.push(node);
  }
  return result;
}
