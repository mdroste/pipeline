import WorkspaceAssistantPresetForm from "./WorkspaceAssistantPresetForm";
import { projectClient, type HostExecutionPreview } from "../lib/projectClient";
import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import WorkspaceRecipesPanel from "./WorkspaceRecipesPanel";
import WorkspaceReleasePanel from "./WorkspaceReleasePanel";
import SidebarPanel, { SidebarHeader } from "./SidebarPanel";
import WorkspaceIcon from "./WorkspaceIcon";
import usePersistentPanelWidth from "../hooks/usePersistentPanelWidth";
import {
  compactAccessSummary,
  compactModuleSummary,
  describePreset,
} from "./harness-editor/harnessHelpers";
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
  ReviewHandoff,
  SourceRecord,
  Workspace,
} from "../lib/workbenchTypes";

function operation(prefix: string) {
  return `${prefix}-${crypto.randomUUID()}`;
}
function lines(value: string) {
  return value
    .split("\n")
    .map((item) => item.trim())
    .filter(Boolean);
}
function compactHash(value?: string | null) {
  return value ? `${value.slice(0, 10)}…${value.slice(-6)}` : "—";
}

export type ResearchTab =
  | "setup"
  | "documents"
  | "recipes"
  | "memory"
  | "evidence"
  | "results"
  | "release";

