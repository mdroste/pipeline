import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { WorkflowSection } from "./WorkflowDefaults";
import { makeSettings } from "../../test/settingsFixtures";
import { settingsSearchEntries } from "./search";
import { DEFAULT_PREFERENCES } from "../../lib/appPreferences";

describe("WorkflowSection", () => {
  it("sets how many queued reviews run at the same time", async () => {
    const user = userEvent.setup();
    const setSettings = vi.fn();
    render(
      <WorkflowSection settings={makeSettings()} setSettings={setSettings} />,
    );

    const control = screen.getByLabelText("Simultaneous Reviews");
    expect(control).toHaveValue("2");
    expect(control).toHaveAttribute("id", "max-concurrent-reviews");
    await user.selectOptions(control, "4");
    expect(setSettings).toHaveBeenLastCalledWith(
      expect.objectContaining({ max_concurrent_reviews: 4 }),
    );
  });

  it("is findable from settings search", () => {
    const entry = settingsSearchEntries(
      makeSettings(),
      DEFAULT_PREFERENCES,
      "system",
    ).find((candidate) => candidate.label === "Simultaneous Reviews");
    expect(entry).toMatchObject({
      section: "workflow",
      target: "max-concurrent-reviews",
    });
  });
});
