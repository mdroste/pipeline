import { beforeEach, describe, expect, it, vi } from "vitest";
import { confirmUpdateRestart, restartWarning } from "./updateRestart";

const invoke = vi.hoisted(() => vi.fn());
const confirmDialog = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("../components/DialogService", () => ({ confirmDialog }));

const idle = {
  unsavedChanges: false,
  reviews: false,
  conversations: false,
  automations: false,
};

describe("restartWarning", () => {
  it("stays quiet when nothing would be lost", () => {
    expect(restartWarning(idle)).toBeNull();
  });

  it("names what a restart would interrupt", () => {
    expect(restartWarning({ ...idle, unsavedChanges: true })).toBe(
      "Restarting will discard your unsaved changes.",
    );
    expect(restartWarning({ ...idle, reviews: true })).toBe(
      "Restarting will stop running reviews.",
    );
    expect(
      restartWarning({
        unsavedChanges: true,
        reviews: true,
        conversations: true,
        automations: true,
      }),
    ).toBe(
      "Restarting will discard your unsaved changes and stop running reviews, conversations and automations.",
    );
  });
});

describe("confirmUpdateRestart", () => {
  const work = { unsavedChanges: false, reviews: false, conversations: false };

  beforeEach(() => {
    invoke.mockReset();
    confirmDialog.mockReset();
  });

  it("restarts without asking when nothing is running", async () => {
    invoke.mockResolvedValue(0);
    await expect(confirmUpdateRestart(work)).resolves.toBe(true);
    expect(confirmDialog).not.toHaveBeenCalled();
  });

  it("asks when an automation is running", async () => {
    invoke.mockResolvedValue(2);
    confirmDialog.mockResolvedValue(false);
    await expect(confirmUpdateRestart(work)).resolves.toBe(false);
    expect(confirmDialog).toHaveBeenCalledWith(
      "Restarting will stop running automations.",
      expect.objectContaining({ confirmLabel: "Restart now" }),
    );
  });

  it("still warns about known work if automations cannot be read", async () => {
    invoke.mockRejectedValue(new Error("unavailable"));
    confirmDialog.mockResolvedValue(true);
    await confirmUpdateRestart({ ...work, reviews: true });
    expect(confirmDialog).toHaveBeenCalledWith(
      "Restarting will stop running reviews.",
      expect.anything(),
    );
  });
});
