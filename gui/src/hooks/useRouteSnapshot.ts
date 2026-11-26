import { useSyncExternalStore } from "react";
import { router } from "../lib/router";

const subscribe = (notify: () => void) => router.subscribe(notify);
export default function useRouteSnapshot() {
  return useSyncExternalStore(subscribe, router.getSnapshot);
}
