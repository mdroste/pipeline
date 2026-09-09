import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
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
vi.mock("../../lib/workbenchClient", () => ({
  workbenchClient: { listExecutionProfiles: mocks.profiles },
}));
vi.mock("../research-studio/Jobs", () => ({
  default: () => <div>Job status</div>,
}));
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
