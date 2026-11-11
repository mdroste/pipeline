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
  invoke.mockImplementation(async (command) =>
    command === "review_pdf_options"
      ? { method: "llm", provider: "claude", snapshotId: "frozen" }
      : {
          text: "A sample",
          image: "data:image/jpeg;base64,AA",
          method: "llm",
          pageCount: 9,
          notes: [],
        },
  );
});
it("makes the model call explicit and binds the sample to the displayed settings", async () => {
  const user = userEvent.setup();
  render(<ReviewPdfOptions {...props} />);
  await user.click(screen.getByText("Check a sample page"));
  await screen.findByText(/uses your model allowance/);
  expect(invoke).not.toHaveBeenCalledWith(
    "preview_review_pdf",
    expect.anything(),
  );
  await user.click(
    screen.getByRole("button", { name: "Preview page with AI" }),
  );
  expect(await screen.findByText("A sample")).toBeVisible();
  expect(invoke).toHaveBeenCalledWith(
    "preview_review_pdf",
    expect.objectContaining({
      page: 1,
      allowModelCall: true,
      expectedSnapshotId: "frozen",
    }),
  );
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
  await screen.findByRole("option", { name: /Workflow default · AI model/ });
  await act(async () =>
    resolve({ method: "pdftotext", provider: "codex", snapshotId: "old" }),
  );
  expect(
    screen.getByRole("option", { name: /Workflow default · AI model/ }),
  ).toBeInTheDocument();
});
