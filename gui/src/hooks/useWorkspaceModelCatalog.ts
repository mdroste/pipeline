import {
  isApiProvider,
  type ConversationProvider,
} from "../lib/workspaceProviders";
import { useEffect, useRef, type Dispatch, type SetStateAction } from "react";
import { runtimeClient } from "../lib/workbench/runtimeClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { WorkspaceModel } from "../lib/workbenchTypes";

function loadCatalog(
  runtime: ConversationProvider,
): Promise<{ models: WorkspaceModel[] }> {
  if (isApiProvider(runtime)) return runtimeClient.apiModelCatalog(runtime);
  if (runtime === "claude") return runtimeClient.claudeModelCatalog();
  return runtimeClient
    .connectCodex()
    .then(() => runtimeClient.accountState())
    .then(async (account) =>
      account.status === "chatgpt"
        ? runtimeClient.modelCatalog()
        : { models: [] },
    );
}

export default function useWorkspaceModelCatalog(
  surface: string,
  sessionId: string | null,
  loadedSessionId: string | undefined,
  runtime: ConversationProvider,
  setModels: (models: WorkspaceModel[]) => void,
  setError: Dispatch<SetStateAction<string | null>>,
) {
  // The list belongs to the runtime, so moving between its conversations keeps
  // it. A failed or abandoned load leaves this empty and runs again.
  const loaded = useRef<ConversationProvider | null>(null);
  const failure = useRef<string | null>(null);
  useEffect(() => {
    // Wait for saved provider selection before connecting any runtime.
    if (
      !(surface === "chat" || sessionId) ||
      (sessionId && loadedSessionId !== sessionId)
    )
      return;
    // Only its own message: a conversation's load reports its errors just
    // before this runs, and those must stay on screen.
    const shown = failure.current;
    if (shown !== null) {
      failure.current = null;
      setError((current) => (current === shown ? null : current));
    }
    const known = loaded.current === runtime;
    // Codex still rejoins here: its connection may have dropped, and a viewed
    // conversation needs it to receive events.
    if (known && runtime !== "codex") return;
    if (!known) setModels([]);
    loaded.current = runtime;
    let disposed = false;
    let settled = false;
    const request = known
      ? runtimeClient.connectCodex().then(() => null)
      : loadCatalog(runtime);
    void request
      .then((catalog) => {
        settled = true;
        if (!disposed && catalog) setModels(catalog.models);
      })
      .catch((cause) => {
        settled = true;
        if (disposed) return;
        loaded.current = null;
        failure.current = `The assistant could not connect. Saved conversations remain available: ${workbenchErrorMessage(cause)}`;
        setError(failure.current);
      });
    return () => {
      disposed = true;
      if (!settled && !known) loaded.current = null;
    };
  }, [surface, sessionId, loadedSessionId, runtime, setModels, setError]);
}
