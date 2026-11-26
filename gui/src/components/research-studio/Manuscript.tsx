import { useManuscriptNavigation } from "./useManuscriptNavigation";
import { useManuscriptBuilds } from "./useManuscriptBuilds";
import { useManuscriptDocument } from "./useManuscriptDocument";
import { manuscriptDraftStorage } from "../../lib/manuscriptDraftStorage";
import { saveRetainedManuscriptDrafts } from "./manuscriptDrafts";
import Citations from "./Citations";
import RecoveredManuscriptDraft from "./RecoveredManuscriptDraft";
import SourceEditor, {
  type SourceEditorHandle,
} from "../file-workspace/SourceEditor";
import SourceDiff from "../file-workspace/SourceDiff";
import { workspaceFileAdapter } from "../../lib/fileWorkspaceClient";
const PdfReader = lazy(() => import("../file-workspace/PdfReader"));
import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
import { EmptyState, IconButton, Menu, MenuItem, Sheet } from "../../ui";
import { Icon } from "../../ui/icons";
import { studioClient } from "../../lib/studioClient";
import { proseForDiff } from "../../lib/studioPresentation";
import { lineDiff } from "../../lib/diff";
import { projectClient } from "../../lib/projectClient";
import useReopenCount from "../../hooks/useReopenCount";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { ErrorNotice, useAction, type StudioProps } from "./shared";
import ManuscriptBuildSheet, { suggestBuild } from "./ManuscriptBuildSheet";
import ManuscriptBuilds from "./ManuscriptBuilds";
import "./manuscript.css";

const BUILD_STARTED = "Build started. The result opens here when it ends.";
const FILE_CHANGED =
  "This file changed outside the editor. Compare with the current file to keep your draft, or reload to discard it.";
