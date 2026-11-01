import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import FileWorkspace from "./FileWorkspace";
import type { FilePreview } from "../../lib/fileWorkspaceClient";

vi.mock("./SourceEditor", () => ({
  default: ({ value }: { value: string }) => <pre>{value}</pre>,
}));
beforeEach(() => localStorage.clear());

it.each(["/project", "\\\\?\\C:\\project"])(
  "uses the backend scope for nested Markdown links and images (%s)",
  async (root) => {
    const absolute = root.startsWith("/") ? "/project" : "C:/project";
    const read = vi.fn(async (path: string): Promise<FilePreview> => ({
      path,
      hash: path,
      bytes: 10,
      editable: false,
      truncated: false,
      externalPath: null,
      scopeRoot: root,
      mime: path.endsWith(".png") ? "image/png" : "text/plain",
      base64: path.endsWith(".png")
        ? path.startsWith("notes/")
          ? "UkVM"
          : "QUJT"
        : null,
      text: path.endsWith(".png")
        ? null
        : path === "notes/review.md"
          ? `[Root result](${absolute}/results.csv)\n\n[Local result](results.csv)\n\n![Root image](${absolute}/figure.png)\n\n![Local image](figure.png)`
          : path === "results.csv"
            ? "Root sentinel"
            : "Nested sentinel",
    }));
    render(
      <FileWorkspace
        adapter={{ id: "navigation", read }}
        paths={["notes/review.md"]}
        initial={{ path: "notes/review.md" }}
      />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Preview" }));
    expect(
      await screen.findByRole("img", { name: "Root image" }),
    ).toHaveAttribute("src", "data:image/png;base64,QUJT");
    expect(
      await screen.findByRole("img", { name: "Local image" }),
    ).toHaveAttribute("src", "data:image/png;base64,UkVM");
    fireEvent.click(screen.getByRole("link", { name: "Root result" }));
    await screen.findByText("Root sentinel");
    expect(read).toHaveBeenCalledWith("results.csv");
    fireEvent.click(screen.getByRole("tab", { name: "review.md" }));
    fireEvent.click(await screen.findByRole("link", { name: "Local result" }));
    await screen.findByText("Nested sentinel");
    await waitFor(() => expect(read).toHaveBeenCalledWith("notes/results.csv"));
  },
);

it("keeps captured Markdown isolated from live absolute paths", async () => {
  const read = vi.fn(async (path: string): Promise<FilePreview> => ({
    path,
    hash: "capture",
    bytes: 20,
    editable: false,
    truncated: false,
    externalPath: null,
    scopeRoot: null,
    base64: null,
    mime: "text/plain",
    text: "[Live](/project/results.csv)\n\n![Live image](/project/figure.png)",
  }));
  render(
    <FileWorkspace
      adapter={{ id: "capture", read }}
      paths={["review.md"]}
      initial={{ path: "review.md" }}
    />,
  );
  fireEvent.click(await screen.findByRole("button", { name: "Preview" }));
  expect(screen.queryByRole("link", { name: "Live" })).toBeNull();
  await screen.findByText(/Image is not available/);
  expect(read).toHaveBeenCalledTimes(1);
});

it("releases the stream a Markdown image opens without needing it", async () => {
  const read = vi.fn(async (path: string): Promise<FilePreview> => ({
    path,
    hash: path,
    bytes: 10,
    editable: false,
    truncated: false,
    externalPath: null,
    mime: path.endsWith(".png") ? "image/png" : "text/plain",
    base64: path.endsWith(".png") ? "QUJT" : null,
    text: path.endsWith(".png") ? null : "![Chart](figure.png)",
    streamToken: path.endsWith(".png") ? "image-token" : null,
  }));
  const release = vi.fn();
  render(
    <FileWorkspace
      adapter={{ id: "report", read, release }}
      paths={["report.md"]}
      initial={{ path: "report.md" }}
    />,
  );
  fireEvent.click(await screen.findByRole("button", { name: "Preview" }));
  await screen.findByRole("img", { name: "Chart" });
  expect(release).toHaveBeenCalledWith("image-token");
});
