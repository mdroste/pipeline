export type DiffType = "same" | "add" | "del";
export interface DiffOp {
  type: DiffType;
  text: string;
}

/** Line-level diff of two texts via a longest-common-subsequence walk.
 *  `del` lines come from `a`, `add` lines from `b`. O(n·m) — fine for the
 *  hundreds-of-lines step outputs it compares. */
export function lineDiff(a: string, b: string): DiffOp[] {
  const aLines = a.split("\n");
  const bLines = b.split("\n");
  const n = aLines.length;
  const m = bLines.length;

  // dp[i][j] = LCS length of aLines[i..] and bLines[j..].
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = aLines[i] === bLines[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }

  const ops: DiffOp[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (aLines[i] === bLines[j]) {
      ops.push({ type: "same", text: aLines[i] });
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      ops.push({ type: "del", text: aLines[i] });
      i++;
    } else {
      ops.push({ type: "add", text: bLines[j] });
      j++;
    }
  }
  while (i < n) ops.push({ type: "del", text: aLines[i++] });
  while (j < m) ops.push({ type: "add", text: bLines[j++] });
  return ops;
}

/** Whether a diff has any actual change (any add/del op). */
export function hasChanges(ops: DiffOp[]): boolean {
  return ops.some((o) => o.type !== "same");
}
