import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Decisions from "./Decisions";

const mocks = vi.hoisted(() => ({
  records: vi.fn(),
  impact: vi.fn(),
  context: vi.fn(),
  relation: vi.fn(),
  home: vi.fn(),
}));
vi.mock("../../lib/deskClient", async (original) => ({
  ...(await original<typeof import("../../lib/deskClient")>()),
  deskClient: mocks,
}));
vi.mock("../../lib/projectClient", () => ({
  projectClient: { home: mocks.home },
}));
vi.mock("../../lib/workbench/researchClient", () => ({ researchClient: {} }));
vi.mock("../../lib/programClient", () => ({ programClient: {} }));

const item = (id: string) => ({
  role: "source",
  object: { kind: "source", id, revision: `${id}-version` },
});
const [first, second, third] = ["first", "second", "third"].map(item);

beforeEach(() => {
  vi.clearAllMocks();
  mocks.records.mockResolvedValue([]);
  mocks.impact.mockResolvedValue({
    impacts: [],
    complete: true,
    limitations: [],
  });
  mocks.home.mockResolvedValue({ contextPreview: "" });
  mocks.relation.mockResolvedValue({});
  mocks.context.mockResolvedValue({ items: [first, second, third] });
});
afterEach(cleanup);

async function choose() {
  render(
    <Decisions
      workspaceId="w"
      sessionId="s"
      onOpen={vi.fn()}
      onError={vi.fn()}
    />,
  );
  const options = await screen.findAllByRole("option", { name: /first/ });
  fireEvent.change(screen.getByLabelText("Dependency Input"), {
    target: { value: (options[0] as HTMLOptionElement).value },
  });
  fireEvent.change(screen.getByLabelText("Dependency Dependent"), {
    target: {
      value: (
        screen.getAllByRole("option", {
          name: /second/,
        })[1] as HTMLOptionElement
      ).value,
    },
  });
  fireEvent.change(screen.getByLabelText("Dependency reason"), {
    target: { value: "Uses its sample" },
  });
}

const refreshSources = async () => {
  fireEvent.click(
    screen.getByRole("button", { name: "Refresh sources and impact" }),
  );
  await waitFor(() => expect(mocks.context).toHaveBeenCalledTimes(2));
};

it("saves the sources that were chosen after a refresh reorders them", async () => {
  await choose();
  mocks.context.mockResolvedValue({ items: [third, second, first] });
  await refreshSources();
  await screen.findAllByRole("option", { name: /third/ });
  fireEvent.click(screen.getByRole("button", { name: "Save link" }));
  await waitFor(() => expect(mocks.relation).toHaveBeenCalledOnce());
  expect(mocks.relation.mock.calls[0][1]).toMatchObject({
    input: first.object,
    dependent: second.object,
  });
});

it("will not save a link to a source the refresh removed", async () => {
  await choose();
  mocks.context.mockResolvedValue({ items: [second, third] });
  await refreshSources();
  expect(await screen.findByLabelText("Dependency Input")).toHaveDisplayValue(
    "No longer in this conversation",
  );
  expect(screen.getByRole("button", { name: "Save link" })).toBeDisabled();
});
