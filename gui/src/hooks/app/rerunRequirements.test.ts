import { beforeEach, describe, expect, it, vi } from "vitest";

const { confirmDialog, promptDialog, rerunRequirements } = vi.hoisted(() => ({
  confirmDialog: vi.fn(),
  promptDialog: vi.fn(),
  rerunRequirements: vi.fn(),
}));

vi.mock("../../components/DialogService", () => ({
  confirmDialog,
  promptDialog,
}));
vi.mock("../../lib/appClient", () => ({
  appClient: { rerunRequirements },
}));

import { confirmRerunRequirements } from "./rerunRequirements";

const twoSecrets = [
  { key: "token", label: "Token" },
  { key: "seed", label: "Seed" },
];

describe("confirmRerunRequirements", () => {
  beforeEach(() => {
    confirmDialog.mockReset();
    promptDialog.mockReset();
    rerunRequirements.mockReset();
  });

  it("asks nothing when the run needs nothing", async () => {
    rerunRequirements.mockResolvedValue({ secrets: [], notice: null });
    await expect(confirmRerunRequirements("run", true)).resolves.toEqual({});
    expect(confirmDialog).not.toHaveBeenCalled();
    expect(promptDialog).not.toHaveBeenCalled();
  });

  it("collects each secret in a concealed field", async () => {
    rerunRequirements.mockResolvedValue({ secrets: twoSecrets, notice: null });
    promptDialog.mockResolvedValueOnce("one").mockResolvedValueOnce("two");

    await expect(confirmRerunRequirements("run", true)).resolves.toEqual({
      token: "one",
      seed: "two",
    });
    expect(rerunRequirements).toHaveBeenCalledWith("run", true);
    expect(promptDialog.mock.calls[0][1]).toMatchObject({
      label: "Token",
      concealed: true,
    });
  });

  it("stops when the researcher cancels a secret", async () => {
    rerunRequirements.mockResolvedValue({ secrets: twoSecrets, notice: null });
    promptDialog.mockResolvedValueOnce(null);

    await expect(confirmRerunRequirements("run", false)).resolves.toBeNull();
    expect(promptDialog).toHaveBeenCalledTimes(1);
  });

  it("shows the notice first and stops when the researcher declines", async () => {
    rerunRequirements.mockResolvedValue({
      secrets: twoSecrets,
      notice: "This run did not save its review design.",
    });
    confirmDialog.mockResolvedValue(false);

    await expect(confirmRerunRequirements("run", true)).resolves.toBeNull();
    expect(confirmDialog.mock.calls[0][0]).toBe(
      "This run did not save its review design.",
    );
    expect(promptDialog).not.toHaveBeenCalled();
  });

  it("continues when the researcher accepts the notice", async () => {
    rerunRequirements.mockResolvedValue({ secrets: [], notice: "Notice" });
    confirmDialog.mockResolvedValue(true);

    await expect(confirmRerunRequirements("run", true)).resolves.toEqual({});
  });
});
