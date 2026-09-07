import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import WorkflowCodexConnection from "./WorkflowCodexConnection";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mocked = vi.mocked(invoke);
let pending = false;
let unresolved: Array<{ id: string; state: string; label: string }> = [];

beforeEach(() => {
  pending = false; unresolved = []; mocked.mockReset();
  mocked.mockImplementation(async (command) => {
    if (command === "workflow_codex_status") return { account: { status: "signedOut", email: null, planType: null }, version: "0.153.4", epoch: 9, loginInProgress: pending, unresolvedAttempts: unresolved };
    if (command === "workflow_codex_login_start") { pending = true; return { loginId: "login-1", epoch: 9 }; }
    if (command === "workflow_codex_login_cancel") { pending = false; return true; }
    if (command === "workflow_codex_acknowledge_attempt") { unresolved = []; return null; }
    throw new Error(`Unexpected command ${command}`);
  });
});

describe("Workflow Codex connection", () => {
  it("refreshes readiness on login and recovery changes without repeated notifications", async () => {
    const onStatusChange = vi.fn();
    render(<WorkflowCodexConnection onStatusChange={onStatusChange} />);
    await waitFor(() => expect(onStatusChange).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Refresh" })).toBeEnabled());
    expect(onStatusChange).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "Sign in to ChatGPT" }));
    await waitFor(() => expect(onStatusChange).toHaveBeenCalledTimes(2));
    fireEvent.click(await screen.findByRole("button", { name: "Cancel sign-in" }));
    await waitFor(() => expect(onStatusChange).toHaveBeenCalledTimes(3));

    unresolved = [{ id: "b".repeat(32), state: "accepted", label: "Review" }];
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(onStatusChange).toHaveBeenCalledTimes(4));
    fireEvent.click(screen.getByRole("button", { name: "Acknowledge and allow a new run" }));
    await waitFor(() => expect(onStatusChange).toHaveBeenCalledTimes(5));
  });

  it("signs in through the Workflow namespace and cancels the matching epoch", async () => {
    render(<WorkflowCodexConnection />);
    const button = await screen.findByRole("button", { name: "Sign in to ChatGPT" });
    await waitFor(() => expect(button).toBeEnabled());
    fireEvent.click(button);
    const cancel = await screen.findByRole("button", { name: "Cancel sign-in" });
    await waitFor(() => expect(cancel).toBeEnabled());
    fireEvent.click(cancel);
    await waitFor(() => expect(mocked).toHaveBeenCalledWith("workflow_codex_login_cancel", { loginId: "login-1", epoch: 9 }));
    expect(mocked.mock.calls.some(([command]) => command.startsWith("workbench_"))).toBe(false);
  });
  it("requires an explicit action to abandon an unresolved attempt", async () => {
    unresolved = [{ id: "a".repeat(32), state: "accepted", label: "Technical review" }];
    render(<WorkflowCodexConnection />);
    const acknowledge = await screen.findByRole("button", { name: "Acknowledge and allow a new run" });
    expect(mocked).not.toHaveBeenCalledWith("workflow_codex_acknowledge_attempt", expect.anything());
    fireEvent.click(acknowledge);
    await waitFor(() => expect(mocked).toHaveBeenCalledWith("workflow_codex_acknowledge_attempt", { id: "a".repeat(32), epoch: 9 }));
  });
});
