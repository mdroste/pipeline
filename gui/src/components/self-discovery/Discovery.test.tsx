import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import Builder from "./Builder";
import Detail from "./Detail";
import SelfDiscovery from "./Page";
import {
  newDiscovery,
  discoveryError,
  type DiscoveryRun,
  type Candidate,
} from "../../lib/discoveryClient";
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("../ReportViewer", () => ({
  default: ({ markdown }: { markdown: string }) => <pre>{markdown}</pre>,
}));
const run: DiscoveryRun = {
  id: "portfolio",
  revision: 3,
  definition: {
    ...newDiscovery(),
    prompt: "Research historical manuscript transmission",
    shortlistCount: 3,
    paperCount: 2,
  },
  state: "awaitingSelection",
  reason: "Choose projects",
  phase: { kind: "select" },
  phaseLabel: "Choose projects",
  candidateCount: 3,
  orientation: {
    fields: ["History"],
    subjectIds: [],
    methodIds: [],
    researchStandards: ["Source criticism"],
    assumptions: [],
    constraints: [],
  },
  selection: {
    shortlist: [1, 2, 3],
    recommended: [1, 2],
    rationale: "Complementary source questions",
  },
  selectionHash: "version-A",
  selected: [],
  ranking: [],
  actionsReserved: 80,
  activeSeconds: 10,
  deadlineAt: 9999999999,
  activeChild: null,
  workspaceId: "project",
  sourceSessionId: "source",
  papers: [],
};
const candidates: Candidate[] = [1, 2, 3].map((id) => ({
  id,
  proposal: {
    title: `Project ${id}`,
    question: "A historical research question",
    contribution: "A new interpretation",
    method: "Source criticism",
    firstTest: "Compare sources",
    requiredEvidence: ["Primary texts"],
    relatedWork: [],
    risks: ["Coverage"],
  },
  assessments: [],
  previousVersions: [],
}));
beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.invoke.mockImplementation(async (name: string) => {
    if (name === "task_sessions")
      return [
        { id: "source", title: "Source materials", workspaceName: "History" },
      ];
    if (name === "list_profiles" || name === "discovery_list") return [];
    if (name === "discovery_get") return run;
    if (name === "discovery_candidates") return candidates;
    if (name === "discovery_start" || name === "discovery_select")
      return { ...run, state: "running" };
    return null;
  });
});
it("starts prompt-only unsupervised research once with the exact name and default funnel", async () => {
  const started = vi.fn();
  render(
    <Builder
      initialMode="unsupervised"
      onStarted={started}
      onClose={vi.fn()}
    />,
  );
  expect(screen.getByLabelText("Mode")).toHaveValue("unsupervised");
  expect(
    screen.getByRole("option", {
      name: "Research discovery — choose projects yourself",
    }),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("option", {
      name: "Research discovery — let Pipeline choose projects",
    }),
  ).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Research topics and questions"), {
    target: { value: "Study source transmission in medieval archives" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Start research discovery" }),
  );
  await waitFor(() => expect(started).toHaveBeenCalledOnce());
  const starts = mocks.invoke.mock.calls.filter(
    (c) => c[0] === "discovery_start",
  );
  expect(starts).toHaveLength(1);
  expect(starts[0][1].request).toMatchObject({
    sessionId: null,
    definition: {
      mode: "unsupervised",
      candidateCount: 75,
      shortlistCount: 10,
      paperCount: 5,
      proposalRevisionPasses: 1,
      acquireLiterature: true,
    },
  });
  expect(
    screen.queryByRole("button", { name: "Develop selected projects" }),
  ).not.toBeInTheDocument();
});
it("enforces fewer than ten papers and K no larger than N", () => {
  expect(
    discoveryError({ ...newDiscovery(), prompt: "Research", paperCount: 10 }),
  ).toContain("1–9");
  expect(
    discoveryError({
      ...newDiscovery(),
      prompt: "Research",
      paperCount: 5,
      shortlistCount: 4,
    }),
  ).toContain("shortlist");
});
it("retains the operation identity when a Start response is uncertain", async () => {
  mocks.invoke.mockImplementation(async (name: string) => {
    if (name === "discovery_start") throw new Error("Connection interrupted");
    return [];
  });
  render(
    <Builder initialMode="supervised" onStarted={vi.fn()} onClose={vi.fn()} />,
  );
  fireEvent.change(screen.getByLabelText("Research topics and questions"), {
    target: { value: "A theoretical physics question" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Start research discovery" }),
  );
  await screen.findByRole("alert");
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Start research discovery" }),
    ).toBeEnabled(),
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Start research discovery" }),
  );
  await waitFor(() =>
    expect(
      mocks.invoke.mock.calls.filter((c) => c[0] === "discovery_start"),
    ).toHaveLength(2),
  );
  const calls = mocks.invoke.mock.calls.filter(
    (c) => c[0] === "discovery_start",
  );
  expect(calls[0][1].request.operationId).toBe(calls[1][1].request.operationId);
});
it("binds supervised selection to the displayed shortlist and prevents over-selection", async () => {
  const updated = vi.fn();
  render(<Detail run={run} onUpdated={updated} onClose={vi.fn()} />);
  const third = await screen.findByRole("checkbox", { name: "3. Project 3" });
  expect(third).toBeDisabled();
  fireEvent.click(screen.getByRole("checkbox", { name: "1. Project 1" }));
  expect(third).toBeEnabled();
  fireEvent.click(third);
  fireEvent.click(
    screen.getByRole("button", { name: "Develop selected projects" }),
  );
  await waitFor(() => expect(updated).toHaveBeenCalledOnce());
  expect(mocks.invoke).toHaveBeenCalledWith(
    "discovery_select",
    expect.objectContaining({
      id: "portfolio",
      selectionHash: "version-A",
      candidateIds: [2, 3],
    }),
  );
});
it("shows server rejection without losing the research or changing the selected IDs", async () => {
  mocks.invoke.mockImplementation(async (name: string) => {
    if (name === "discovery_candidates") return candidates;
    if (name === "discovery_select")
      throw new Error("The shortlist changed; reload");
    return null;
  });
  render(<Detail run={run} onUpdated={vi.fn()} onClose={vi.fn()} />);
  await screen.findByRole("checkbox", { name: "1. Project 1" });
  fireEvent.click(
    screen.getByRole("button", { name: "Develop selected projects" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "shortlist changed",
  );
  expect(screen.getByRole("checkbox", { name: "1. Project 1" })).toBeChecked();
});

it("opens the exact portfolio selected by a shell notification", async () => {
  render(<SelfDiscovery initialRunId="portfolio" onBack={vi.fn()} />);
  await screen.findByRole("heading", { name: "Choose projects to develop" });
  expect(mocks.invoke).toHaveBeenCalledWith("discovery_get", {
    id: "portfolio",
  });
});

it("keeps blank lines while typing input files and drops them at Start", async () => {
  render(
    <Builder
      initialMode="unsupervised"
      initialSessionId="source"
      onStarted={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  fireEvent.change(screen.getByLabelText("Research topics and questions"), {
    target: { value: "Study source transmission" },
  });
  const files = screen.getByLabelText("Research input files");
  for (const value of ["data/a.csv\n", "data/a.csv\n\n data/b.csv "]) {
    fireEvent.change(files, { target: { value } });
    expect(files).toHaveValue(value);
  }
  fireEvent.click(
    screen.getByRole("button", { name: "Start research discovery" }),
  );
  await waitFor(() =>
    expect(
      mocks.invoke.mock.calls.find((c) => c[0] === "discovery_start")?.[1]
        .request.definition.inputPaths,
    ).toEqual(["data/a.csv", "data/b.csv"]),
  );
});
it("says why Start is unavailable", () => {
  render(
    <Builder
      initialMode="unsupervised"
      onStarted={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  const start = screen.getByRole("button", {
    name: "Start research discovery",
  });
  expect(start).toBeDisabled();
  expect(start).toHaveAccessibleDescription(
    "Describe the topics or questions to research.",
  );
  fireEvent.change(screen.getByLabelText("Research topics and questions"), {
    target: { value: "Study source transmission" },
  });
  fireEvent.change(screen.getByLabelText("Shortlisted projects"), {
    target: { value: "3" },
  });
  expect(start).toBeDisabled();
  expect(start).toHaveAccessibleDescription(
    "Choose 1–9 papers, no more than the shortlist size.",
  );
});
it("opens a paper in a bounded reader and focuses it", async () => {
  const scroll = vi.fn();
  const original = Element.prototype.scrollIntoView;
  Element.prototype.scrollIntoView = scroll;
  const manuscript = {
    title: "Transmission paper",
    abstractText: "Abstract",
    markdown: "# Transmission paper",
    latex: "",
    bibliography: "",
    evidenceIds: [],
    limitations: [],
    responseToReview: [],
    files: [],
  };
  mocks.invoke.mockImplementation(async (name: string) =>
    name === "discovery_paper"
      ? {
          candidateId: 1,
          state: "completed",
          reason: "",
          rounds: [],
          challenges: [],
          evidence: {},
          assessment: null,
          versions: [
            {
              version: 2,
              hash: "h",
              manuscript,
              artifacts: {},
              reviews: [],
              externalReview: null,
            },
          ],
        }
      : null,
  );
  try {
    render(
      <Detail
        run={{
          ...run,
          state: "completed",
          papers: [
            {
              candidateId: 1,
              title: "Transmission paper",
              state: "completed",
              reason: "",
              rounds: 2,
              versions: 2,
              reviewCount: 0,
              hash: "h",
              assessment: null,
            },
          ],
        }}
        onUpdated={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    const opener = screen.getByRole("button", {
      name: "Read paper and reviews",
    });
    fireEvent.click(opener);
    const heading = await screen.findByRole("heading", {
      name: "Transmission paper",
      level: 3,
    });
    await waitFor(() => expect(heading).toHaveFocus());
    expect(scroll).toHaveBeenCalled();
    const viewer = await screen.findByText("# Transmission paper");
    expect(viewer.closest(".discovery-reader")).not.toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Close paper reader" }));
    expect(opener).toHaveFocus();
  } finally {
    Element.prototype.scrollIntoView = original;
  }
});
