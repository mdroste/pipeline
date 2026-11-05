import { act, renderHook, waitFor } from "@testing-library/react";
import { useState } from "react";
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

it("keeps a runtime's models when moving between its conversations", async () => {
  vi.clearAllMocks();
  client.claudeModelCatalog.mockResolvedValue({
    models: [{ model: "sonnet" }],
  });
  const models = vi.fn();
  const hook = renderHook(
    ({ id, runtime }) =>
      useWorkspaceModelCatalog("chat", id, id, runtime, models, vi.fn()),
    {
      initialProps: {
        id: "first",
        runtime: "claude" as "claude" | "openai-api",
      },
    },
  );
  await waitFor(() =>
    expect(models).toHaveBeenLastCalledWith([{ model: "sonnet" }]),
  );
  models.mockClear();
  hook.rerender({ id: "second", runtime: "claude" });
  await Promise.resolve();
  expect(client.claudeModelCatalog).toHaveBeenCalledTimes(1);
  expect(models).not.toHaveBeenCalled();

  client.apiModelCatalog.mockResolvedValue({ models: [{ model: "gpt" }] });
  hook.rerender({ id: "third", runtime: "openai-api" });
  await waitFor(() =>
    expect(models).toHaveBeenLastCalledWith([{ model: "gpt" }]),
  );
  hook.unmount();
});

it("rejoins Codex on each conversation without blanking its models", async () => {
  vi.clearAllMocks();
  client.connectCodex.mockResolvedValue({});
  client.accountState.mockResolvedValue({ status: "chatgpt" });
  client.modelCatalog.mockResolvedValue({ models: [{ model: "gpt" }] });
  const models = vi.fn();
  const hook = renderHook(
    ({ id }) =>
      useWorkspaceModelCatalog("chat", id, id, "codex", models, vi.fn()),
    { initialProps: { id: "first" } },
  );
  await waitFor(() =>
    expect(models).toHaveBeenLastCalledWith([{ model: "gpt" }]),
  );
  models.mockClear();
  hook.rerender({ id: "second" });
  await waitFor(() => expect(client.connectCodex).toHaveBeenCalledTimes(2));
  expect(client.modelCatalog).toHaveBeenCalledTimes(1);
  expect(models).not.toHaveBeenCalled();
  hook.unmount();
});

it("retries a catalog that failed when the next conversation opens", async () => {
  vi.clearAllMocks();
  client.claudeModelCatalog.mockRejectedValueOnce(new Error("Sign in"));
  client.claudeModelCatalog.mockResolvedValue({
    models: [{ model: "sonnet" }],
  });
  const models = vi.fn();
  const error = vi.fn();
  const hook = renderHook(
    ({ id }) =>
      useWorkspaceModelCatalog("chat", id, id, "claude", models, error),
    { initialProps: { id: "first" } },
  );
  await waitFor(() =>
    expect(error).toHaveBeenLastCalledWith(expect.stringContaining("Sign in")),
  );
  hook.rerender({ id: "second" });
  await waitFor(() =>
    expect(models).toHaveBeenLastCalledWith([{ model: "sonnet" }]),
  );
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

it("clears only its own failure when the conversation changes", async () => {
  vi.clearAllMocks();
  client.claudeModelCatalog.mockRejectedValueOnce(new Error("Sign in"));
  client.claudeModelCatalog.mockResolvedValue({ models: [] });
  const models = vi.fn();
  const hook = renderHook(
    ({ id }) => {
      const [error, setError] = useState<string | null>(null);
      useWorkspaceModelCatalog("chat", id, id, "claude", models, setError);
      return { error, setError };
    },
    { initialProps: { id: "first" } },
  );
  await waitFor(() => expect(hook.result.current.error).toContain("Sign in"));
  hook.rerender({ id: "second" });
  await waitFor(() => expect(hook.result.current.error).toBeNull());

  // A load error from the conversation is not the catalog's to clear.
  act(() => hook.result.current.setError("Could not refresh"));
  hook.rerender({ id: "third" });
  await act(async () => {});
  expect(hook.result.current.error).toBe("Could not refresh");
  hook.unmount();
});
