import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Theory from "./Theory";
import type { ProjectHome } from "../../lib/projectClient";
import type { TheoryOverview } from "../../lib/studioClient";
const mocks = vi.hoisted(() => ({
  theory: vi.fn(),
  mutate: vi.fn(),
  invoke: vi.fn(),
}));
vi.mock("../../lib/studioClient", async (original) => ({
  ...(await original<typeof import("../../lib/studioClient")>()),
  studioClient: mocks,
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
const anchor = (id: string, quote: string) => ({
  id,
  workspaceId: "ws",
  kind: "anchor",
  revision: 1,
  updatedAt: "",
  body: {
    selection: {
      revisionId: `rev-${id}`,
      revisionHash: "h",
      start: 0,
      end: quote.length,
      page: null,
      region: null,
      quote,
    },
    body: quote,
    prefix: "",
    suffix: "",
    origin: "manual",
  },
});
const data = {
  settings: { body: { manuscriptRevisionId: null } },
  papers: [],
  changes: [],
  tasks: [],
  applications: [],
  anchors: [
    anchor("a1", "Proposition 1"),
    anchor("a2", "Proof of Proposition 1"),
  ],
  executions: [],
} as unknown as ProjectHome;
const note = (
  id: string,
  kind: string,
  extra: Record<string, unknown> = {},
) => ({
  id,
  workspaceId: "ws",
  kind: "theory",
  revision: 2,
  updatedAt: "",
  body: {
    kind,
    title: `${kind} title`,
    statement: "Statement",
    body: "Derivation text",
    assumptions: [],
    assumptionIds: [],
    anchorIds: [],
    relatedIds: [],
    unresolvedSteps: [],
    status: "open",
    rejectionReason: "",
    origin: "manual",
    promotions: [],
    ...extra,
  },
});
const overview = (partial: Partial<TheoryOverview>): TheoryOverview => ({
  notes: [],
  checks: [],
  directions: [],
  evidence: {},
  ...partial,
});
const props = () => ({
  workspaceId: "ws",
  data,
  onRefresh: vi.fn().mockResolvedValue(undefined),
  onDocument: vi.fn(),
  onAnchors: vi.fn(),
});
beforeEach(() => {
  vi.clearAllMocks();
  mocks.mutate.mockResolvedValue({ id: "new", revision: 1, body: {} });
});
afterEach(cleanup);

it("never lets a numerical check claim generality and requires its domain", async () => {
  mocks.theory.mockResolvedValue(
    overview({
      notes: [note("t1", "proposition")],
      evidence: { t1: { label: "No checks recorded." } as never },
    }),
  );
  render(<Theory {...props()} />);
  fireEvent.click(await screen.findByText("Edit / checks"));
  const generality = screen.getByLabelText("Claims generality");
  fireEvent.click(generality);
  expect(generality).toBeChecked();
  fireEvent.change(screen.getByLabelText("Method"), {
    target: { value: "numerical_verification" },
  });
  expect(screen.getByLabelText("Claims generality")).toBeDisabled();
  expect(screen.getByLabelText("Claims generality")).not.toBeChecked();
  expect(
    screen.getByLabelText(
      "Domain of tested instances (required for numerical checks)",
    ),
  ).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Summary"), {
    target: { value: "Grid search" },
  });
  fireEvent.change(
    screen.getByLabelText(
      "Domain of tested instances (required for numerical checks)",
    ),
    { target: { value: "beta in (0.5, 0.99)" } },
  );
  fireEvent.change(screen.getByLabelText("Tolerance"), {
    target: { value: "1e-8" },
  });
  fireEvent.click(screen.getByText("Record check"));
  await waitFor(() =>
    expect(mocks.mutate).toHaveBeenCalledWith(
      "ws",
      expect.objectContaining({
        action: "saveCheck",
        id: null,
        expectedRevision: 0,
        check: expect.objectContaining({
          theoryId: "t1",
          method: "numerical_verification",
          claimsGenerality: false,
          domain: "beta in (0.5, 0.99)",
          tolerance: 1e-8,
        }),
      }),
    ),
  );
});

it("asks for a reason when abandoning and retains unresolved steps on save", async () => {
  mocks.theory.mockResolvedValue(overview({}));
  render(<Theory {...props()} />);
  await screen.findByText("No theory notes yet", { exact: false });
  expect(
    screen.queryByLabelText("Why this approach was rejected or abandoned"),
  ).toBeNull();
  fireEvent.change(screen.getByLabelText("Kind"), {
    target: { value: "proof_sketch" },
  });
  fireEvent.change(screen.getByLabelText("Title"), {
    target: { value: "Existence" },
  });
  fireEvent.change(screen.getByLabelText("Statement"), {
    target: { value: "An equilibrium exists." },
  });
  fireEvent.change(screen.getByLabelText("Unresolved steps"), {
    target: { value: "Continuity\n\nCompactness " },
  });
  fireEvent.change(screen.getByLabelText("Status (your disposition)"), {
    target: { value: "abandoned" },
  });
  fireEvent.change(
    screen.getByLabelText("Why this approach was rejected or abandoned"),
    { target: { value: "Contraction fails" } },
  );
  fireEvent.click(screen.getByText("Save theory note"));
  await waitFor(() =>
    expect(mocks.mutate).toHaveBeenCalledWith(
      "ws",
      expect.objectContaining({
        action: "saveTheory",
        id: null,
        note: expect.objectContaining({
          kind: "proof_sketch",
          status: "abandoned",
          rejectionReason: "Contraction fails",
          unresolvedSteps: ["Continuity", "Compactness"],
          promotions: [],
        }),
      }),
    ),
  );
});

it("opens a proposition and its proof side by side through saved selections", async () => {
  mocks.theory.mockResolvedValue(
    overview({
      notes: [
        note("p", "proposition", { anchorIds: ["a1"], relatedIds: ["q"] }),
        note("q", "derivation", { anchorIds: ["a2"] }),
      ],
      evidence: {
        p: { label: "No checks recorded." } as never,
        q: { label: "No checks recorded." } as never,
      },
    }),
  );
  const p = props();
  render(<Theory {...p} />);
  fireEvent.click(
    await screen.findByText("Open statement and proof side by side"),
  );
  expect(p.onAnchors).toHaveBeenCalledWith("a1", "a2");
});

it("converts a direction into a task only after its discriminating test is stated", async () => {
  const direction = (id: string, firstDiscriminatingTest: string) => ({
    id,
    workspaceId: "ws",
    kind: "direction",
    revision: 3,
    updatedAt: "",
    body: {
      question: `Question ${id}`,
      mechanism: "",
      closestKnownWork: "",
      minimalModelOrData: "",
      firstDiscriminatingTest,
      likelyFailureMode: "",
      nextAction: "",
      status: "idea",
      taskId: null,
      theoryIds: [],
      dropReason: "",
    },
  });
  mocks.theory.mockResolvedValue(
    overview({
      directions: [
        direction("d1", ""),
        direction("d2", "Compare the two cases"),
      ],
    }),
  );
  render(<Theory {...props()} />);
  const buttons = await screen.findAllByText("Convert to task");
  expect(buttons[0]).toBeDisabled();
  expect(buttons[1]).toBeEnabled();
  expect(screen.getByText("not stated")).toBeInTheDocument();
  fireEvent.click(buttons[1]);
  await waitFor(() =>
    expect(mocks.mutate).toHaveBeenCalledWith("ws", {
      action: "convertDirection",
      id: "d2",
      expectedRevision: 3,
    }),
  );
});
