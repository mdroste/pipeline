import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  ProjectIssueLedger,
  ProjectsResponse,
  RunSummary,
} from "../lib/types";
import { studioClient } from "../lib/studioClient";
import type {
  BuildReceipt,
  BuildRecord,
  ResponseRecord,
} from "../lib/studioClient";
import { deskClient } from "../lib/deskClient";
import {
  programClient,
  type Campaign,
  type CampaignStatus,
  type MonitorState,
} from "../lib/programClient";
import { taskClient } from "../lib/taskClient";
import { repositoryClient } from "../lib/repositoryClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import {
  NO_SIGNALS,
  underRoot,
  type ProjectSignals,
} from "../lib/projectOverview";

/** A background data refresh reloads derived state only when it is older than this. */
const FRESH_MS = 120_000;
/** Returning to the overview reloads it unless it was loaded moments ago. */
const RETURN_FRESH_MS = 10_000;
/** Ledgers are synced for at most this many linked review collections. */
const LEDGER_LIMIT = 3;

/**
 * Loads the overview's derived state after first paint. Every source settles
 * on its own: a slow or failing one leaves its rows out instead of blocking
 * or breaking the page.
 */
export default function useProjectSignals({
  workspaceId,
  root,
  sessionIds,
  enabled,
  stamp,
}: {
  workspaceId: string;
  root: string | null;
  sessionIds: string[];
  /** False for projects with nothing to derive, and while the overview is hidden. */
  enabled: boolean;
  /** Changes whenever project data was refreshed; triggers a throttled reload. */
  stamp: unknown;
}): {
  signals: ProjectSignals;
  loading: boolean;
  reload: () => void;
  /** Asks the Git remote for new commits; rejects with a readable message. */
  checkRemote: () => Promise<void>;
} {
  const [signals, setSignals] = useState<ProjectSignals>(NO_SIGNALS);
  const [pending, setPending] = useState(0);
  const [request, setRequest] = useState(0);
  const loaded = useRef({ at: 0, request: -1, key: "" });
  const sessions = useRef(sessionIds);
  sessions.current = sessionIds;
  // A load outlives the effect run that started it (a data refresh re-runs the
  // effect without reloading), so staleness is tracked per load, not per run.
  const generation = useRef(0);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const wasEnabled = useRef(false);
  useEffect(() => {
    const returned = enabled && !wasEnabled.current;
    wasEnabled.current = enabled;
    if (!enabled) return;
    const key = `${workspaceId}\n${root ?? ""}`;
    const forced =
      loaded.current.request !== request || loaded.current.key !== key;
    if (
      !forced &&
      Date.now() - loaded.current.at < (returned ? RETURN_FRESH_MS : FRESH_MS)
    )
      return;
    loaded.current = { at: Date.now(), request, key };
    const mine = ++generation.current;
    const live = () => mounted.current && generation.current === mine;
    const load = <T>(source: () => Promise<T>, apply: (value: T) => void) => {
      setPending((count) => count + 1);
      let work: Promise<T>;
      try {
        work = source();
      } catch (cause) {
        work = Promise.reject(cause);
      }
      void work
        .then((value) => {
          if (live()) apply(value);
        })
        .catch(() => undefined)
        .finally(() => {
          if (mounted.current) setPending((count) => count - 1);
        });
    };
    const set =
      <K extends keyof ProjectSignals>(name: K) =>
      (value: ProjectSignals[K]) =>
        setSignals((current) => ({ ...current, [name]: value }));

    load(() => studioClient.coverage(workspaceId), set("coverage"));
    load(() => deskClient.impact(workspaceId), set("impact"));
    load(
      () =>
        studioClient.records<BuildRecord | BuildReceipt>(workspaceId, "build"),
      set("builds"),
    );
    load(
      () => studioClient.records<ResponseRecord>(workspaceId, "response"),
      set("responses"),
    );
    load(
      () =>
        programClient.call<MonitorState>(workspaceId, { action: "monitors" }),
      set("monitors"),
    );
    load(async () => {
      const campaigns = await deskClient.records<Campaign>(
        workspaceId,
        "revision_campaign",
      );
      const superseded = new Set(campaigns.map((record) => record.supersedes));
      const latest = campaigns
        .filter((record) => !superseded.has(record.id))
        .sort((a, b) => b.createdAt.localeCompare(a.createdAt))[0];
      return latest
        ? programClient.call<CampaignStatus>(workspaceId, {
            action: "campaignStatus",
            id: latest.id,
          })
        : null;
    }, set("campaign"));
    load(async () => {
      const tasks = await taskClient.list("session");
      const own = new Set(sessions.current);
      return tasks.filter((task) => task.sessionId && own.has(task.sessionId));
    }, set("automations"));
    if (root)
      load(() => repositoryClient.status(workspaceId), set("repository"));
    // Reviews are scoped the way the project's Reviews page scopes them: runs
    // whose input lies under the attached folder, and collections holding them.
    if (root)
      load(async () => {
        const [runs, collections] = await Promise.all([
          invoke<RunSummary[]>("list_runs"),
          invoke<ProjectsResponse>("list_projects"),
        ]);
        const own = runs.filter((run) => underRoot(run.input_path, root));
        const ids = new Set(own.map((run) => run.run_id));
        if (live()) set("runs")(own);
        const ledgers = await Promise.allSettled(
          collections.projects
            .filter((collection) =>
              collection.run_ids.some((id) => ids.has(id)),
            )
            .slice(0, LEDGER_LIMIT)
            .map((collection) =>
              invoke<ProjectIssueLedger>("sync_project_issue_ledger", {
                projectId: collection.id,
              }),
            ),
        );
        return ledgers.flatMap((result) =>
          result.status === "fulfilled" ? [result.value] : [],
        );
      }, set("ledgers"));
  }, [enabled, workspaceId, root, request, stamp]);

  return {
    signals,
    loading: pending > 0,
    reload: () => setRequest((value) => value + 1),
    checkRemote: async () => {
      setPending((count) => count + 1);
      try {
        const repository = await repositoryClient.fetch(workspaceId);
        if (mounted.current)
          setSignals((current) => ({ ...current, repository }));
      } catch (cause) {
        throw new Error(workbenchErrorMessage(cause));
      } finally {
        if (mounted.current) setPending((count) => count - 1);
      }
    },
  };
}
