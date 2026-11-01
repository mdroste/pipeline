import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Assets from "./Assets";

const mocks = vi.hoisted(() => ({ call: vi.fn(), records: vi.fn() }));
vi.mock("../../lib/programClient", () => ({
  programClient: { call: mocks.call },
}));
vi.mock("../../lib/deskClient", async (original) => ({
  ...(await original<typeof import("../../lib/deskClient")>()),
  deskClient: { records: mocks.records },
}));

const choices = {
  objects: [
    {
      reference: { kind: "result", id: "run/beta", revision: "abcdef12" },
      title: "Main estimate",
      label: "Main estimate",
      recordKind: null,
    },
    {
      reference: { kind: "record", id: "series-1", revision: "1" },
      title: "series",
      label: "output · irf_1 · full",
      recordKind: "series",
    },
  ],
  profiles: [],
  executions: [],
  checkpoints: [],
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.call.mockImplementation(async (_workspace, action) =>
    action.action === "choices" ? choices : {},
  );
  mocks.records.mockResolvedValue([]);
});
afterEach(cleanup);

it("offers a Python script profile for figures and leaves out other programs", async () => {
  const user = userEvent.setup();
  mocks.call.mockImplementation(async (_workspace, action) =>
    action.action === "choices"
      ? {
          ...choices,
          profiles: [
            {
              id: "py",
              name: "Python script",
              adapter: "python",
              argv: ["python3", "a.py"],
            },
            {
              id: "cmd",
              name: "Venv command",
              adapter: "command",
              argv: [".venv/bin/python3.12", "a.py"],
            },
            {
              id: "win",
              name: "Windows command",
              adapter: "command",
              argv: ["C:\\Py\\python.exe", "a.py"],
            },
            {
              id: "other",
              name: "Plain command",
              adapter: "command",
              argv: ["/usr/bin/true"],
            },
            {
              id: "r",
              name: "R script",
              adapter: "r",
              argv: ["Rscript", "a.R"],
            },
          ],
        }
      : {},
  );
  render(<Assets workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />);
  await user.selectOptions(screen.getByLabelText("Asset format"), "irf");

  const series = await screen.findByRole("option", {
    name: /output · irf_1 · full · series/,
  });
  expect(series).toBeInTheDocument();
  const python = screen.getByLabelText("Local Python profile with matplotlib");
  expect(
    within(python)
      .getAllByRole("option")
      .map((option) => option.textContent),
  ).toEqual([
    "Choose configured Python toolchain",
    "Python script",
    "Venv command",
    "Windows command",
  ]);
});

it("tells the project when a save succeeds, and clears old failures first", async () => {
  const user = userEvent.setup();
  const onChanged = vi.fn();
  const onActionStart = vi.fn();
  render(
    <Assets
      workspaceId="w"
      onOpen={vi.fn()}
      onError={vi.fn()}
      onChanged={onChanged}
      onActionStart={onActionStart}
    />,
  );
  await user.selectOptions(
    await screen.findByLabelText("Structured numeric result"),
    await screen.findByRole("option", { name: /Main estimate/ }),
  );
  await user.click(
    screen.getByRole("button", { name: "Add result in this order" }),
  );
  await user.click(
    screen.getByRole("button", { name: "Generate table draft" }),
  );
  await waitFor(() => expect(onChanged).toHaveBeenCalledOnce());
  expect(onActionStart).toHaveBeenCalledOnce();
});

it("saves a negative manual value exactly as typed", async () => {
  const user = userEvent.setup();
  render(<Assets workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />);
  await screen.findByRole("option", { name: /Main estimate/ });
  await user.selectOptions(
    screen.getByLabelText("Structured numeric result"),
    screen.getByRole("option", { name: /Main estimate/ }),
  );
  await user.click(
    screen.getByRole("button", { name: "Add result in this order" }),
  );
  await user.click(
    screen.getByRole("checkbox", { name: "Record a manual value" }),
  );
  const field = screen.getByLabelText("Manual value");
  await user.clear(field);
  await user.type(field, "-0.25");
  await user.type(screen.getByLabelText("Reason for override"), "From table 2");
  await user.click(
    screen.getByRole("button", { name: "Generate table draft" }),
  );

  await waitFor(() =>
    expect(mocks.call).toHaveBeenCalledWith(
      "w",
      expect.objectContaining({ action: "table" }),
    ),
  );
  const saved = mocks.call.mock.calls.find(
    ([, action]) => action.action === "table",
  )?.[1];
  expect(saved.spec.entries[0].manual).toEqual({
    value: -0.25,
    reason: "From table 2",
  });
});

it("brings the form into view when editing a saved table", async () => {
  const scroll = vi.fn();
  Element.prototype.scrollIntoView = scroll;
  mocks.records.mockImplementation(async (_w: string, kind: string) =>
    kind === "publication_asset"
      ? [
          {
            id: "asset",
            title: "Table 1",
            contentHash: "abcdef123456",
            createdAt: "2026-09-07T00:00:00Z",
            body: {
              spec: {
                title: "Table 1",
                kind: "table",
                entries: [],
                digits: 3,
                uncertainty: "none",
                notes: "",
              },
              entries: [],
              artifacts: [],
            },
          },
        ]
      : [],
  );
  render(<Assets workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />);
  await userEvent.click(
    await screen.findByRole("button", { name: "Edit as new version" }),
  );
  const form = screen.getByText("Tables & figures").closest("section");
  await waitFor(() => expect(form).toHaveFocus());
  expect(scroll.mock.contexts).toContain(form);
  delete (Element.prototype as Partial<Element>).scrollIntoView;
});
