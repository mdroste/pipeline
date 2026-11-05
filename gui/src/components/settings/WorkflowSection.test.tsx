import { useState } from "react";
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

    const control = screen.getByLabelText("Reviews at once");
    expect(control).toHaveValue("2");
    expect(control).toHaveAttribute("id", "max-concurrent-reviews");
    await user.selectOptions(control, "4");
    expect(setSettings).toHaveBeenLastCalledWith(
      expect.objectContaining({ max_concurrent_reviews: 4 }),
    );
  });

  describe("review limits", () => {
    function Limits() {
      const [settings, setSettings] = useState(makeSettings());
      return (
        <>
          <WorkflowSection settings={settings} setSettings={setSettings} />
          <output>{String(settings.review_max_cost_usd)}</output>
        </>
      );
    }
    const cost = () => screen.getByLabelText("Review limit: Cost (USD)");
    const saved = () => screen.getByRole("status");
    const enter = async (text: string) => {
      const user = userEvent.setup();
      await user.clear(cost());
      await user.tab();
      await user.click(cost());
      await user.keyboard(`{Control>}a{/Control}${text}`);
      await user.tab();
    };

    it("saves what was typed, not each keystroke", async () => {
      const user = userEvent.setup();
      const setSettings = vi.fn();
      render(
        <WorkflowSection settings={makeSettings()} setSettings={setSettings} />,
      );
      await user.click(cost());
      await user.keyboard("{Control>}a{/Control}2.5");
      expect(setSettings).not.toHaveBeenCalled();
      await user.tab();
      expect(setSettings).toHaveBeenCalledOnce();
      expect(setSettings).toHaveBeenCalledWith(
        expect.objectContaining({ review_max_cost_usd: 2.5 }),
      );
    });

    it("keeps a value that starts with a decimal point", async () => {
      render(<Limits />);
      await enter(".5");
      expect(saved()).toHaveTextContent("0.5");
    });

    it("keeps the saved limit when the field is left empty", async () => {
      const user = userEvent.setup();
      const setSettings = vi.fn();
      render(
        <WorkflowSection
          settings={{ ...makeSettings(), review_max_cost_usd: 3 }}
          setSettings={setSettings}
        />,
      );
      await user.clear(cost());
      await user.tab();
      expect(setSettings).not.toHaveBeenCalled();
      expect(cost()).toHaveValue(3);
    });

    it("treats a negative limit as no limit", async () => {
      const user = userEvent.setup();
      const setSettings = vi.fn();
      render(
        <WorkflowSection
          settings={{ ...makeSettings(), review_max_cost_usd: 3 }}
          setSettings={setSettings}
        />,
      );
      await user.clear(cost());
      await user.type(cost(), "-1");
      await user.tab();
      expect(setSettings).toHaveBeenCalledWith(
        expect.objectContaining({ review_max_cost_usd: 0 }),
      );
    });
  });

  it("is findable from settings search", () => {
    const entry = settingsSearchEntries(
      makeSettings(),
      DEFAULT_PREFERENCES,
      "system",
    ).find((candidate) => candidate.label === "Reviews at once");
    expect(entry).toMatchObject({
      section: "workflow",
      target: "max-concurrent-reviews",
    });
  });
});
