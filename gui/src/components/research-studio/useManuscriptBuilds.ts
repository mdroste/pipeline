/** Build configurations and receipts; preparation/saving stays in Manuscript. */
import { useEffect, useRef, useState } from "react";
import { studioClient } from "../../lib/studioClient";
import type { BuildRecord, BuildReceipt } from "../../lib/studioTypes";
import type { ProjectRecord } from "../../lib/projectTypes";
import { manuscriptDraftStorage } from "../../lib/manuscriptDraftStorage";
import { executionClient } from "../../lib/workbench/executionClient";
import type { ExecutionProfile } from "../../lib/workbench/executionTypes";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { initialBuild } from "./ManuscriptBuildSheet";
import type { StudioProps, useAction } from "./shared";
type Options = Pick<
  StudioProps,
  "workspaceId" | "documentId" | "data" | "onRefresh"
> &
  Pick<ReturnType<typeof useAction>, "setError"> & { reopened: number };
export function useManuscriptBuilds({
  workspaceId,
  documentId,
  data,
  onRefresh,
  setError,
  reopened,
}: Options) {
  const draftStorage = manuscriptDraftStorage(workspaceId, documentId);
  const [builds, setBuilds] = useState<
    ProjectRecord<BuildRecord | BuildReceipt>[]
  >([]);
  const [buildsLoaded, setBuildsLoaded] = useState(false);
  const [config, setConfig] = useState(initialBuild);
  const [profiles, setProfiles] = useState<ExecutionProfile[]>([]);
  const [configId, selectConfigId] = useState("");
  const setConfigId = (id: string) => {
    selectConfigId(id);
    try {
      draftStorage.saveBuild(id);
    } catch {
      /* Saved configurations remain in the project. */
    }
  };
  const [advanced, setAdvanced] = useState("");
  const [selectedBuild, setSelectedBuild] = useState("");
  const [compareBuild, setCompareBuild] = useState("");
  const refresh = async () =>
    setBuilds(await studioClient.records(workspaceId, "build"));
  useEffect(() => {
    let stopped = false;
    void Promise.all([
      studioClient.records(workspaceId, "build"),
      executionClient.listExecutionProfiles(workspaceId),
    ])
      .then(([v, p]) => {
        if (!stopped) {
          setBuilds(v);
          setProfiles(p);
          setBuildsLoaded(true);
        }
      })
      .catch((e) => {
        if (!stopped) setError(workbenchErrorMessage(e));
      });
    return () => {
      stopped = true;
    };
  }, [workspaceId, data.executions, setError, reopened]);
  const belongsToDocument = (id?: string | null) =>
    documentId ? id === documentId : !id;
  const configs = builds.filter(
    (r): r is ProjectRecord<BuildRecord> =>
      r.body.recordType === "configuration" &&
      belongsToDocument(r.body.config.documentId),
  );
  const receipts = builds.filter(
    (r): r is ProjectRecord<BuildReceipt> =>
      r.body.recordType === "receipt" && belongsToDocument(r.body.documentId),
  );
  const chosen = configs.find((r) => r.id === configId);
  const finished = data.executions.filter(
    (e) =>
      e.adapter === "latex" &&
      !["queued", "running"].includes(e.outcome) &&
      (!documentId || configs.some((c) => c.body.profile.id === e.profileId)),
  );
  const latestBuild = receipts.some((r) => r.id === `build_${finished[0]?.id}`)
    ? `build_${finished[0].id}`
    : receipts[0]?.id;
  const shownBuild = selectedBuild || latestBuild || "";
  const receipt = receipts.find((r) => r.id === shownBuild)?.body;
  const comparison = receipts.find((r) => r.id === compareBuild)?.body;
  // A finished build's result opens by itself. inspectBuild returns the saved
  // record when one exists, so repeating it is harmless.
  const inspected = useRef(new Set<string>());
  useEffect(() => {
    const latest = finished[0];
    if (
      !buildsLoaded ||
      !latest ||
      inspected.current.has(latest.id) ||
      receipts.some((r) => r.id === `build_${latest.id}`)
    )
      return;
    inspected.current.add(latest.id);
    void studioClient
      .mutate(workspaceId, { action: "inspectBuild", executionId: latest.id })
      .then(async () => {
        await refresh();
        await onRefresh();
      })
      .catch(() => undefined);
  }, [buildsLoaded, finished[0]?.id, builds]);
  return {
    buildsLoaded,
    config,
    setConfig,
    profiles,
    configId,
    setConfigId,
    advanced,
    setAdvanced,
    compareBuild,
    setCompareBuild,
    setSelectedBuild,
    configs,
    receipts,
    chosen,
    finished,
    latestBuild,
    shownBuild,
    receipt,
    comparison,
    refresh,
  };
}
