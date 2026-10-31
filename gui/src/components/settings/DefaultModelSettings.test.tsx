import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { makeSettings } from "../../test/settingsFixtures";
import type { DepsReport, ModelCatalog, Settings } from "../../lib/types";
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

  it("names a connection with one choice, including its access mode", async () => {
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

    await user.selectOptions(
      screen.getByLabelText("Default provider"),
      "anthropic-api",
    );
    expect(latest).toMatchObject({
      preferred_provider: "claude",
      claude_access_mode: "api",
      default_model: "",
      default_effort: "",
    });
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
      screen.getByRole("option", { name: "Claude Code" }),
    ).toBeInTheDocument();
    const gemini = screen.getByRole("option", {
      name: "Gemini API (not connected)",
    });
    expect(gemini).toBeEnabled();

    await user.selectOptions(
      screen.getByLabelText("Default provider"),
      "google-api",
    );
    expect(latest.preferred_provider).toBe("antigravity");
    expect(
      screen.getByText(/Gemini API isn’t connected\. Connect it below/),
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
    expect(defaultModelSummary(makeSettings(), catalogs)).toBe(
      "Claude Code · Automatic",
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
