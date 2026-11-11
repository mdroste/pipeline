import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceProjectSurface from "../WorkspaceProjectSurface";
import { makeProjectHome } from "../../test/projectSurfaceFixtures";
import type { WorkspaceDestination } from "../../lib/workspaceNavigation";

const mocks = vi.hoisted(() => ({
  home: vi.fn(),
  mutate: vi.fn(),
  getWorkspace: vi.fn(),
  listExecutionProfiles: vi.fn(),
  records: vi.fn(),
  programCall: vi.fn(),
}));
vi.mock("../../lib/projectClient", async (original) => ({
  ...(await original<typeof import("../../lib/projectClient")>()),
  projectClient: mocks,
}));
vi.mock("../../lib/workbench/workspaceClient", () => ({
  workspaceClient: { getWorkspace: mocks.getWorkspace },
}));
vi.mock("../../lib/workbench/executionClient", () => ({
  executionClient: {
    listExecutionProfiles: mocks.listExecutionProfiles,
    listStructuredResults: vi.fn().mockResolvedValue([]),
  },
}));
vi.mock("../../lib/deskClient", async (original) => ({
  ...(await original<typeof import("../../lib/deskClient")>()),
  deskClient: { records: mocks.records },
}));
vi.mock("../../lib/programClient", async (original) => ({
  ...(await original<typeof import("../../lib/programClient")>()),
  programClient: { call: mocks.programCall },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));
vi.mock("../research-studio/Jobs", () => ({ default: () => null }));
vi.mock("../WorkspaceResearchPanel", () => ({
  default: ({ title }: { title: string }) => <h2>{title}</h2>,
}));

const profile = (id: string, name: string) => ({
  id,
  name,
  adapter: "command",
  revision: 1,
});
const surface = (destination: WorkspaceDestination) => (
  <WorkspaceProjectSurface
    workspaceId="workspace"
    onConversation={vi.fn()}
    onWorkspaceChanged={vi.fn()}
    destination={destination}
  />
);

beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  mocks.home.mockResolvedValue(makeProjectHome());
  mocks.getWorkspace.mockResolvedValue({
    id: "workspace",
    name: "Local research",
    root: "/research",
    revision: 1,
  });
  mocks.mutate.mockResolvedValue({ id: "x", revision: 1, body: {} });
  mocks.records.mockResolvedValue([]);
  mocks.programCall.mockResolvedValue({
    objects: [],
    plans: [],
    profiles: [],
    executions: [],
    checkpoints: [],
  });
  mocks.listExecutionProfiles.mockResolvedValue([profile("old", "First")]);
});
afterEach(cleanup);

it("loads a view's own records only after that view is opened", async () => {
  render(surface("overview"));
  await screen.findByRole("heading", { name: "Local research" });
  await waitFor(() => expect(mocks.home).toHaveBeenCalled());
  expect(mocks.listExecutionProfiles).not.toHaveBeenCalled();
  expect(mocks.records).not.toHaveBeenCalledWith("workspace", "execution_plan");
});
