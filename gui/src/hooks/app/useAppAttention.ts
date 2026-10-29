import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { AppPage } from "../../components/NavRail";
import type { WorkbenchEvent } from "../../lib/workbenchTypes";
import { pendingRequestIds } from "../../lib/workspaceAttention";

export function useAppAttention(page: AppPage) {
  const [tasksAttention, setTasksAttention] = useState(false);
  useEffect(() => {
    const changed = () => {
      if (page !== "tasks") setTasksAttention(true);
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
  }, [page]);
  const [workspaceActive, setWorkspaceActive] = useState(false);
  const [workspaceAttention, setWorkspaceAttention] = useState(false);
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
    let unresolved = new Set<string>();
    let bootstrapping = true;
    const observed: WorkbenchEvent[] = [];
    void import("../../lib/workbench/runtimeClient")
      .then(({ runtimeClient }) => runtimeClient.pendingRequests())
      .then((requests) => {
        if (!disposed) {
          let ids = new Set<string>();
          for (const r of [...requests, ...observed])
            ids = pendingRequestIds(ids, r);
          unresolved = ids;
          bootstrapping = false;
          setWorkspaceAttention(ids.size > 0);
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
      if (payload.kind === "turnStarted") setWorkspaceActive(true);
      if (
        payload.kind === "turnCompleted" ||
        payload.kind === "connectionClosed"
      )
        setWorkspaceActive(false);
      unresolved = pendingRequestIds(unresolved, payload);
      setWorkspaceAttention(unresolved.size > 0);
    }).then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  return {
    tasksAttention,
    setTasksAttention,
    workspaceActive,
    workspaceAttention,
    researchNotice,
    setResearchNotice,
  };
}
