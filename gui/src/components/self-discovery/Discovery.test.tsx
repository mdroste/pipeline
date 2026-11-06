import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { listen } from "@tauri-apps/api/event";
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
vi.mock("./Activity", () => ({ default: () => null }));
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
    if (name === "discovery_activity")
      return {
        revision: run.revision,
        observedAt: Math.floor(Date.now() / 1000),
        lastActivityAt: null,
        step: null,
        entries: [],
        agents: [],
      };
    if (name === "discovery_candidates") return candidates;
    if (name === "discovery_start" || name === "discovery_select")
      return { ...run, state: "running" };
    return null;
  });
});

it("refreshes proposal assessments when the revision changes without changing candidate counts", async () => {
  const props = { run, onUpdated: vi.fn(), onClose: vi.fn() };
  const view = render(<Detail {...props} />);
  await screen.findByText(/Project 1/);
  mocks.invoke.mockImplementation(async (name: string) => {
    if (name === "discovery_candidates")
      return candidates.map((c) => ({
        ...c,
        proposal: { ...c.proposal, question: "A revised research question" },
      }));
    return null;
  });
  view.rerender(
    <Detail {...props} run={{ ...run, revision: run.revision + 1 }} />,
  );
  await screen.findAllByText("A revised research question");
  expect(
    screen.queryByText("A historical research question"),
  ).not.toBeInTheDocument();
});

