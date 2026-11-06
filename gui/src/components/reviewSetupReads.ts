import { createContext, useContext, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PipelineConfig, ProfileSummary } from "../lib/types";

function once<T>(load: () => Promise<T>) {
  let pending: Promise<T> | undefined;
  return () =>
    (pending ??= load().catch((error) => {
      pending = undefined;
      throw error;
    }));
}
export function createReviewSetupReads() {
  const config = once(() => invoke<PipelineConfig>("get_pipeline_config"));
  const profiles = once(() => invoke<ProfileSummary[]>("list_profiles"));
  const active = once(() => invoke<string>("get_active_profile"));
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
