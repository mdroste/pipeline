import { act, cleanup, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, expect, it, vi } from "vitest";
import WorkspaceProjectSurface from "./WorkspaceProjectSurface";
import { makeProjectHome as home } from "../test/projectSurfaceFixtures";

const mocks = vi.hoisted(() => ({
  home: vi.fn(),
  getWorkspace: vi.fn(),
  destination: vi.fn(),
}));
vi.mock("../lib/projectClient", async (original) => ({
  ...(await original<typeof import("../lib/projectClient")>()),
  projectClient: { home: mocks.home },
}));
vi.mock("../lib/workbench/workspaceClient", () => ({
  workspaceClient: { getWorkspace: mocks.getWorkspace },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("./project-surface/ProjectDestination", () => ({
  default: () => {
    mocks.destination();
    return <p>Project views</p>;
  },
}));

afterEach(cleanup);

it("leaves the project views alone when its parent renders for something else", async () => {
  mocks.home.mockResolvedValue(home());
  mocks.getWorkspace.mockResolvedValue({
    id: "workspace",
    name: "Local research",
    root: "/research",
    revision: 1,
  });
  const onConversation = vi.fn();
  const onWorkspaceChanged = vi.fn();
  let tick = () => {};
  function Parent() {
    const [, setCount] = useState(0);
    tick = () => setCount((count) => count + 1);
    return (
      <WorkspaceProjectSurface
        workspaceId="workspace"
        onConversation={onConversation}
        onWorkspaceChanged={onWorkspaceChanged}
      />
    );
  }
  render(<Parent />);
  await screen.findByText("Project views");
  await act(async () => {});
  const renders = mocks.destination.mock.calls.length;
  for (let flush = 0; flush < 20; flush += 1) act(() => tick());
  expect(mocks.destination).toHaveBeenCalledTimes(renders);
});
