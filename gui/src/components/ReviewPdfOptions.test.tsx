import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import ReviewPdfOptions from "./ReviewPdfOptions";
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const props = {
  path: "/paper.pdf",
  value: null,
  disabled: false,
  refreshKey: 0,
  onChange: vi.fn(),
  onBusy: vi.fn(),
};
beforeEach(() => {
  vi.clearAllMocks();
  invoke.mockResolvedValue({ method: "paddleocr-vl-full" });
});
it("shows the default PDF method without offering a page preview", async () => {
  render(<ReviewPdfOptions {...props} />);
  expect(
    await screen.findByRole("option", { name: /Workflow default · PaddleOCR/ }),
  ).toBeInTheDocument();
  expect(screen.queryByText("Check a sample page")).not.toBeInTheDocument();
  expect(invoke.mock.calls).toEqual([
    ["review_pdf_options", { runParallelOverrides: null }],
  ]);
});
it("changes this review's method without saving global settings", async () => {
  const user = userEvent.setup();
  render(<ReviewPdfOptions {...props} />);
  await user.selectOptions(screen.getByRole("combobox"), "pdftotext");
  expect(props.onChange).toHaveBeenCalledWith(
    expect.objectContaining({ extraction_method: "pdftotext" }),
  );
  expect(invoke).not.toHaveBeenCalledWith("save_settings", expect.anything());
});
it("ignores options for an earlier document", async () => {
  let resolve!: (value: unknown) => void;
  invoke.mockImplementationOnce(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  const { rerender } = render(<ReviewPdfOptions {...props} />);
  rerender(<ReviewPdfOptions {...props} path="/new.pdf" />);
  await screen.findByRole("option", { name: /Workflow default · PaddleOCR/ });
  await act(async () => resolve({ method: "pdftotext" }));
  expect(
    screen.getByRole("option", { name: /Workflow default · PaddleOCR/ }),
  ).toBeInTheDocument();
});
