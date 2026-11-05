import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { open } from "@tauri-apps/plugin-dialog";
import WorkspaceProjectSurface from "./WorkspaceProjectSurface";
import type ProjectOverview from "./project-surface/ProjectOverview";
import type { ComponentProps } from "react";
import { appEvents } from "../lib/appEvents";
import { makeProjectHome } from "../test/projectSurfaceFixtures";

const mocks = vi.hoisted(() => ({
  home: vi.fn(),
  workspace: vi.fn(),
  listPapers: vi.fn(),
  listSources: vi.fn(),
  importPaper: vi.fn(),
  importSource: vi.fn(),
  prompt: vi.fn(),
  notify: vi.fn(),
  reader: vi.fn(),
  overview: vi.fn(),
}));
vi.mock("../lib/projectClient", async (original) => ({
  ...(await original<typeof import("../lib/projectClient")>()),
  projectClient: { home: mocks.home },
}));
vi.mock("../lib/workbench/workspaceClient", () => ({
  workspaceClient: { getWorkspace: mocks.workspace },
}));
vi.mock("../lib/workbench/researchClient", () => ({
  researchClient: {
    listPapers: mocks.listPapers,
    listSources: mocks.listSources,
    importPaper: mocks.importPaper,
    importSource: mocks.importSource,
  },
}));
vi.mock("./DialogService", async (original) => ({
  ...(await original<typeof import("./DialogService")>()),
  promptDialog: mocks.prompt,
  notify: mocks.notify,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("./WorkspaceDocumentReader", () => ({
  default: (props: unknown) => {
    mocks.reader(props);
    return <div>Reader fixture</div>;
  },
}));
vi.mock("./project-surface/ProjectOverview", () => ({
  default: (props: ComponentProps<typeof ProjectOverview>) => {
    mocks.overview(props);
    return <div>Overview fixture</div>;
  },
}));

const imported = {
  paper: { id: "paper", title: "New paper", role: "manuscript" },
  revision: {
    id: "revision",
    contentHash: "hash",
    capturedAt: "2026-10-01T10:00:00Z",
    inputKind: "pdf",
    extraction: { status: "complete" },
  },
};
const view = (destination: "sources" | "documents" | "overview") => (
  <WorkspaceProjectSurface
    workspaceId="workspace"
    destination={destination}
    onConversation={vi.fn()}
    onWorkspaceChanged={vi.fn()}
  />
);

beforeEach(() => {
  localStorage.clear();
  sessionStorage.clear();
  vi.clearAllMocks();
  mocks.home.mockResolvedValue(makeProjectHome());
  mocks.workspace.mockResolvedValue({
    id: "workspace",
    root: null,
    name: "Research",
    revision: 1,
  });
  mocks.listPapers.mockResolvedValue([]);
  mocks.listSources.mockResolvedValue([]);
  vi.mocked(open).mockResolvedValue("/research/new.pdf");
  mocks.prompt.mockResolvedValue("New paper");
  mocks.importPaper.mockResolvedValue(imported);
});
afterEach(cleanup);

it("reloads the project after Papers & sources imports a paper, without a folder", async () => {
  const page = render(view("sources"));
  const importButton = await screen.findByRole("button", {
    name: "Import paper",
  });
  await waitFor(() => expect(mocks.home).toHaveBeenCalledOnce());
  mocks.home.mockResolvedValue({ ...makeProjectHome(), papers: [imported] });
  fireEvent.click(importButton);
  await waitFor(() => expect(mocks.importPaper).toHaveBeenCalledOnce());
  await waitFor(() => expect(mocks.home).toHaveBeenCalledTimes(2));

  page.rerender(view("documents"));
  const picker = await screen.findByRole("combobox", { name: "Document" });
  expect(picker).toHaveTextContent("New paper");
});

it("clears the banner when a retried action succeeds", async () => {
  render(view("sources"));
  const importButton = await screen.findByRole("button", {
    name: "Import paper",
  });
  mocks.importPaper.mockRejectedValueOnce({ message: "The file is locked" });
  fireEvent.click(importButton);
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The file is locked",
  );
  fireEvent.click(importButton);
  await waitFor(() => expect(mocks.importPaper).toHaveBeenCalledTimes(2));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
});

it("reports a possible duplicate as a notice, not a failure", async () => {
  mocks.importSource.mockResolvedValue({ duplicateCandidates: [{}] });
  mocks.prompt.mockResolvedValue("A source");
  render(view("sources"));
  fireEvent.click(await screen.findByRole("button", { name: "Import source" }));
  await waitFor(() => expect(mocks.notify).toHaveBeenCalledOnce());
  expect(mocks.notify.mock.calls[0][0]).toContain("possible duplicate");
  expect(mocks.notify.mock.calls[0][1]).toBe("info");
  expect(screen.queryByRole("alert")).toBeNull();
});

it("opens a Search hit at its saved place with nothing selected", async () => {
  mocks.home.mockResolvedValue({ ...makeProjectHome(), papers: [imported] });
  render(view("documents"));
  await screen.findByText("Reader fixture");
  appEvents.emit("open-object", {
    workspaceId: "workspace",
    object: { kind: "paper", id: "revision", revision: "hash" },
  });
  await waitFor(() =>
    expect(mocks.reader.mock.lastCall![0]).toMatchObject({
      initialSelection: { start: null, quote: "", page: null, region: null },
    }),
  );
});

it("opens a paper from the Overview without the last passage's selection", async () => {
  const passage = {
    id: "anchor",
    body: {
      selection: {
        revisionId: "revision",
        revisionHash: "hash",
        start: 4,
        end: 9,
        page: null,
        region: null,
        quote: "Old passage",
      },
    },
  };
  mocks.home.mockResolvedValue({
    ...makeProjectHome(),
    papers: [imported],
    anchors: [passage],
  });
  const page = render(view("overview"));
  await screen.findByText("Overview fixture");
  const overview = () => mocks.overview.mock.lastCall![0];
  overview().openAnnotation(passage);
  page.rerender(view("documents"));
  await waitFor(() =>
    expect(mocks.reader.mock.lastCall![0].initialSelection?.quote).toBe(
      "Old passage",
    ),
  );
  overview().onOpenDocument("revision");
  await waitFor(() =>
    expect(mocks.reader.mock.lastCall![0].initialSelection).toBeUndefined(),
  );
});
