import { describe, it, expect } from "vitest";
import { lineDiff, hasChanges } from "./diff";

describe("lineDiff", () => {
  it("marks identical text as all same", () => {
    const ops = lineDiff("a\nb\nc", "a\nb\nc");
    expect(ops.every((o) => o.type === "same")).toBe(true);
    expect(hasChanges(ops)).toBe(false);
  });

  it("detects an inserted line", () => {
    const ops = lineDiff("a\nc", "a\nb\nc");
    expect(ops).toEqual([
      { type: "same", text: "a" },
      { type: "add", text: "b" },
      { type: "same", text: "c" },
    ]);
    expect(hasChanges(ops)).toBe(true);
  });

  it("detects a deleted line", () => {
    const ops = lineDiff("a\nb\nc", "a\nc");
    expect(ops.filter((o) => o.type === "del")).toEqual([
      { type: "del", text: "b" },
    ]);
  });

  it("detects a changed line as del + add", () => {
    const ops = lineDiff("x", "y");
    expect(ops).toEqual([
      { type: "del", text: "x" },
      { type: "add", text: "y" },
    ]);
  });

  it("handles one side empty", () => {
    expect(
      lineDiff("", "a\nb").every((o) => o.type === "add" || o.text === ""),
    ).toBe(true);
    expect(lineDiff("a\nb", "").filter((o) => o.type === "del")).toHaveLength(
      2,
    );
  });

  it("uses a bounded fallback for oversized comparisons", () => {
    const before = Array.from({ length: 5000 }, (_, i) => `old ${i}`).join(
      "\n",
    );
    const after = Array.from({ length: 5000 }, (_, i) => `new ${i}`).join("\n");
    const ops = lineDiff(before, after);
    expect(ops).toHaveLength(3);
    expect(ops[1].text).toContain("omitted");
    expect(hasChanges(ops)).toBe(true);
  });

  it("does not allocate per-line state for a huge identical output", () => {
    const text = "same line\n".repeat(100_000);
    const ops = lineDiff(text, text);
    expect(ops).toHaveLength(1);
    expect(hasChanges(ops)).toBe(false);
  });
});
