import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ModelOverrides from "./ModelOverrides";
import { makeSettings } from "../../test/settingsFixtures";
import type { StepConfig } from "../../lib/types";

function step(phase: StepConfig["phase"]): StepConfig {
  return {
    id: "technical",
    label: "Technical",
    prompt: "",
    enabled: true,
    phase,
    tools: [],
    agents: [],
    context: { include: [] },
  };
}

describe("ModelOverrides", () => {
  it("lists the phase default agents for a step without its own agents", async () => {
    const user = userEvent.setup();
    const settings = {
      ...makeSettings(),
      preferred_provider: "claude",
      default_parallel_agents: ["codex", "antigravity"],
      default_sequential_agent: "codex",
    };
    const onChange = vi.fn();
    const { rerender } = render(
      <ModelOverrides
        step={step("parallel")}
        settings={settings}
        catalogs={{}}
        onChange={onChange}
      />,
    );
    await user.click(screen.getByRole("button", { name: /Model overrides/ }));
    expect(
      screen.getByRole("combobox", { name: "codex cli model override" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("combobox", { name: "antigravity api model override" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("combobox", { name: /^claude/ }),
    ).not.toBeInTheDocument();

    rerender(
      <ModelOverrides
        step={step("sequential")}
        settings={{ ...settings, default_sequential_agent: "claude" }}
        catalogs={{}}
        onChange={onChange}
      />,
    );
    expect(
      screen.getByRole("combobox", { name: "claude cli model override" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("combobox", { name: /^codex/ }),
    ).not.toBeInTheDocument();
  });
});
