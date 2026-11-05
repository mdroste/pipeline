import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ProjectEdits from "./ProjectEdits";
import type { SurfaceApi } from "./shared";
import type { ProjectHome } from "../../lib/projectClient";
import { makeProjectHome } from "../../test/projectSurfaceFixtures";

const mocks = vi.hoisted(() => ({
  preview: vi.fn(),
  mutate: vi.fn(),
  confirm: vi.fn(),
}));
vi.mock("../DialogService", async (original) => ({
  ...(await original<typeof import("../DialogService")>()),
  confirmDialog: mocks.confirm,
}));
vi.mock("../../lib/projectClient", async (original) => ({
  ...(await original<typeof import("../../lib/projectClient")>()),
  projectClient: { preview: mocks.preview, mutate: mocks.mutate },
}));

const texts: Record<string, string> = {
  before: "intro\nold line\noutro\n",
  after: "intro\nnew line\noutro\n",
  revised: "intro\nnewer line\noutro\n",
  two: "alpha\nmiddle\nomega\n",
  twoEdited: "ALPHA\nmiddle\nOMEGA\n",
  firstAccepted: "ALPHA\nmiddle\nomega\n",
};
const entry = (hash: string) => ({
  path: "paper.tex",
  hash,
  size: 20,
  status: "current",
  executable: false,
});
function home(
  proposed = "after",
  { base = "before", revision = 1, state = "review" } = {},
): ProjectHome {
  return {
    ...makeProjectHome(),
    changes: [
      {
        id: "edit",
        workspaceId: "workspace",
        kind: "checkpoint",
        revision,
        updatedAt: "now",
        body: {
          taskId: "task",
          backend: "copy",
          rootIdentity: "root",
          taskRoot: "/copy",
          taskRootIdentity: "copy",
          base: { "paper.tex": entry(base) },
          proposed: { "paper.tex": entry(proposed) },
          state,
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

describe("accepting selected changes", () => {
  const twoHunks = () => home("twoEdited", { base: "two" });
  const surface = (data: ProjectHome, refresh: () => void = vi.fn()) =>
    ({
      workspaceId: "workspace",
      workspace: null,
      act: refresh,
      reportError: vi.fn(),
      setTab: vi.fn(),
      taskId: "",
      setTaskId: vi.fn(),
      data,
      onConversation: vi.fn(),
    }) as unknown as SurfaceApi & {
      onConversation: () => Promise<void>;
    };
  const hunks = () =>
    screen.queryAllByRole("checkbox", { name: /Accept change at line/ });
  beforeEach(() => {
    mocks.preview.mockReset();
    mocks.mutate.mockReset();
    mocks.mutate.mockResolvedValue({ id: "apply", revision: 1, body: {} });
    mocks.preview.mockImplementation(
      async (_workspace: string, hash: string) => ({
        text: texts[hash],
        imageUrl: null,
        hash,
        size: 20,
      }),
    );
  });

  it("records them on the edit, shrinks the diff, and confirms beside the button", async () => {
    const refresh = vi.fn(async () => undefined);
    const view = render(<ProjectEdits {...surface(twoHunks(), refresh)} />);
    const refreshed = (data: ProjectHome) =>
      view.rerender(<ProjectEdits {...surface(data, refresh)} />);
    fireEvent.click(screen.getByRole("button", { name: "paper.tex" }));
    await screen.findByText("2 changes");

    await waitFor(() => expect(hunks()[1]).toBeChecked());
    fireEvent.click(hunks()[1]);
    fireEvent.click(
      screen.getByRole("button", { name: "Accept 1 selected change" }),
    );

    await waitFor(() =>
      expect(mocks.mutate).toHaveBeenCalledWith("workspace", {
        action: "acceptChanges",
        checkpointId: "edit",
        path: "paper.tex",
        content: texts.firstAccepted,
        expectedRevision: 1,
      }),
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Accepted 1 change.",
    );
    expect(refresh).toHaveBeenCalledWith({ action: "refresh" });

    // The refreshed edit has a new base, so only the other change is left, and
    // the next acceptance names the new revision.
    refreshed(home("twoEdited", { base: "firstAccepted", revision: 2 }));
    await screen.findByText("1 change");
    expect(screen.getByRole("status")).toHaveTextContent("Accepted 1 change.");
    await waitFor(() => expect(hunks()).toHaveLength(1));
    fireEvent.click(
      await screen.findByRole("button", { name: "Accept 1 selected change" }),
    );
    await waitFor(() =>
      expect(mocks.mutate).toHaveBeenLastCalledWith(
        "workspace",
        expect.objectContaining({
          content: texts.twoEdited,
          expectedRevision: 2,
        }),
      ),
    );

    // Nothing differs now, so the file leaves the list and its diff closes.
    refreshed(
      home("twoEdited", {
        base: "twoEdited",
        revision: 3,
        state: "accepted",
      }),
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "No changes remain in paper.tex.",
    );
    expect(screen.getByText("No files changed.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "paper.tex" })).toBeNull();
  });

  it("shows the reason in words when the file changed outside Pipeline", async () => {
    mocks.mutate.mockRejectedValue({
      code: "conflict",
      message: "External change detected in paper.tex; no changes applied",
    });
    const refresh = vi.fn();
    render(<ProjectEdits {...surface(twoHunks(), refresh)} />);
    fireEvent.click(screen.getByRole("button", { name: "paper.tex" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "Accept 2 selected changes" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "External change detected in paper.tex; no changes applied",
    );
    expect(screen.queryByText(/object Object/)).toBeNull();
    expect(screen.queryByRole("status")).toBeNull();
    expect(refresh).not.toHaveBeenCalled();
  });

  it("discards an edit only after the researcher confirms", async () => {
    const api = surface(twoHunks());
    render(<ProjectEdits {...api} />);
    mocks.confirm.mockResolvedValueOnce(false);
    fireEvent.click(screen.getByRole("button", { name: "Discard this edit" }));
    await waitFor(() => expect(mocks.confirm).toHaveBeenCalledOnce());
    await act(async () => {});
    expect(mocks.confirm.mock.calls[0][0]).toContain("stays on your computer");
    expect(api.act).not.toHaveBeenCalled();

    mocks.confirm.mockResolvedValueOnce(true);
    fireEvent.click(screen.getByRole("button", { name: "Discard this edit" }));
    await waitFor(() =>
      expect(api.act).toHaveBeenCalledWith({
        action: "reject",
        checkpointId: "edit",
      }),
    );
  });

  it("offers no change selection once the edit is no longer under review", async () => {
    render(
      <ProjectEdits
        {...surface(
          home("twoEdited", { base: "two", state: "partially_accepted" }),
        )}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "paper.tex" }));
    await screen.findByText("2 changes");
    expect(hunks()).toHaveLength(0);
    expect(
      screen.queryByRole("button", { name: /selected change/ }),
    ).toBeNull();
  });
});
