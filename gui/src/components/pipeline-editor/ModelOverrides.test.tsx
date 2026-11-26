import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ModelOverrides from "./ModelOverrides";
import { makeSettings } from "../../test/settingsFixtures";
import type { StepConfig } from "../../lib/workflowTypes";
import type { ModelCatalog } from "../../lib/reviewModelTypes";

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
  it("shows the thinking default for the effective inherited or pinned model", async () => {
    const user = userEvent.setup();
    const settings = {
      ...makeSettings(),
      preferred_provider: "codex",
      default_sequential_agent: "codex",
      default_sequential_model_overrides: {
        "codex:cli": { mode: "pinned" as const, model: "model-b" },
      },
    };
    const catalog = {
      provider: "codex",
      transport: "cli",
      default_model: "model-a",
      roles: [],
      models: [
        {
          id: "model-a",
          display_name: "A",
          default_effort: "medium",
          supported_efforts: ["medium"],
        },
        {
          id: "model-b",
          display_name: "B",
          default_effort: "high",
          supported_efforts: ["high"],
        },
      ],
    } as unknown as ModelCatalog;
    const props = { settings, catalogs: { codex: catalog }, onChange: vi.fn() };
    const { rerender } = render(
      <ModelOverrides {...props} step={step("sequential")} />,
    );
    await user.click(screen.getByRole("button", { name: /Model overrides/ }));
    const thinking = screen.getByLabelText("codex cli effort override");
    expect(thinking).toHaveDisplayValue("High (default)");
    expect(thinking).toHaveAttribute("title", "ChatGPT default thinking: High");
    rerender(
      <ModelOverrides
        {...props}
        step={{ ...step("sequential"), model: "model-a" }}
      />,
    );
    expect(thinking).toHaveDisplayValue("Medium (default)");
    rerender(
      <ModelOverrides
        {...props}
        step={{
          ...step("sequential"),
          model_overrides: {
            "codex:cli": { mode: "pinned", model: "model-a" },
          },
        }}
      />,
    );
    expect(thinking).toHaveDisplayValue("Medium (default)");
    rerender(
      <ModelOverrides
        {...props}
        step={{ ...step("sequential"), agents: ["codex"] }}
      />,
    );
    expect(thinking).toHaveDisplayValue("Medium (default)");
    rerender(
      <ModelOverrides
        {...props}
        step={{ ...step("sequential"), effort: "low" }}
      />,
    );
    expect(thinking).toHaveDisplayValue("Low (step)");
  });

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
