import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import Experiments from "./Experiments";
import type { ProjectHome } from "../../lib/projectClient";

const mocks = vi.hoisted(() => ({
  confirm: vi.fn(),
  listStructuredResults: vi.fn(),
  listExecutionProfiles: vi.fn(),
  saveExecutionProfile: vi.fn(),
  records: vi.fn(),
  compare: vi.fn(),
  getWorkspace: vi.fn(),
}));
vi.mock("../../lib/studioClient", async (original) => ({
  ...(await original<typeof import("../../lib/studioClient")>()),
  studioClient: { records: mocks.records, compare: mocks.compare },
}));
vi.mock("../../lib/workbench/executionClient", () => ({
  executionClient: {
    listStructuredResults: mocks.listStructuredResults,
    listExecutionProfiles: mocks.listExecutionProfiles,
    saveExecutionProfile: mocks.saveExecutionProfile,
  },
}));
vi.mock("../../lib/workbench/workspaceClient", () => ({
  workspaceClient: { getWorkspace: mocks.getWorkspace },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./Jobs", () => ({ JobLauncher: () => null }));
vi.mock("../DialogService", async (original) => ({
  ...(await original<typeof import("../DialogService")>()),
  confirmDialog: mocks.confirm,
}));

const data = { executions: [] } as unknown as ProjectHome;
const result = (resultId: string, units: string) => ({
  resultId,
  sourceExecutionId: `run-${resultId}`,
  units,
  estimand: resultId,
  specificationId: "s",
  sampleId: "n",
});
const results = [result("left", "percent"), result("right", "points")];
const view = (active?: boolean) => (
  <Experiments
    workspaceId="w"
    data={data}
    onRefresh={async () => {}}
    onDocument={() => {}}
    active={active}
  />
);

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listStructuredResults.mockResolvedValue(results);
  mocks.listExecutionProfiles.mockResolvedValue([]);
  mocks.records.mockResolvedValue([]);
  mocks.compare.mockResolvedValue({ comparable: true, blockers: [] });
  mocks.getWorkspace.mockResolvedValue({ root: "/research" });
});

it("records a negative factor and offset in a unit conversion", async () => {
  const user = userEvent.setup();
  render(view());
  const baseline = screen.getByLabelText("Baseline result");
  await screen.findAllByRole("option", { name: /left/ });
  await user.selectOptions(baseline, "run-left/left");
  await user.selectOptions(
    screen.getByLabelText("Alternative result"),
    "run-right/right",
  );
  await user.click(
    screen.getByRole("checkbox", {
      name: /Record a conversion from alternative units/,
    }),
  );
  const factor = screen.getByLabelText("Multiply points by");
  await user.clear(factor);
  await user.type(factor, "-2.5");
  const offset = screen.getByLabelText("Then add (in percent)");
  await user.clear(offset);
  await user.type(offset, "-0.25");
  await user.click(
    screen.getByRole("button", { name: "Compare declared quantities" }),
  );

  await waitFor(() => expect(mocks.compare).toHaveBeenCalled());
  expect(mocks.compare.mock.calls[0][4]).toMatchObject({
    factor: -2.5,
    offset: -0.25,
  });
});

it("reloads saved results each time the view comes back", async () => {
  const page = render(view(true));
  await screen.findAllByRole("option", { name: /left/ });
  expect(mocks.listStructuredResults).toHaveBeenCalledTimes(1);
  mocks.listStructuredResults.mockResolvedValue([
    ...results,
    result("new", "percent"),
  ]);

  page.rerender(view(false));
  page.rerender(view(false));
  await act(async () => undefined);
  expect(mocks.listStructuredResults).toHaveBeenCalledTimes(1);

  page.rerender(view(true));
  await screen.findAllByRole("option", { name: /new/ });
  expect(mocks.listStructuredResults).toHaveBeenCalledTimes(2);
});

it("asks before another experiment replaces unsaved notebook text", async () => {
  const user = userEvent.setup();
  mocks.records.mockImplementation(async (_w: string, kind: string) =>
    kind === "experiment"
      ? [
          {
            id: "exp-1",
            revision: 1,
            body: {
              question: "Saved question",
              baselineExecutionId: "",
              intendedChange: "",
              executionIds: [],
              interpretation: "",
            },
          },
        ]
      : [],
  );
  render(view());
  const selector = screen.getByLabelText("Experiment");
  await screen.findByRole("option", { name: "Saved question" });
  const question = screen.getByLabelText("Research question");
  await user.type(question, "My unsaved question");

  mocks.confirm.mockResolvedValueOnce(false);
  await user.selectOptions(selector, "exp-1");
  await waitFor(() => expect(mocks.confirm).toHaveBeenCalledOnce());
  expect(question).toHaveValue("My unsaved question");
  expect(selector).toHaveValue("");

  mocks.confirm.mockResolvedValueOnce(true);
  await user.selectOptions(selector, "exp-1");
  await waitFor(() => expect(question).toHaveValue("Saved question"));
});

it("offers Stata without one person's shell alias", async () => {
  render(view());
  const toolchain = await screen.findByLabelText("Toolchain");
  const options = within(toolchain).getAllByRole("option");
  expect(options.map((option) => option.textContent).join()).not.toMatch(
    /oldstata/,
  );
  expect(document.body.textContent).not.toMatch(/oldstata/);

  await userEvent.selectOptions(toolchain, "stata");
  await userEvent.click(
    screen.getByRole("button", { name: "Save local profile" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Enter the Stata command under Advanced argv, then save.",
  );
  expect(mocks.saveExecutionProfile).not.toHaveBeenCalled();
});

it("keeps a saved Stata profile a Stata profile", async () => {
  mocks.listExecutionProfiles.mockResolvedValue([
    {
      id: "stata",
      name: "Main do-file",
      adapter: "stata",
      argv: ["/bin/zsh", "-lic", "mystata -q -b do main.do"],
      cwd: "/project",
      inputs: ["main.do"],
      outputs: ["main.log"],
      timeoutSeconds: 300,
      revision: 2,
    },
  ]);
  mocks.saveExecutionProfile.mockResolvedValue({ id: "stata" });
  render(view());
  await screen.findByRole("option", { name: "Main do-file" });
  await userEvent.selectOptions(
    screen.getByLabelText("Configured profile"),
    "stata",
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Save local profile" }),
  );
  await waitFor(() =>
    expect(mocks.saveExecutionProfile).toHaveBeenCalledWith(
      expect.objectContaining({
        adapter: "stata",
        argv: ["/bin/zsh", "-lic", "mystata -q -b do main.do"],
      }),
    ),
  );
});
