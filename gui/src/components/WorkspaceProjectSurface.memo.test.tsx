import { act, cleanup, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, expect, it, vi } from "vitest";
import WorkspaceProjectSurface from "./WorkspaceProjectSurface";
import { makeProjectHome as home } from "../test/projectSurfaceFixtures";
import { router } from "../lib/router";

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
  default: (props: unknown) => {
    mocks.destination(props);
    return <p>Project views</p>;
  },
}));

afterEach(cleanup);

it("follows a full-text file hit at its line and a later hit in the same file", async () => {
  router.init({
    page: "workspace",
    projectId: "workspace",
    file: "analysis/code.do",
    fileLine: 73,
  });
  mocks.home.mockResolvedValue(home());
  mocks.getWorkspace.mockResolvedValue({
    id: "workspace",
    name: "Project",
    root: "/research",
    revision: 1,
  });
  render(
    <WorkspaceProjectSurface
      workspaceId="workspace"
      onConversation={vi.fn()}
      onWorkspaceChanged={vi.fn()}
    />,
  );
  await screen.findByText("Project views");
  expect(mocks.destination.mock.lastCall?.[0]).toEqual(
    expect.objectContaining({
      tab: "files",
      fileLocation: expect.objectContaining({
        path: "analysis/code.do",
        line: 73,
      }),
    }),
  );
  await act(async () => {
    await router.navigate({
      page: "workspace",
      projectId: "workspace",
      file: "analysis/code.do",
      fileLine: 90,
    });
  });
  expect(mocks.destination.mock.lastCall?.[0]).toEqual(
    expect.objectContaining({
      fileLocation: expect.objectContaining({ line: 90 }),
    }),
  );
  router.init({ page: "home" });
});

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

it("opens the requested document version after project data loads", async () => {
  router.init({
    page: "workspace",
    projectId: "workspace",
    surface: "project",
    destination: "documents",
    revisionId: "exact-version",
    documentHash: "saved-hash",
    documentStart: 2048,
    documentEnd: 2060,
    documentPage: 3,
  });
  mocks.home.mockResolvedValue(home());
  mocks.getWorkspace.mockResolvedValue({
    id: "workspace",
    name: "Project",
    root: "/research",
    revision: 1,
  });
  render(
    <WorkspaceProjectSurface
      workspaceId="workspace"
      onConversation={vi.fn()}
      onWorkspaceChanged={vi.fn()}
    />,
  );
  await screen.findByText("Project views");
  await act(async () => {});
  expect(mocks.destination.mock.lastCall?.[0]).toEqual(
    expect.objectContaining({
      tab: "documents",
      reading: expect.objectContaining({
        revisionId: "exact-version",
        initialSelection: expect.objectContaining({
          revisionId: "exact-version",
          revisionHash: "saved-hash",
          start: 2048,
          end: 2060,
          page: 3,
        }),
      }),
    }),
  );
  router.init({ page: "home" });
});