export default function WorkspaceResearchPanel({
  snapshot,
  onSnapshot,
  onClose,
  onError,
  onReviewHandoff,
  onEditHarness,
  onMove,
  workspace: projectWorkspace,
  embedded = false,
  initialTab = "setup",
  allowedTabs,
  title = "Research",
  settingsDisabled = false,
  resultsView = "all",
  releaseView = "all",
  active = true,
  onBusy,
  beforeChange,
}: {
  snapshot: ConversationSnapshot | null;
  workspace?: Workspace | null;
  embedded?: boolean;
  initialTab?: ResearchTab;
  allowedTabs?: ResearchTab[];
  title?: string;
  settingsDisabled?: boolean;
  active?: boolean;
  onBusy?: (busy: boolean) => void;
  beforeChange?: () => Promise<void>;
  resultsView?: "all" | "runs" | "profiles";
  releaseView?: "all" | "share" | "review";
  onSnapshot: (snapshot: ConversationSnapshot) => void;
  onClose: () => void;
  onError: (message: string) => void;
  onReviewHandoff?: (handoff: ReviewHandoff) => void;
  /** Opens the full harness editor; the Setup tab is a read-only summary. */
  onEditHarness?: () => void;
  /** Opens the move dialog so an unfiled conversation can be filed in a project. */
  onMove?: () => void;
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
    if (!active || tab !== "results" || !workspaceId || (!busy && !hasActiveExecution))
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
    papers.find((item) => item.paper.id === (projectPaperId ?? snapshot?.session.paperId)) ?? null;
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

  return (
    <SidebarPanel
      fill={embedded}
      onKeyDown={event => { if (event.key === "Escape" && !busy) { event.stopPropagation(); onClose(); } }}
      aria-label={embedded ? title : "Research workspace"}
      side="right"
      width={width}
      defaultWidth={432}
      min={320}
      max={560}
      onResize={setWidth}
      resizeLabel="Resize research workspace"
    >
      <SidebarHeader
        title={title}
        actions={
          <button
            type="button"
            onClick={onClose}
            disabled={busy}
            className="workspace-sidebar-icon-button"
            aria-label="Close research workspace"
          >
            <WorkspaceIcon name="close" />
          </button>
        }
      >
        <p className="mt-2 truncate text-xs text-gray-500 dark:text-gray-400">
          {tab === "setup" ? effective?.preset.name ?? "Loading assistant settings…" : workspace?.name}
        </p>
      </SidebarHeader>
      {(!allowedTabs || allowedTabs.length > 1) && <div className="workspace-research-tabs flex shrink-0 flex-wrap gap-1 border-b border-gray-200 px-5 pb-4 dark:border-gray-800">
        {tabs.filter(([id]) => !allowedTabs || allowedTabs.includes(id)).map(([id, label]) => (
          <button
            key={id}
            type="button"
            aria-pressed={tab === id}
            onClick={() => setTab(id)}
            className={`rounded-md px-2 py-1.5 text-xs font-medium transition-colors ${tab === id ? "bg-gray-100 text-gray-950 dark:bg-gray-800 dark:text-gray-100" : "text-gray-500 hover:bg-gray-50 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800/60 dark:hover:text-gray-100"}`}
          >
            {label}
          </button>
        ))}
      </div>}
      <div className="workspace-research-content min-h-0 min-w-0 flex-1 overflow-auto p-5 text-sm">
        {!workspaceId && tab !== "setup" && (
          <div className="rounded border border-amber-200 bg-amber-50 p-3 text-amber-800 dark:border-amber-900 dark:bg-amber-950/20 dark:text-amber-200">
            <p>
              Move this conversation to a project to use documents, notes,
              evidence, and results.
            </p>
            {onMove && (
              <button
                type="button"
                disabled={busy}
                onClick={onMove}
                className="mt-2 rounded border border-amber-400 px-2 py-1 text-xs"
              >
                Move to project…
              </button>
            )}
          </div>
        )}

        {/* Conversation-scoped quick settings with a full editor for inheritance. */}
        {tab === "setup" && snapshot && effective && catalog && (
          <div className="space-y-4">
            <label className="block">
              <span className="text-xs font-medium">Agent profile · This conversation</span>
              <select
                value={snapshot.session.presetId ?? "plain"}
                disabled={busy || settingsDisabled}
                onChange={(event) =>
                  void act(async () => {
                    await patchSession({ presetId: event.target.value });
                  })
                }
                className="mt-1 w-full rounded border bg-white p-2 dark:bg-neutral-950"
              >
                {catalog.presets.map((preset) => (
                  <option key={preset.id} value={preset.id}>
                    {preset.name}
                    {preset.builtIn ? "" : " · custom"}
                  </option>
                ))}
              </select>
            </label>
            <p
              data-testid="harness-summary"
              className="text-xs text-gray-600 dark:text-neutral-300"
            >
              {describePreset(effective.preset, catalog.modules)}
            </p>
            <label className="block">
              <span className="text-xs font-medium">Access mode</span>
              <select
                aria-label="Access mode"
                value={typeof snapshot.session.overrides.mode === "string" ? snapshot.session.overrides.mode : ""}
                disabled={busy || settingsDisabled}
                onChange={(event) =>
                  void act(async () => {
                    const latest = await workbenchClient.conversationSnapshot(snapshot.session.id);
                    const overrides = { ...latest.session.overrides };
                    if (event.target.value) overrides.mode = event.target.value;
                    else delete overrides.mode;
                    await patchSession({ overrides });
                  })
                }
                className="mt-1 w-full rounded border bg-white p-2 dark:bg-neutral-950"
              >
                <option value="">Use inherited · {effective.mode === "inspect" ? "Read only" : "Allow edits"}</option>
                <option value="inspect">Read only</option>
                <option value="edit">Allow edits</option>
              </select>
            </label>
            <p className="text-[11px] text-gray-500">
              {compactAccessSummary(effective)} ·{" "}
              {compactModuleSummary(effective)}
            </p>
            {successor && (
              <p className="rounded border border-blue-200 bg-blue-50 p-2 text-xs text-blue-800 dark:border-blue-900 dark:bg-blue-950/30 dark:text-blue-200">
                The assistant will start fresh with the new settings and saved
                project context. Earlier messages remain in the transcript.
              </p>
            )}
            {effective.diagnostics.map((item) => (
              <p
                key={item}
                className="rounded border border-amber-200 bg-amber-50 p-2 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/20 dark:text-amber-200"
              >
                {item}
              </p>
            ))}
            <WorkspaceAssistantPresetForm key={`${effective.preset.id}:${effective.preset.revision}`} effective={effective} catalog={catalog}
              disabled={busy || settingsDisabled} onSave={(name, instructions, modules) => void act(async () => {
                const copy = await workbenchClient.clonePreset({ workspaceId, sourcePresetId: effective.preset.id, name, operationId: operation("assistant-copy") });
                await workbenchClient.updatePreset({ presetId: copy.id, expectedRevision: copy.revision, name, description: effective.preset.description, instructions, modules, operationId: operation("assistant-customize") });
                await patchSession({ presetId: copy.id });
              })} />
            {onEditHarness && (
              <div className="border-t border-gray-200 pt-3 dark:border-neutral-800">
                <button
                  type="button"
                  disabled={busy || settingsDisabled}
                  onClick={onEditHarness}
                  className="rounded border px-3 py-2 text-xs font-medium"
                >
                  Manage agent profiles…
                </button>
                <p className="mt-1 text-[11px] text-gray-500">
                  Create profiles, view system prompts, and choose allowed tools.
                </p>
              </div>
            )}
          </div>
        )}

        {tab === "documents" && workspaceId && (
          <div className="space-y-4">
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void act(async () => {
                    const path = await open({
                      multiple: false,
                      filters: [
                        {
                          name: "Research documents",
                          extensions: [
                            "pdf",
                            "tex",
                            "docx",
                            "md",
                            "txt",
                            "bib",
                          ],
                        },
                      ],
                    });
                    if (typeof path !== "string") return;
                    const title = window
                      .prompt(
                        "Paper title",
                        path
                          .split(/[\\/]/)
                          .pop()
                          ?.replace(/\.[^.]+$/, "") ?? "Paper",
                      )
                      ?.trim();
                    if (!title) return;
                    const result = await workbenchClient.importPaper({
                      workspaceId,
                      paperId: null,
                      title,
                      role: "manuscript",
                      path,
                      operationId: operation("paper-import"),
                    });
                    setProjectPaperId(result.paper.id);
                    if (!projectWorkspace) await patchSession({ paperId: result.paper.id });
                  })
                }
                className="rounded bg-gray-900 px-3 py-2 text-xs font-medium text-white dark:bg-neutral-100 dark:text-neutral-900"
              >
                Import paper
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void act(async () => {
                    const path = await open({
                      multiple: false,
                      directory: true,
                    });
                    if (typeof path !== "string") return;
                    const title = window
                      .prompt(
                        "Source-tree title",
                        path.split(/[\\/]/).pop() ?? "TeX project",
                      )
                      ?.trim();
                    if (!title) return;
                    const result = await workbenchClient.importPaper({
                      workspaceId,
                      paperId: null,
                      title,
                      role: "manuscript",
                      path,
                      operationId: operation("tree-import"),
                    });
                    setProjectPaperId(result.paper.id);
                    if (!projectWorkspace) await patchSession({ paperId: result.paper.id });
                  })
                }
                className="rounded border px-3 py-2 text-xs"
              >
                Import source tree
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void act(async () => {
                    const path = await open({
                      multiple: false,
                      filters: [
                        {
                          name: "Sources",
                          extensions: ["pdf", "bib", "txt", "md"],
                        },
                      ],
                    });
                    if (typeof path !== "string") return;
                    const title = window
                      .prompt(
                        "Source title",
                        path
                          .split(/[\\/]/)
                          .pop()
                          ?.replace(/\.[^.]+$/, "") ?? "Source",
                      )
                      ?.trim();
                    if (!title) return;
                    const result = await workbenchClient.importSource({
                      workspaceId,
                      title,
                      citationKey: null,
                      identifiers: {},
                      versionLabel: null,
                      path,
                      locator: null,
                      accessState: "full",
                      acquiredVia: path.toLowerCase().endsWith(".pdf")
                        ? "local_pdf"
                        : path.toLowerCase().endsWith(".bib")
                          ? "local_bibtex"
                          : "local_file",
                      operationId: operation("source-import"),
                    });
                    if (result.duplicateCandidates.length)
                      onError(
                        `Source imported separately; ${result.duplicateCandidates.length} possible duplicate${result.duplicateCandidates.length === 1 ? "" : "s"} retained for review.`,
                      );
                  })
                }
                className="rounded border px-3 py-2 text-xs"
              >
                Import source
              </button>
            </div>
            <section>
              <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
                Papers and versions
              </h3>
              <div className="mt-2 space-y-2">
                {papers.map((item) => (
                  <button
                    key={item.paper.id}
                    type="button"
                    onClick={() =>
                      void act(async () => {
                        setProjectPaperId(item.paper.id);
                        if (!projectWorkspace) await patchSession({ paperId: item.paper.id });
                      })
                    }
                    className={`block w-full rounded border p-3 text-left ${item.paper.id === selectedPaper?.paper.id ? "border-blue-400 bg-blue-50 dark:bg-blue-950/20" : "bg-white dark:bg-neutral-950"}`}
                  >
                    <span className="block font-medium">
                      {item.paper.title}
                    </span>
                    <span className="block text-[11px] text-gray-500">
                      {item.paper.role} ·{" "}
                      {item.revision?.inputKind ?? "no revision"} ·{" "}
                      {compactHash(item.revision?.contentHash)}
                    </span>
                    <span
                      className={`mt-1 block text-[11px] ${item.revision?.extraction.status === "failed" ? "text-red-600" : "text-gray-500"}`}
                    >
                      Extraction:{" "}
                      {String(
                        item.revision?.extraction.status ?? "unavailable",
                      )}
                      {item.revision?.extraction.error
                        ? ` — ${String(item.revision.extraction.error)}`
                        : ""}
                    </span>
                  </button>
                ))}
              </div>
            </section>
            {selectedPaper?.revision && (
              <section>
                <div className="flex gap-2">
                  <input
                    value={query}
                    onChange={(event) => setQuery(event.target.value)}
                    placeholder="Search this version"
                    className="min-w-0 flex-1 rounded border bg-white p-2 text-xs dark:bg-neutral-950"
                  />
                  <button
                    type="button"
                    disabled={!query.trim()}
                    onClick={() =>
                      void act(async () => {
                        setHits(
                          await workbenchClient.paperSearch(
                            workspaceId,
                            selectedPaper.revision!.id,
                            query,
                            20,
                          ),
                        );
                      })
                    }
                    className="rounded border px-3 text-xs"
                  >
                    Search
                  </button>
                </div>
                <div className="mt-2 space-y-2">
                  {hits.map((hit) => (
                    <details
                      key={`${hit.start}-${hit.end}`}
                      className="rounded border bg-white p-2 text-xs dark:bg-neutral-950"
                    >
                      <summary className="cursor-pointer">
                        {hit.page ? `p. ${hit.page}` : `line ${hit.line}`} ·
                        bytes {hit.start}–{hit.end}
                      </summary>
                      <p className="mt-2 whitespace-pre-wrap text-gray-600 dark:text-neutral-300">
                        {hit.excerpt}
                      </p>
                    </details>
                  ))}
                </div>
              </section>
            )}
            <section>
              <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
                Sources
              </h3>
              <div className="mt-2 space-y-2">
                {sources.map((source) => (
                  <div
                    key={source.id}
                    className="rounded border bg-white p-2 dark:bg-neutral-950"
                  >
                    <span className="font-medium">{source.title}</span>
                    <span className="block text-[11px] text-gray-500">
                      {source.accessState} · {source.acquiredVia} ·{" "}
                      {compactHash(source.contentHash)}
                    </span>
                  </div>
                ))}
              </div>
            </section>
          </div>
        )}

        {tab === "recipes" && snapshot && (
          <fieldset disabled={settingsDisabled}><WorkspaceRecipesPanel
            snapshot={snapshot}
            onSnapshot={onSnapshot}
            onError={onError}
          /></fieldset>
        )}

        {tab === "memory" && workspaceId && (
          <div className="space-y-4">
            <div>
              <div className="flex gap-2">
                <select
                  value={noteKind}
                  onChange={(event) =>
                    setNoteKind(event.target.value as ResearchNote["kind"])
                  }
                  className="rounded border bg-white p-2 text-xs dark:bg-neutral-950"
                >
                  {[
                    "question",
                    "assumption",
                    "decision",
                    "next_step",
                    "notation",
                    "handoff",
                  ].map((kind) => (
                    <option key={kind}>{kind}</option>
                  ))}
                </select>
                <button
                  type="button"
                  disabled={!noteBody.trim() || busy}
                  onClick={() =>
                    void act(async () => {
                      await workbenchClient.createNote({
                        workspaceId,
                        paperId: snapshot?.session.paperId ?? null,
                        kind: noteKind,
                        body: noteBody,
                        state: "accepted",
                        origin: "user",
                        pinned: noteKind === "handoff",
                        operationId: operation("note"),
                      });
                      setNoteBody("");
                    })
                  }
                  className="rounded bg-gray-900 px-3 text-xs text-white dark:bg-neutral-100 dark:text-neutral-900"
                >
                  Add reviewed note
                </button>
              </div>
              <textarea
                value={noteBody}
                onChange={(event) => setNoteBody(event.target.value)}
                rows={3}
                placeholder="Question, assumption, decision, next step, notation, or handoff…"
                className="mt-2 w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
              />
            </div>
            <div className="space-y-2">
              {notes.map((note) => (
                <div
                  key={note.id}
                  className="rounded border bg-white p-3 dark:bg-neutral-950"
                >
                  <div className="flex items-center gap-2 text-[11px] text-gray-500">
                    <span>{note.kind}</span>
                    <span>· {note.state}</span>
                    <span>· {note.origin}</span>
                    {note.pinned && <span>· pinned</span>}
                  </div>
                  <p className="mt-1 whitespace-pre-wrap">{note.body}</p>
                  <div className="mt-2 flex gap-1">
                    {note.state === "proposed" && (
                      <>
                        <button
                          type="button"
                          onClick={() =>
                            void act(async () => {
                              await workbenchClient.updateNote({
                                noteId: note.id,
                                expectedRevision: note.revision,
                                state: "accepted",
                                operationId: operation("accept-note"),
                              });
                            })
                          }
                          className="rounded border px-2 py-1 text-[11px]"
                        >
                          Accept
                        </button>
                        <button
                          type="button"
                          onClick={() =>
                            void act(async () => {
                              await workbenchClient.updateNote({
                                noteId: note.id,
                                expectedRevision: note.revision,
                                state: "rejected",
                                operationId: operation("reject-note"),
                              });
                            })
                          }
                          className="rounded border px-2 py-1 text-[11px]"
                        >
                          Reject
                        </button>
                      </>
                    )}
                    <button
                      type="button"
                      onClick={() =>
                        void act(async () => {
                          await workbenchClient.updateNote({
                            noteId: note.id,
                            expectedRevision: note.revision,
                            pinned: !note.pinned,
                            operationId: operation("pin-note"),
                          });
                        })
                      }
                      className="rounded border px-2 py-1 text-[11px]"
                    >
                      {note.pinned ? "Unpin" : "Pin"}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {tab === "evidence" && workspaceId && ledger && (
          <div className="space-y-4">
            <div>
              <textarea
                value={claim}
                onChange={(event) => setClaim(event.target.value)}
                rows={3}
                placeholder="Record a claim for review…"
                className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
              />
              <button
                type="button"
                disabled={!claim.trim() || busy}
                onClick={() =>
                  void act(async () => {
                    await workbenchClient.proposeClaim({
                      workspaceId,
                      paperId: snapshot?.session.paperId ?? null,
                      claim,
                      kind: "research_claim",
                      origin: "user",
                      operationId: operation("claim"),
                    });
                    setClaim("");
                  })
                }
                className="mt-2 rounded bg-gray-900 px-3 py-2 text-xs text-white dark:bg-neutral-100 dark:text-neutral-900"
              >
                Add proposed claim
              </button>
            </div>
            {(ledger.unsupportedAcceptedClaims.length > 0 ||
              ledger.staleClaims.length > 0) && (
              <p className="rounded border border-amber-200 bg-amber-50 p-2 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/20 dark:text-amber-200">
                {ledger.unsupportedAcceptedClaims.length} accepted claim(s) lack
                current supporting evidence; {ledger.staleClaims.length}{" "}
                claim(s) have stale evidence.
              </p>
            )}
            <div className="space-y-2">
              {ledger.claims.map((item) => (
                <div
                  key={item.id}
                  className="rounded border bg-white p-3 dark:bg-neutral-950"
                >
                  <div className="text-[11px] text-gray-500">
                    {item.workflowState} · {item.origin} · v{item.version}
                  </div>
                  <p className="mt-1">{item.claim}</p>
                  {item.workflowState !== "accepted" && (
                    <button
                      type="button"
                      onClick={() =>
                        void act(async () => {
                          await workbenchClient.setClaimState({
                            claimId: item.id,
                            state: "accepted",
                            operationId: operation("accept-claim"),
                          });
                        })
                      }
                      className="mt-2 rounded border px-2 py-1 text-[11px]"
                    >
                      Accept claim
                    </button>
                  )}
                </div>
              ))}
            </div>
            <section>
              <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
                Evidence links
              </h3>
              <div className="mt-2 space-y-2">
                {ledger.evidence.map((item) => (
                  <div
                    key={item.id}
                    className="rounded border bg-white p-2 text-xs dark:bg-neutral-950"
                  >
                    <div>
                      {item.relation} · {item.targetType}
                    </div>
                    <div
                      className={
                        item.freshness === "stale"
                          ? "text-red-600"
                          : "text-gray-500"
                      }
                    >
                      {item.assessment} by {item.assessor} · {item.freshness}
                      {item.staleReason ? ` — ${item.staleReason}` : ""}
                    </div>
                    {item.assessment !== "human_confirmed" && (
                      <button
                        type="button"
                        onClick={() =>
                          void act(async () => {
                            await workbenchClient.confirmEvidence({
                              evidenceId: item.id,
                              operationId: operation("confirm-evidence"),
                            });
                          })
                        }
                        className="mt-1 rounded border px-2 py-1 text-[11px]"
                      >
                        Confirm evidence
                      </button>
                    )}
                  </div>
                ))}
              </div>
            </section>
          </div>
        )}

        {tab === "results" && workspaceId && (
          <div className="space-y-4">
            <section hidden={resultsView === "runs"} className="space-y-2">
              <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
                New execution profile
              </h3>
              <input
                value={profileName}
                onChange={(event) => setProfileName(event.target.value)}
                className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
              />
              <select
                value={adapter}
                onChange={(event) =>
                  setAdapter(event.target.value as ExecutionProfile["adapter"])
                }
                className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
              >
                <option value="command">Command</option>
                <option value="latex">LaTeX</option>
                <option value="stata">Stata via oldstata</option>
              </select>
              <textarea
                value={argv}
                onChange={(event) => setArgv(event.target.value)}
                rows={3}
                aria-label="Argument vector JSON"
                className="w-full rounded border bg-white p-2 font-mono text-[11px] dark:bg-neutral-950"
              />
              <textarea
                value={inputs}
                onChange={(event) => setInputs(event.target.value)}
                rows={2}
                placeholder="Input paths, one relative path per line"
                className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
              />
              <textarea
                value={outputs}
                onChange={(event) => setOutputs(event.target.value)}
                rows={2}
                placeholder="Expected output paths, one per line"
                className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
              />
              <button
                type="button"
                disabled={busy || !workspace?.root}
                onClick={() =>
                  void act(async () => {
                    const parsed = JSON.parse(argv);
                    if (
                      !Array.isArray(parsed) ||
                      !parsed.every((value) => typeof value === "string")
                    )
                      throw new Error(
                        "Argument vector must be a JSON array of strings",
                      );
                    await workbenchClient.saveExecutionProfile({
                      profileId: null,
                      workspaceId,
                      name: profileName,
                      adapter,
                      argv: parsed,
                      cwd: workspace!.root!,
                      environment: {},
                      inputs: lines(inputs),
                      timeoutSeconds: 300,
                      outputs: lines(outputs),
                      expectedRevision: null,
                      operationId: operation("profile"),
                    });
                  })
                }
                className="rounded border px-3 py-2 text-xs"
              >
                Save profile
              </button>
              {!workspace?.root && (
                <p className="text-xs text-amber-700">
                  Attach a project folder before setting up commands.
                </p>
              )}
            </section>
            <section hidden={resultsView === "runs"}>
              <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
                Profiles
              </h3>
              <div className="mt-2 space-y-2">
                {profiles.map((profile) => (
                  <div
                    key={profile.id}
                    className="rounded border bg-white p-3 dark:bg-neutral-950"
                  >
                    <div className="font-medium">{profile.name}</div>
                    <pre className="mt-1 overflow-auto whitespace-pre-wrap text-[10px] text-gray-500">
                      {JSON.stringify(profile.argv)}
                    </pre>
                    <div className="mt-2 flex items-center gap-2">
                      <span
                        className={`text-[11px] ${profile.testStatus === "passed" ? "text-emerald-600" : "text-gray-500"}`}
                      >
                        {profile.testStatus ?? "untested"}
                      </span>
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() =>
                          void act(async () => {
                            setHostPreview({
                              preview: await projectClient.previewExecution(
                                profile.id,
                              ),
                              testOnly: true,
                            });
                          })
                        }
                        className="rounded border px-2 py-1 text-[11px]"
                      >
                        Test
                      </button>
                      <button
                        type="button"
                        disabled={busy || profile.testStatus !== "passed"}
                        onClick={() =>
                          void act(async () => {
                            setHostPreview({
                              preview: await projectClient.previewExecution(
                                profile.id,
                              ),
                              testOnly: false,
                            });
                          })
                        }
                        className="rounded border px-2 py-1 text-[11px]"
                      >
                        Run
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            </section>
            {hostPreview && (
              <section className="space-y-2 rounded border border-amber-300 p-3">
                <h3 className="text-xs font-semibold">Review host execution</h3>
                <p className="text-xs">{hostPreview.preview.boundary}</p>
                <pre className="overflow-auto whitespace-pre-wrap text-[11px]">
                  {JSON.stringify(
                    {
                      command: hostPreview.preview.command,
                      cwd: hostPreview.preview.cwd,
                      inputs: hostPreview.preview.inputs,
                      launch: hostPreview.preview.launch,
                      outputs: hostPreview.preview.outputs,
                    },
                    null,
                    2,
                  )}
                </pre>
                <button
                  className="rounded border px-3 py-2 text-xs"
                  disabled={busy}
                  onClick={() =>
                    void act(async () => {
                      const pending = hostPreview;
                      await projectClient.authorizeExecution(
                        pending.preview.profileId,
                        pending.preview.fingerprint,
                      );
                      setHostPreview(null);
                      await workbenchClient.runExecution({
                        profileId: pending.preview.profileId,
                        sessionId: snapshot?.session.id ?? null,
                        testOnly: pending.testOnly,
                        operationId: operation("authorized-host-run"),
                      });
                    })
                  }
                >
                  Authorize this command and{" "}
                  {hostPreview.testOnly ? "test" : "run"}
                </button>
                <button
                  className="ml-2 rounded border px-3 py-2 text-xs"
                  onClick={() => setHostPreview(null)}
                >
                  Cancel
                </button>
              </section>
            )}
            <section hidden={resultsView === "profiles"}>
              <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
                Run history
              </h3>
              <div className="mt-2 space-y-2">
                {executions.map((run) => (
                  <details
                    key={run.id}
                    className="rounded border bg-white p-3 dark:bg-neutral-950"
                  >
                    <summary className="cursor-pointer text-xs">
                      <span
                        className={
                          run.outcome === "completed"
                            ? "text-emerald-600"
                            : run.outcome === "running"
                              ? "text-blue-600"
                              : "text-red-600"
                        }
                      >
                        {run.outcome}
                      </span>{" "}
                      · {run.adapter} · {compactHash(run.dependencyHash)}
                    </summary>
                    <div className="mt-2 space-y-2 text-[11px]">
                      <pre className="overflow-auto whitespace-pre-wrap">
                        {JSON.stringify(run.command)}
                      </pre>
                      <p>
                        {run.inputManifest.snapshotConsistency ===
                        "captured_inputs_verified"
                          ? "Verified captured inputs · declared dependencies · host access"
                          : `Snapshot: ${run.snapshotConsistency}`}{" "}
                        · exit {run.exitStatus ?? "—"}
                      </p>
                      {["queued", "running"].includes(run.outcome) && (
                        <button
                          className="rounded border px-2 py-1 text-red-600"
                          onClick={() =>
                            void workbenchClient
                              .cancelExecution(run.id)
                              .catch((error) =>
                                onError(workbenchErrorMessage(error)),
                              )
                          }
                        >
                          Stop execution
                        </button>
                      )}
                      {run.adapter === "stata" && (
                        <p>
                          Stopping allows up to 30 seconds for oldstata to exit
                          through Legacy Time Off.
                        </p>
                      )}
                      {run.validation.stataCleanupRequired === true && (
                        <p role="alert" className="text-red-600">
                          The Stata wrapper was forcibly stopped. Verify its
                          process has exited and run the Legacy Time Off
                          shortcut to restore the normal clock before
                          continuing.
                        </p>
                      )}
                      <pre className="max-h-40 overflow-auto whitespace-pre-wrap rounded bg-gray-100 p-2 dark:bg-neutral-900">
                        {run.stderr || run.stdout || "No captured output"}
                      </pre>
                      <pre className="overflow-auto whitespace-pre-wrap">
                        {JSON.stringify(run.validation, null, 2)}
                      </pre>
                    </div>
                  </details>
                ))}
              </div>
              <p className="mt-2 text-[10px] text-gray-500">
                These checks compare exported numbers. Assess identification,
                causal interpretation, and model validity separately.
              </p>
            </section>
          </div>
        )}

        {tab === "release" && (
          <>
          {(releaseView === "all" || releaseView === "review") && <label className="mb-4 block text-xs">
            Paper to review
            <select aria-label="Paper to review" className="mt-2 block w-full rounded border bg-transparent p-2" value={selectedPaper?.paper.id ?? ""}
              disabled={busy} onChange={event => setProjectPaperId(event.target.value)}>
              <option value="">Choose a paper…</option>
              {papers.map(item => <option key={item.paper.id} value={item.paper.id}>{item.paper.title}</option>)}
            </select>
            {selectedPaper?.revision && <span className="mt-2 block text-gray-500">Saved revision · {compactHash(selectedPaper.revision.contentHash)}</span>}
          </label>}
          <WorkspaceReleasePanel
            snapshot={snapshot}
            workspaceId={workspaceId}
            view={releaseView}
            selectedPaper={selectedPaper}
            busy={busy}
            onAction={(action) => {
              void act(action);
            }}
            onError={onError}
            onReviewHandoff={onReviewHandoff}
          />
          </>
        )}
      </div>
    </SidebarPanel>
  );
}
