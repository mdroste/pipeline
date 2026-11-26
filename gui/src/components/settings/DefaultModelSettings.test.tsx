import { useState } from "react";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { makeSettings } from "../../test/settingsFixtures";
import type { DepsReport, Settings } from "../../lib/settingsTypes";
import type { ModelCatalog } from "../../lib/reviewModelTypes";
import { DEFAULT_PREFERENCES } from "../../lib/appPreferences";
import { withAccessMode, defaultModelSummary } from "../../lib/providers";
import DefaultModelSettings from "./DefaultModelSettings";
import { settingsSearchEntries } from "./search";

const model = (id: string, supported_efforts: string[]) => ({
  id,
  display_name: id.toUpperCase(),
  description: "",
  is_default: false,
  supported_efforts,
  capabilities: [],
  deprecated: false,
});

const catalogs = {
  claude: {
    models: [model("opus", ["low", "high", "max"]), model("haiku", ["low"])],
    roles: [],
  },
  local: { models: [model("llama", [])], roles: [] },
} as unknown as Record<string, ModelCatalog>;

let latest: Settings;
function Harness({
  initial,
  dependencies,
}: {
  initial: Partial<Settings>;
  dependencies?: DepsReport | null;
}) {
  const [settings, setSettings] = useState<Settings>({
    ...makeSettings(),
    ...initial,
  });
  latest = settings;
  return (
    <DefaultModelSettings
      settings={settings}
      setSettings={setSettings}
      catalogs={catalogs}
      catalogBlocked={{}}
      dependencies={dependencies}
    />
  );
}

