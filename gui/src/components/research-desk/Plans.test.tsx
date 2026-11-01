import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import Plans from "./Plans";
const mocks = vi.hoisted(() => ({
  records: vi.fn(),
  planStatus: vi.fn(),
  profiles: vi.fn(),
}));
vi.mock("../../lib/deskClient", () => ({
  deskClient: mocks,
  reference: vi.fn(),
}));
vi.mock("../../lib/workbench/executionClient", () => ({
  executionClient: { listExecutionProfiles: mocks.profiles },
}));
vi.mock("../research-studio/Jobs", () => ({
  default: () => <div>Job status</div>,
}));
afterEach(cleanup);

it("says what is still needed to capture a plan", async () => {
  mocks.profiles.mockResolvedValue([{ id: "p", name: "Estimate" }]);
  mocks.records.mockResolvedValue([]);
  render(<Plans workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />);
  fireEvent.change(
    await screen.findByLabelText("Execution profile to capture"),
    { target: { value: "p" } },
  );
  await screen.findByText("To capture a plan, enter the toolchain version.");
  expect(
    screen.getByRole("button", { name: "Capture inputs and review plan" }),
  ).toBeDisabled();
  fireEvent.change(screen.getByLabelText("Toolchain version"), {
    target: { value: "Stata 18" },
  });
  expect(screen.queryByText(/To capture a plan/)).toBeNull();
});

it("brings the plan under review into view", async () => {
  const scroll = vi.fn();
  Element.prototype.scrollIntoView = scroll;
  const plan = {
    id: "First",
    title: "First capture",
    createdAt: "2026-09-07T00:00:00Z",
    contentHash: "First",
    body: { profile: { id: "profile" } },
  };
  mocks.profiles.mockResolvedValue([]);
  mocks.records.mockImplementation(async (_ws, kind) =>
    kind === "execution_plan" ? [plan] : [],
  );
  mocks.planStatus.mockResolvedValue({
    record: plan,
    authorized: false,
    testStatus: null,
  });
  render(<Plans workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: /First capture/ }));
  const review = (await screen.findByText("Review exact plan")).closest(
    "section",
  );
  await waitFor(() => expect(review).toHaveFocus());
  expect(scroll.mock.contexts).toContain(review);
  delete (Element.prototype as Partial<Element>).scrollIntoView;
});

it("can switch the reviewed captured plan without restoring the previous preview", async () => {
  const plans = ["First", "Second"].map((id) => ({
    id,
    workspaceId: "w",
    kind: "execution_plan",
    title: `${id} capture`,
    createdAt: "2026-09-07T00:00:00Z",
    contentHash: id,
    body: { description: `${id} inputs`, profile: { id: "profile" } },
  }));
  mocks.profiles.mockResolvedValue([]);
  mocks.records.mockImplementation(async (_ws, kind) =>
    kind === "execution_plan" ? plans : [],
  );
  mocks.planStatus.mockImplementation(async (_ws, id) => ({
    record: plans.find((p) => p.id === id),
    authorized: false,
    testStatus: null,
  }));
  render(<Plans workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: /First capture/ }));
  await screen.findByText(/"description": "First inputs"/);
  fireEvent.click(screen.getByRole("button", { name: /Second capture/ }));
  await waitFor(() =>
    expect(
      screen.queryByText(/"description": "First inputs"/),
    ).not.toBeInTheDocument(),
  );
  expect(
    screen.getByText(/"description": "Second inputs"/),
  ).toBeInTheDocument();
});
