import { act, render, screen, waitFor } from "@testing-library/react";
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
const overrides = {
  agents: [],
  model_overrides: {},
  effort_overrides: {},
};
beforeEach(() => {
  vi.clearAllMocks();
  invoke.mockResolvedValue({ method: "paddleocr-vl-full" });
});
it("shows the default PDF method by name with a Default tag", async () => {
  render(<ReviewPdfOptions {...props} />);
  const picker = screen.getByRole("combobox", { name: "Read this PDF with" });
  await waitFor(() => expect(picker).toHaveTextContent("PaddleOCRDefault"));
  expect(picker).not.toHaveTextContent(/Workflow default|local/);
  expect(screen.queryByText("Check a sample page")).not.toBeInTheDocument();
  expect(invoke.mock.calls).toEqual([
    ["review_pdf_options", { runParallelOverrides: null }],
  ]);
});
it("changes this review's method without saving global settings", async () => {
  const user = userEvent.setup();
  render(<ReviewPdfOptions {...props} />);
  const picker = screen.getByRole("combobox");
  await waitFor(() => expect(picker).toHaveTextContent("PaddleOCR"));
  await user.click(picker);
  await user.click(screen.getByRole("option", { name: /Text layer/ }));
  expect(props.onChange).toHaveBeenCalledWith(
    expect.objectContaining({ extraction_method: "pdftotext" }),
  );
  expect(invoke).not.toHaveBeenCalledWith("save_settings", expect.anything());
});
it("keeps the Default tag on the workflow's method after an override", async () => {
  const user = userEvent.setup();
  render(
    <ReviewPdfOptions
      {...props}
      value={{ ...overrides, extraction_method: "llm" }}
    />,
  );
  const picker = screen.getByRole("combobox");
  expect(picker).toHaveTextContent("AI model");
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("review_pdf_options", {
      runParallelOverrides: null,
    }),
  );
  await user.click(picker);
  expect(
    await screen.findByRole("option", { name: /PaddleOCR\s*Default/ }),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("option", { name: /PaddleOCR/ }));
  expect(props.onChange).toHaveBeenCalledWith(null);
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
  const picker = screen.getByRole("combobox");
  await waitFor(() => expect(picker).toHaveTextContent("PaddleOCR"));
  await act(async () => resolve({ method: "pdftotext" }));
  expect(picker).toHaveTextContent("PaddleOCRDefault");
});
