import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import WorkspaceMessageActions from "./WorkspaceMessageActions";

afterEach(() => { Reflect.deleteProperty(navigator, "clipboard"); Reflect.deleteProperty(document, "execCommand"); });
it("copies original Markdown, math, and code and confirms only after success", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  const text = "## Result\n\n$x^2$\n\n```r\nx <- 1\n```";
  render(<WorkspaceMessageActions text={text} label="response" />);
  fireEvent.click(screen.getByRole("button", { name: "Copy response" }));
  await screen.findByText("Copied"); expect(writeText).toHaveBeenCalledWith(text);
});
it("falls back in native webviews and restores keyboard focus", async () => {
  Object.defineProperty(document, "execCommand", { configurable: true, value: vi.fn().mockReturnValue(true) });
  render(<><input aria-label="Draft" /><WorkspaceMessageActions text="Copied response" label="response" /></>);
  screen.getByLabelText("Draft").focus(); fireEvent.click(screen.getByRole("button", { name: "Copy response" }));
  await screen.findByText("Copied"); expect(screen.getByLabelText("Draft")).toHaveFocus(); expect(document.querySelector("textarea")).toBeNull();
});
it("reports denied clipboard access without a false copied state", async () => {
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: vi.fn().mockRejectedValue(new Error("denied")) } });
  Object.defineProperty(document, "execCommand", { configurable: true, value: vi.fn().mockReturnValue(false) });
  render(<WorkspaceMessageActions text="Response" label="response" />);
  fireEvent.click(screen.getByRole("button", { name: "Copy response" }));
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Could not copy"));
  expect(screen.queryByText("Copied")).not.toBeInTheDocument();
});