it("refreshes an open paper without moving focus and ignores responses after closing", async () => {
  const makePaper = (text: string) => ({
    candidateId: 1,
    state: "drafting",
    reason: "",
    rounds: [],
    challenges: [],
    evidence: {},
    assessment: null,
    versions: [
      {
        version: 1,
        hash: "hash",
        reviews: [],
        manuscript: {
          title: "Live paper",
          abstractText: text,
          markdown: text,
          latex: "",
          bibliography: "",
          evidenceIds: [],
          limitations: [],
          responseToReview: [],
          files: [],
        },
      },
    ],
  });
  let pending: ((value: ReturnType<typeof makePaper>) => void) | undefined;
  mocks.invoke.mockImplementation(async (name: string) =>
    name === "discovery_paper" ? makePaper("First draft") : null,
  );
  const props = {
    run: {
      ...run,
      state: "running",
      papers: [
        {
          candidateId: 1,
          title: "Live paper",
          state: "drafting",
          reason: "",
          rounds: 1,
          versions: 1,
          reviewCount: 0,
          hash: "hash",
          assessment: null,
        },
      ],
    },
    onUpdated: vi.fn(),
    onClose: vi.fn(),
  };
  const view = render(<Detail {...props} />);
  fireEvent.click(
    screen.getByRole("button", { name: "Read paper and reviews" }),
  );
  await screen.findAllByText("First draft");
  await waitFor(() =>
    expect(
      screen.getByRole("heading", { name: "Live paper", level: 3 }),
    ).toHaveFocus(),
  );
  const close = screen.getByRole("button", { name: "Close paper reader" });
  close.focus();
  mocks.invoke.mockImplementation(async (name: string) =>
    name === "discovery_paper" ? makePaper("Revised draft") : null,
  );
  view.rerender(
    <Detail {...props} run={{ ...props.run, revision: run.revision + 1 }} />,
  );
  await screen.findAllByText("Revised draft");
  expect(close).toHaveFocus();
  mocks.invoke.mockImplementation((name: string) =>
    name === "discovery_paper"
      ? new Promise((resolve) => {
          pending = resolve;
        })
      : Promise.resolve(null),
  );
  view.rerender(
    <Detail {...props} run={{ ...props.run, revision: run.revision + 2 }} />,
  );
  await waitFor(() => expect(pending).toBeDefined());
  fireEvent.click(close);
  pending!(makePaper("Late draft"));
  await waitFor(() =>
    expect(
      screen.queryByRole("region", { name: "Paper reader" }),
    ).not.toBeInTheDocument(),
  );
  expect(screen.queryByText("Late draft")).not.toBeInTheDocument();
});
it("starts prompt-only unsupervised research once with the exact name and default funnel", async () => {
  const started = vi.fn();
  render(<Builder initialMode="unsupervised" onStarted={started} />);
  expect(
    screen.getByRole("radio", { name: /Let Pipeline choose/ }),
  ).toBeChecked();
  expect(screen.getByRole("radio", { name: /I’ll choose/ })).not.toBeChecked();
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
it("preserves a customized setup through history and submits the chosen settings", async () => {
  render(<SelfDiscovery onBack={vi.fn()} />);
  await screen.findByRole("option", { name: /Source materials/ });
  const change = (name: string, value: string) =>
    fireEvent.change(screen.getByLabelText(name), { target: { value } });
  change("Research topics and questions", "Study source transmission");
  change("Project context", "source");
  fireEvent.click(screen.getByRole("radio", { name: /Let Pipeline choose/ }));
  fireEvent.click(screen.getByText("Sources & tools"));
  expect(
    screen.getByLabelText(
      "Search public literature records and available abstracts",
    ),
  ).not.toBeChecked();
  change("Research input files", "sources/archive.txt");
  fireEvent.click(
    screen.getByText("Allow code and computation in isolated paper folders"),
  );
  fireEvent.click(screen.getByText("Research & review"));
  change("Idea subagents", "4");
  change("Detailed proposal reviewers", "0");
  change("Investigation rounds per paper", "6");
  fireEvent.click(
    screen.getByText("Challenge findings after each research round"),
  );
  fireEvent.click(screen.getByText("Models & ranking"));
  change("Reviewer model", "reviewer-override");
  change("Ranking priorities", "Strength of primary evidence.");
  fireEvent.click(screen.getByText("Resource limits"));
  change("Maximum time spent running (hours)", "3");
  change("Minutes per action", "12");
  fireEvent.click(screen.getByRole("button", { name: "Portfolio history" }));
  expect(screen.queryByRole("form")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Return to setup" }));
  expect(screen.getByLabelText("Research topics and questions")).toHaveValue(
    "Study source transmission",
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Start research discovery" }),
  );
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("discovery_start", {
      request: {
        operationId: expect.any(String),
        sessionId: "source",
        definition: {
          ...newDiscovery("unsupervised"),
          prompt: "Study source transmission",
          inputPaths: ["sources/archive.txt"],
          acquireLiterature: false,
          allowComputation: false,
          ideaAgents: 4,
          proposalReviewers: 0,
          researchRounds: 6,
          challengeResearch: false,
          reviewerModel: "reviewer-override",
          rankingPriorities: "Strength of primary evidence.",
          activeSeconds: 10800,
          actionTimeoutSeconds: 720,
        },
      },
    }),
  );
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
  render(<Builder initialMode="supervised" onStarted={vi.fn()} />);
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

it("offers Retry on a run that needs attention and labels the state", async () => {
  const updated = vi.fn();
  const attention = {
    ...run,
    state: "attention",
    reason: "A research action did not finish",
  };
  mocks.invoke.mockImplementation(async (name: string) =>
    name === "discovery_control" ? { ...attention, state: "running" } : [],
  );
  render(<Detail run={attention} onUpdated={updated} onClose={vi.fn()} />);
  expect(screen.getByRole("status")).toHaveTextContent("Needs attention");
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  await waitFor(() => expect(updated).toHaveBeenCalledOnce());
  expect(mocks.invoke).toHaveBeenCalledWith("discovery_control", {
    id: "portfolio",
    revision: 3,
    action: "retry",
  });
});

it("refreshes when a task event moves the run", async () => {
  const handlers = new Map<string, () => void>();
  vi.mocked(listen).mockImplementation(async (name, handler) => {
    handlers.set(name, () => handler({ event: name, id: 0, payload: null }));
    return () => {};
  });
  render(<SelfDiscovery initialRunId="portfolio" onBack={vi.fn()} />);
  await screen.findByText("Choose projects to develop");
  const reads = () =>
    mocks.invoke.mock.calls.filter((c) => c[0] === "discovery_get").length;
  const before = reads();
  await waitFor(() => expect(handlers.has("tasks:changed")).toBe(true));
  handlers.get("tasks:changed")!();
  await waitFor(() => expect(reads()).toBeGreaterThan(before));
});
it("carries out Pause and Stop on the newer run when the revision has moved", async () => {
  const running: DiscoveryRun = { ...run, state: "running" };
  const newer: DiscoveryRun = { ...running, revision: 9 };
  const updated = vi.fn();
  mocks.invoke.mockImplementation(async (name: string, args) => {
    if (name === "discovery_get") return newer;
    if (name === "discovery_control") {
      if (args.revision !== 9)
        throw "This research discovery changed; reload before continuing";
      return { ...newer, state: "paused", revision: 10 };
    }
    return [];
  });
  render(<Detail run={running} onUpdated={updated} onClose={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: /^Pause/ }));
  await waitFor(() =>
    expect(updated).toHaveBeenCalledWith(
      expect.objectContaining({ state: "paused", revision: 10 }),
    ),
  );
  expect(screen.queryByRole("alert")).toBeNull();
});
it("asks again before retrying a run that has moved", async () => {
  const stuck: DiscoveryRun = { ...run, state: "attention" };
  const updated = vi.fn();
  mocks.invoke.mockImplementation(async (name: string) => {
    if (name === "discovery_get") return { ...stuck, revision: 9 };
    if (name === "discovery_control")
      throw "This research discovery changed; reload before continuing";
    return [];
  });
  render(<Detail run={stuck} onUpdated={updated} onClose={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "This run just changed",
  );
  expect(updated).toHaveBeenCalledWith(
    expect.objectContaining({ revision: 9 }),
  );
  expect(
    mocks.invoke.mock.calls.filter((c) => c[0] === "discovery_control"),
  ).toHaveLength(1);
});
it("offers no Retry after a run has stopped for review", () => {
  render(
    <Detail
      run={{ ...run, state: "blocked" }}
      onUpdated={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  expect(screen.getByRole("status")).toHaveTextContent("Stopped: needs review");
  expect(
    screen.queryByRole("button", { name: "Retry" }),
  ).not.toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Stop" }),
  ).not.toBeInTheDocument();
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
  render(<Builder initialMode="unsupervised" onStarted={vi.fn()} />);
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

it("keeps delivery separate from evidence and accepts only the displayed paper version", async () => {
  const record = {
    schemaVersion: 1,
    subjectVersion: "paper-hash",
    assessorRole: "Final paper assessor",
    assessorChild: "child",
    claims: [],
    objections: ["Missing assumption"],
    supported: false,
    acceptedAt: null,
  };
  const p = {
    candidateId: 2,
    title: "Proposition",
    state: "incomplete",
    reason: "Draft written",
    rounds: 1,
    versions: 1,
    reviewCount: 0,
    hash: "paper-hash",
    assessment: null,
    draftDelivered: true,
    claimRecord: record,
  };
  const props = { onUpdated: vi.fn(), onClose: vi.fn() };
  const view = render(<Detail {...props} run={{ ...run, papers: [p] }} />);
  expect(screen.getByText("Draft delivered.")).toBeInTheDocument();
  expect(
    screen.getByText("Evidence requirements unresolved"),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Accept this assessment" }),
  ).toBeNull();
  const next = {
    ...p,
    claimRecord: { ...record, objections: [], supported: true },
  };
  view.rerender(
    <Detail {...props} run={{ ...run, revision: 6, papers: [next] }} />,
  );
  expect(
    mocks.invoke.mock.calls.some(
      ([cmd]) => cmd === "discovery_accept_assessment",
    ),
  ).toBe(false);
  fireEvent.click(
    screen.getByRole("button", { name: "Accept this assessment" }),
  );
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("discovery_accept_assessment", {
      id: run.id,
      revision: 6,
      candidateId: 2,
      subjectVersion: "paper-hash",
    }),
  );
});
