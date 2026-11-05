import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { AppPage } from "../../components/NavRail";
import type { WorkbenchEvent } from "../../lib/workbenchTypes";
import { pendingRequestSessions } from "../../lib/workspaceAttention";
import { useActiveTurn } from "../workspace/useActiveTurn";

// Lazy like the pending-request read below: plain pages never load the client.
const readActiveTurn = () =>
  import("../../lib/workbench/runtimeClient").then(({ runtimeClient }) =>
    runtimeClient.activeTurn(),
  );

export function useAppAttention(page: AppPage) {
  const [tasksAttention, setTasksAttention] = useState(false);
  const pageRef = useRef(page);
  pageRef.current = page;
  useEffect(() => {
    const changed = () => {
      if (pageRef.current !== "tasks") setTasksAttention(true);
    };
    const pending = [
      listen("tasks:notice", changed),
      listen("missions:notice", changed),
      listen("discovery:notice", changed),
    ];
    return () => {
      for (const off of pending)
        void off.then((f) => f()).catch(() => undefined);
    };
  }, []);
  const { activeTurn, refreshActiveTurn, observeActiveTurn } =
    useActiveTurn(readActiveTurn);
  const [waiting, setWaiting] = useState<ReadonlyMap<string, string | null>>(
    new Map(),
  );
  const [researchNotice, setResearchNotice] = useState<{
    workspaceId: string;
    checkId: string;
  } | null>(null);
  useEffect(() => {
    const off = listen<{ workspaceId: string; checkId: string }>(
      "workbench-research-attention",
      ({ payload }) => setResearchNotice(payload),
    );
    return () => {
      void off.then((f) => f());
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let unresolved: ReadonlyMap<string, string | null> = new Map();
    let bootstrapping = true;
    const observed: WorkbenchEvent[] = [];
    void import("../../lib/workbench/runtimeClient")
      .then(({ runtimeClient }) => runtimeClient.pendingRequests())
      .then((requests) => {
        if (!disposed) {
          let ids: ReadonlyMap<string, string | null> = new Map();
          for (const r of [...requests, ...observed])
            ids = pendingRequestSessions(ids, r);
          unresolved = ids;
          bootstrapping = false;
          setWaiting(ids);
        }
      })
      .catch(() => {
        bootstrapping = false;
      });
    void listen<WorkbenchEvent>("workbench:event", ({ payload }) => {
      if (
        bootstrapping &&
        ["serverRequest", "serverRequestResolved", "connectionClosed"].includes(
          payload.kind,
        )
      )
        observed.push(payload);
      observeActiveTurn(payload);
      unresolved = pendingRequestSessions(unresolved, payload);
      setWaiting(unresolved);
    }).then((dispose) => {
      if (disposed) dispose();
      else {
        unlisten = dispose;
        // Read after subscribing, so no change can fall between the two.
        void refreshActiveTurn();
      }
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [observeActiveTurn, refreshActiveTurn]);
  return {
    tasksAttention,
    setTasksAttention,
    workspaceActive: Boolean(activeTurn),
    workspaceAttention: waiting.size > 0,
    /** The running turn, which names the conversation doing the work. */
    workspaceTurn: activeTurn ?? null,
    /** The conversation an unresolved approval is waiting in. */
    waitingSession:
      [...waiting.values()].find(Boolean) ?? activeTurn?.sessionId ?? null,
    researchNotice,
  };
}
