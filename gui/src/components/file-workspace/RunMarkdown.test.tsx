import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import ReactMarkdown from "react-markdown";
import RunMarkdown from "./RunMarkdown";
import { fileMarkdownComponents } from "./markdownComponents";
import type { FileLocation } from "../../lib/fileLinks";

const client = vi.hoisted(() => ({
  manifest: vi.fn(),
  read: vi.fn(),
  stream: vi.fn(),
  pdfBytes: vi.fn(),
}));
vi.mock("../../lib/artifactClient", () => ({ artifactClient: client }));

const retained = (...paths: string[]) => ({
  artifacts: paths.map((rel_path) => ({ rel_path })),
});
const image = {
  kind: "image",
  bytes: 3,
  text: null,
  base64: "QUJD",
  truncated: false,
  abs_path: "/runs/r/figure.png",
};

beforeEach(() => {
  vi.clearAllMocks();
  client.read.mockResolvedValue(image);
});

const report = (onOpen: (location: FileLocation) => void) => (
  <RunMarkdown runId="r" onOpen={onOpen}>
    <ReactMarkdown components={fileMarkdownComponents}>
      {"![One](figures/one.png)\n\n![Two](figures/two.png)\n\n[Data](data.csv)"}
    </ReactMarkdown>
  </RunMarkdown>
);

it("keeps report images loaded when the parent passes a new handler", async () => {
  client.manifest.mockResolvedValue(
    retained("report.md", "figures/one.png", "figures/two.png", "data.csv"),
  );
  const first = vi.fn();
  const { rerender } = render(report(first));
  expect(await screen.findByRole("img", { name: "One" })).toHaveAttribute(
    "src",
    "data:image/png;base64,QUJD",
  );
  expect(await screen.findByRole("img", { name: "Two" })).toBeVisible();
  // One manifest read serves the document, and inline images open no stream.
  expect(client.manifest).toHaveBeenCalledTimes(1);
  expect(client.read).toHaveBeenCalledTimes(2);
  expect(client.stream).not.toHaveBeenCalled();

  // An inline handler is a new function on every parent render.
  const second = vi.fn();
  rerender(report(second));
  expect(screen.getByRole("img", { name: "One" })).toBeVisible();
  expect(client.read).toHaveBeenCalledTimes(2);

  fireEvent.click(screen.getByRole("link", { name: "Data" }));
  await waitFor(() =>
    expect(second).toHaveBeenCalledWith({ path: "data.csv" }),
  );
  expect(first).not.toHaveBeenCalled();
  expect(client.stream).not.toHaveBeenCalled();
});

it("rereads the manifest once before refusing an unretained image", async () => {
  client.manifest
    .mockResolvedValueOnce(retained("report.md"))
    .mockResolvedValueOnce(retained("report.md", "figures/one.png"))
    .mockResolvedValue(retained("report.md", "figures/one.png"));
  render(report(vi.fn()));
  expect(await screen.findByRole("img", { name: "One" })).toBeVisible();
  expect(
    await screen.findByText(/Image unavailable: figures\/two\.png/),
  ).toBeInTheDocument();
  expect(client.read).toHaveBeenCalledTimes(1);
});
