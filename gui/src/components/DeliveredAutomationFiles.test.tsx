import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import DeliveredAutomationFiles from "./DeliveredAutomationFiles";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
afterEach(() => {
  cleanup();
  invoke.mockReset();
});

it("saves a conversation-owned file without opening the automation run", async () => {
  invoke.mockImplementation(async (command: string) =>
    command === "task_delivered_files"
      ? {
          files: [
            {
              exchangeId: "receipt",
              ordinal: 2,
              filename: "paper.md",
              createdAt: "2026-11-01",
            },
          ],
          warning: null,
        }
      : null,
  );
  render(<DeliveredAutomationFiles sessionId="chat" onClose={() => {}} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "Save paper.md…" }),
  );
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("task_save_delivered_file", {
      sessionId: "chat",
      exchangeId: "receipt",
      ordinal: 2,
    }),
  );
});