const FILE_COMPARED =
  "Changes now compares your draft with the current file. Saving replaces the current file with your draft.";
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
export default function Manuscript({
  workspaceId,
  documentId,
  documentPath,
  data,
  onRefresh,
  onDocument,
  onAskSource,
  active,
}: StudioProps) {
  /* The file opens by itself, so the page starts on the manuscript rather than
     on a form. Build opens a sheet, which still shows the exact command for
     approval. The side panel holds the build's PDF and its results. */
  const [diffMode, setDiffMode] = useState("source");
  const [showChanges, setShowChanges] = useState(false);
  const [menu, setMenu] = useState(false);
  const menuButton = useRef<HTMLButtonElement>(null);
  const [buildSheet, setBuildSheet] = useState(false);
  const [buildPrepared, setBuildPrepared] = useState(false);
  const [side, setSide] = useState<SidePanel | null>(null);
  const sideClosed = useRef(false);
  const editor = useRef<SourceEditorHandle>(null);
  const saveButton = useRef<HTMLButtonElement>(null);
  const [completionTexts, setCompletionTexts] = useState<string[]>([]);
  const { error, busy, run, setError } = useAction();
  const reopened = useReopenCount(active);
  const draftStorage = manuscriptDraftStorage(workspaceId, documentId);
  const {
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
  } = useManuscriptBuilds({
    workspaceId,
    documentId,
    data,
    onRefresh,
    setError,
    reopened,
  });
  const {
    path,
    setPath,
    checkpoint,
    setCheckpoint,
    file,
    draft,
    recoveredDraft,
    setRecoveredDraft,
    fileState,
    notice,
    setNotice,
    dirty,
    editDraft,
    stashDraft,
    openFile,
    forgetDraft,
    discard,
    load,
    switchFile,
    compare,
    saveCurrent,
    save,
    hasRetainedDraft,
  } = useManuscriptDocument({
    workspaceId,
    documentId,
    active,
    onRefresh,
    reopened,
    run,
    setError,
    onOpen: () => setSourcePassage(null),
    onCompare: () => setShowChanges(true),
  });
  // New projects need an inventory before the file switcher can list sections.
  const inventoryRequested = useRef(false);
  useEffect(() => {
    if (active === false || data.inventory || inventoryRequested.current)
      return;
    inventoryRequested.current = true;
    void projectClient
      .mutate(workspaceId, { action: "checkForChanges" })
      .then(onRefresh)
      .catch((cause) => setError(workbenchErrorMessage(cause)));
  }, [workspaceId, active, data.inventory, onRefresh, setError]);
  const prefix = buildFolder(
    data.executions.find((e) => e.id === receipt?.executionId)?.cwd,
    configs.map((c) => c.body.config.directory),
  );
  const {
    sync,
    syncCandidates,
    sourceLine,
    setSourceLine,
    sourcePassage,
    setSourcePassage,
    sourceRequest,
    pdfPage,
    setPdfPage,
    pdfPosition,
    showSync,
    revealLine,
    fromPdf,
  } = useManuscriptNavigation({
    workspaceId,
    receipt,
    prefix,
    run,
    navigateSource: (target, line) => navigateSource(target, line),
  });
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
  const diff = useMemo(
    () =>
      file && showChanges && diffMode === "prose"
        ? lineDiff(proseForDiff(file.content), proseForDiff(draft))
        : [],
    [file, draft, diffMode, showChanges],
  );
  // Open the manuscript without asking: a retained draft's file, the saved
  // build's root, then the project's main TeX file. A project with none of
  // these starts on the file menu, which is not an error.
  const autoOpened = useRef(false);
  useEffect(() => {
    if (autoOpened.current || !buildsLoaded) return;
    if (documentId && !documentPath && !data.inventory && !configs.length)
      return;
    autoOpened.current = true;
    let preferred: string | null = null;
    try {
      preferred = draftStorage.readBuild();
    } catch {
      /* Use the latest saved configuration. */
    }
    const selected = configs.find((c) => c.id === preferred) ?? configs[0];
    const saved = selected?.body.config;
    if (saved) {
      setConfigId(selected.id);
      setConfig(documentId ? { ...saved, documentId } : saved);
      setAdvanced(
        saved.advancedArgv ? JSON.stringify(saved.advancedArgv, null, 2) : "",
      );
    }
    const root =
      saved &&
      (saved.directory === "." || !saved.directory
        ? saved.rootDocument
        : `${saved.directory}/${saved.rootDocument}`);
    const target =
      path ||
      root ||
      documentPath ||
      (!documentId
        ? fileOptions.find((p) => /(^|\/)main\.tex$/i.test(p)) ||
          fileOptions.find((p) => /\.tex$/i.test(p)) ||
          fileOptions[0] ||
          "main.tex"
        : "");
    if (!target) return;
    void run(async () => {
      try {
        await openFile(
          target,
          hasRetainedDraft ? checkpoint : (saved?.checkpointId ?? ""),
        );
      } catch {
        setPath(fileOptions.includes(target) ? target : "");
      }
    });
  }, [buildsLoaded, documentPath, data.inventory]);
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
  const navigateSource = async (target: string, line: number) => {
    if (file?.path === target) {
      revealLine(line);
      return;
    }
    stashDraft();
    await openFile(target, checkpoint);
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

  const openBuild = () =>
    run(async () => {
      if (recoveredDraft !== null)
        throw new Error(
          "Resolve the earlier manuscript draft before preparing a build. Both versions are still available.",
        );
      sideClosed.current = false;
      setBuildPrepared(false);
      if (configId && (config.checkpointId ?? "") !== checkpoint) {
        setBuildSheet(true);
        throw new Error(
          "The saved build uses a different working copy. Choose Build in under Build settings to match the editor before building.",
        );
      }
      if (dirty && !(await saveCurrent())) return;
      await saveRetainedManuscriptDrafts(workspaceId, checkpoint, file?.path);
      if (!configId && !configs.length && file)
        setConfig({
          ...suggestBuild(file.path),
          checkpointId: checkpoint || null,
        });
      setBuildPrepared(true);
      setBuildSheet(true);
    });
  const building = data.executions.some(
    (e) => e.adapter === "latex" && ["queued", "running"].includes(e.outcome),
  );
  const errors = receipt?.diagnostics.filter(
    (d) => d.severity === "error",
  ).length;
  const shownFiles = [...new Set([...(path ? [path] : []), ...fileOptions])];
  return (
    <div className="manuscript">
      <div className="manuscript-bar">
        {shownFiles.length > 1 || !path ? (
          <select
            disabled={busy}
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
          <strong className="manuscript-file" title={path}>
            {path}
          </strong>
        )}
        {(workingCopies.length > 0 || checkpoint) && (
          <select
            disabled={busy}
            aria-label="Edit in"
            className="manuscript-file"
            value={checkpoint}
            onChange={(e) => {
              if (path) void switchFile(path, e.target.value);
              else setCheckpoint(e.target.value);
            }}
          >
            <option value="">Project working copy</option>
            {checkpoint &&
              !workingCopies.some((copy) => copy.value === checkpoint) && (
                <option value={checkpoint} disabled>
                  {checkpoint} (no longer available)
                </option>
              )}
            {workingCopies.map((c) => (
              <option key={c.value} value={c.value}>
                {c.label}
              </option>
            ))}
          </select>
        )}
        <span role="status" className="manuscript-status">
          {!dirty
            ? notice
            : fileState === "changed"
              ? FILE_CHANGED
              : fileState === "compared"
                ? FILE_COMPARED
                : "Unsaved changes"}
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
        {dirty && fileState === "changed" && (
          <button
            className="file-action"
            disabled={busy}
            onClick={() => void compare()}
          >
            Compare with current file
          </button>
        )}
        {file && dirty && (
          <button
            ref={saveButton}
            className="file-action"
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
          className="file-action primary"
          disabled={busy || building || fileState === "changed"}
          onClick={() => void openBuild()}
        >
          {building ? "Building…" : dirty ? "Save and build" : "Build"}
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
          <MenuItem
            disabled={busy}
            onSelect={() => {
              setBuildPrepared(false);
              setBuildSheet(true);
            }}
          >
            Build settings
          </MenuItem>
          <MenuItem disabled={!path || busy} onSelect={() => void load()}>
            Reload file
          </MenuItem>
          {dirty && (
            <MenuItem disabled={busy} onSelect={() => void discard()}>
              Discard unsaved draft
            </MenuItem>
          )}
          {dirty && (
            <MenuItem disabled={busy} onSelect={() => void compare()}>
              Compare with current file
            </MenuItem>
          )}
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
      {file && (
        <Citations
          key={`${workspaceId}/${file.checkpointId ?? "working"}/${file.path}`}
          workspaceId={workspaceId}
          paths={(data.inventory?.body.files ?? [])
            .filter((entry) => entry.path.endsWith(".bib"))
            .map((entry) => entry.path)}
          sourcePath={file.path}
          sourceText={draft}
          checkpointId={file.checkpointId}
          disabled={busy}
          onInsert={(text) => editor.current?.insertText(text)}
        />
      )}
      {file && sourcePassage?.quote && onAskSource && (
        <button
          type="button"
          className="file-action self-start"
          onClick={() =>
            onAskSource({
              ...sourcePassage,
              path: file.path,
              hash: file.hash,
              draft: dirty,
              checkpointId: file.checkpointId,
            })
          }
        >
          Ask about selected source
        </button>
      )}
      {recoveredDraft !== null && (
        <RecoveredManuscriptDraft
          text={recoveredDraft}
          onUse={() => {
            editDraft(recoveredDraft);
            forgetDraft();
            setRecoveredDraft(null);
          }}
          onKeep={() => {
            forgetDraft();
            setRecoveredDraft(null);
          }}
        />
      )}
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
                  onChange={editDraft}
                  onSelection={({ text, ...position }) =>
                    setSourcePassage({ ...position, quote: text })
                  }
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
                title={
                  !buildsLoaded || busy
                    ? "Opening manuscript…"
                    : "No manuscript file open"
                }
                description={
                  !buildsLoaded || busy
                    ? "Loading your manuscript and build settings."
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
                      position={{ ...pdfPosition.current, page: pdfPage }}
                      onPosition={(position) => {
                        pdfPosition.current = position;
                        setPdfPage(position.page);
                      }}
                      capture={workspaceFileAdapter({
                        workspaceId,
                        revisionId: receipt.pdf.revision.id,
                      })}
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
            readyToBuild={
              buildPrepared &&
              !dirty &&
              !building &&
              (config.checkpointId ?? "") === checkpoint
            }
            workspaceId={workspaceId}
            busy={busy}
            run={run}
            documentId={documentId}
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
              sideClosed.current = false;
              void run(onRefresh);
            }}
          />
        </Sheet>
      </div>
    </div>
  );
}
