import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Literature from "./Literature";
import type { ProjectHome } from "../../lib/projectClient";

const mocks = vi.hoisted(() => ({
  records: vi.fn(),
  citations: vi.fn(),
  source: vi.fn(),
  listSources: vi.fn(),
  importSource: vi.fn(),
  open: vi.fn(),
  confirm: vi.fn(),
}));
vi.mock("../../lib/studioClient", async (original) => ({
  ...(await original<typeof import("../../lib/studioClient")>()),
  studioClient: {
    records: mocks.records,
    citations: mocks.citations,
    source: mocks.source,
  },
}));
vi.mock("../../lib/workbench/researchClient", () => ({
  researchClient: {
    listSources: mocks.listSources,
    importSource: mocks.importSource,
  },
}));
vi.mock("../DialogService", async (original) => ({
  ...(await original<typeof import("../DialogService")>()),
  confirmDialog: mocks.confirm,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));

const data = {
  settings: { body: { manuscriptRevisionId: "manuscript" } },
  papers: [{ paper: { title: "Draft" }, revision: { id: "manuscript" } }],
  anchors: [],
} as unknown as ProjectHome;
const saved = {
  id: "n1",
  revision: 1,
  body: {
    note: {
      question: "Saved question",
      statement: "Saved claim",
      citationKey: "",
      sourceVersionId: null,
      start: null,
      end: null,
      quote: "",
      identityChecked: false,
      support: "unsupported",
      method: "manual",
      relatedVersionIds: [],
    },
    checklist: {
      access: "full",
      passageRelevance: "none",
      assessmentMethod: "manual",
    },
  },
};
const view = (onDocument = vi.fn()) => (
  <Literature
    workspaceId="ws"
    data={data}
    onRefresh={async () => {}}
    onDocument={onDocument}
  />
);

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listSources.mockResolvedValue([]);
  mocks.records.mockImplementation(async (_w: string, kind: string) =>
    kind === "literature" ? [saved] : [],
  );
});
afterEach(cleanup);

it("opens the manuscript at a citation's own bytes", async () => {
  mocks.citations.mockResolvedValue({
    citations: [
      { key: "card1994", start: 120, end: 128, sources: [], notes: [] },
    ],
  });
  const onDocument = vi.fn();
  render(view(onDocument));
  fireEvent.click(
    await screen.findByRole("button", { name: "Find literal TeX citations" }),
  );
  fireEvent.click(await screen.findByText(/card1994 · bytes 120/));
  expect(onDocument).toHaveBeenCalledWith("manuscript", {
    start: 120,
    end: 128,
  });
});

it("says what the citation search can miss", async () => {
  mocks.citations.mockResolvedValue({
    citations: [],
    coverage: "Finds citation commands written out in the TeX source.",
  });
  render(view());
  fireEvent.click(
    await screen.findByRole("button", { name: "Find literal TeX citations" }),
  );
  expect(
    await screen.findByText(
      "Finds citation commands written out in the TeX source.",
    ),
  ).toBeInTheDocument();
});

it("adding a source drops the old passage and keeps a key left blank", async () => {
  const passage = {
    ...saved,
    body: {
      ...saved.body,
      note: {
        ...saved.body.note,
        citationKey: "card1994",
        sourceVersionId: "old-version",
        start: 4,
        end: 9,
        quote: "wages",
        support: "supports",
      },
    },
  };
  mocks.records.mockImplementation(async (_w: string, kind: string) =>
    kind === "literature" ? [passage] : [],
  );
  mocks.open.mockResolvedValue("/project/new.pdf");
  mocks.importSource.mockResolvedValue({
    source: { versionId: "new-version" },
  });
  render(view());
  fireEvent.click(
    await screen.findByRole("button", { name: "Saved question" }),
  );
  await waitFor(() =>
    expect(screen.getByLabelText("Citation key")).toHaveValue("card1994"),
  );
  fireEvent.change(screen.getByLabelText(/Source title/), {
    target: { value: "A newer version" },
  });
  fireEvent.click(screen.getByText("Choose and retain local source"));
  await waitFor(() => expect(mocks.importSource).toHaveBeenCalledOnce());
  await waitFor(() =>
    expect(screen.getByLabelText("Passage assessment")).toHaveValue(
      "unsupported",
    ),
  );
  expect(screen.getByLabelText("Citation key")).toHaveValue("card1994");
  expect(
    screen.getByText(/Exact passage:\s+None — supporting evidence/),
  ).toBeInTheDocument();
});

it("asks before a question in the table replaces an unsaved note", async () => {
  render(view());
  const question = await screen.findByLabelText("Research question");
  const row = await screen.findByRole("button", { name: "Saved question" });
  fireEvent.change(question, { target: { value: "My unsaved question" } });

  mocks.confirm.mockResolvedValueOnce(false);
  fireEvent.click(row);
  await waitFor(() => expect(mocks.confirm).toHaveBeenCalledOnce());
  await act(async () => {});
  expect(question).toHaveValue("My unsaved question");

  mocks.confirm.mockResolvedValueOnce(true);
  fireEvent.click(row);
  await waitFor(() => expect(question).toHaveValue("Saved question"));
});

it("brings the note form into view from the comparison table", async () => {
  const scroll = vi.fn();
  Element.prototype.scrollIntoView = scroll;
  render(view());
  fireEvent.click(
    await screen.findByRole("button", { name: "Saved question" }),
  );
  const form = screen
    .getByText(/Literature note and source-support/)
    .closest("section");
  await waitFor(() => expect(form).toHaveFocus());
  expect(scroll.mock.contexts).toContain(form);
  delete (Element.prototype as Partial<Element>).scrollIntoView;
});

it("stops paging at the end of a saved source, on a fresh pane each time", async () => {
  mocks.records.mockImplementation(async (_w: string, kind: string) =>
    kind === "literature"
      ? [
          {
            ...saved,
            body: {
              ...saved.body,
              note: { ...saved.body.note, sourceVersionId: "version" },
            },
          },
        ]
      : [],
  );
  const segment = (start: number, text: string) => ({
    text,
    start,
    end: start + text.length,
    accessState: "full",
  });
  mocks.source.mockResolvedValueOnce(segment(0, "First segment"));
  render(view());
  fireEvent.click(
    await screen.findByRole("button", { name: "Saved question" }),
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Read saved source text" }),
  );
  const first = await screen.findByText("First segment");
  const next = screen.getByRole("button", { name: "Next source segment" });
  mocks.source.mockResolvedValueOnce(segment(13, "Second segment"));
  fireEvent.click(next);
  const second = await screen.findByText("Second segment");
  // A new element starts at its top, not at the old scroll offset.
  expect(second).not.toBe(first);

  await waitFor(() => expect(next).toBeEnabled());
  mocks.source.mockResolvedValueOnce(segment(27, ""));
  fireEvent.click(next);
  await screen.findByText(/This is the end of the saved source/);
  expect(next).toBeDisabled();
});

it("marks a sender's assessment and says when a list shows only its newest", async () => {
  const imported = {
    ...saved,
    body: {
      ...saved.body,
      importedFrom: "namespace",
      importedStatus: "researcher_checked",
    },
  };
  mocks.records.mockImplementation(async (_w: string, kind: string) =>
    kind === "literature"
      ? [imported]
      : Array.from({ length: 500 }, (_, n) => ({
          id: `b${n}`,
          body: { entry: { key: `k${n}` }, source: {} },
        })),
  );
  render(view());
  expect(
    await screen.findByText(/the sender checked the source identity/),
  ).toBeInTheDocument();
  expect(
    screen.getByText(/most recently imported entries/),
  ).toBeInTheDocument();
});