describe("DefaultModelSettings", () => {
  it("keeps live defaults compact and separate from saved explicit choices", async () => {
    const user = userEvent.setup();
    const settings = {
      ...makeSettings(),
      preferred_provider: "codex",
      default_model_chosen: true,
      default_model: "model-b",
      default_effort: "high",
    };
    const live = {
      ...catalogs.claude,
      provider: "codex",
      transport: "cli",
      default_model: "model-a",
      models: [
        {
          ...model("model-a", ["low", "medium", "high"]),
          default_effort: "medium",
        },
        {
          ...model("model-b", ["low", "medium", "high"]),
          default_effort: "low",
        },
      ],
    } as ModelCatalog;
    const setSettings = vi.fn();
    const { rerender } = render(
      <DefaultModelSettings
        settings={settings}
        setSettings={setSettings}
        catalogs={{ codex: live }}
        catalogBlocked={{}}
      />,
    );
    const modelSelect = screen.getByLabelText("Default model");
    expect(
      within(modelSelect).getByRole("option", { name: "MODEL-A (auto)" }),
    ).toHaveValue("");
    const thinking = screen.getByLabelText("Default thinking");
    expect(
      within(thinking).getByRole("option", { name: "Low (default)" }),
    ).toHaveValue("");
    await user.selectOptions(thinking, "");
    expect(setSettings).toHaveBeenLastCalledWith(
      expect.objectContaining({ default_effort: "" }),
    );
    rerender(
      <DefaultModelSettings
        settings={{ ...settings, default_model: "", default_effort: "" }}
        setSettings={setSettings}
        catalogs={{ codex: live }}
        catalogBlocked={{}}
      />,
    );
    expect(thinking).toHaveAttribute(
      "title",
      "ChatGPT default thinking: Medium",
    );
    expect(
      within(thinking).getByRole("option", { name: "Medium (default)" }),
    ).toHaveValue("");
    rerender(
      <DefaultModelSettings
        settings={{ ...settings, default_model: "", default_effort: "" }}
        setSettings={setSettings}
        catalogs={{ codex: live }}
        catalogBlocked={{ codex: true }}
      />,
    );
    expect(
      within(thinking).getByRole("option", { name: "Provider default" }),
    ).toHaveValue("");
  });

  it("waits for a choice and says what happens until then", async () => {
    const user = userEvent.setup();
    render(<Harness initial={{}} />);

    expect(screen.getByLabelText("Default provider")).toHaveValue("");
    expect(screen.getByLabelText("Default model")).toBeDisabled();
    expect(
      screen.getByText(/reviews use Claude and new conversations start on/),
    ).toBeVisible();

    await user.selectOptions(screen.getByLabelText("Default provider"), [
      "codex",
    ]);
    expect(latest).toMatchObject({
      preferred_provider: "codex",
      codex_access_mode: "subscription",
      default_model_chosen: true,
    });
    expect(screen.queryByText(/until you choose/i)).not.toBeInTheDocument();
  });

  it("lists each provider once and offers Claude connections separately", async () => {
    const user = userEvent.setup();
    render(
      <Harness
        initial={{
          default_model_chosen: true,
          default_model: "opus",
          default_effort: "high",
        }}
      />,
    );

    const provider = screen.getByLabelText("Default provider");
    expect(
      within(provider)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["Anthropic", "OpenAI", "Google", "Local / custom"]);
    expect(provider).toHaveValue("claude");
    const connection = screen.getByLabelText("Default connection");
    expect(within(connection).getAllByRole("option")).toHaveLength(3);
    await user.selectOptions(connection, "claude-code-api");
    expect(latest).toMatchObject({
      preferred_provider: "claude",
      claude_access_mode: "code_api",
      default_model: "",
      default_effort: "",
    });
    expect(connection).toHaveAccessibleDescription(
      /Uses your Console sign-in in Claude Code/,
    );
    await user.selectOptions(screen.getByLabelText("Default model"), "opus");
    await user.selectOptions(screen.getByLabelText("Default thinking"), "high");
    await user.selectOptions(connection, "anthropic-api");
    expect(latest).toMatchObject({
      preferred_provider: "claude",
      claude_access_mode: "api",
      default_model: "",
      default_effort: "",
    });
    expect(connection).toHaveAccessibleDescription(
      /Uses the Anthropic API key saved in Pipeline/,
    );
    await user.selectOptions(connection, "claude");
    expect(latest.claude_access_mode).toBe("subscription");
    expect(connection).toHaveAccessibleDescription(
      /Uses your Claude Code subscription sign-in/,
    );
  });

  it.each([
    ["subscription", "claude"],
    ["code_api", "claude-code-api"],
    ["api", "anthropic-api"],
  ] as const)(
    "loads the saved %s connection without changing its defaults",
    (mode, id) => {
      const settings = {
        ...makeSettings(),
        default_model_chosen: true,
        claude_access_mode: mode,
        anthropic_api_key: "saved-key",
        default_model: "opus",
        default_effort: "high",
      };
      const setSettings = vi.fn();
      render(
        <DefaultModelSettings
          settings={settings}
          setSettings={setSettings}
          catalogs={catalogs}
          catalogBlocked={{}}
        />,
      );
      expect(screen.getByLabelText("Default provider")).toHaveValue("claude");
      expect(screen.getByLabelText("Default connection")).toHaveValue(id);
      expect(screen.getByLabelText("Default model")).toHaveValue("opus");
      expect(screen.getByLabelText("Default thinking")).toHaveValue("high");
      expect(setSettings).not.toHaveBeenCalled();
    },
  );

  it("retains each provider's connection when switching providers", async () => {
    const user = userEvent.setup();
    render(
      <Harness
        initial={{
          default_model_chosen: true,
          claude_access_mode: "code_api",
          codex_access_mode: "api",
          openai_api_key: "saved-openai",
          anthropic_api_key: "saved-anthropic",
        }}
      />,
    );
    const provider = screen.getByLabelText("Default provider");
    await user.selectOptions(provider, "codex");
    expect(screen.getByLabelText("Default connection")).toHaveValue(
      "openai-api",
    );
    await user.selectOptions(provider, "claude");
    expect(screen.getByLabelText("Default connection")).toHaveValue(
      "claude-code-api",
    );
    expect(latest).toMatchObject({
      codex_access_mode: "api",
      claude_access_mode: "code_api",
      openai_api_key: "saved-openai",
      anthropic_api_key: "saved-anthropic",
    });
    await user.selectOptions(
      screen.getByLabelText("Default connection"),
      "claude",
    );
    await user.selectOptions(provider, "codex");
    await user.selectOptions(provider, "claude");
    expect(screen.getByLabelText("Default connection")).toHaveValue("claude");
  });

  it("sets thinking only for a model that offers it", async () => {
    const user = userEvent.setup();
    render(<Harness initial={{ default_model_chosen: true }} />);

    expect(screen.getByLabelText("Default thinking")).toBeDisabled();
    await user.selectOptions(screen.getByLabelText("Default model"), "opus");
    await user.selectOptions(screen.getByLabelText("Default thinking"), "high");
    expect(latest).toMatchObject({
      default_model: "opus",
      default_effort: "high",
    });

    // Haiku has no High, so the level clears rather than silently failing.
    await user.selectOptions(screen.getByLabelText("Default model"), "haiku");
    expect(latest).toMatchObject({
      default_model: "haiku",
      default_effort: "",
    });
  });

  it("labels connections that aren't ready but still lets you pick them", async () => {
    const user = userEvent.setup();
    render(
      <Harness
        initial={{ default_model_chosen: true }}
        dependencies={
          {
            ready: false,
            deps: [
              {
                name: "Claude CLI",
                found: true,
                cli_auth_status: "signed_in",
              },
            ],
          } as DepsReport
        }
      />,
    );

    expect(
      screen.getByRole("option", { name: "Claude Code · Subscription" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("option", {
        name: "Direct API · API key (not connected)",
      }),
    ).toBeEnabled();

    await user.selectOptions(
      screen.getByLabelText("Default provider"),
      "antigravity",
    );
    expect(latest.preferred_provider).toBe("antigravity");
    expect(screen.getByText("Not connected.")).toBeVisible();
    expect(
      screen.queryByLabelText("Default connection"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Set up connection" }),
    ).toBeVisible();
  });

  it("keeps a saved model visible when it is no longer offered", () => {
    render(
      <Harness
        initial={{ default_model_chosen: true, default_model: "retired" }}
      />,
    );
    expect(screen.getByLabelText("Default model")).toHaveValue("retired");
    expect(
      screen.getByRole("option", { name: "retired (saved, not available)" }),
    ).toBeInTheDocument();
  });

  it("uses the local server's own model setting", async () => {
    const user = userEvent.setup();
    render(
      <Harness
        initial={{ default_model_chosen: true, preferred_provider: "local" }}
      />,
    );

    await user.selectOptions(screen.getByLabelText("Default model"), "llama");
    expect(latest.local_model).toBe("llama");
    expect(latest.default_model).toBeFalsy();
  });
});

describe("the shared default", () => {
  it("clears its model when its connection's access mode changes", () => {
    const settings = {
      ...makeSettings(),
      default_model: "opus",
      default_effort: "high",
    };
    expect(withAccessMode(settings, "claude", "api")).toMatchObject({
      claude_access_mode: "api",
      default_model: "",
      default_effort: "",
    });
    expect(withAccessMode(settings, "codex", "api")).toMatchObject({
      codex_access_mode: "api",
      default_model: "opus",
    });
  });

  it("summarizes the connection, model, and thinking level", () => {
    expect(
      defaultModelSummary(
        { ...makeSettings(), default_model: "opus", default_effort: "high" },
        catalogs,
      ),
    ).toBe("Claude Code · OPUS · High");
    // An unset default names who picks the model rather than "Automatic".
    expect(defaultModelSummary(makeSettings(), catalogs)).toBe(
      "Claude Code · Claude Code's choice",
    );
  });

  it("is still found by its old name in Settings search", () => {
    const entry = settingsSearchEntries(
      makeSettings(),
      DEFAULT_PREFERENCES,
      "light",
    ).find(({ description }) => description.includes("Preferred provider"));
    expect(entry).toMatchObject({
      section: "providers",
      target: "default-model",
      label: "Default model",
    });
  });
});
