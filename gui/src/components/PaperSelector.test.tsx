import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import PaperSelector from "./PaperSelector";
import type { FileDropEvent } from "../lib/fileDrop";

const openDialog = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openDialog }));
const drop = vi.hoisted(() => ({
  emit: null as ((event: FileDropEvent) => void) | null,
  inputFiles: vi.fn(),
}));
vi.mock("../lib/fileDrop", () => ({
  onFileDrop: async (handler: (event: FileDropEvent) => void) => {
    drop.emit = handler;
    return () => {
      drop.emit = null;
    };
  },
}));
vi.mock("../lib/appClient", () => ({
  appClient: { inputFiles: drop.inputFiles },
}));

const dropFiles = async (paths: string[], target: Element) => {
  document.elementFromPoint = () => target;
  await act(async () => drop.emit!({ type: "drop", x: 10, y: 10, paths }));
};

describe("PaperSelector", () => {
  beforeEach(() => {
    openDialog.mockReset();
    drop.inputFiles.mockReset();
    localStorage.clear();
  });

  it("renders the default placeholder when nothing is selected", () => {
    render(<PaperSelector onPathChange={() => {}} disabled={false} />);
    expect(
      screen.getByRole("button", { name: /select file/i }),
    ).toBeInTheDocument();
  });

  it("disables both buttons when disabled=true", () => {
    render(<PaperSelector onPathChange={() => {}} disabled={true} />);
    const buttons = screen.getAllByRole("button");
    for (const b of buttons) expect(b).toBeDisabled();
  });

  it("calls onPathChange with the chosen file and displays its basename", async () => {
    const onPathChange = vi.fn();
    const onSelectionChange = vi.fn();
    openDialog.mockResolvedValueOnce("/papers/example/draft.pdf");

    render(
      <PaperSelector
        onPathChange={onPathChange}
        onSelectionChange={onSelectionChange}
        disabled={false}
      />,
    );
    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: /select file/i }));

    expect(openDialog).toHaveBeenCalledWith({
      multiple: true,
      filters: [
        {
          name: "Documents and slides",
          extensions: ["pdf", "tex", "docx", "pptx"],
        },
      ],
    });
    expect(onPathChange).toHaveBeenCalledWith("/papers/example/draft.pdf");
    expect(onSelectionChange).toHaveBeenCalledWith({
      paths: ["/papers/example/draft.pdf"],
      interpretation: "document",
      selectionKind: "file",
    });
    expect(
      screen.getByRole("button", { name: "draft.pdf" }),
    ).toBeInTheDocument();
    expect(screen.getByText("/papers/example/draft.pdf")).toBeInTheDocument();
  });

  it("does not call onPathChange when the file dialog is cancelled", async () => {
    const onPathChange = vi.fn();
    openDialog.mockResolvedValueOnce(null);

    render(<PaperSelector onPathChange={onPathChange} disabled={false} />);
    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: /select file/i }));

    expect(onPathChange).not.toHaveBeenCalled();
  });

  it("passes directory=true when the folder button is used", async () => {
    openDialog.mockResolvedValueOnce("/papers/latex-project");
    const onPathChange = vi.fn();
    render(<PaperSelector onPathChange={onPathChange} disabled={false} />);

    const folderBtn = screen.getByTitle("Select folder");
    await userEvent.setup().click(folderBtn);

    expect(openDialog).toHaveBeenCalledWith({
      directory: true,
      multiple: false,
    });
    expect(onPathChange).toHaveBeenCalledWith("/papers/latex-project");
    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: "Input options" }));
    expect(
      screen.getByRole("menuitemradio", { name: /^One LaTeX document/ }),
    ).toHaveAttribute("aria-checked", "true");
    expect(
      screen.queryByRole("option", { name: "Watch for new documents" }),
    ).not.toBeInTheDocument();
  });

  it("turns several selected files into an explicit batch", async () => {
    const onSelectionChange = vi.fn();
    openDialog.mockResolvedValueOnce(["/papers/a.pdf", "/papers/b.docx"]);
    render(
      <PaperSelector
        onPathChange={() => {}}
        onSelectionChange={onSelectionChange}
        disabled={false}
      />,
    );

    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: /select files/i }));

    expect(onSelectionChange).toHaveBeenCalledWith({
      paths: ["/papers/a.pdf", "/papers/b.docx"],
      interpretation: "batch",
      selectionKind: "file",
    });
    expect(screen.getByText("Batch · 2 independent reviews")).toBeVisible();
    expect(
      screen.queryByRole("combobox", { name: "Input interpretation" }),
    ).not.toBeInTheDocument();
  });

  it("lets a selected folder be reinterpreted as a source tree", async () => {
    const onSelectionChange = vi.fn();
    openDialog.mockResolvedValueOnce("/papers/project");
    render(
      <PaperSelector
        onPathChange={() => {}}
        onSelectionChange={onSelectionChange}
        disabled={false}
      />,
    );
    const user = userEvent.setup();
    await user.click(screen.getByTitle("Select folder"));
    await user.click(screen.getByRole("button", { name: "Input options" }));
    await user.click(
      screen.getByRole("menuitemradio", { name: /^Browsable source folder/ }),
    );

    expect(onSelectionChange).toHaveBeenLastCalledWith({
      paths: ["/papers/project"],
      interpretation: "source_tree",
      selectionKind: "folder",
    });
  });

  it("surfaces file picker failures while treating a resolved null as cancellation", async () => {
    openDialog.mockRejectedValueOnce(new Error("dialog plugin unavailable"));
    const onPathChange = vi.fn();

    render(<PaperSelector onPathChange={onPathChange} disabled={false} />);
    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: /select file/i }));

    expect(onPathChange).not.toHaveBeenCalled();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not open the file picker: dialog plugin unavailable",
    );
  });

  it("can clear and replace selections without showing recent inputs", async () => {
    const onPathChange = vi.fn();
    openDialog.mockResolvedValueOnce("/papers/first.pdf");
    render(<PaperSelector onPathChange={onPathChange} disabled={false} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /select file/i }));
    await user.click(screen.getByRole("button", { name: "Clear selection" }));
    expect(onPathChange).toHaveBeenLastCalledWith(null);
    expect(screen.queryByText("Recent input")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("combobox", { name: "Choose a recent input" }),
    ).not.toBeInTheDocument();
    openDialog.mockResolvedValueOnce("/papers/first.pdf");
    await user.click(screen.getByRole("button", { name: /select file/i }));
    openDialog.mockResolvedValueOnce("/papers/replacement.pdf");
    await user.click(screen.getByRole("button", { name: "Replace" }));
    expect(onPathChange).toHaveBeenLastCalledWith("/papers/replacement.pdf");
  });

  it("shows the selection its caller holds across remounts", () => {
    const selection = {
      paths: ["/papers/held.pdf"],
      interpretation: "document" as const,
      selectionKind: "file" as const,
    };
    const { unmount } = render(
      <PaperSelector
        selection={selection}
        onPathChange={() => {}}
        disabled={false}
      />,
    );
    unmount();
    const { rerender } = render(
      <PaperSelector
        selection={selection}
        onPathChange={() => {}}
        disabled={false}
      />,
    );
    expect(
      screen.getByRole("button", { name: "held.pdf" }),
    ).toBeInTheDocument();
    rerender(
      <PaperSelector
        selection={null}
        onPathChange={() => {}}
        disabled={false}
      />,
    );
    expect(
      screen.getByRole("button", { name: /select files/i }),
    ).toBeInTheDocument();
  });

  it("selects supported files dropped from the desktop onto the inputs", async () => {
    const onSelectionChange = vi.fn();
    render(
      <>
        <PaperSelector
          onPathChange={() => {}}
          onSelectionChange={onSelectionChange}
          disabled={false}
        />
        <div data-testid="elsewhere" />
      </>,
    );
    await dropFiles(["/papers/a.pdf"], screen.getByTestId("elsewhere"));
    expect(onSelectionChange).not.toHaveBeenCalled();

    const inputs = screen.getByRole("button", { name: "Select files..." });
    await dropFiles(["/papers/notes.txt"], inputs);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Only PDF, LaTeX, Word, and PowerPoint files can be dropped here.",
    );

    await dropFiles(
      [
        "/papers/a.PDF",
        "/papers/notes.txt",
        "/papers/b.docx",
        "/papers/deck.PPTX",
        "/papers/talk.tex",
      ],
      inputs,
    );
    expect(onSelectionChange).toHaveBeenLastCalledWith({
      paths: [
        "/papers/a.PDF",
        "/papers/b.docx",
        "/papers/deck.PPTX",
        "/papers/talk.tex",
      ],
      interpretation: "batch",
      selectionKind: "file",
    });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("accepts one dropped folder for a folder workflow", async () => {
    const onSelectionChange = vi.fn();
    drop.inputFiles.mockImplementation(async (path: string) => {
      if (path === "/projects/code") return [];
      throw new Error("Cannot read folder: Not a directory");
    });
    render(
      <PaperSelector
        inputMode="folder"
        onPathChange={() => {}}
        onSelectionChange={onSelectionChange}
        disabled={false}
      />,
    );
    const inputs = screen.getByRole("button", { name: "Select folder..." });
    await dropFiles(["/projects/readme.md"], inputs);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Drop one folder, or use Select folder.",
    );
    await dropFiles(["/projects/code"], inputs);
    expect(onSelectionChange).toHaveBeenLastCalledWith({
      paths: ["/projects/code"],
      interpretation: "source_tree",
      selectionKind: "folder",
    });
  });

  it("ignores drops while disabled", async () => {
    const onPathChange = vi.fn();
    render(<PaperSelector onPathChange={onPathChange} disabled={true} />);
    await dropFiles(
      ["/papers/a.pdf"],
      screen.getByRole("button", { name: "Select files..." }),
    );
    expect(onPathChange).not.toHaveBeenCalled();
  });

  it("keeps single-file batching in the input menu instead of a Use as field", async () => {
    const onSelectionChange = vi.fn();
    openDialog.mockResolvedValueOnce("/papers/a.pdf");
    render(
      <PaperSelector
        onPathChange={() => {}}
        onSelectionChange={onSelectionChange}
        disabled={false}
      />,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Select files..." }));
    expect(screen.queryByText("Use as")).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Input options" }));
    await user.click(
      screen.getByRole("menuitemradio", { name: "One-item batch" }),
    );
    expect(onSelectionChange).toHaveBeenLastCalledWith({
      paths: ["/papers/a.pdf"],
      selectionKind: "file",
      interpretation: "batch",
    });
    await user.click(screen.getByRole("button", { name: "Input options" }));
    await user.click(
      screen.getByRole("menuitemradio", { name: "One document" }),
    );
    expect(onSelectionChange).toHaveBeenLastCalledWith({
      paths: ["/papers/a.pdf"],
      selectionKind: "file",
      interpretation: "document",
    });
  });
});
