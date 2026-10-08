import { renderHook, act } from "@testing-library/react";
import { it, expect, vi } from "vitest";
import { useWorkflowEditor } from "./useWorkflowEditor";

it("retains an auto-review recovery draft until profile loading inspects it", () => {
  localStorage.clear();
  localStorage.setItem(
    "pipeline.workflowDraft.auto-review",
    '{"unsaved":"precious work"}',
  );
  renderHook(() => useWorkflowEditor());
  expect(localStorage.getItem("pipeline.workflowDraft.auto-review")).toBe(
    '{"unsaved":"precious work"}',
  );
});

it.each(["setItem", "removeItem"] as const)(
  "preserves edits when recovery %s throws",
  (method) => {
    const { result, unmount } = renderHook(() => useWorkflowEditor());
    const spy = vi.spyOn(window.localStorage, method).mockImplementation(() => {
      throw new DOMException("Unavailable", "QuotaExceededError");
    });
    try {
      const config = {
        steps: [],
        merge: { enabled: false, prompt: "draft", agents: [] },
      } as any;
      act(() => {
        result.current.setConfig(config);
        result.current.setDirty(method === "setItem");
      });
      expect(result.current.config).toEqual(config);
      expect(result.current.dirty).toBe(method === "setItem");
      expect(result.current.recoveryWarning).toContain("save or export");
    } finally {
      spy.mockRestore();
      unmount();
    }
  },
);
