import { createContext, useContext } from "react";
import { router } from "../../lib/router";
import type { ArtifactSelectionTarget } from "../../lib/artifactTypes";

export type OpenHistoryRun = (
  runId: string,
  source: ArtifactSelectionTarget | null,
) => void;

/** The app shell's way to open a saved run in Review history, at a cited
 *  source when one is given. Views nested deep inside a project use it
 *  rather than navigating themselves, which would drop the source. */
export const HistoryRunOpener = createContext<OpenHistoryRun | null>(null);

const openWithoutSource: OpenHistoryRun = (runId) =>
  void router.navigate({ page: "history", runId });

/** Outside the app shell (isolated views), the run opens without a source. */
export function useOpenHistoryRun(): OpenHistoryRun {
  return useContext(HistoryRunOpener) ?? openWithoutSource;
}
