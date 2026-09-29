import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import WorkflowCodexConnection from "./WorkflowCodexConnection";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mocked = vi.mocked(invoke);
let pending = false;
let unresolved: Array<{ id: string; state: string; label: string }> = [];

beforeEach(() => {
  pending = false;
  unresolved = [];
  mocked.mockReset();
  mocked.mockImplementation(async (command) => {
    if (command === "workflow_codex_status")
      return {
        account: { status: "signedOut", email: null, planType: null },
        version: "0.153.4",
        epoch: 9,
        loginInProgress: pending,
        unresolvedAttempts: unresolved,
      };
    if (command === "workflow_codex_login_start") {
      pending = true;
      return { loginId: "login-1", epoch: 9 };
    }
    if (command === "workflow_codex_login_cancel") {
      pending = false;
      return true;
    }
    if (command === "workflow_codex_acknowledge_attempt") {
      unresolved = [];
      return null;
    }
    throw new Error(`Unexpected command ${command}`);
  });
});

describe("Workflow Codex connection", () => {
  it("requires an explicit action to abandon an unresolved attempt", async () => {
    unresolved = [
      { id: "a".repeat(32), state: "accepted", label: "Technical review" },
    ];
    render(<WorkflowCodexConnection />);
    const acknowledge = await screen.findByRole("button", {
      name: "Acknowledge and allow a new run",
    });
    expect(mocked).not.toHaveBeenCalledWith(
      "workflow_codex_acknowledge_attempt",
      expect.anything(),
    );
    fireEvent.click(acknowledge);
    await waitFor(() =>
      expect(mocked).toHaveBeenCalledWith(
        "workflow_codex_acknowledge_attempt",
        { id: "a".repeat(32), epoch: 9 },
      ),
    );
  });
});
