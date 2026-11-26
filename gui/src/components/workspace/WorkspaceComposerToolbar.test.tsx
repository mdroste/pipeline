import { expect, it } from "vitest";
import { defaultApprovalMode } from "./WorkspaceComposerToolbar";

it("shows the approval mode the backend applies when none is saved", () => {
  // workbench/runtime/submission.rs: Claude runtimes fall back to Sandbox,
  // ChatGPT to Ask. A mismatch would show "Ask" while commands run unprompted.
  expect(defaultApprovalMode("claude")).toBe("sandbox");
  expect(defaultApprovalMode("claude-code-api")).toBe("sandbox");
  expect(defaultApprovalMode("codex")).toBe("ask");
});
