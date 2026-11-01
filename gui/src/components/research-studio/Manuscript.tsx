import SourceEditor, {
  type SourceEditorHandle,
  type SourceLineRequest,
} from "../file-workspace/SourceEditor";
import SourceDiff from "../file-workspace/SourceDiff";
import {
  readCapturedPdf,
  workspaceFileAdapter,
} from "../../lib/fileWorkspaceClient";
const PdfReader = lazy(() => import("../file-workspace/PdfReader"));
import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
import { EmptyState, IconButton, Menu, MenuItem, Sheet } from "../../ui";
import { Icon } from "../../ui/icons";
import {
  studioClient,
  type BuildReceipt,
  type BuildRecord,
  type EditorFile,
} from "../../lib/studioClient";
import { proseForDiff } from "../../lib/studioPresentation";
import { lineDiff } from "../../lib/diff";
import type { ProjectRecord } from "../../lib/projectClient";
import { executionClient } from "../../lib/workbench/executionClient";
import type { ExecutionProfile } from "../../lib/workbenchTypes";
import useReopenCount from "../../hooks/useReopenCount";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { ErrorNotice, useAction, type StudioProps } from "./shared";
import ManuscriptBuildSheet, {
  initialBuild,
  suggestBuild,
} from "./ManuscriptBuildSheet";
import ManuscriptBuilds from "./ManuscriptBuilds";
import "./manuscript.css";

const BUILD_STARTED = "Build started. The result opens here when it ends.";
const EDITABLE = /\.(tex|bib|md|markdown|rmd|qmd|txt)$/i;
type SidePanel = "pdf" | "builds";

/** LaTeX names files from its build folder, and the editor from the project's
 * top folder. The saved settings say which folder a build ran in. */
