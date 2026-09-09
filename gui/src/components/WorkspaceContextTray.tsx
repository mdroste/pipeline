import { useCallback, useEffect, useRef, useState } from "react";
import {
  deskClient,
  type ContextItem,
  type ContextSelection,
  type OpenResearchObject,
} from "../lib/deskClient";
import { workbenchErrorMessage } from "../lib/workbenchError";

export const addContextObject = (
  workspaceId: string,
  object: OpenResearchObject,
) =>
  window.dispatchEvent(
    new CustomEvent("pipeline-context-add", {
      detail: { workspaceId, object },
    }),
  );
const key = (o: OpenResearchObject) =>
  `${o.kind}:${o.id}:${o.revision}:${o.start ?? ""}:${o.end ?? ""}`;
const roles: ContextItem["role"][] = [
  "main",
  "source",
  "data_dictionary",
  "prior_draft",
  "referee_report",
  "result",
  "supporting",
];

export default function WorkspaceContextTray({
  workspaceId,
  sessionId,
  disabled,
  onError,
}: {
  workspaceId: string;
  sessionId: string;
  disabled: boolean;
  onError: (message: string) => void;
}) {
  const [selection, setSelection] = useState<ContextSelection>({
    revision: 0,
    items: [],
  });
  const [busy, setBusy] = useState(false);
  const [loadedSession, setLoadedSession] = useState<string | null>(null);
  const [names, setNames] = useState<Record<string, string>>({});
  const scope = useRef(0);
  const saving = useRef(false);
  useEffect(() => {
    const epoch = ++scope.current;
    saving.current = false;
    setLoadedSession(null);
    setSelection({ revision: 0, items: [] });
    setNames({});
    setBusy(false);
    void deskClient
      .context(sessionId)
      .then((value) => {
        if (scope.current === epoch) {
          setSelection(value);
          setLoadedSession(sessionId);
        }
      })
      .catch((e) => {
        if (scope.current === epoch) onError(workbenchErrorMessage(e));
      });
    return () => {
      ++scope.current;
    };
  }, [sessionId, workspaceId, onError]);
  useEffect(() => {
    let alive = true;
    void Promise.all(
      selection.items.map(
        async (item) =>
          [
            key(item.object),
            (await deskClient.read(workspaceId, item.object)).title,
          ] as const,
      ),
    )
      .then((pairs) => {
        if (alive) setNames(Object.fromEntries(pairs));
      })
      .catch((e) => {
        if (alive) onError(workbenchErrorMessage(e));
      });
    return () => {
      alive = false;
    };
  }, [selection, workspaceId, onError]);
  const save = useCallback(
    async (items: ContextItem[]) => {
      if (disabled || saving.current || loadedSession !== sessionId) return;
      const epoch = scope.current;
      saving.current = true;
      setBusy(true);
      try {
        const updated = await deskClient.saveContext(
          sessionId,
          selection,
          items,
        );
        if (scope.current === epoch) setSelection(updated);
      } catch (e) {
        if (scope.current !== epoch) return;
        onError(workbenchErrorMessage(e));
        try {
          const current = await deskClient.context(sessionId);
          if (scope.current === epoch) setSelection(current);
        } catch (reloadError) {
          if (scope.current === epoch) {
            setLoadedSession(null);
            onError(workbenchErrorMessage(reloadError));
          }
        }
      } finally {
        if (scope.current === epoch) {
          saving.current = false;
          setBusy(false);
        }
      }
    },
    [disabled, loadedSession, sessionId, selection, onError],
  );
  useEffect(() => {
    const add = (event: Event) => {
      const detail = (
        event as CustomEvent<{
          workspaceId: string;
          object: OpenResearchObject;
        }>
      ).detail;
      if (detail.workspaceId !== workspaceId) return;
      if (disabled || loadedSession !== sessionId || saving.current) {
        onError(
          "Wait for the conversation and its sources to finish updating.",
        );
        return;
      }
      if (
        selection.items.some((item) => key(item.object) === key(detail.object))
      )
        return;
      void save([
        ...selection.items,
        {
          role:
            detail.object.kind === "dataset"
              ? "data_dictionary"
              : selection.items.length
                ? "supporting"
                : "main",
          object: detail.object,
        },
      ]);
    };
    window.addEventListener("pipeline-context-add", add);
    return () => window.removeEventListener("pipeline-context-add", add);
  }, [
    workspaceId,
    sessionId,
    loadedSession,
    disabled,
    onError,
    save,
    selection,
  ]);
  return (
    <details
      aria-label="Conversation sources"
      className="workspace-context-summary"
      hidden={loadedSession === sessionId && !selection.items.length}
    >
      <summary>
        {selection.items.length
          ? `Sources · ${selection.items.length}`
          : "Loading sources…"}
      </summary>
      <div className="flex flex-wrap gap-2 py-2 text-xs">
        {loadedSession === sessionId &&
          selection.items.map((item, index) => {
            const name = names[key(item.object)] ?? item.object.kind;
            return (
              <div
                key={key(item.object)}
                className="flex max-w-full items-center gap-1 rounded border bg-blue-50 px-2 py-1 dark:bg-blue-950/30"
              >
                <span
                  className="max-w-40 truncate"
                  title={`${item.object.id} · ${item.object.revision}`}
                >
                  {name}
                </span>
                <select
                  aria-label={`Role for ${name}`}
                  disabled={disabled || busy}
                  className="max-w-28 bg-transparent"
                  value={item.role}
                  onChange={(e) =>
                    void save(
                      selection.items.map((value, n) =>
                        n === index
                          ? {
                              ...value,
                              role: e.target.value as ContextItem["role"],
                            }
                          : value,
                      ),
                    )
                  }
                >
                  {roles.map((role) => (
                    <option key={role}>{role}</option>
                  ))}
                </select>
                <button
                  type="button"
                  disabled={disabled || busy}
                  aria-label={`Remove ${name} from context`}
                  onClick={() =>
                    void save(selection.items.filter((_, n) => n !== index))
                  }
                >
                  ×
                </button>
              </div>
            );
          })}
        {!selection.items.length && (
          <span className="text-gray-500">
            {loadedSession === sessionId
              ? "Add exact sources from the research desk."
              : "Loading conversation sources…"}
          </span>
        )}
      </div>
    </details>
  );
}
