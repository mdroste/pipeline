import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { listProjectIndex, type ProjectIndexItem } from "../lib/projectIndex";
import { workbenchErrorMessage } from "../lib/workbenchError";

export default function useProjectIndex() {
  const [projects, setProjects] = useState<ProjectIndexItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const sequence = useRef(0);
  const refresh = useCallback(async () => {
    const request = ++sequence.current;
    setLoading(true);
    try {
      const result = await listProjectIndex();
      if (request !== sequence.current) return;
      setProjects(result);
      setError(null);
    } catch (cause) {
      if (request === sequence.current) setError(workbenchErrorMessage(cause));
    } finally {
      if (request === sequence.current) setLoading(false);
    }
  }, []);
  useEffect(() => {
    void refresh();
    let timer: ReturnType<typeof setTimeout> | undefined;
    const changed = () => {
      clearTimeout(timer);
      timer = setTimeout(() => void refresh(), 250);
    };
    window.addEventListener("focus", changed);
    const off = listen<{ kind: string }>("workbench:event", ({ payload }) => {
      if (["turnCompleted", "connectionClosed"].includes(payload.kind))
        changed();
    });
    return () => {
      sequence.current++;
      clearTimeout(timer);
      window.removeEventListener("focus", changed);
      void off.then((dispose) => dispose()).catch(() => undefined);
    };
  }, [refresh]);
  return { projects, loading, error, refresh };
}
