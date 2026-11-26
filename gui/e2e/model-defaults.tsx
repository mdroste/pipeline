// Real model selectors with deterministic discovery metadata; no provider calls.
import { useState } from "react";
import { createRoot } from "react-dom/client";
import DefaultModelSettings from "../src/components/settings/DefaultModelSettings";
import AgentDefaultsControl from "../src/components/AgentDefaultsControl";
import type { Settings } from "../src/lib/settingsTypes";
import type { ModelCatalog, ModelSelection } from "../src/lib/reviewModelTypes";
import "../src/App.css";
import "../src/components/SettingsPage.css";

const catalogs = {
  claude: {
    provider: "claude",
    transport: "cli",
    default_model: "claude-fixture",
    roles: [],
    models: [
      {
        id: "claude-fixture",
        display_name: "Claude fixture",
        supported_efforts: ["low", "high", "max"],
      },
    ],
  },
  codex: {
    provider: "codex",
    transport: "cli",
    default_model: "fixture",
    roles: [],
    models: [
      {
        id: "fixture",
        display_name: "GPT-5.6 Sol",
        default_effort: "medium",
        supported_efforts: ["low", "medium", "high"],
      },
    ],
  },
} as unknown as Record<string, ModelCatalog>;
const query = new URLSearchParams(location.search);
document.documentElement.classList.toggle("dark", query.has("dark"));
function Fixture() {
  const [settings, setSettings] = useState({
    preferred_provider: query.get("provider") === "claude" ? "claude" : "codex",
    codex_access_mode: "subscription",
    claude_access_mode: ["code_api", "api"].includes(query.get("mode") ?? "")
      ? query.get("mode")
      : "subscription",
    default_model_chosen: true,
    default_model: "",
    default_effort: "",
    local_model: "",
  } as Settings);
  const [value, setValue] = useState({
    agents: ["codex"],
    modelOverrides: {} as Record<string, ModelSelection>,
    effortOverrides: {} as Record<string, string>,
  });
  return (
    <main
      className="settings-shell bg-surface text-ink"
      style={{
        padding: 16,
        display: "block",
        width: Number(query.get("width") || 360),
        maxWidth: "100%",
        minHeight: "100vh",
      }}
    >
      <DefaultModelSettings
        settings={settings}
        setSettings={setSettings}
        catalogs={catalogs}
        catalogBlocked={{}}
      />
      <section style={{ marginTop: 32 }}>
        <AgentDefaultsControl
          {...value}
          onChange={setValue}
          settings={settings}
          catalogs={catalogs}
          providers={["codex"]}
          label="Stage"
        />
      </section>
    </main>
  );
}
createRoot(document.getElementById("root")!).render(<Fixture />);
