import { expect, it } from "vitest";
import { turnStateLabel, turnTimeLabel } from "./workspaceTurnLabels";

it.each([
  ["inProgress", null, "Working"],
  ["completed", "2026-11-01T10:00:00Z", "Completed"],
  ["interrupted", "2026-11-01T10:00:00Z", "Stopped"],
  ["failed", "2026-11-01T10:00:00Z", "Failed"],
  ["somethingNew", "2026-11-01T10:00:00Z", "Finished"],
  ["somethingNew", null, "Working"],
])("labels a %s turn", (state, terminalAt, label) => {
  expect(turnStateLabel({ state, terminalAt })).toBe(label);
});

it("formats a stored time and keeps an unreadable one as stored", () => {
  const stored = "2026-11-01T10:30:00Z";
  expect(turnTimeLabel(stored)).not.toContain("T10:30");
  expect(turnTimeLabel(stored)).toMatch(/2026/);
  expect(turnTimeLabel("not a time")).toBe("not a time");
});
