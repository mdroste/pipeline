import { createContext, useContext, useMemo } from "react";
import { workflowProfileClient } from "../lib/workflowProfileClient";

function once<T>(load: () => Promise<T>) {
  let pending: Promise<T> | undefined;
  return () =>
    (pending ??= load().catch((error) => {
      pending = undefined;
      throw error;
    }));
}
export function createReviewSetupReads() {
  const config = once(() => workflowProfileClient.config());
  const profiles = once(() => workflowProfileClient.profiles());
  const active = once(() => workflowProfileClient.active());
  return {
    config,
    workflow: () => Promise.all([config(), profiles(), active()] as const),
  };
}
export const ReviewSetupReads = createContext<ReturnType<
  typeof createReviewSetupReads
> | null>(null);
export function useSetupReads(revision: number) {
  const shared = useContext(ReviewSetupReads);
  return useMemo(() => shared ?? createReviewSetupReads(), [shared, revision]);
}
