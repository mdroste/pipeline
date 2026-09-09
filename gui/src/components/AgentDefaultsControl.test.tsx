import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import type { ModelCatalog, Settings } from "../lib/types";
import AgentDefaultsControl from "./AgentDefaultsControl";

const catalog: ModelCatalog = {
  provider: "claude",
  transport: "cli",
  source: "test",
  source_version: "test",
  fetched_at: "now",
  stale: false,
  models: [
    {
      id: "claude-exact",
      display_name: "Claude Exact",
      description: "",
      is_default: false,
      supported_efforts: ["high"],
      capabilities: [],
      deprecated: false,
    },
  ],
  roles: [],
};

it("clears legacy provider overrides when inheriting transport defaults", async () => {
  const onChange = vi.fn();
  const user = userEvent.setup();
  render(
    <AgentDefaultsControl
      agents={["claude"]}
      modelOverrides={{ claude: { mode: "pinned", model: "claude-exact" } }}
      effortOverrides={{ claude: "high" }}
      settings={{ claude_access_mode: "subscription" } as Settings}
      catalogs={{ claude: catalog }}
      providers={["claude"]}
      label="Test agents"
      onChange={onChange}
    />,
  );

  await user.selectOptions(
    screen.getByLabelText("Test agents Claude model"),
    "inherit",
  );
  expect(onChange).toHaveBeenLastCalledWith(
    expect.objectContaining({
      modelOverrides: {},
    }),
  );

  await user.selectOptions(
    screen.getByLabelText("Test agents Claude thinking"),
    "",
  );
  expect(onChange).toHaveBeenLastCalledWith(
    expect.objectContaining({
      effortOverrides: {},
    }),
  );
});
