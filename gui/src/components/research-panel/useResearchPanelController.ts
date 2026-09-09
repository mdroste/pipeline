import { useCallback, useEffect, useState } from "react";
import usePersistentPanelWidth from "../../hooks/usePersistentPanelWidth";
import type { HostExecutionPreview } from "../../lib/projectClient";
import { workbenchClient } from "../../lib/workbenchClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import type {
  ConversationSnapshot,
  EffectiveHarness,
  ExecutionProfile,
  HarnessCatalog,
  PaperSearchHit,
  PaperWithRevision,
  ResearchExecution,
  ResearchLedger,
  ResearchNote,
  SourceRecord,
  Workspace,
} from "../../lib/workbenchTypes";
import type { ResearchTab } from "../WorkspaceResearchPanel";

export function operation(prefix: string) {
  return `${prefix}-${crypto.randomUUID()}`;
}

export function useResearchPanelController({
  snapshot,
  onSnapshot,
  onError,
  workspace: projectWorkspace,
  initialTab,
  active,
  onBusy,
  beforeChange,
}: {
  snapshot: ConversationSnapshot | null;
  workspace?: Workspace | null;
  initialTab: ResearchTab;
  active: boolean;
  onBusy?: (busy: boolean) => void;
  beforeChange?: () => Promise<void>;
  onSnapshot: (snapshot: ConversationSnapshot) => void;
  onError: (message: string) => void;
}) {
  const [width, setWidth] = usePersistentPanelWidth(
    "pipeline.workspace.researchWidth",
    432,
    320,
    560,
  );
  const workspace = projectWorkspace ?? snapshot?.workspace;
  const workspaceId = workspace?.id ?? snapshot?.session.workspaceId ?? null;
  const [hostPreview, setHostPreview] = useState<{
    preview: HostExecutionPreview;
    testOnly: boolean;
  } | null>(null);
  const [tab, setTab] = useState<ResearchTab>(initialTab);
  const [catalog, setCatalog] = useState<HarnessCatalog | null>(null);
  const [effective, setEffective] = useState<EffectiveHarness | null>(null);
  const [papers, setPapers] = useState<PaperWithRevision[]>([]);
  const [projectPaperId, setProjectPaperId] = useState<string | null>(null);
  const [sources, setSources] = useState<SourceRecord[]>([]);
  const [notes, setNotes] = useState<ResearchNote[]>([]);
  const [ledger, setLedger] = useState<ResearchLedger | null>(null);
  const [profiles, setProfiles] = useState<ExecutionProfile[]>([]);
  const [executions, setExecutions] = useState<ResearchExecution[]>([]);
  const [busy, setBusy] = useState(false);
  const hasActiveExecution = executions.some((e) =>
    ["queued", "running"].includes(e.outcome),
  );
  useEffect(() => {
    if (
      !active ||
      tab !== "results" ||
      !workspaceId ||
      (!busy && !hasActiveExecution)
    )
      return;
    let disposed = false,
      pending = false;
    const timer = setInterval(() => {
      if (pending) return;
      pending = true;
      void workbenchClient
        .listExecutions(workspaceId)
        .then((value) => {
          if (!disposed) setExecutions(value);
        })
        .catch(() => undefined)
        .finally(() => {
          pending = false;
        });
    }, 500);
    return () => {
      disposed = true;
      clearInterval(timer);
    };
  }, [active, busy, hasActiveExecution, tab, workspaceId]);
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<PaperSearchHit[]>([]);
  const [noteBody, setNoteBody] = useState("");
  const [noteKind, setNoteKind] = useState<ResearchNote["kind"]>("next_step");
  const [claim, setClaim] = useState("");
  const [profileName, setProfileName] = useState("Research command");
  const [adapter, setAdapter] =
    useState<ExecutionProfile["adapter"]>("command");
  const [argv, setArgv] = useState('["/usr/bin/true"]');
  const [inputs, setInputs] = useState("");
  const [outputs, setOutputs] = useState("");

  const loadBase = useCallback(async () => {
    if (!snapshot || tab !== "setup") return;
    const base = await Promise.all([
      workbenchClient.harnessCatalog(workspaceId),
      workbenchClient.effectiveHarness(snapshot.session.id),
    ]);
    setCatalog(base[0]);
    setEffective(base[1]);
  }, [snapshot?.session.id, workspaceId, tab]);

  const loadTab = useCallback(async () => {
    if (!workspaceId) {
      setPapers([]);
      setSources([]);
      setNotes([]);
      setLedger(null);
      setProfiles([]);
      setExecutions([]);
      return;
    }
    if (tab === "documents") {
      const [nextPapers, nextSources] = await Promise.all([
        workbenchClient.listPapers(workspaceId),
        workbenchClient.listSources(workspaceId),
      ]);
      setPapers(nextPapers);
      setSources(nextSources);
    } else if (tab === "memory") {
      setNotes(await workbenchClient.listNotes(workspaceId, true));
    } else if (tab === "evidence") {
      setLedger(await workbenchClient.researchLedger(workspaceId));
    } else if (tab === "results") {
      const [nextProfiles, nextExecutions] = await Promise.all([
        workbenchClient.listExecutionProfiles(workspaceId),
        workbenchClient.listExecutions(workspaceId),
      ]);
      setProfiles(nextProfiles);
      setExecutions(nextExecutions);
    } else if (tab === "release") {
      setPapers(await workbenchClient.listPapers(workspaceId));
    }
  }, [tab, workspaceId]);

  useEffect(() => {
    if (!active) return;
    void loadBase().catch((cause) => onError(workbenchErrorMessage(cause)));
  }, [active, loadBase, onError]);
  useEffect(() => {
    if (!active) return;
    void loadTab().catch((cause) => onError(workbenchErrorMessage(cause)));
  }, [active, loadTab, onError]);

  const act = async (fn: () => Promise<void>) => {
    setBusy(true);
    onBusy?.(true);
    try {
      await beforeChange?.();
      await fn();
      await Promise.all([loadBase(), loadTab()]);
    } catch (cause) {
      onError(workbenchErrorMessage(cause));
    } finally {
      setBusy(false);
      onBusy?.(false);
    }
  };

  const patchSession = async (
    values: Partial<{
      presetId: string;
      paperId: string | null;
      overrides: Record<string, unknown>;
    }>,
  ) => {
    if (!snapshot) return;
    const latest = await workbenchClient.conversationSnapshot(
      snapshot.session.id,
    );
    const updated = await workbenchClient.updateSession({
      sessionId: latest.session.id,
      expectedRevision: latest.session.revision,
      operationId: operation("research-session"),
      presetId: values.presetId,
      paperId: values.paperId ?? undefined,
      clearPaper: values.paperId === null,
      overrides: values.overrides,
    });
    const next = {
      ...latest,
      session: updated.record,
      sequence: updated.sequence,
    };
    onSnapshot(next);
    return next;
  };

  const selectedPaper =
    papers.find(
      (item) => item.paper.id === (projectPaperId ?? snapshot?.session.paperId),
    ) ?? null;
  const successor = Boolean(
    snapshot?.activeBinding?.harnessFingerprint &&
    effective &&
    snapshot.activeBinding.harnessFingerprint !== effective.fingerprint,
  );
  const tabs: Array<[ResearchTab, string]> = [
    ["setup", "Assistant settings"],
    ["documents", "Documents"],
    ["recipes", "Recipes"],
    ["memory", "Notes"],
    ["evidence", "Evidence"],
    ["results", "Results"],
    ["release", "Sharing & backup"],
  ];

  return {
    width,
    setWidth,
    workspace,
    workspaceId,
    hostPreview,
    setHostPreview,
    tab,
    setTab,
    catalog,
    effective,
    papers,
    setPapers,
    projectPaperId,
    setProjectPaperId,
    sources,
    setSources,
    notes,
    setNotes,
    ledger,
    setLedger,
    profiles,
    setProfiles,
    executions,
    setExecutions,
    busy,
    query,
    setQuery,
    hits,
    setHits,
    noteBody,
    setNoteBody,
    noteKind,
    setNoteKind,
    claim,
    setClaim,
    profileName,
    setProfileName,
    adapter,
    setAdapter,
    argv,
    setArgv,
    inputs,
    setInputs,
    outputs,
    setOutputs,
    loadTab,
    act,
    patchSession,
    selectedPaper,
    successor,
    tabs,
  };
}
