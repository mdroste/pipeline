import { renderHook, act, waitFor } from "@testing-library/react";
import { it, expect, vi } from "vitest";
import { useWorkflowEditor } from "./useWorkflowEditor";
import { useProfileOperations } from "./useProfileOperations";
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  confirm: vi.fn().mockResolvedValue(true),
  open: vi.fn().mockResolvedValue("/tmp/bundle.json"),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));
vi.mock("../DialogService", () => ({
  confirmDialog: mocks.confirm,
  notify: vi.fn(),
}));
it("resets per-profile history and invalidates launch setup after bundle changes the active profile", async () => {
  localStorage.clear();
  let imported = false;
  const config = (prompt: string) => ({
    steps: [
      {
        id: "technical",
        label: "Technical",
        prompt,
        enabled: true,
        phase: "parallel",
        tools: [],
        agents: [],
        context: { include: [] },
      },
    ],
    merge: { enabled: false, prompt: "", agents: [] },
    extraction: { method: "" },
    orientation_prompt: "",
    parallel_context_template: "",
  });
  mocks.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "get_pipeline_config")
      return config(imported ? "B prompt" : "A prompt");
    if (cmd === "get_active_profile") return imported ? "B" : "A";
    if (cmd === "list_profiles") return [];
    if (cmd === "import_item") return { type: "bundle", profiles: [] };
    if (cmd === "import_bundle") {
      imported = true;
      return;
    }
    throw new Error(cmd);
  });
  const changed = vi.fn();
  const { result } = renderHook(() => {
    const editor = useWorkflowEditor();
    return { editor, ops: useProfileOperations(editor, null, changed) };
  });
  await waitFor(() => expect(result.current.ops.loading).toBe(false));
  await act(async () => await result.current.ops.handleImport());
  expect(result.current.editor.activeProfile).toBe("B");
  expect(result.current.editor.config?.steps[0].prompt).toBe("B prompt");
  expect.soft(result.current.editor.history.canUndo).toBe(false);
  expect.soft(changed).toHaveBeenCalled();
  act(() => result.current.editor.history.undo());
  expect(result.current.editor.activeProfile).toBe("B");
  expect(result.current.editor.config?.steps[0].prompt).toBe("B prompt");
});

it("loads the saved workflow when recovery reads fail without deleting unread recovery data", async () => {
  const config = {
    steps: [],
    merge: { enabled: false, prompt: "saved", agents: [] },
  };
  mocks.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "get_pipeline_config") return config;
    if (cmd === "get_active_profile") return "saved";
    if (cmd === "list_profiles") return [];
    throw new Error(cmd);
  });
  const read = vi
    .spyOn(window.localStorage, "getItem")
    .mockImplementation(() => {
      throw new DOMException("Denied", "SecurityError");
    });
  const remove = vi.spyOn(window.localStorage, "removeItem");
  const { result, unmount } = renderHook(() => {
    const editor = useWorkflowEditor();
    return { editor, ops: useProfileOperations(editor, null) };
  });
  try {
    await waitFor(() => expect(result.current.ops.loading).toBe(false));
    expect(result.current.ops.loadError).toBeNull();
    expect(result.current.editor.config?.merge.prompt).toBe("saved");
    expect(result.current.editor.recoveryWarning).toContain("save or export");
    expect(remove).not.toHaveBeenCalled();
  } finally {
    unmount();
    read.mockRestore();
    remove.mockRestore();
  }
});
