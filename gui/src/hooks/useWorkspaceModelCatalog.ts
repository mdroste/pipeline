import { useEffect } from "react";
import { runtimeClient } from "../lib/workbench/runtimeClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { WorkspaceModel } from "../lib/workbenchTypes";

export default function useWorkspaceModelCatalog(
  surface: string,
  sessionId: string | null,
  loadedSessionId: string | undefined,
  runtime: "codex" | "claude",
  setModels: (models: WorkspaceModel[]) => void,
  setError: (error: string | null) => void,
) {
  useEffect(() => {
    let disposed = false;
    // Wait for saved provider selection before connecting any runtime.
    if (
      (surface === "chat" || sessionId) &&
      (!sessionId || loadedSessionId === sessionId)
    ) {
      setModels([]);
      setError(null);
      const catalog =
        runtime === "claude"
          ? runtimeClient.claudeModelCatalog()
          : runtimeClient
              .connectCodex()
              .then(() => runtimeClient.accountState())
              .then(async (account) =>
                account.status === "chatgpt"
                  ? runtimeClient.modelCatalog()
                  : { models: [] },
              );
      void catalog
        .then((catalog) => {
          if (!disposed) setModels(catalog.models);
        })
        .catch((cause) => {
          if (!disposed)
            setError(
              `The assistant could not connect. Saved conversations remain available: ${workbenchErrorMessage(cause)}`,
            );
        });
    }
    return () => {
      disposed = true;
    };
  }, [surface, sessionId, loadedSessionId, runtime, setModels, setError]);
}
