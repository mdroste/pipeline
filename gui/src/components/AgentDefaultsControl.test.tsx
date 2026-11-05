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

it("keeps a saved role's name without offering roles as new choices", () => {
  const props = {
    agents: ["claude"],
    settings: { claude_access_mode: "subscription" } as Settings,
    providers: ["claude"] as const,
    label: "Test agents",
    onChange: vi.fn(),
  };
  const roles = [
    { id: "quality", label: "Quality", description: "", model: "opus" },
    { id: "fast", label: "Fast", description: "", model: "haiku" },
  ];
  const { rerender } = render(
    <AgentDefaultsControl
      {...props}
      modelOverrides={{ "claude:cli": { mode: "role", role: "quality" } }}
      catalogs={{ claude: { ...catalog, roles } }}
    />,
  );
  expect(screen.getByLabelText("Test agents Claude model")).toHaveValue(
    "role:quality",
  );
  expect(
    screen.getByRole("option", { name: "Quality · opus" }),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole("option", { name: /Fast/ }),
  ).not.toBeInTheDocument();

  rerender(
    <AgentDefaultsControl
      {...props}
      modelOverrides={{ "claude:cli": { mode: "role", role: "retired" } }}
      catalogs={{ claude: catalog }}
    />,
  );
  expect(
    screen.getByRole("option", { name: "retired (saved, not available)" }),
  ).toBeInTheDocument();
});

it("marks a provider that isn't connected without locking it", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  const props = {
    settings: {
      claude_access_mode: "subscription",
      google_api_key: "",
    } as Settings,
    catalogs: { claude: catalog },
    providers: ["claude", "antigravity"] as const,
    label: "Test agents",
    onChange,
    dependencies: null,
  };
  const { rerender } = render(
    <AgentDefaultsControl {...props} agents={["claude"]} />,
  );

  const gemini = screen.getByRole("button", { name: "Gemini" });
  expect(gemini).toHaveAttribute("title", "Not connected");
  expect(screen.getByRole("button", { name: "Claude" })).not.toHaveAttribute(
    "title",
  );
  expect(screen.queryByText(/Not connected\./)).not.toBeInTheDocument();

  // Sign-in status can be out of date, so a label must never be a dead end.
  await user.click(gemini);
  expect(onChange).toHaveBeenLastCalledWith(
    expect.objectContaining({ agents: ["antigravity"] }),
  );

  rerender(<AgentDefaultsControl {...props} agents={["antigravity"]} />);
  expect(screen.getByText(/Not connected\./)).toBeVisible();
});

it("leaves providers unlabeled without readiness information", () => {
  render(
    <AgentDefaultsControl
      agents={["claude"]}
      settings={{ google_api_key: "" } as Settings}
      catalogs={{}}
      providers={["antigravity"]}
      label="Test agents"
      onChange={vi.fn()}
    />,
  );
  expect(screen.getByRole("button", { name: "Gemini" })).not.toHaveAttribute(
    "title",
  );
});

it("hides stage choices while it follows the default", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  const onSelect = vi.fn();
  render(
    <AgentDefaultsControl
      agents={["claude"]}
      settings={{ claude_access_mode: "subscription" } as Settings}
      catalogs={{ claude: catalog }}
      providers={["claude", "codex"]}
      multi
      inherit={{ active: true, label: "Default model", onSelect }}
      label="Test agents"
      onChange={onChange}
    />,
  );

  expect(screen.getByRole("button", { name: "Default model" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  expect(screen.getByRole("button", { name: "Claude" })).toHaveAttribute(
    "aria-pressed",
    "false",
  );
  expect(
    screen.queryByLabelText("Test agents Claude model"),
  ).not.toBeInTheDocument();

  // Adding a second provider keeps the default's own provider.
  await user.click(screen.getByRole("button", { name: "ChatGPT" }));
  expect(onChange).toHaveBeenLastCalledWith(
    expect.objectContaining({ agents: ["claude", "codex"] }),
  );
  await user.click(screen.getByRole("button", { name: "Claude" }));
  expect(onChange).toHaveBeenLastCalledWith(
    expect.objectContaining({ agents: ["claude"] }),
  );
  await user.click(screen.getByRole("button", { name: "Default model" }));
  expect(onSelect).toHaveBeenCalledOnce();
});

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
