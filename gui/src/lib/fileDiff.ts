import { diffLines } from "diff";
export interface FileHunk {
  id: number;
  from: number;
  to: number;
  before: string;
  after: string;
  line: number;
}
/** Exact offsets, including final newlines. Never accept a truncated diff preview. */
export function fileHunks(before: string, after: string): FileHunk[] {
  if (before === after) return [];
  const changes =
    before.length + after.length <= 4 * 1024 * 1024
      ? diffLines(before, after, { timeout: 100, maxEditLength: 3000 })
      : undefined;
  if (!changes)
    return [{ id: 0, from: 0, to: before.length, before, after, line: 1 }];
  const hunks: FileHunk[] = [];
  let offset = 0;
  let line = 1;
  let current: FileHunk | null = null;
  for (const change of changes) {
    if (!change.added && !change.removed) {
      if (current) hunks.push(current);
      current = null;
      offset += change.value.length;
      line += change.count ?? 0;
      continue;
    }
    current ??= {
      id: hunks.length,
      from: offset,
      to: offset,
      before: "",
      after: "",
      line,
    };
    if (change.removed) {
      current.before += change.value;
      offset += change.value.length;
      current.to = offset;
      line += change.count ?? 0;
    } else current.after += change.value;
  }
  if (current) hunks.push(current);
  return hunks;
}
export function applyFileHunks(
  before: string,
  hunks: FileHunk[],
  selected: number[],
): string {
  let result = before;
  let boundary = before.length;
  for (const hunk of [...hunks].reverse()) {
    if (
      hunk.to > boundary ||
      hunk.from > hunk.to ||
      before.slice(hunk.from, hunk.to) !== hunk.before
    )
      throw new Error("The source changed; refresh the comparison.");
    boundary = hunk.from;
    if (selected.includes(hunk.id))
      result = result.slice(0, hunk.from) + hunk.after + result.slice(hunk.to);
  }
  return result;
}
