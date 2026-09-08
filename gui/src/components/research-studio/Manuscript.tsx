import SourceEditor, { type SourceEditorHandle } from "../file-workspace/SourceEditor";
import SourceDiff from "../file-workspace/SourceDiff";
import { workspaceFileAdapter } from "../../lib/fileWorkspaceClient";
const PdfComparison = lazy(() => import("../file-workspace/PdfComparison"));
const PdfReader = lazy(() => import("../file-workspace/PdfReader"));
import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
import {
  studioClient,
  proseForDiff,
  type BuildConfig,
  type BuildReceipt,
  type BuildRecord,
  type EditorFile,
} from "../../lib/studioClient";
import { lineDiff } from "../../lib/diff";
import type { ProjectRecord } from "../../lib/projectClient";
import { workbenchClient } from "../../lib/workbenchClient";
import type { ExecutionProfile } from "../../lib/workbenchTypes";
import {
  button,
  input,
  panel,
  Field,
  Select,
  Text,
  ErrorNotice,
  Inspect,
  lines,
  useAction,
  type StudioProps,
} from "./shared";
import { JobLauncher } from "./Jobs";
const initialBuild: BuildConfig = {
  name: "Manuscript build",
  checkpointId: null,
  directory: ".",
  rootDocument: "main.tex",
  expectedPdf: "main.pdf",
  engine: "pdflatex",
  timeoutSeconds: 120,
  inputs: ["main.tex"],
  advancedArgv: null,
};
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
}: StudioProps) {
  const draftKey = `pipeline.manuscriptDraft.${workspaceId}`;
  const [initialDraft] = useState(() => readDraft(draftKey));
  const cached = useRef(initialDraft);
  const [path, setPath] = useState(cached.current?.file.path ?? "main.tex");
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
  const editor = useRef<SourceEditorHandle>(null);
  const saveButton = useRef<HTMLButtonElement>(null);
  const [beside, setBeside] = useState(false);
  const [completionTexts, setCompletionTexts] = useState<string[]>([]);
  const [builds, setBuilds] = useState<
    ProjectRecord<BuildRecord | BuildReceipt>[]
  >([]);
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
  const [sourceLine, setSourceLine] = useState(1);
  const [pdfPage, setPdfPage] = useState(1);
  const [pdfX, setPdfX] = useState(0);
  const [pdfY, setPdfY] = useState(0);
  const { error, busy, run, setError } = useAction();
  const refresh = async () =>
    setBuilds(await studioClient.records(workspaceId, "build"));
  useEffect(() => {
    let stopped = false;
    void Promise.all([
      studioClient.records<BuildRecord | BuildReceipt>(workspaceId, "build"),
      workbenchClient.listExecutionProfiles(workspaceId),
    ])
      .then(([v, p]) => {
        if (!stopped) {
          setBuilds(v);
          setProfiles(p);
        }
      })
      .catch((e) => {
        if (!stopped) setError(String(e));
      });
    return () => {
      stopped = true;
    };
  }, [workspaceId, data.executions, setError]);
  const configs = builds.filter(
    (r): r is ProjectRecord<BuildRecord> =>
      r.body.recordType === "configuration",
  );
  const receipts = builds.filter(
    (r): r is ProjectRecord<BuildReceipt> => r.body.recordType === "receipt",
  );
  const chosen = configs.find((r) => r.id === configId);
  const receipt = receipts.find((r) => r.id === selectedBuild)?.body;
  const comparison = receipts.find((r) => r.id === compareBuild)?.body;
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
      request,
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
        if (current.draft !== current.file.content) { localStorage.setItem(draftKey, JSON.stringify(current)); localStorage.setItem(key, JSON.stringify(current)); }
        else { localStorage.removeItem(draftKey); localStorage.removeItem(key); }
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
          localStorage.setItem(`${draftKey}.${current.file.checkpointId ?? "working"}.${current.file.path}`, JSON.stringify(current));
        } else if (current.file) {
          localStorage.removeItem(draftKey);
          localStorage.removeItem(`${draftKey}.${current.file.checkpointId ?? "working"}.${current.file.path}`);
        }
      } catch {
        /* Current save state already discloses draft-storage failure. */
      }
    },
    [draftKey],
  );
  const diff = useMemo(
    () =>
      file
        ? lineDiff(
            diffMode === "source" ? file.content : proseForDiff(file.content),
            diffMode === "source" ? draft : proseForDiff(draft),
          )
        : [],
    [file, draft, diffMode],
  );
  const load = () =>
    run(async () => {
      if (dirty)
        throw new Error(
          "Save or discard the current draft before loading another file.",
        );
      const next = await studioClient.editor(
        workspaceId,
        path,
        checkpoint || null,
      );
      const retained = readDraft(`${draftKey}.${checkpoint || "working"}.${path}`) ?? cached.current;
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
        setNotice("Loaded current file bytes");
      }
    });
  const jumpLine = (line: number) => editor.current?.goToLine(line);
  const navigateSource = async (target: string, line: number) => {
    if (file?.path === target) { setSourceLine(line); jumpLine(line); return; }
    if (dirty && file) localStorage.setItem(`${draftKey}.${file.checkpointId ?? "working"}.${file.path}`, JSON.stringify({file,draft}));
    const next = await studioClient.editor(workspaceId, target, checkpoint || null);
    const retained = readDraft(`${draftKey}.${checkpoint || "working"}.${target}`);
    setPath(target); setFile(retained?.file ?? next); setDraft(retained?.draft ?? next.content); setSourceLine(line);
    setNotice("Opened source at the build location. The working source may have changed since this build; previous unsaved drafts remain retained by file.");
  };
  useEffect(() => {
    let live = true;
    const candidates = (data.inventory?.body.files ?? []).filter(f=>/\.(bib|tex)$/i.test(f.path)).slice(0, 40);
    void Promise.all(candidates.map(f=>studioClient.editor(workspaceId,f.path,checkpoint||null).then(v=>v.content).catch(()=>""))).then(texts=>{if(live)setCompletionTexts(texts);});
    return()=>{live=false;};
  },[workspaceId,checkpoint,data.inventory]);

  return (
    <div className="space-y-5">
      <ErrorNotice error={error} />
      <div className="grid gap-4 xl:grid-cols-[minmax(0,3fr)_minmax(18rem,2fr)]">
        <section className={panel}>
          <h2 className="font-semibold">Manuscript editor</h2>
          <div className="grid gap-3 md:grid-cols-2">
            <Text
              label="Relative source path"
              value={path}
              onChange={setPath}
            />
            <Select
              label="Edit in"
              value={checkpoint}
              onChange={setCheckpoint}
              placeholder="Project working copy"
              options={data.changes
                .filter((c) => ["isolated", "review"].includes(c.body.state))
                .map((c) => ({
                  value: c.id,
                  label:
                    data.tasks.find((t) => t.id === c.body.taskId)?.body
                      .objective ?? c.id,
                }))}
            />
          </div>
          <div className="flex flex-wrap gap-2">
            <button
              className={button}
              disabled={busy}
              onClick={() => void load()}
            >
              Load / refresh file
            </button>
            <button
              className={button}
              ref={saveButton}
              disabled={!file || !dirty || busy}
              onClick={() =>
                void run(async () => {
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
                    if(file)localStorage.removeItem(`${draftKey}.${file.checkpointId ?? "working"}.${file.path}`);
                  } catch {
                    /* Saving still succeeded. */
                  }
                  setNotice(
                    "Saved. Rebuild to validate this file combination.",
                  );
                  await onRefresh();
                })
              }
            >
              Save {file?.checkpointId ? "task draft" : "working copy"}
            </button>
            <button
              className={button}
              disabled={!dirty}
              onClick={() => {
                cached.current = null;
                try {
                  localStorage.removeItem(draftKey);
                  if(file)localStorage.removeItem(`${draftKey}.${file.checkpointId ?? "working"}.${file.path}`);
                } catch {
                  /* Current draft is still discarded. */
                }
                setDraft(file?.content ?? "");
                setNotice("Draft discarded");
              }}
            >
              Discard unsaved draft
            </button>
            <button
              className={button}
              disabled={!file || busy}
              onClick={() =>
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
            </button>
          </div>
          <p role="status" className="text-xs text-gray-500">
            {dirty
              ? "Unsaved changes"
              : notice || "Load a TeX, Markdown, BibTeX or text file."}
          </p>
          {file && (
            <>
              <div className={beside ? "grid h-[38rem] grid-cols-2 gap-3" : "h-96"}>
                <SourceEditor ref={editor} label="Manuscript source editor" readOnly={busy} path={`${workspaceId}/${checkpoint || "working"}/${file.path}`} value={draft} onChange={setDraft} line={sourceLine} onSave={()=>saveButton.current?.click()} completionTexts={completionTexts}/>
                {beside && receipt?.pdf?.revision && <Suspense fallback={<p>Loading build PDF…</p>}><PdfReader documentKey={`${workspaceId}:${receipt.pdf.revision.id}`} title="Exact build PDF" initialPage={pdfPage} load={async()=>{const preview=await workspaceFileAdapter({workspaceId,revisionId:receipt.pdf!.revision!.id}).read("");if(!preview.base64)throw new Error("Build PDF unavailable");return preview.base64;}} onSource={point=>void run(async()=>{const mapped=await studioClient.sync(workspaceId,receipt.executionId,point);setSyncCandidates(mapped.candidates);const first=mapped.candidates.find(c=>c.sourcePath&&c.line);if(first)await navigateSource(first.sourcePath!,first.line!);})}/></Suspense>}
              </div>
              {receipt?.pdf?.revision && <button className={button} onClick={()=>setBeside(!beside)}>{beside ? "Close PDF beside source" : "Show build PDF beside source"}</button>}
              <details>
                <summary className="cursor-pointer text-sm">
                  Review unsaved changes
                </summary>
                <div className="my-2 flex gap-2">
                  {["source", "prose"].map((m) => (
                    <button
                      className={button}
                      key={m}
                      aria-pressed={diffMode === m}
                      onClick={() => setDiffMode(m)}
                    >
                      {m === "source" ? "Source diff" : "Prose aid"}
                    </button>
                  ))}
                </div>
                {diffMode === "prose" && (
                  <p className="text-xs text-gray-500">
                    Simplified prose is an aid. Inspect source for equations,
                    labels and citations.
                  </p>
                )}
                {diffMode === "source" ? <SourceDiff before={file.content} after={draft} path={file.path}/> : <pre className="max-h-72 overflow-auto whitespace-pre-wrap text-xs">{diff.map((line,i)=><div key={i}>{line.type === "add" ? "+ " : line.type === "del" ? "− " : "  "}{line.text}</div>)}</pre>}
              </details>
            </>
          )}
        </section>
        <section className={panel}>
          <h2 className="font-semibold">Build settings</h2>
          <Select
            label="Saved configuration"
            value={configId}
            onChange={(id) => {
              setConfigId(id);
              const found = configs.find((c) => c.id === id);
              setConfig(found?.body.config ?? initialBuild);
              setAdvanced(
                found?.body.config.advancedArgv
                  ? JSON.stringify(found.body.config.advancedArgv, null, 2)
                  : "",
              );
            }}
            placeholder="New build configuration"
            options={configs.map((c) => ({
              value: c.id,
              label: c.body.config.name,
            }))}
          />
          <Text
            label="Build name"
            value={config.name}
            onChange={(name) => setConfig((c) => ({ ...c, name }))}
          />
          <Select
            label="Build in"
            value={config.checkpointId ?? ""}
            onChange={(checkpointId) =>
              setConfig((c) => ({ ...c, checkpointId: checkpointId || null }))
            }
            placeholder="Project working copy"
            options={data.changes
              .filter((c) => ["isolated", "review"].includes(c.body.state))
              .map((c) => ({
                value: c.id,
                label:
                  data.tasks.find((t) => t.id === c.body.taskId)?.body
                    .objective ?? c.id,
              }))}
          />
          <div className="grid grid-cols-2 gap-3">
            <Text
              label="Working directory (relative)"
              value={config.directory}
              onChange={(directory) => setConfig((c) => ({ ...c, directory }))}
            />
            <Select
              label="Engine"
              value={config.engine}
              onChange={(engine) => setConfig((c) => ({ ...c, engine }))}
              options={["pdflatex", "xelatex", "lualatex", "latexmk"].map(
                (v) => ({ value: v, label: v }),
              )}
            />
            <Text
              label="Root TeX document"
              value={config.rootDocument}
              onChange={(rootDocument) =>
                setConfig((c) => ({ ...c, rootDocument }))
              }
            />
            <Text
              label="Expected PDF"
              value={config.expectedPdf}
              onChange={(expectedPdf) =>
                setConfig((c) => ({ ...c, expectedPdf }))
              }
            />
          </div>
          <Text
            label="Declared inputs (one per line, relative to build directory)"
            multiline
            value={config.inputs.join("\n")}
            onChange={(s) =>
              setConfig((c) => ({ ...c, inputs: s.split("\n") }))
            }
          />
          <p className="text-xs text-gray-500">
            Include sections, bibliography, generated tables and figures.
            Dependency coverage is limited to these declarations.
          </p>
          <Field label="Timeout (seconds)">
            <input
              className={input}
              type="number"
              min={1}
              max={7200}
              value={config.timeoutSeconds}
              onChange={(e) =>
                setConfig((c) => ({
                  ...c,
                  timeoutSeconds: Number(e.target.value),
                }))
              }
            />
          </Field>
          <details>
            <summary className="cursor-pointer text-xs">
              Advanced engine arguments
            </summary>
            <Text
              label="Explicit argv JSON (optional)"
              multiline
              value={advanced}
              onChange={setAdvanced}
            />
          </details>
          <button
            className={button}
            disabled={busy}
            onClick={() =>
              void run(async () => {
                const saved = await studioClient.mutate<
                  ProjectRecord<BuildRecord>
                >(workspaceId, {
                  action: "saveBuild",
                  id: configId || null,
                  expectedRevision: chosen?.revision ?? 0,
                  config: {
                    ...config,
                    inputs: lines(config.inputs.join("\n")),
                    advancedArgv: advanced.trim() ? JSON.parse(advanced) : null,
                  },
                });
                setConfigId(saved.id);
                await refresh();
              })
            }
          >
            Save build settings
          </button>
          {chosen && (
            <JobLauncher
              profile={
                profiles.find(
                  (p) =>
                    p.id === chosen.body.profile.id &&
                    p.revision === chosen.body.profile.revision,
                ) ?? chosen.body.profile
              }
              onQueued={() =>
                setNotice("Build queued. Open Local jobs for streamed logs.")
              }
            />
          )}
        </section>
      </div>
      <section className={panel}>
        <h2 className="font-semibold">Build history and PDF comparison</h2>
        <div className="flex flex-wrap gap-2">
          {data.executions
            .filter(
              (e) =>
                e.adapter === "latex" &&
                !["queued", "running"].includes(e.outcome),
            )
            .slice(0, 12)
            .map((e) => (
              <button
                key={e.id}
                className={button}
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await studioClient.mutate(workspaceId, {
                      action: "inspectBuild",
                      executionId: e.id,
                    });
                    await refresh();
                    setSelectedBuild(`build_${e.id}`);
                    await onRefresh();
                  })
                }
              >
                Inspect {e.outcome} · {e.id.slice(-8)}
              </button>
            ))}
        </div>
        <div className="grid gap-3 md:grid-cols-2">
          <Select
            label="Build"
            value={selectedBuild}
            onChange={setSelectedBuild}
            options={receipts.map((r) => ({
              value: r.id,
              label: `${r.body.outcome} · ${r.body.executionId}`,
            }))}
          />
          <Select
            label="Compare PDF with"
            value={compareBuild}
            onChange={setCompareBuild}
            options={receipts
              .filter((r) => r.id !== selectedBuild && r.body.pdf?.revision)
              .map((r) => ({ value: r.id, label: r.body.executionId }))}
          />
        </div>
        {receipt && (
          <>
            <p className="text-xs">
              Compilation: {receipt.outcome}. Page inspection is a separate
              researcher check.
            </p>
            {receipt.diagnostics.map((d, i) => (
              <div key={i} className="text-xs">
                <strong>{d.severity}: </strong>
                {d.path && d.line ? (
                  <button
                    className="text-blue-600 underline"
                    onClick={() => {
                      void run(()=>navigateSource(d.path!,d.line!));
                    }}
                  >
                    {d.path}:{d.line}
                  </button>
                ) : null}{" "}
                {d.message}
              </div>
            ))}
            {receipt.pdf?.revision ? (
              <>
                <div className="flex gap-2">
                  <button
                    className={button}
                    onClick={() => onDocument(receipt.pdf!.revision!.id)}
                  >
                    Open exact build PDF in Documents
                  </button>
                  <Field label="Source line">
                    <input
                      className={input}
                      type="number"
                      min={1}
                      value={sourceLine}
                      onChange={(e) => setSourceLine(Number(e.target.value))}
                    />
                  </Field>
                  <button
                    className={button}
                    onClick={() => jumpLine(sourceLine)}
                  >
                    Go to line
                  </button>
                  <button
                    className={button}
                    disabled={busy || !receipt.mappingArtifactId}
                    onClick={() =>
                      void run(async () =>
                        showSync({
                          sourcePath: file?.path ?? path,
                          line: sourceLine,
                        }),
                      )
                    }
                  >
                    Find line in PDF
                  </button>
                </div>
                <details>
                  <summary className="text-xs">
                    Find source from a PDF coordinate
                  </summary>
                  <div className="flex gap-2">
                    {(
                      [
                        ["Page", pdfPage, setPdfPage],
                        ["X (PDF points)", pdfX, setPdfX],
                        ["Y (PDF points)", pdfY, setPdfY],
                      ] as const
                    ).map(([label, value, set]) => (
                      <Field key={label} label={label}>
                        <input
                          className={input}
                          type="number"
                          value={value}
                          onChange={(e) => set(Number(e.target.value))}
                        />
                      </Field>
                    ))}
                    <button
                      className={button}
                      disabled={busy || !receipt.mappingArtifactId}
                      onClick={() =>
                        void run(async () =>
                          showSync({ page: pdfPage, x: pdfX, y: pdfY }),
                        )
                      }
                    >
                      Find source
                    </button>
                  </div>
                </details>
                {syncCandidates.map((c, i) => (
                  <div className="flex gap-2 text-xs" key={i}>
                    {c.page && (
                      <button
                        className={button}
                        onClick={() => setPdfPage(c.page!)}
                      >
                        Open build page {c.page}
                      </button>
                    )}
                    {c.sourcePath && c.line && (
                      <button
                        className={button}
                        onClick={() => {
                          setPath(c.sourcePath!);
                          setSourceLine(c.line!);
                          if (file?.path === c.sourcePath) jumpLine(c.line!);
                          setNotice(
                            `Exact build maps to ${c.sourcePath}:${c.line}. Working source may have changed; reload only after preserving your draft.`,
                          );
                        }}
                      >
                        Source {c.sourcePath}:{c.line}
                      </button>
                    )}
                  </div>
                ))}
                <button
                  className={button}
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      await studioClient.mutate(workspaceId, {
                        action: "inspectPages",
                        executionId: receipt.executionId,
                        pages: [pdfPage],
                        note: "Researcher inspected the displayed compiled page",
                      });
                      await refresh();
                    })
                  }
                >
                  Record inspection of build page {pdfPage}
                </button>
                {receipt.inspectedPages?.length ? (
                  <p className="text-xs">
                    Pages marked as inspected:{" "}
                    {receipt.inspectedPages.join(", ")}
                  </p>
                ) : null}
                {sync && (
                  <pre className="max-h-40 overflow-auto whitespace-pre-wrap text-xs">
                    {sync}
                  </pre>
                )}
                <div className="h-[42rem]"><Suspense fallback={<p>Loading PDF comparison…</p>}><PdfComparison workspaceId={workspaceId} primary={{id:receipt.pdf.revision.id,title:"Selected build"}} secondary={comparison?.pdf?.revision?{id:comparison.pdf.revision.id,title:"Comparison build"}:undefined} page={pdfPage} onSource={point=>void run(async()=>{const mapped=await studioClient.sync(workspaceId,receipt.executionId,point);setSyncCandidates(mapped.candidates);const first=mapped.candidates.find(c=>c.sourcePath&&c.line);if(first)await navigateSource(first.sourcePath!,first.line!);})}/></Suspense></div>
              </>
            ) : (
              <p className="text-sm">
                This build did not produce a PDF. Select an earlier successful build to view its PDF.
              </p>
            )}
            <Inspect
              value={receipt.sourceManifest}
              label="Captured build dependencies"
            />
          </>
        )}
      </section>
    </div>
  );
}
