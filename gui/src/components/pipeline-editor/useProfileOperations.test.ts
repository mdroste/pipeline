import { renderHook, act, waitFor } from "@testing-library/react";
import { beforeEach, describe, it, expect, vi } from "vitest";
import stepEditing from "../../../tests/fixtures/workflow/step-editing.json";
import { useWorkflowEditor } from "./useWorkflowEditor";
import { useProfileOperations } from "./useProfileOperations";
const stepIds = stepEditing.step_ids;
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

it("passes the saved workflow to the run setup so a same-mode input survives", async () => {
  localStorage.clear();
  const config = {
    steps: [],
    merge: { enabled: false, prompt: "saved", agents: [] },
    extraction: { method: "", input_mode: "document" },
    orientation_prompt: "",
    parallel_context_template: "",
  };
  mocks.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "get_pipeline_config") return config;
    if (cmd === "get_active_profile") return "saved";
    if (cmd === "list_profiles") return [];
    if (cmd === "save_pipeline_config") return;
    throw new Error(cmd);
  });
  const changed = vi.fn();
  const { result } = renderHook(() => {
    const editor = useWorkflowEditor();
    return { editor, ops: useProfileOperations(editor, null, changed) };
  });
  await waitFor(() => expect(result.current.ops.loading).toBe(false));
  await act(async () => await result.current.ops.handleSave());
  expect(changed).toHaveBeenCalledTimes(1);
  expect(changed.mock.calls[0][0]?.extraction.input_mode).toBe("document");
  expect(changed.mock.calls[0][0]).toBe(result.current.editor.config);
});

it("gives an imported step a unique id the backend accepts", async () => {
  localStorage.clear();
  const { base, taken, id } = stepIds[2];
  const step = (stepId: string) => ({
    id: stepId,
    label: "Imported",
    prompt: "Check",
    enabled: true,
    phase: "parallel",
    tools: [],
    agents: [],
    context: { include: [] },
  });
  mocks.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "get_pipeline_config")
      return {
        steps: taken.map(step),
        merge: { enabled: false, prompt: "", agents: [] },
        extraction: { method: "" },
        orientation_prompt: "",
        parallel_context_template: "",
      };
    if (cmd === "get_active_profile") return "A";
    if (cmd === "list_profiles") return [];
    if (cmd === "import_item") return { type: "step", data: step(base) };
    throw new Error(cmd);
  });
  const { result } = renderHook(() => {
    const editor = useWorkflowEditor();
    return { editor, ops: useProfileOperations(editor, null) };
  });
  await waitFor(() => expect(result.current.ops.loading).toBe(false));
  await act(async () => await result.current.ops.handleImport());
  expect(result.current.editor.config?.steps.map((item) => item.id)).toEqual([
    ...taken,
    id,
  ]);
});

describe("workflow draft recovery", () => {
  const key = (profile: string) => `pipeline.workflowDraft.${profile}`;
  const config = (prompt: string) => ({
    steps: [],
    merge: { enabled: false, prompt, agents: [] },
    extraction: { method: "" },
    orientation_prompt: "",
    parallel_context_template: "",
  });
  let active = "A";
  const setup = () => {
    active = "A";
    mocks.confirm.mockReset().mockResolvedValue(true);
    mocks.invoke.mockImplementation(
      async (cmd: string, args?: { id: string }) => {
        if (cmd === "get_pipeline_config") return config(`${active} saved`);
        if (cmd === "get_active_profile") return active;
        if (cmd === "list_profiles") return [];
        if (cmd === "switch_profile") {
          if (args?.id === "broken") throw new Error("refused");
          active = args!.id;
          return config(`${active} saved`);
        }
        throw new Error(cmd);
      },
    );
    return renderHook(() => {
      const editor = useWorkflowEditor();
      return { editor, ops: useProfileOperations(editor, null) };
    });
  };
  const dirty = (result: ReturnType<typeof setup>["result"]) =>
    act(() => {
      result.current.editor.setConfig(config("A edited") as never);
      result.current.editor.setDirty(true);
    });

  beforeEach(() => localStorage.clear());

  it("does not offer edits discarded by switching, on the next visit", async () => {
    const first = setup();
    await waitFor(() => expect(first.result.current.ops.loading).toBe(false));
    dirty(first.result);
    expect(localStorage.getItem(key("A"))).toContain("A edited");
    await act(() => first.result.current.ops.handleSwitchProfile("B"));
    expect(localStorage.getItem(key("A"))).toBeNull();
    first.unmount();

    const second = setup();
    await waitFor(() => expect(second.result.current.ops.loading).toBe(false));
    expect(mocks.confirm).not.toHaveBeenCalled();
    expect(second.result.current.editor.config?.merge.prompt).toBe("A saved");
  });

  it("keeps the recovery copy when the switch fails", async () => {
    const { result } = setup();
    await waitFor(() => expect(result.current.ops.loading).toBe(false));
    dirty(result);
    await act(() => result.current.ops.handleSwitchProfile("broken"));
    expect(localStorage.getItem(key("A"))).toContain("A edited");
  });

  it("offers a stored draft on switching to its workflow", async () => {
    localStorage.setItem(key("B"), JSON.stringify(config("B crash draft")));
    const { result } = setup();
    await waitFor(() => expect(result.current.ops.loading).toBe(false));
    await act(() => result.current.ops.handleSwitchProfile("B"));
    expect(mocks.confirm).toHaveBeenCalledWith(
      expect.stringContaining("unsaved draft"),
      expect.objectContaining({ confirmLabel: "Restore draft" }),
    );
    expect(result.current.editor.activeProfile).toBe("B");
    expect(result.current.editor.config?.merge.prompt).toBe("B crash draft");
    expect(result.current.editor.dirty).toBe(true);
    expect(localStorage.getItem(key("B"))).toContain("B crash draft");
  });

  it("removes a declined draft only after asking", async () => {
    localStorage.setItem(key("B"), JSON.stringify(config("B crash draft")));
    const { result } = setup();
    await waitFor(() => expect(result.current.ops.loading).toBe(false));
    let answer: (value: boolean) => void = () => undefined;
    mocks.confirm.mockImplementationOnce(
      () => new Promise((resolve) => (answer = resolve)),
    );
    const switching = result.current.ops.handleSwitchProfile("B");
    await waitFor(() => expect(mocks.confirm).toHaveBeenCalled());
    expect(localStorage.getItem(key("B"))).toContain("B crash draft");
    await act(async () => {
      answer(false);
      await switching;
    });
    expect(result.current.editor.config?.merge.prompt).toBe("B saved");
    expect(localStorage.getItem(key("B"))).toBeNull();
  });
});
