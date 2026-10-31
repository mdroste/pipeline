import { renderHook, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import useWorkspaceModelCatalog from "./useWorkspaceModelCatalog";

const client = vi.hoisted(() => ({
  claudeModelCatalog: vi.fn(),
  apiModelCatalog: vi.fn(),
  connectCodex: vi.fn(),
  accountState: vi.fn(),
  modelCatalog: vi.fn(),
}));
vi.mock("../lib/workbench/runtimeClient", () => ({ runtimeClient: client }));

it("waits for the saved provider and never connects ChatGPT for a Claude conversation", async () => {
  vi.clearAllMocks();
  client.claudeModelCatalog.mockResolvedValue({
    models: [{ model: "sonnet" }],
  });
  const models = vi.fn();
  const error = vi.fn();
  const hook = renderHook(
    ({ loaded }) =>
      useWorkspaceModelCatalog(
        "chat",
        "session",
        loaded,
        "claude",
        models,
        error,
      ),
    { initialProps: { loaded: undefined as string | undefined } },
  );
  expect(client.claudeModelCatalog).not.toHaveBeenCalled();
  hook.rerender({ loaded: "session" });
  await waitFor(() =>
    expect(models).toHaveBeenLastCalledWith([{ model: "sonnet" }]),
  );
  expect(client.connectCodex).not.toHaveBeenCalled();
  expect(client.accountState).not.toHaveBeenCalled();
  hook.unmount();
});

it("does not fall back to another provider when Claude authentication fails", async () => {
  vi.clearAllMocks();
  client.claudeModelCatalog.mockRejectedValue(
    new Error("Subscription sign-in required"),
  );
  const error = vi.fn();
  const hook = renderHook(() =>
    useWorkspaceModelCatalog(
      "chat",
      "session",
      "session",
      "claude",
      vi.fn(),
      error,
    ),
  );
  await waitFor(() =>
    expect(error).toHaveBeenLastCalledWith(
      expect.stringContaining("Subscription sign-in required"),
    ),
  );
  expect(client.connectCodex).not.toHaveBeenCalled();
  hook.unmount();
});

for (const runtime of [
  "openai-api",
  "anthropic-api",
  "google-api",
  "local",
] as const) {
  it(`loads ${runtime} without connecting subscription runtimes`, async () => {
    vi.clearAllMocks();
    client.apiModelCatalog.mockResolvedValue({
      models: [{ model: "api-model" }],
    });
    const models = vi.fn();
    const hook = renderHook(() =>
      useWorkspaceModelCatalog(
        "chat",
        "session",
        "session",
        runtime,
        models,
        vi.fn(),
      ),
    );
    await waitFor(() =>
      expect(models).toHaveBeenLastCalledWith([{ model: "api-model" }]),
    );
    expect(client.apiModelCatalog).toHaveBeenCalledWith(runtime);
    expect(client.connectCodex).not.toHaveBeenCalled();
    expect(client.claudeModelCatalog).not.toHaveBeenCalled();
    hook.unmount();
  });
}
