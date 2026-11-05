import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Discovery from "./Discovery";

const mocks = vi.hoisted(() => ({
  network: vi.fn(),
  records: vi.fn(),
  crossrefContact: vi.fn(),
  open: vi.fn(),
  importSource: vi.fn(),
}));
vi.mock("../../lib/deskClient", () => ({ deskClient: mocks }));
vi.mock("../../lib/workbench/researchClient", () => ({
  researchClient: { importSource: mocks.importSource, listSources: vi.fn() },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));

beforeEach(() => {
  mocks.network.mockReset().mockResolvedValue(true);
  mocks.records.mockReset().mockResolvedValue([]);
  mocks.crossrefContact.mockReset().mockResolvedValue(null);
});
afterEach(cleanup);

it("saves and removes the researcher's own Crossref contact email", async () => {
  render(<Discovery workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />);
  const email = await screen.findByLabelText("Crossref contact email");
  await waitFor(() => expect(mocks.crossrefContact).toHaveBeenCalledWith());
  expect(email).toHaveValue("");
  expect(screen.getByRole("button", { name: "Remove contact" })).toBeDisabled();
  expect(
    screen.getByText(/it never uses a shared address/),
  ).toBeInTheDocument();
  mocks.crossrefContact.mockResolvedValue("jane.doe@econ.ucla.edu");
  fireEvent.change(email, { target: { value: " jane.doe@econ.ucla.edu " } });
  fireEvent.click(screen.getByRole("button", { name: "Save contact" }));
  await waitFor(() =>
    expect(mocks.crossrefContact).toHaveBeenCalledWith(
      "jane.doe@econ.ucla.edu",
      true,
    ),
  );
  await waitFor(() => expect(email).toHaveValue("jane.doe@econ.ucla.edu"));
  fireEvent.change(email, { target: { value: "" } });
  fireEvent.click(screen.getByRole("button", { name: "Remove contact" }));
  await waitFor(() =>
    expect(mocks.crossrefContact).toHaveBeenCalledWith(null, true),
  );
});

it("keeps a half-typed contact when the view comes back, and picks up saved changes", async () => {
  mocks.crossrefContact.mockResolvedValue("old@econ.edu");
  const onError = vi.fn();
  const view = (active: boolean) => (
    <Discovery
      workspaceId="w"
      onOpen={vi.fn()}
      onError={onError}
      active={active}
    />
  );
  const page = render(view(true));
  const email = await screen.findByLabelText("Crossref contact email");
  await waitFor(() => expect(email).toHaveValue("old@econ.edu"));

  // An untouched field follows what was saved elsewhere.
  mocks.crossrefContact.mockResolvedValue("new@econ.edu");
  page.rerender(view(false));
  page.rerender(view(true));
  await waitFor(() => expect(email).toHaveValue("new@econ.edu"));

  fireEvent.change(email, { target: { value: "half.typed@" } });
  page.rerender(view(false));
  page.rerender(view(true));
  await waitFor(() => expect(mocks.records).toHaveBeenCalledTimes(3));
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Save contact" })).toBeEnabled(),
  );
  expect(email).toHaveValue("half.typed@");
});

it("reaches inbox items older than the first page", async () => {
  mocks.records.mockResolvedValue(
    Array.from({ length: 61 }, (_, i) => ({
      id: `r${i}`,
      title: `Lookup ${i}`,
      supersedes: null,
      createdAt: "2026-09-07T00:00:00Z",
      body: { provider: "Crossref", state: "unread" },
    })),
  );
  render(<Discovery workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />);
  await screen.findByText("Lookup 59");
  expect(screen.queryByText("Lookup 60")).toBeNull();
  expect(screen.getByText("Showing the 60 newest of 61 items.")).toBeVisible();

  fireEvent.click(screen.getByRole("button", { name: "Show older" }));
  expect(screen.getByText("Lookup 60")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Show older" })).toBeNull();
});

it("titles a local PDF from a Windows path by its file name", async () => {
  mocks.open.mockResolvedValue("C:\\Users\\jane\\papers\\card1994.pdf");
  mocks.importSource.mockResolvedValue({});
  render(<Discovery workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "Import local PDF" }),
  );
  await waitFor(() =>
    expect(mocks.importSource).toHaveBeenCalledWith(
      expect.objectContaining({ title: "card1994.pdf" }),
    ),
  );
});
