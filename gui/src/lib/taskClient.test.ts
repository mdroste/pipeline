import { describe, expect, it } from "vitest";
import { template, type Step } from "./taskClient";
import reviewChain from "../../src-tauri/src/orchestration/fixtures/review-chain.json";
function flatten(steps: Step[]): Step[] {
  return steps.flatMap((s) => [
    s,
    ...(s.kind === "repeat" || s.kind === "while" || s.kind === "forEach"
      ? flatten(s.steps)
      : s.kind === "if"
        ? [...flatten(s.thenSteps), ...flatten(s.elseSteps ?? [])]
        : s.kind === "parallel"
          ? s.branches.flatMap(flatten)
          : s.kind === "chain"
            ? flatten(s.chain.steps)
            : []),
  ]);
}
describe("task templates", () => {
  it("keeps the default review template aligned with the Rust validation fixture", () => {
    expect(
      template("review", "A macroeconomic idea", "auto-review", 3),
    ).toEqual(reviewChain);
  });
  it("always ends a revision round with a review of an immutable artifact", () => {
    const chain = template("review", "A macroeconomic idea", "auto-review", 3);
    const loop = chain.steps.find((s) => s.kind === "repeat");
    expect(loop?.kind).toBe("repeat");
    if (loop?.kind !== "repeat") throw new Error("Missing loop");
    expect(loop.maxIterations).toBe(3);
    expect(loop.steps[loop.steps.length - 1].kind).toBe("review");
    expect(chain.steps[chain.steps.length - 1].kind).toBe("deliver");
    const steps = flatten(chain.steps);
    expect(new Set(steps.map((s) => s.id)).size).toBe(steps.length);
    expect(steps.find((s) => s.id === "revisedPaper")).toMatchObject({
      kind: "snapshot",
      requireChange: true,
    });
    expect(JSON.stringify(loop.until)).toContain("unknownPriorityCount");
    expect(JSON.stringify(loop.until)).toContain("complete");
  });
  it("captures an existing paper without first drafting another", () => {
    const chain = template(
      "review",
      "",
      "auto-review",
      2,
      "/research/paper.pdf",
    );
    expect(chain.steps[0]).toMatchObject({
      kind: "snapshot",
      filename: "paper.pdf",
      input: { kind: "literal", value: { path: "/research/paper.pdf" } },
    });
    expect(flatten(chain.steps).some((s) => s.id === "draft")).toBe(false);
  });
  it("waits for durable input before the follow-up", () => {
    const chain = template("input", "Use the new calibration", "");
    expect(chain.steps.map((s) => s.kind)).toEqual(["input", "workspace"]);
    expect(chain.steps[1]).toMatchObject({
      prompt: expect.stringContaining("{{output:answer}}"),
    });
  });
  it("a single follow-up does not require a Review profile", () => {
    expect(template("prompt", "Check progress", "").steps).toEqual([
      {
        id: "followup",
        label: "Continue in Workspace",
        kind: "workspace",
        prompt: "Check progress",
      },
    ]);
  });
});
