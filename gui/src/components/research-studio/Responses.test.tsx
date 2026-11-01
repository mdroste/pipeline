import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Responses from "./Responses";
import type { ProjectHome } from "../../lib/projectClient";

const mocks = vi.hoisted(() => ({
  records: vi.fn(),
  mutate: vi.fn(),
  confirm: vi.fn(),
  exportResponses: vi.fn(),
}));
vi.mock("../../lib/studioClient", async (original) => ({
  ...(await original<typeof import("../../lib/studioClient")>()),
  studioClient: {
    records: mocks.records,
    mutate: mocks.mutate,
    exportResponses: mocks.exportResponses,
  },
}));
vi.mock("../DialogService", async (original) => ({
  ...(await original<typeof import("../DialogService")>()),
  confirmDialog: mocks.confirm,
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

const data = {
  papers: [],
  tasks: [],
  applications: [],
  executions: [],
  anchors: [
    {
      id: "a1",
      body: { selection: { quote: "Changed passage" }, body: "" },
    },
  ],
} as unknown as ProjectHome;
const response = (id: string, number: string, draft: string) => ({
  id,
  revision: 1,
  body: {
    decision: {
      number,
      category: "observed_error",
      severity: "high",
      disposition: "open",
      intendedResponse: "",
      draft,
      rationale: "",
      reportsAnalysisAdded: false,
      taskId: null,
      manuscriptRevisionId: null,
      applicationId: null,
      executionId: null,
      evidenceAnchorIds: [],
      disputedPremise: "",
      counterargument: "",
      resolvingCheck: "",
    },
    source: { comment: { text: `Comment ${number}` } },
    flags: [],
  },
});
const view = (onReviewHandoff?: () => void, active?: boolean) => (
  <Responses
    workspaceId="ws"
    data={data}
    onRefresh={async () => {}}
    onDocument={() => {}}
    onReviewHandoff={onReviewHandoff}
    active={active}
  />
);

beforeEach(() => {
  vi.clearAllMocks();
  mocks.records.mockResolvedValue([
    response("r1", "1", "First draft"),
    response("r2", "2", "Second draft"),
  ]);
});
afterEach(cleanup);

it("keeps an unsaved reply when the prompt for another comment is cancelled", async () => {
  render(view());
  fireEvent.click(await screen.findByRole("button", { name: /^1\./ }));
  const draft = screen.getByLabelText("Draft response");
  fireEvent.change(draft, { target: { value: "A long reply" } });

  mocks.confirm.mockResolvedValueOnce(false);
  fireEvent.click(screen.getByRole("button", { name: /^2\./ }));
  await waitFor(() => expect(mocks.confirm).toHaveBeenCalledOnce());
  await act(async () => {});
  expect(draft).toHaveValue("A long reply");

  mocks.confirm.mockResolvedValueOnce(true);
  fireEvent.click(screen.getByRole("button", { name: /^2\./ }));
  await waitFor(() =>
    expect(screen.getByLabelText("Draft response")).toHaveValue("Second draft"),
  );
});

it("moves between comments without asking when nothing has changed or it was saved", async () => {
  mocks.mutate.mockResolvedValue({});
  render(view());
  fireEvent.click(await screen.findByRole("button", { name: /^1\./ }));
  fireEvent.click(screen.getByRole("button", { name: /^2\./ }));
  const draft = screen.getByLabelText("Draft response");
  expect(draft).toHaveValue("Second draft");

  fireEvent.change(draft, { target: { value: "Saved reply" } });
  fireEvent.click(screen.getByText("Save response decision"));
  await waitFor(() => expect(mocks.mutate).toHaveBeenCalled());
  await waitFor(() =>
    expect(screen.getByText("Save response decision")).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole("button", { name: /^1\./ }));
  await waitFor(() =>
    expect(screen.getByLabelText("Draft response")).toHaveValue("First draft"),
  );
  expect(mocks.confirm).not.toHaveBeenCalled();
});

it("offers Focused re-review without the findings import being ticked", async () => {
  render(view(vi.fn()));
  await screen.findByRole("button", { name: /^1\./ });
  expect(
    screen.getByRole("checkbox", {
      name: "Enable importing review findings",
    }),
  ).not.toBeChecked();
  expect(screen.getByText("Focused re-review")).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Preview review of selected passages" }),
  ).toBeInTheDocument();
});

it("leaves Focused re-review out where no review can be started", async () => {
  render(view());
  await screen.findByRole("button", { name: /^1\./ });
  expect(screen.queryByText("Focused re-review")).toBeNull();
});

describe("the response letter", () => {
  const letter = {
    text: "Dear editor",
    warnings: [],
    assessment: "Ready",
    format: "markdown",
    draft: false,
  };
  const preview = async () => {
    render(view());
    fireEvent.click(await screen.findByLabelText("Select response 1"));
    fireEvent.click(screen.getByText(/Preview selected responses/));
    await screen.findByText("Dear editor");
  };
  beforeEach(() => mocks.exportResponses.mockResolvedValue(letter));

  it("is withdrawn once a selected reply is saved", async () => {
    await preview();
    mocks.mutate.mockResolvedValue({});
    mocks.records.mockResolvedValue([
      { ...response("r1", "1", "Changed reply"), revision: 2 },
      response("r2", "2", "Second draft"),
    ]);
    fireEvent.click(screen.getByRole("button", { name: /^1\./ }));
    fireEvent.click(screen.getByText("Save response decision"));
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /^Export/ })).toBeNull(),
    );
  });

  it("is withdrawn when the selection changes", async () => {
    await preview();
    fireEvent.click(screen.getByLabelText("Select response 2"));
    expect(screen.queryByRole("button", { name: /^Export/ })).toBeNull();
  });

  it("is withdrawn when the format changes", async () => {
    await preview();
    fireEvent.change(screen.getByLabelText("Letter format"), {
      target: { value: "latex" },
    });
    expect(screen.queryByRole("button", { name: /^Export/ })).toBeNull();
  });

  it("stays through a reload that changes nothing", async () => {
    const page = render(view(undefined, true));
    fireEvent.click(await screen.findByLabelText("Select response 1"));
    fireEvent.click(screen.getByText(/Preview selected responses/));
    await screen.findByText("Dear editor");
    mocks.records.mockClear();
    page.rerender(view(undefined, false));
    page.rerender(view(undefined, true));
    await waitFor(() => expect(mocks.records).toHaveBeenCalled());
    await act(async () => {});
    expect(screen.getByRole("button", { name: /^Export/ })).toBeInTheDocument();
  });

  it("does not show a preview that finishes after the selection moved on", async () => {
    let finish!: (value: typeof letter) => void;
    mocks.exportResponses.mockReturnValue(
      new Promise((resolve) => (finish = resolve)),
    );
    render(view());
    fireEvent.click(await screen.findByLabelText("Select response 1"));
    fireEvent.click(screen.getByText(/Preview selected responses/));
    fireEvent.click(screen.getByLabelText("Select response 2"));
    await act(async () => finish(letter));
    expect(screen.queryByText("Dear editor")).toBeNull();
  });
});

it("brings a comment's form into view", async () => {
  const scroll = vi.fn();
  Element.prototype.scrollIntoView = scroll;
  render(view());
  fireEvent.click(await screen.findByRole("button", { name: /^1\./ }));
  const form = screen.getByLabelText("Comment number").closest("[tabindex]");
  await waitFor(() => expect(form).toHaveFocus());
  expect(scroll.mock.contexts).toContain(form);
  delete (Element.prototype as Partial<Element>).scrollIntoView;
});
