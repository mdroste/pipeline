import type { Binding, Chain, Step } from "./taskTypes";

export const output = (step: string, pointer = ""): Binding => ({
  kind: "output",
  step,
  pointer,
});
export function template(
  kind: "review" | "prompt" | "input",
  prompt: string,
  profileId: string,
  rounds = 3,
  path = "",
): Chain {
  const limits = {
    // A review cycle takes three actions: revise, capture and review.
    maxActions: kind === "review" ? Math.max(64, 3 * rounds + 4) : 64,
    deadlineHours: 168,
    actionTimeoutSecs: 7200,
  };
  if (kind === "prompt")
    return {
      schemaVersion: 1,
      name: "Workspace follow-up",
      description: "Continue a conversation at the right time.",
      limits,
      steps: [
        {
          id: "followup",
          label: "Continue in Workspace",
          kind: "workspace",
          prompt,
        },
      ],
    };
  if (kind === "input")
    return {
      schemaVersion: 1,
      name: "Wait and continue",
      description:
        "Resume this conversation when you provide the missing input.",
      limits,
      steps: [
        {
          id: "answer",
          label: "Wait for input",
          kind: "input",
          prompt: "Provide the input needed to continue.",
        },
        {
          id: "followup",
          label: "Continue in Workspace",
          kind: "workspace",
          prompt: `${prompt}\n\nNew input:\n{{output:answer}}`,
        },
      ],
    };
  const steps: Step[] = path
    ? [
        {
          id: "initialPaper",
          label: "Capture the selected paper",
          kind: "snapshot",
          input: { kind: "literal", value: { path } },
          filename: path.split(/[\\/]/).pop() || "paper.md",
        },
      ]
    : [
        {
          id: "draft",
          label: "Draft the paper",
          kind: "workspace",
          prompt: `${prompt}\n\nReturn the complete paper as Markdown, with no enclosing code fence. This text will be saved as paper.md and reviewed. Keep uncertain claims explicit.`,
        },
        {
          id: "initialPaper",
          label: "Save the paper",
          kind: "snapshot",
          input: output("draft", "/answer"),
          filename: "paper.md",
        },
      ];
  steps.push({
    id: "reviewCycle",
    label: "Review and revise",
    kind: "repeat",
    maxIterations: rounds,
    until: {
      op: "all",
      conditions: [
        { op: "equals", left: output("review", "/complete"), right: true },
        {
          op: "lessThan",
          left: output("review", "/highPriorityCount"),
          right: 1,
        },
        {
          op: "lessThan",
          left: output("review", "/unknownPriorityCount"),
          right: 1,
        },
      ],
    },
    steps: [
      {
        id: "needsRevision",
        label: "Revise after an unsuccessful review",
        kind: "if",
        condition: { op: "exists", value: output("review") },
        thenSteps: [
          {
            id: "revise",
            label: "Address the review",
            kind: "workspace",
            prompt:
              "Revise the paper below to address the review. Preserve the argument and flag claims you cannot verify. Return the complete revised paper as Markdown, without a surrounding code fence.\n\nPaper:\n{{output:review#/paperText}}\n\nReview:\n{{output:review#/report}}\n\nReview quality and limitations:\n{{output:review#/quality}}",
          },
          {
            id: "revisedPaper",
            label: "Capture the revised paper",
            kind: "snapshot",
            input: output("revise", "/answer"),
            filename: "paper.md",
            requireChange: true,
          },
        ],
      },
      {
        id: "review",
        label: "Run the Review profile",
        kind: "review",
        profileId,
        interpretation: "document",
        input: {
          kind: "firstAvailable",
          values: [output("revisedPaper"), output("initialPaper")],
        },
      },
    ],
  });
  steps.push({
    id: "result",
    label: "Return the reviewed paper and findings",
    kind: "deliver",
    input: output("review"),
  });
  return {
    schemaVersion: 1,
    name: "Review and revise",
    description:
      "Stop when a complete review has no high-priority or unclassified findings, or the review limit is reached. The final paper is always the reviewed revision.",
    limits,
    steps,
  };
}
