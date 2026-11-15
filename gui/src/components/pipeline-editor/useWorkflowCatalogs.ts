import { useState, useEffect } from "react";
import { settingsClient } from "../../lib/settingsClient";

import type { Settings } from "../../lib/settingsTypes";
import type { ModelCatalog } from "../../lib/reviewModelTypes";
import { PROVIDERS } from "../../lib/providers";

export function useWorkflowCatalogs() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [catalogs, setCatalogs] = useState<Record<string, ModelCatalog>>({});
  useEffect(() => {
    let live = true;
    settingsClient
      .load()
      .then((response) => {
        if (!live) return;
        setSettings(response.settings);
        for (const provider of PROVIDERS) {
          settingsClient
            .modelCatalog({
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
