import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import Discovery from "./Discovery";

const mocks = vi.hoisted(() => ({
  network: vi.fn(),
  records: vi.fn(),
  crossrefContact: vi.fn(),
}));
vi.mock("../../lib/deskClient", () => ({ deskClient: mocks }));
vi.mock("../../lib/workbench/researchClient", () => ({
  researchClient: { importSource: vi.fn(), listSources: vi.fn() },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

beforeEach(() => {
  mocks.network.mockReset().mockResolvedValue(true);
  mocks.records.mockReset().mockResolvedValue([]);
  mocks.crossrefContact.mockReset().mockResolvedValue(null);
});

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
