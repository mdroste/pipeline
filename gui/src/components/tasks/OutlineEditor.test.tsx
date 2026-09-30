import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import Builder from "./Builder";
import { describeCondition, describeStep } from "./language";
import { modelFromCondition, modelToCondition } from "./ConditionEditor";
import { template, type Chain, type Step } from "../../lib/taskClient";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

let prepared: Chain | null = null;
beforeEach(() => {
  prepared = null;
  mocks.invoke.mockReset();
  mocks.invoke.mockImplementation(
    async (command: string, args: Record<string, unknown> = {}) => {
      switch (command) {
        case "task_saved_chains":
          return [];
        case "list_profiles":
          return [{ id: "auto", name: "Automatic Paper Review" }];
        case "task_sessions":
          return [
            { id: "session", title: "Minimum wage", workspaceName: null },
          ];
        case "task_preview_times":
          return [];
        case "task_validate_chain":
          return JSON.parse(String(args.json));
        case "task_prepare": {
          const request = args.request as { chain: Chain };
          prepared = request.chain;
          return { id: "run", chain: request.chain };
        }
        default:
          throw new Error(`Unexpected command ${command}`);
      }
    },
  );
});

it("renders every template step as a plain-language sentence, never a kind id", () => {
  const chain = template("review", "Study X", "auto", 3);
  const sentences: string[] = [];
  const walk = (steps: Step[]) => {
    for (const step of steps) {
      sentences.push(describeStep(chain, step, () => "Auto Paper Review"));
      if (step.kind === "repeat") walk(step.steps);
      if (step.kind === "if") walk(step.thenSteps);
    }
  };
  walk(chain.steps);
  expect(sentences.join("\n")).not.toMatch(
    /\b(workspace|snapshot|deliver|forEach|capturedCheck)\b/,
  );
  expect(sentences.join("\n")).toContain("Save");
  expect(sentences.join("\n")).toContain("Review");
});

it("describes and round-trips the template's stop condition", () => {
  const chain = template("review", "Study X", "auto", 3);
  const repeat = chain.steps.find((step) => step.kind === "repeat");
  if (repeat?.kind !== "repeat") throw new Error("missing repeat");
  const sentence = describeCondition(chain, repeat.until);
  expect(sentence).toContain("review finished");
  expect(sentence).toContain("high-priority findings");
  expect(sentence).toContain("below 1");
  const model = modelFromCondition(repeat.until);
  expect(model).not.toBeNull();
  expect(modelToCondition(model!)).toEqual(repeat.until);
});

it("composes the draft→review→revise loop from an empty outline and prepares it", async () => {
  const user = userEvent.setup();
  render(
    <Builder
      initialSessionId="session"
      onPrepared={() => {}}
      onClose={() => {}}
    />,
  );
  await screen.findByRole("button", { name: "Start empty" });
  await user.click(screen.getByRole("button", { name: "Start empty" }));

  const outline = () => screen.getByRole("list", { name: "Automation steps" });
  const add = (kind: string, listName = "Automation steps") => {
    const list = screen.getByRole("list", { name: listName });
    fireEvent.change(
      within(list).getByRole("combobox", { name: `Add to ${listName}` }),
      {
        target: { value: kind },
      },
    );
  };

  add("workspace");
  await screen.findByRole("textbox", { name: "Prompt" });
  fireEvent.change(screen.getByRole("textbox", { name: "Prompt" }), {
    target: { value: "Draft a paper about minimum wage spillovers." },
  });
  add("snapshot");
  // The new snapshot editor is open; point it at the assistant reply.
  const saveThis = screen.getByRole("combobox", {
    name: /Save this for Save a version/,
  });
  fireEvent.change(saveThis, { target: { value: "output:workspace1:/text" } });
  add("repeat");
  add("review", "Steps inside Repeat until…");
  const reviewList = screen.getByRole("list", {
    name: "Steps inside Repeat until…",
  });
  fireEvent.change(
    within(reviewList).getByRole("combobox", {
      name: /Review workflow/,
    }),
    { target: { value: "auto" } },
  );
  add("deliver");

  expect(within(outline()).getAllByRole("listitem").length).toBeGreaterThan(4);
  // No internal kind ids leak into the outline copy.
  expect(outline().textContent).not.toMatch(/\bworkspace\b|\bsnapshot\b/);

  await user.click(screen.getByRole("button", { name: /Prepare automation/ }));
  expect(prepared).not.toBeNull();
  const kinds = prepared!.steps.map((step) => step.kind);
  expect(kinds).toEqual(["workspace", "snapshot", "repeat", "deliver"]);
  const repeat = prepared!.steps[2];
  if (repeat.kind !== "repeat") throw new Error("expected repeat");
  expect(repeat.steps[0].kind).toBe("review");
  if (repeat.steps[0].kind !== "review") throw new Error("expected review");
  expect(repeat.steps[0].profileId).toBe("auto");
  const snapshot = prepared!.steps[1];
  if (snapshot.kind !== "snapshot") throw new Error("expected snapshot");
  expect(snapshot.input).toEqual({
    kind: "output",
    step: "workspace1",
    pointer: "/text",
  });
});

it("keeps the review template editable and validates the edited chain", async () => {
  const user = userEvent.setup();
  render(
    <Builder
      initialSessionId="session"
      onPrepared={() => {}}
      onClose={() => {}}
    />,
  );
  await screen.findByRole("combobox", { name: /Review workflow/ });
  fireEvent.change(screen.getByLabelText(/Idea or instructions/), {
    target: { value: "Minimum wage employment effects" },
  });
  // Structural edit: remove the final deliver step; the chain stays valid.
  await user.click(
    screen.getByRole("button", { name: /Remove Return the reviewed paper/ }),
  );
  await user.click(screen.getByRole("button", { name: /Prepare automation/ }));
  expect(prepared).not.toBeNull();
  expect(prepared!.steps.some((step) => step.kind === "deliver")).toBe(false);
  expect(prepared!.steps.some((step) => step.kind === "repeat")).toBe(true);
});
