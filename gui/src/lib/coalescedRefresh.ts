/** At most one read at a time. A burst during a read requests one fresh follow-up;
 * superseded results are not published. Failed reads can be retried normally. */
export function coalescedRefresh<T>(
  read: () => Promise<T>,
  publish: (value: T) => void,
) {
  let running = false;
  let queued = false;
  let disposed = false;
  const refresh = async () => {
    if (disposed) return;
    queued = true;
    if (running) return;
    running = true;
    try {
      do {
        queued = false;
        try {
          const value = await read();
          if (!queued && !disposed) publish(value);
        } catch {
          /* Retain the last known value; a later refresh retries. */
        }
      } while (queued && !disposed);
    } finally {
      running = false;
    }
  };
  return {
    refresh,
    dispose: () => {
      disposed = true;
    },
  };
}
