import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import PaperSelector from "./PaperSelector";

const openDialog = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openDialog }));

describe("PaperSelector", () => {
  beforeEach(() => {
    openDialog.mockReset();
  });

  it("renders the default placeholder when nothing is selected", () => {
    render(<PaperSelector onPathChange={() => {}} disabled={false} />);
    expect(screen.getByRole("button", { name: /select file/i })).toBeInTheDocument();
  });

  it("disables both buttons when disabled=true", () => {
    render(<PaperSelector onPathChange={() => {}} disabled={true} />);
    const buttons = screen.getAllByRole("button");
    for (const b of buttons) expect(b).toBeDisabled();
  });

  it("calls onPathChange with the chosen file and displays its basename", async () => {
    const onPathChange = vi.fn();
    openDialog.mockResolvedValueOnce("/papers/example/draft.pdf");

    render(<PaperSelector onPathChange={onPathChange} disabled={false} />);
    await userEvent.setup().click(screen.getByRole("button", { name: /select file/i }));

    expect(openDialog).toHaveBeenCalledWith({
      multiple: false,
      filters: [{ name: "Papers", extensions: ["pdf", "tex", "docx"] }],
    });
    expect(onPathChange).toHaveBeenCalledWith("/papers/example/draft.pdf");
    expect(screen.getByRole("button", { name: "draft.pdf" })).toBeInTheDocument();
    expect(screen.getByText("/papers/example/draft.pdf")).toBeInTheDocument();
  });

  it("does not call onPathChange when the file dialog is cancelled", async () => {
    const onPathChange = vi.fn();
    openDialog.mockResolvedValueOnce(null);

    render(<PaperSelector onPathChange={onPathChange} disabled={false} />);
    await userEvent.setup().click(screen.getByRole("button", { name: /select file/i }));

    expect(onPathChange).not.toHaveBeenCalled();
  });

  it("passes directory=true when the folder button is used", async () => {
    openDialog.mockResolvedValueOnce("/papers/latex-project");
    const onPathChange = vi.fn();
    render(<PaperSelector onPathChange={onPathChange} disabled={false} />);

    // The folder button has title="Select LaTeX project folder".
    const folderBtn = screen.getByTitle("Select LaTeX project folder");
    await userEvent.setup().click(folderBtn);

    expect(openDialog).toHaveBeenCalledWith({ directory: true, multiple: false });
    expect(onPathChange).toHaveBeenCalledWith("/papers/latex-project");
  });

  it("surfaces file picker failures while treating a resolved null as cancellation", async () => {
    openDialog.mockRejectedValueOnce(new Error("dialog plugin unavailable"));
    const onPathChange = vi.fn();

    render(<PaperSelector onPathChange={onPathChange} disabled={false} />);
    await userEvent.setup().click(screen.getByRole("button", { name: /select file/i }));

    expect(onPathChange).not.toHaveBeenCalled();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not open the file picker: dialog plugin unavailable",
    );
  });
});
