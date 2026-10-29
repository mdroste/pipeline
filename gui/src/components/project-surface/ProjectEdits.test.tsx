import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { expect, it, vi } from "vitest";
import ProjectEdits from "./ProjectEdits";
import type { SurfaceApi } from "./shared";
import type { ProjectHome } from "../../lib/projectClient";
import { makeProjectHome } from "../../test/projectSurfaceFixtures";

const mocks = vi.hoisted(() => ({ preview: vi.fn() }));
vi.mock("../../lib/projectClient", async (original) => ({
  ...(await original<typeof import("../../lib/projectClient")>()),
  projectClient: { preview: mocks.preview },
}));

const texts: Record<string, string> = {
  before: "intro\nold line\noutro\n",
  after: "intro\nnew line\noutro\n",
  revised: "intro\nnewer line\noutro\n",
};
const entry = (hash: string) => ({
  path: "paper.tex",
  hash,
  size: 20,
  status: "current",
  executable: false,
});
function home(proposed = "after"): ProjectHome {
  return {
    ...makeProjectHome(),
    changes: [
      {
        id: "edit",
        workspaceId: "workspace",
        kind: "checkpoint",
        revision: 1,
        updatedAt: "now",
        body: {
          taskId: "task",
          backend: "copy",
          rootIdentity: "root",
          taskRoot: "/copy",
          taskRootIdentity: "copy",
          base: { "paper.tex": entry("before") },
          proposed: { "paper.tex": entry(proposed) },
          state: "review",
          instruction: "Tighten the introduction",
          git: null,
          limitations: [],
          checkedExecutionIds: [],
          capturedAt: "now",
        },
      },
    ],
  };
}

it("keeps the open diff and its hunk choices when the project refreshes", async () => {
  let finish!: () => void;
  const revisedReady = new Promise<void>((resolve) => {
    finish = resolve;
  });
  mocks.preview.mockImplementation(async (_workspace: string, hash: string) => {
    if (hash === "revised") await revisedReady;
    return { text: texts[hash], imageUrl: null, hash, size: 20 };
  });
  const api = {
    workspaceId: "workspace",
    workspace: null,
    act: vi.fn(),
    reportError: vi.fn(),
    setTab: vi.fn(),
    taskId: "",
    setTaskId: vi.fn(),
  } as unknown as SurfaceApi;
  const view = render(
    <ProjectEdits {...api} data={home()} onConversation={vi.fn()} />,
  );
  fireEvent.click(screen.getByRole("button", { name: "paper.tex" }));
  const hunk = await screen.findByRole("checkbox", {
    name: /Accept change at line/,
  });
  await waitFor(() => expect(hunk).toBeChecked());
  fireEvent.click(hunk);
  expect(hunk).not.toBeChecked();
  view.rerender(
    <ProjectEdits {...api} data={home()} onConversation={vi.fn()} />,
  );
  await act(async () => undefined);
  expect(mocks.preview).toHaveBeenCalledTimes(2);
  expect(screen.queryByText("Loading changes…")).toBeNull();
  expect(
    screen.getByRole("checkbox", { name: /Accept change at line/ }),
  ).not.toBeChecked();

  // A new proposal reloads the preview without blanking the current diff.
  view.rerender(
    <ProjectEdits {...api} data={home("revised")} onConversation={vi.fn()} />,
  );
  expect(mocks.preview).toHaveBeenCalledTimes(4);
  expect(screen.queryByText("Loading changes…")).toBeNull();
  expect(screen.getByText(/new line/)).toBeInTheDocument();
  await act(async () => finish());
  expect(await screen.findByText(/newer line/)).toBeInTheDocument();
});
