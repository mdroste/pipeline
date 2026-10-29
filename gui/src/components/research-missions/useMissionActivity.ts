import { useCallback, useEffect, useRef, useState } from "react";
import { missionClient } from "../../lib/missionClient";
import { type TaskEvent } from "../../lib/taskTypes";

/** Rows in one `mission_events` page, which the backend returns oldest first. */
const PAGE = 100;

/**
 * Page a mission's activity log while its tab is open.
 *
 * Opening the log, or another mission, loads the first page. A mission update
 * appends only events after the last loaded one, and only once every earlier
 * page is loaded, so pages the researcher already loaded are never replaced.
 */
export default function useMissionActivity(
  missionId: string,
  revision: number,
  open: boolean,
  onError: (error: string) => void,
) {
  const [events, setEvents] = useState<TaskEvent[]>([]);
  const [more, setMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const log = useRef({
    missionId: "",
    version: 0,
    after: 0,
    more: false,
    busy: false,
    again: false,
  });
  const load = useCallback(
    (reset: boolean) => {
      const state = log.current;
      if (reset) {
        state.version++;
        state.after = 0;
        state.more = false;
        state.busy = false;
        state.again = false;
        setEvents([]);
        setMore(false);
      } else if (state.busy) {
        state.again = true;
        return;
      }
      const version = state.version;
      state.busy = true;
      setLoading(true);
      missionClient
        .events(state.missionId, state.after)
        .then((rows) => {
          if (version !== state.version) return;
          if (rows.length) state.after = rows[rows.length - 1]!.sequence;
          state.more = rows.length >= PAGE;
          setEvents((old) => [...old, ...rows]);
          setMore(state.more);
        })
        .catch((e) => {
          if (version === state.version) onError(String(e));
        })
        .finally(() => {
          if (version !== state.version) return;
          state.busy = false;
          setLoading(false);
          // An update arrived during this request: fetch what it added.
          if (state.again && !state.more) {
            state.again = false;
            load(false);
          }
        });
    },
    [onError],
  );
  useEffect(() => {
    const state = log.current;
    if (!open) {
      state.missionId = "";
      state.version++;
      state.busy = false;
      setLoading(false);
      return;
    }
    if (state.missionId !== missionId) {
      state.missionId = missionId;
      load(true);
    } else if (!state.more) load(false);
  }, [missionId, revision, open, load]);
  useEffect(
    () => () => {
      log.current.version++;
    },
    [],
  );
  return {
    events,
    more,
    loading,
    loadMore: () => {
      if (log.current.more && !log.current.busy) load(false);
    },
  };
}
