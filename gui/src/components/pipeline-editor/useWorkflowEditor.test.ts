import { renderHook } from "@testing-library/react";
import { it, expect } from "vitest";
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
