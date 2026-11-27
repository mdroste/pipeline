// Conversations open as tabs, per project, like open files. The selected
// conversation always has a tab. Each tab keeps the title it last showed, so
// a conversation outside the loaded page of the session list still has a
// label. Tabs are a device preference; closing one deletes nothing.

import { useEffect, useMemo, useState } from "react";

export interface ConversationTab {
  id: string;
  title: string;
}

const MAX_TABS = 20;
const UNTITLED = "Conversation";

const storageKey = (workspaceId: string | null) =>
  `pipeline.workspace.openConversations.${workspaceId ?? "unfiled"}`;

export function loadConversationTabs(
  workspaceId: string | null,
): ConversationTab[] {
  try {
    const parsed: unknown = JSON.parse(
      localStorage.getItem(storageKey(workspaceId)) ?? "[]",
    );
    if (!Array.isArray(parsed)) return [];
    const seen = new Set<string>();
    return parsed
      .filter(
        (tab): tab is ConversationTab =>
          typeof tab?.id === "string" &&
          typeof tab?.title === "string" &&
          !seen.has(tab.id) &&
          Boolean(seen.add(tab.id)),
      )
      .map(({ id, title }) => ({ id, title }))
      .slice(0, MAX_TABS);
  } catch {
    return [];
  }
}

function saveConversationTabs(
  workspaceId: string | null,
  tabs: ConversationTab[],
) {
  try {
    localStorage.setItem(storageKey(workspaceId), JSON.stringify(tabs));
  } catch {
    /* Tabs remain open for this visit without storage. */
  }
}

/** The tab that takes over when `id` closes: the next one, else the previous. */
export function neighbourTab(tabs: readonly ConversationTab[], id: string) {
  const at = tabs.findIndex((tab) => tab.id === id);
  if (at < 0) return null;
  return (tabs[at + 1] ?? tabs[at - 1])?.id ?? null;
}

export default function useConversationTabs(
  workspaceId: string | null,
  sessionId: string | null,
  sessions: readonly { id: string; title: string }[],
) {
  const [state, setState] = useState(() => ({
    scope: workspaceId,
    tabs: loadConversationTabs(workspaceId),
  }));
  // Another project brings its own tabs.
  let { tabs } = state;
  if (state.scope !== workspaceId) {
    tabs = loadConversationTabs(workspaceId);
    setState({ scope: workspaceId, tabs });
  }

  const titled = useMemo(
    () =>
      tabs.map((tab) => {
        const title = sessions.find((s) => s.id === tab.id)?.title;
        return title && title !== tab.title ? { ...tab, title } : tab;
      }),
    [tabs, sessions],
  );

  useEffect(() => {
    if (!sessionId) return;
    setState((old) =>
      old.scope !== workspaceId || old.tabs.some((tab) => tab.id === sessionId)
        ? old
        : {
            ...old,
            tabs: [
              ...old.tabs,
              {
                id: sessionId,
                title:
                  sessions.find((s) => s.id === sessionId)?.title ?? UNTITLED,
              },
            ].slice(-MAX_TABS),
          },
    );
    // Titles arrive later through `sessions`; adding a tab waits only on the id.
  }, [sessionId, workspaceId]);

  // Keep the latest titles, for when the list no longer holds a conversation.
  useEffect(() => {
    if (!titled.some((tab, i) => tab !== tabs[i])) return;
    setState((old) =>
      old.scope !== workspaceId
        ? old
        : {
            ...old,
            tabs: old.tabs.map(
              (tab) => titled.find((next) => next.id === tab.id) ?? tab,
            ),
          },
    );
  }, [titled, tabs, workspaceId]);

  useEffect(() => {
    if (state.scope === workspaceId)
      saveConversationTabs(workspaceId, state.tabs);
  }, [state, workspaceId]);

  const update = (next: (tabs: ConversationTab[]) => ConversationTab[]) =>
    setState((old) => ({ ...old, tabs: next(old.tabs) }));

  return {
    tabs: titled,
    /** Closes a tab here. */
    close: (id: string) => update((old) => old.filter((tab) => tab.id !== id)),
    /** Moves a tab to `index` among the tabs. */
    move: (id: string, index: number) =>
      update((old) => {
        const from = old.findIndex((tab) => tab.id === id);
        if (from < 0) return old;
        const rest = old.filter((tab) => tab.id !== id);
        rest.splice(Math.max(0, Math.min(index, rest.length)), 0, old[from]);
        return rest;
      }),
    /** Closes a conversation's tab in a project, open here or not. */
    forget: (id: string, scope: string | null) => {
      if (scope === workspaceId)
        update((old) => old.filter((tab) => tab.id !== id));
      else
        saveConversationTabs(
          scope,
          loadConversationTabs(scope).filter((tab) => tab.id !== id),
        );
    },
  };
}
