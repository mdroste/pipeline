import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

import type { Settings, ModelCatalog } from "../../lib/types";
import { PROVIDERS } from "../../lib/providers";

export function useWorkflowCatalogs() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [catalogs, setCatalogs] = useState<Record<string, ModelCatalog>>({});
  useEffect(() => {
    let live = true;
    invoke<{ settings: Settings; warnings: string[] }>("get_settings")
      .then((response) => {
        if (!live) return;
        setSettings(response.settings);
        for (const provider of PROVIDERS) {
          invoke<ModelCatalog>("get_model_catalog", {
            provider,
            settings: response.settings,
            refresh: false,
          })
            .then((catalog) => {
              if (live) setCatalogs((old) => ({ ...old, [provider]: catalog }));
            })
            .catch(() => {});
        }
      })
      .catch(() => {
        // Model overrides remain usable with legacy fields against an older
        // backend; catalog loading is an enhancement, not an editor blocker.
      });
    return () => {
      live = false;
    };
  }, []);

  return { settings, catalogs };
}
