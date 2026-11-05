import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Data from "./Data";

const mocks = vi.hoisted(() => ({
  records: vi.fn(),
  policy: vi.fn(),
  importDataset: vi.fn(),
  importMetadata: vi.fn(),
  rows: vi.fn(),
  open: vi.fn(),
}));
vi.mock("../../lib/deskClient", async (original) => ({
  ...(await original<typeof import("../../lib/deskClient")>()),
  deskClient: mocks,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));
vi.mock("./DataConnectors", () => ({ default: () => null }));
vi.mock("./DatasetDictionary", () => ({ default: () => null }));
vi.mock("../WorkspaceContextTray", () => ({ addContextObject: vi.fn() }));

const dataset = {
  id: "d1",
  title: "Wages",
  contentHash: "abcdef123456",
  revision: 1,
  body: {
    rows: 3,
    columns: [],
    diagnosticsCoverage: "Full",
    acquisition: { requestedVintage: null },
    artifactId: "artifact",
  },
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.records.mockImplementation(async (_w: string, kind: string) =>
    kind === "dataset" ? [dataset] : [],
  );
  mocks.policy.mockResolvedValue({
    dictionary: true,
    summaries: true,
    assistantRows: false,
    packageData: false,
  });
});
afterEach(cleanup);

const view = () => <Data workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />;

it("titles a data file from a Windows path by its file name", async () => {
  mocks.open.mockResolvedValue("C:\\Users\\jane\\data\\wages.csv");
  mocks.importDataset.mockResolvedValue({});
  render(view());
  fireEvent.click(
    await screen.findByRole("button", { name: "Import data file" }),
  );
  await waitFor(() => expect(mocks.importDataset).toHaveBeenCalledOnce());
  expect(mocks.importDataset.mock.calls[0][1]).toBe("wages.csv");
});

it("gives the data dictionary import its own title", async () => {
  mocks.importMetadata.mockResolvedValue({});
  render(view());
  const importButton = await screen.findByRole("button", {
    name: "Import dataset details",
  });
  fireEvent.change(screen.getByLabelText("Dataset metadata JSON"), {
    target: { value: "{}" },
  });
  expect(importButton).toBeDisabled();
  expect(
    screen.getByText("Enter a dataset title and paste the JSON to import."),
  ).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Dictionary dataset title"), {
    target: { value: "Survey dictionary" },
  });
  fireEvent.click(importButton);
  await waitFor(() => expect(mocks.importMetadata).toHaveBeenCalledOnce());
  expect(mocks.importMetadata.mock.calls[0][1]).toBe("Survey dictionary");
});

it("brings a row preview into view", async () => {
  const scroll = vi.fn();
  Element.prototype.scrollIntoView = scroll;
  mocks.rows.mockResolvedValue({
    coverage: "All rows.",
    totalRows: 1,
    columns: ["a"],
    rows: [["1"]],
  });
  render(view());
  fireEvent.click(
    await screen.findByRole("button", { name: "Preview local rows" }),
  );
  const preview = (await screen.findByText(/Total rows/)).closest("section");
  await waitFor(() => expect(preview).toHaveFocus());
  expect(scroll.mock.contexts).toContain(preview);
  delete (Element.prototype as Partial<Element>).scrollIntoView;
});