export function buildFolder(cwd: string | undefined, directories: string[]) {
  const from = (cwd ?? "").replaceAll("\\", "/");
  const folder = directories
    .filter((d) => d !== "." && from.endsWith(`/${d}`))
    .sort((a, b) => b.length - a.length)[0];
  return folder ? `${folder}/` : "";
}
function readDraft(key: string): { file: EditorFile; draft: string } | null {
  try {
    const value = JSON.parse(localStorage.getItem(key) ?? "null");
    return value &&
      typeof value.draft === "string" &&
      typeof value.file?.path === "string" &&
      typeof value.file.hash === "string" &&
      typeof value.file.content === "string" &&
      (value.file.checkpointId === null ||
        typeof value.file.checkpointId === "string")
      ? value
      : null;
  } catch {
    return null;
  }
}
export default function Manuscript({
  workspaceId,
  data,
  onRefresh,
  onDocument,
  active,
}: StudioProps) {
  /* The file opens by itself, so the page starts on the manuscript rather than
     on a form. Build opens a sheet, which still shows the exact command for
     approval. The side panel holds the build's PDF and its results. */
  const draftKey = `pipeline.manuscriptDraft.${workspaceId}`;
  const [initialDraft] = useState(() => readDraft(draftKey));
  const cached = useRef(initialDraft);
  const [path, setPath] = useState(cached.current?.file.path ?? "");
  const [checkpoint, setCheckpoint] = useState(
    cached.current?.file.checkpointId ?? "",
  );
  const [file, setFile] = useState<EditorFile | null>(null);
  const [draft, setDraft] = useState("");
  const [notice, setNotice] = useState(
    cached.current
      ? "An unsaved draft is retained. Load its file to restore it."
      : "",
  );
  const [diffMode, setDiffMode] = useState("source");
  const [showChanges, setShowChanges] = useState(false);
  const [menu, setMenu] = useState(false);
  const menuButton = useRef<HTMLButtonElement>(null);
  const [buildSheet, setBuildSheet] = useState(false);
  const [side, setSide] = useState<SidePanel | null>(null);
  const sideClosed = useRef(false);
  const editor = useRef<SourceEditorHandle>(null);
  const saveButton = useRef<HTMLButtonElement>(null);
  const [completionTexts, setCompletionTexts] = useState<string[]>([]);
  const [builds, setBuilds] = useState<
    ProjectRecord<BuildRecord | BuildReceipt>[]
  >([]);
  const [buildsLoaded, setBuildsLoaded] = useState(false);
  const [config, setConfig] = useState(initialBuild);
  const [profiles, setProfiles] = useState<ExecutionProfile[]>([]);
  const [configId, setConfigId] = useState("");
  const [advanced, setAdvanced] = useState("");
  const [selectedBuild, setSelectedBuild] = useState("");
  const [compareBuild, setCompareBuild] = useState("");
  const [sync, setSync] = useState("");
  const [syncCandidates, setSyncCandidates] = useState<
    import("../../lib/studioClient").SyncResult["candidates"]
  >([]);
  // The Source line field stays local; only explicit actions move the editor.
  const [sourceLine, setSourceLine] = useState("1");
  const [sourceRequest, setSourceRequest] = useState<SourceLineRequest | null>(
    null,
  );
  const [pdfPage, setPdfPage] = useState(1);
  const { error, busy, run, setError } = useAction();
  const refresh = async () =>
    setBuilds(await studioClient.records(workspaceId, "build"));
  const reopened = useReopenCount(active);
  useEffect(() => {
    let stopped = false;
    void Promise.all([
      studioClient.records<BuildRecord | BuildReceipt>(workspaceId, "build"),
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
  const configs = builds.filter(
    (r): r is ProjectRecord<BuildRecord> =>
      r.body.recordType === "configuration",
  );
  const receipts = builds.filter(
    (r): r is ProjectRecord<BuildReceipt> => r.body.recordType === "receipt",
  );
  const chosen = configs.find((r) => r.id === configId);
  const finished = data.executions.filter(
    (e) => e.adapter === "latex" && !["queued", "running"].includes(e.outcome),
  );
  const latestBuild = receipts.some((r) => r.id === `build_${finished[0]?.id}`)
    ? `build_${finished[0].id}`
    : receipts[0]?.id;
  const shownBuild = selectedBuild || latestBuild || "";
  const receipt = receipts.find((r) => r.id === shownBuild)?.body;
  const comparison = receipts.find((r) => r.id === compareBuild)?.body;
  const prefix = buildFolder(
    data.executions.find((e) => e.id === receipt?.executionId)?.cwd,
    configs.map((c) => c.body.config.directory),
  );
  const workingCopies = data.changes
    .filter((c) => ["isolated", "review"].includes(c.body.state))
    .map((c) => ({
      value: c.id,
      label:
        data.tasks.find((t) => t.id === c.body.taskId)?.body.objective ?? c.id,
    }));
  const projectFiles = useMemo(
    () => (data.inventory?.body.files ?? []).map((f) => f.path),
    [data.inventory],
  );
  const fileOptions = useMemo(
    () => projectFiles.filter((p) => EDITABLE.test(p)),
    [projectFiles],
  );
  const showSync = async (request: {
    sourcePath?: string;
    line?: number;
    page?: number;
    x?: number;
    y?: number;
  }) => {
    if (!receipt) return;
    const mapped = await studioClient.sync(
      workspaceId,
      receipt.executionId,
      request.sourcePath?.startsWith(prefix)
        ? { ...request, sourcePath: request.sourcePath.slice(prefix.length) }
        : request,
    );
    setSync(mapped.result);
    setSyncCandidates(mapped.candidates);
    if (mapped.candidates[0]?.page) setPdfPage(mapped.candidates[0].page);
  };
  const dirty = file !== null && draft !== file.content;
  const draftState = useRef({ file, draft });
  draftState.current = { file, draft };
  const persistDraft = () => {
    const current = draftState.current;
    try {
      if (current.file) {
        const key = `${draftKey}.${current.file.checkpointId ?? "working"}.${current.file.path}`;
        if (current.draft !== current.file.content) {
          localStorage.setItem(draftKey, JSON.stringify(current));
          localStorage.setItem(key, JSON.stringify(current));
        } else {
          localStorage.removeItem(draftKey);
          localStorage.removeItem(key);
        }
      }
    } catch {
      setNotice(
        "Draft storage is full. Keep this editor open until the file is saved.",
      );
    }
  };
  useEffect(() => {
    const timer = setTimeout(persistDraft, 300);
    return () => clearTimeout(timer);
  }, [file, draft]);
  useEffect(
    () => () => {
      const current = draftState.current;
      try {
        if (current.file && current.draft !== current.file.content) {
          localStorage.setItem(draftKey, JSON.stringify(current));
          localStorage.setItem(
            `${draftKey}.${current.file.checkpointId ?? "working"}.${current.file.path}`,
            JSON.stringify(current),
          );
        } else if (current.file) {
          localStorage.removeItem(draftKey);
          localStorage.removeItem(
            `${draftKey}.${current.file.checkpointId ?? "working"}.${current.file.path}`,
          );
        }
      } catch {
        /* Current save state already discloses draft-storage failure. */
      }
    },
    [draftKey],
  );
  const diff = useMemo(
    () =>
      file && showChanges && diffMode === "prose"
        ? lineDiff(proseForDiff(file.content), proseForDiff(draft))
        : [],
    [file, draft, diffMode, showChanges],
  );
  /** Keep the open draft so it returns when this file is opened again. */
  const stashDraft = () => {
    if (dirty && file)
      localStorage.setItem(
        `${draftKey}.${file.checkpointId ?? "working"}.${file.path}`,
        JSON.stringify({ file, draft }),
      );
  };
  const openFile = async (target: string, inCopy: string) => {
    const next = await studioClient.editor(workspaceId, target, inCopy || null);
    const retained =
      readDraft(`${draftKey}.${inCopy || "working"}.${target}`) ??
      cached.current;
    setPath(target);
    if (
      retained?.file.path === next.path &&
      retained.file.checkpointId === next.checkpointId
    ) {
      setFile(retained.file);
      setDraft(retained.draft);
      setNotice(
        retained.file.hash === next.hash
          ? "Restored unsaved draft"
          : "Restored unsaved draft; the file changed externally. Preserve the draft before discarding and reloading.",
      );
    } else {
      setFile(next);
      setDraft(next.content);
      setNotice("");
    }
  };
  const load = () =>
    run(async () => {
      if (dirty)
        throw new Error(
          "Save or discard the current draft before reloading this file.",
        );
      await openFile(path, checkpoint);
      setNotice((old) => old || "Reloaded from the project");
    });
  const switchFile = (target: string, inCopy = checkpoint) =>
    run(async () => {
      stashDraft();
      await openFile(target, inCopy);
    });
  // Open the manuscript without asking: a retained draft's file, the saved
  // build's root, then the project's main TeX file. A project with none of
  // these starts on the file menu, which is not an error.
  const autoOpened = useRef(false);
  useEffect(() => {
    if (autoOpened.current || !buildsLoaded) return;
    autoOpened.current = true;
    const saved = configs[0]?.body.config;
    if (saved) {
      setConfigId(configs[0].id);
      setConfig(saved);
      setAdvanced(
        saved.advancedArgv ? JSON.stringify(saved.advancedArgv, null, 2) : "",
      );
    }
    const root =
      saved &&
      (saved.directory === "."
        ? saved.rootDocument
        : `${saved.directory}/${saved.rootDocument}`);
    const target =
      path ||
      root ||
      fileOptions.find((p) => /(^|\/)main\.tex$/i.test(p)) ||
      fileOptions.find((p) => /\.tex$/i.test(p)) ||
      fileOptions[0] ||
      "main.tex";
    void openFile(target, checkpoint).catch(() => {
      setPath(fileOptions.includes(target) ? target : "");
    });
  }, [buildsLoaded]);
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
  // A new result opens the PDF, or the errors when there is no PDF. Choosing
  // an older build leaves the panel where it is.
  useEffect(() => {
    const latest = receipts.find((r) => r.id === latestBuild)?.body;
    if (!latest || sideClosed.current) return;
    setSelectedBuild("");
    setNotice((old) => (old === BUILD_STARTED ? "" : old));
    setSide(latest.pdf?.revision ? "pdf" : "builds");
  }, [latestBuild]);
  const jumpLine = (line: number) => editor.current?.goToLine(line);
  // Reveal a line without taking focus from the control that asked for it.
  const revealLine = (line: number) => {
    setSourceLine(String(line));
    setSourceRequest({ line });
  };
  const navigateSource = async (target: string, line: number) => {
    if (file?.path === target) {
      revealLine(line);
      return;
    }
    stashDraft();
    const next = await studioClient.editor(
      workspaceId,
      target,
      checkpoint || null,
    );
    const retained = readDraft(
      `${draftKey}.${checkpoint || "working"}.${target}`,
    );
    setPath(target);
    setFile(retained?.file ?? next);
    setDraft(retained?.draft ?? next.content);
    revealLine(line);
    setNotice(
      "Opened the source location recorded in this build. The file may have changed since then. Unsaved drafts are still available for each file.",
    );
  };
  useEffect(() => {
    let live = true;
    const candidates = (data.inventory?.body.files ?? [])
      .filter((f) => /\.(bib|tex)$/i.test(f.path))
      .slice(0, 40);
    void Promise.all(
      candidates.map((f) =>
        studioClient
          .editor(workspaceId, f.path, checkpoint || null)
          .then((v) => v.content)
          .catch(() => ""),
      ),
    ).then((texts) => {
      if (live) setCompletionTexts(texts);
    });
    return () => {
      live = false;
    };
  }, [workspaceId, checkpoint, data.inventory]);

  const save = () =>
    run(async () => {
      if (!file) return;
      const response = await studioClient.mutate<{ hash: string }>(
        workspaceId,
        {
          action: "saveText",
          checkpointId: file.checkpointId,
          path: file.path,
          expectedHash: file.hash,
          content: draft,
        },
      );
      setFile({ ...file, content: draft, hash: response.hash });
      cached.current = null;
      try {
        localStorage.removeItem(draftKey);
        if (file)
          localStorage.removeItem(
            `${draftKey}.${file.checkpointId ?? "working"}.${file.path}`,
          );
      } catch {
        /* Saving still succeeded. */
      }
      setNotice("Saved. Rebuild the manuscript to check the updated files.");
      await onRefresh();
    });
  const discard = () => {
    cached.current = null;
    try {
      localStorage.removeItem(draftKey);
      if (file)
        localStorage.removeItem(
          `${draftKey}.${file.checkpointId ?? "working"}.${file.path}`,
        );
    } catch {
      /* Current draft is still discarded. */
    }
    setDraft(file?.content ?? "");
    setNotice("Draft discarded");
  };
  /** Follow a click in the PDF back to its source line. */
  const fromPdf = (point: { page: number; x: number; y: number }) =>
    run(async () => {
      if (!receipt) return;
      const mapped = await studioClient.sync(
        workspaceId,
        receipt.executionId,
        point,
      );
      setSyncCandidates(mapped.candidates);
      const first = mapped.candidates.find((c) => c.sourcePath && c.line);
      if (first) await navigateSource(prefix + first.sourcePath!, first.line!);
    });
  const openBuild = () => {
    sideClosed.current = false;
    if (!configId && !configs.length && file)
      setConfig(suggestBuild(file.path, projectFiles));
    setBuildSheet(true);
  };
  const errors = receipt?.diagnostics.filter(
    (d) => d.severity === "error",
  ).length;
  const shownFiles = [...new Set([...(path ? [path] : []), ...fileOptions])];
  return (
    <div className="manuscript">
      <div className="manuscript-bar">
        {shownFiles.length > 1 || !path ? (
          <select
            aria-label="File"
            className="manuscript-file"
            value={path}
            onChange={(e) => void switchFile(e.target.value)}
          >
            {!path && <option value="">Choose a file…</option>}
            {shownFiles.map((p) => (
              <option key={p} value={p}>
                {p}
              </option>
            ))}
          </select>
        ) : (
          <strong className="manuscript-file">{path}</strong>
        )}
        {workingCopies.length > 0 && (
          <select
            aria-label="Edit in"
            className="manuscript-file"
            value={checkpoint}
            onChange={(e) => {
              setCheckpoint(e.target.value);
              if (path) void switchFile(path, e.target.value);
            }}
          >
            <option value="">Project working copy</option>
            {workingCopies.map((c) => (
              <option key={c.value} value={c.value}>
                {c.label}
              </option>
            ))}
          </select>
        )}
        <span role="status" className="manuscript-status">
          {dirty ? "Unsaved changes" : notice}
        </span>
        {file && dirty && (
          <div className="file-modes" role="group" aria-label="View">
            <button
              aria-pressed={!showChanges}
              onClick={() => setShowChanges(false)}
            >
              Source
            </button>
            <button
              aria-pressed={showChanges}
              onClick={() => setShowChanges(true)}
            >
              Changes
            </button>
          </div>
        )}
        {dirty && (
          <button className="file-action" onClick={discard}>
            Discard unsaved draft
          </button>
        )}
        {file && dirty && (
          <button
            ref={saveButton}
            className="file-action primary"
            disabled={busy}
            title="Command/Ctrl+S"
            onClick={() => void save()}
          >
            Save {file.checkpointId ? "task draft" : "working copy"}
          </button>
        )}
        {receipt && (
          <button
            className="file-action"
            onClick={() => {
              sideClosed.current = false;
              setSide("builds");
            }}
          >
            {/complete|succe/.test(receipt.outcome)
              ? "Last build succeeded"
              : `Last build ${receipt.outcome}`}
            {errors ? ` · ${errors} error${errors === 1 ? "" : "s"}` : ""}
          </button>
        )}
        <button
          className={`file-action${dirty ? "" : " primary"}`}
          onClick={openBuild}
        >
          Build
        </button>
        <IconButton
          size="sm"
          label={side ? "Hide PDF and builds" : "Show PDF and builds"}
          tooltipSide="bottom"
          aria-pressed={Boolean(side)}
          onClick={() => {
            sideClosed.current = Boolean(side);
            setSide(side ? null : receipt?.pdf?.revision ? "pdf" : "builds");
          }}
        >
          <Icon name="columns" className="h-4 w-4" />
        </IconButton>
        <IconButton
          ref={menuButton}
          size="sm"
          label="More manuscript actions"
          tooltipSide="bottom"
          aria-haspopup="menu"
          aria-expanded={menu}
          onClick={() => setMenu(!menu)}
        >
          <Icon name="more" className="h-4 w-4" />
        </IconButton>
        <Menu
          open={menu}
          onClose={() => setMenu(false)}
          anchorRef={menuButton}
          label="More manuscript actions"
        >
          <MenuItem disabled={!path || busy} onSelect={() => void load()}>
            Reload file
          </MenuItem>
          <MenuItem
            disabled={!file || busy}
            onSelect={() =>
              void run(() =>
                studioClient.externalEditor(
                  workspaceId,
                  file!.path,
                  file!.checkpointId,
                ),
              )
            }
          >
            Open external editor
          </MenuItem>
        </Menu>
      </div>
      <ErrorNotice error={error} />
      <div className="manuscript-body" data-side={Boolean(side)}>
        <div className="manuscript-source">
          {file ? (
            <>
              <div className="manuscript-editor" hidden={showChanges && dirty}>
                <SourceEditor
                  ref={editor}
                  label="Manuscript source editor"
                  readOnly={busy}
                  path={`${workspaceId}/${file.checkpointId ?? "working"}/${file.path}`}
                  value={draft}
                  onChange={setDraft}
                  reveal={sourceRequest}
                  onSave={() => saveButton.current?.click()}
                  completionTexts={completionTexts}
                  wrapByDefault
                />
              </div>
              {showChanges && dirty && (
                <div className="manuscript-changes">
                  <div className="file-modes" role="group" aria-label="Compare">
                    {["source", "prose"].map((m) => (
                      <button
                        key={m}
                        aria-pressed={diffMode === m}
                        onClick={() => setDiffMode(m)}
                      >
                        {m === "source" ? "Source diff" : "Prose aid"}
                      </button>
                    ))}
                  </div>
                  {diffMode === "prose" && (
                    <p className="text-xs text-ink-muted">
                      Check equations, labels, and citations in the source view.
                    </p>
                  )}
                  {diffMode === "source" ? (
                    <SourceDiff
                      before={file.content}
                      after={draft}
                      path={file.path}
                    />
                  ) : (
                    <pre className="whitespace-pre-wrap text-xs">
                      {diff.map((line, i) => (
                        <div key={i}>
                          {line.type === "add"
                            ? "+ "
                            : line.type === "del"
                              ? "− "
                              : "  "}
                          {line.text}
                        </div>
                      ))}
                    </pre>
                  )}
                </div>
              )}
            </>
          ) : (
            <div className="file-empty">
              <EmptyState
                icon={<Icon name="file" className="h-8 w-8" />}
                title="No manuscript file open"
                description={
                  busy
                    ? "Opening the file…"
                    : "Choose a TeX, Markdown, BibTeX, or text file from the project."
                }
              />
            </div>
          )}
        </div>
        {side && (
          <div className="manuscript-side">
            <div className="manuscript-side-tabs">
              <div className="file-modes" role="group" aria-label="Side panel">
                <button
                  aria-pressed={side === "pdf"}
                  onClick={() => setSide("pdf")}
                >
                  PDF
                </button>
                <button
                  aria-pressed={side === "builds"}
                  onClick={() => setSide("builds")}
                >
                  Builds
                </button>
              </div>
            </div>
            {side === "pdf" &&
              (receipt?.pdf?.revision ? (
                <div className="manuscript-pdf">
                  <Suspense fallback={<p>Loading build PDF…</p>}>
                    <PdfReader
                      key={receipt.pdf.revision.id}
                      documentKey={`${workspaceId}:${receipt.pdf.revision.id}`}
                      title="Exact build PDF"
                      compact
                      initialPage={pdfPage}
                      load={() =>
                        readCapturedPdf(
                          workspaceFileAdapter({
                            workspaceId,
                            revisionId: receipt.pdf!.revision!.id,
                          }),
                        )
                      }
                      onSource={(point) => void fromPdf(point)}
                    />
                  </Suspense>
                </div>
              ) : (
                <div className="file-empty">
                  <EmptyState
                    icon={<Icon name="file" className="h-8 w-8" />}
                    title="No PDF yet"
                    description="Build the manuscript to see it here."
                  />
                </div>
              ))}
            {side === "builds" && (
              <ManuscriptBuilds
                workspaceId={workspaceId}
                builds={finished}
                receipts={receipts}
                shownBuild={shownBuild}
                receipt={receipt}
                compareBuild={compareBuild}
                comparison={comparison}
                busy={busy}
                run={run}
                fileOpen={Boolean(file)}
                sourcePath={file?.path ?? path}
                sourceLine={sourceLine}
                setSourceLine={setSourceLine}
                pdfPage={pdfPage}
                setPdfPage={setPdfPage}
                sync={sync}
                syncCandidates={syncCandidates}
                onSync={showSync}
                onPdfPoint={fromPdf}
                onSelect={setSelectedBuild}
                onCompare={setCompareBuild}
                onInspected={async (id) => {
                  await refresh();
                  setSelectedBuild(`build_${id}`);
                  await onRefresh();
                }}
                onRefreshBuilds={refresh}
                onDocument={onDocument}
                onJumpLine={jumpLine}
                prefix={prefix}
                onNavigate={(target, line) =>
                  run(() => navigateSource(prefix + target, line))
                }
                onCandidate={(found, line) => {
                  const target = prefix + found;
                  void run(async () => {
                    await navigateSource(target, line);
                    setNotice(
                      `Exact build maps to ${target}:${line}. The file may have changed since that build. Unsaved drafts are still available for each file.`,
                    );
                  });
                }}
              />
            )}
          </div>
        )}
        <Sheet
          open={buildSheet}
          onClose={() => setBuildSheet(false)}
          title="Build manuscript"
          width={420}
        >
          <ManuscriptBuildSheet
            workspaceId={workspaceId}
            busy={busy}
            run={run}
            configs={configs}
            configId={configId}
            setConfigId={setConfigId}
            config={config}
            setConfig={setConfig}
            advanced={advanced}
            setAdvanced={setAdvanced}
            workingCopies={workingCopies}
            profile={
              chosen
                ? (profiles.find(
                    (p) =>
                      p.id === chosen.body.profile.id &&
                      p.revision === chosen.body.profile.revision,
                  ) ?? chosen.body.profile)
                : null
            }
            onSaved={refresh}
            onQueued={() => {
              setBuildSheet(false);
              setNotice(BUILD_STARTED);
            }}
          />
        </Sheet>
      </div>
    </div>
  );
}
