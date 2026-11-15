import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { collectBatchListeners } from "../../lib/batchListeners";
import { appClient } from "../../lib/appClient";
import { hasActiveBatch } from "../../lib/appRunPreparation";
import type { BatchJob } from "../../lib/reviewTypes";

export function useBatchActivity() {
  const [batchActive, setBatchActive] = useState(false);
  useEffect(() => {
    let live = true;
    let unlisteners: Array<() => void> = [];
    Promise.allSettled([
      listen<BatchJob[]>("batch:progress", (event) => {
        if (live) setBatchActive(hasActiveBatch(event.payload));
      }),
      listen("batch:done", () => {
        if (live) setBatchActive(false);
      }),
    ])
      .then((registrations) => {
        const registered = collectBatchListeners(registrations, live);
        if (!registered) return;
        unlisteners = registered;
        // Register listeners before taking the snapshot so a just-started
        // batch cannot fall into the gap between the read and subscription.
        return appClient.batchStatus().then((jobs) => {
          if (live) setBatchActive(hasActiveBatch(jobs));
        });
      })
      .catch((error) => {
        console.warn("Unable to track batch status:", error);
      });
    return () => {
      live = false;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  return batchActive;
}
