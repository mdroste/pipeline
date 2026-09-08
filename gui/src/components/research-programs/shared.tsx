import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { save } from "@tauri-apps/plugin-dialog";
import {
  deskClient,
  operation,
  reference,
  type DeskRecord,
  type OpenResearchObject,
} from "../../lib/deskClient";
import {
  programClient,
  type Artifact,
  type Choices,
  type ObjectChoice,
  type ProgramAction,
} from "../../lib/programClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import {
  button,
  input,
  card,
  muted,
  type DeskProps,
} from "../research-desk/shared";
export { button, input, card, muted, operation, reference };
export type { DeskProps };
export const lines = (s: string) =>
  s
    .split("\n")
    .map((v) => v.trim())
    .filter(Boolean);
export const exactRecord = (r: {
  id: string;
  revision: number;
}): OpenResearchObject => ({
  kind: "record",
  id: r.id,
  revision: String(r.revision),
});
export const key = (r: OpenResearchObject) =>
  JSON.stringify([r.kind, r.id, r.revision, r.start ?? null, r.end ?? null]);
export function useProgram(
  { workspaceId, onError }: DeskProps,
  kinds: string[],
) {
  const [choices, setChoices] = useState<Choices | null>(null),
    [records, setRecords] = useState<Record<string, DeskRecord<unknown>[]>>({}),
    [busy, setBusy] = useState(false);
  const gate = useRef(false);
  const kindKey = kinds.join(",");
  const refresh = useCallback(async () => {
    const [c, ...lists] = await Promise.all([
      programClient.call<Choices>(workspaceId, { action: "choices" }),
      ...kindKey
        .split(",")
        .filter(Boolean)
        .map((k) => deskClient.records<unknown>(workspaceId, k)),
    ]);
    setChoices(c as Choices);
    setRecords(
      Object.fromEntries(
        kindKey
          .split(",")
          .filter(Boolean)
          .map((k, i) => [k, lists[i]]),
      ),
    );
  }, [workspaceId, kindKey]);
  useEffect(() => {
    let alive = true;
    void refresh().catch((e) => {
      if (alive) onError(workbenchErrorMessage(e));
    });
    return () => {
      alive = false;
    };
  }, [refresh, onError]);
  const run = async (fn: () => Promise<unknown>) => {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    try {
      await fn();
      await refresh();
    } catch (e) {
      onError(workbenchErrorMessage(e));
    } finally {
      gate.current = false;
      setBusy(false);
    }
  };
  const call = <T,>(a: ProgramAction) => programClient.call<T>(workspaceId, a);
  return { choices, records, busy, run, call, refresh };
}
export function Field({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <label className="block space-y-1 text-xs">
      <span>{label}</span>
      {children}
    </label>
  );
}
export function Inspect({
  value,
  label = "Inspect retained record",
}: {
  value: unknown;
  label?: string;
}) {
  return (
    <details className="text-xs">
      <summary>{label}</summary>
      <pre className="max-h-80 overflow-auto whitespace-pre-wrap break-words pt-2">
        {JSON.stringify(value, null, 2)}
      </pre>
    </details>
  );
}
export function ObjectSelect({
  choices,
  value,
  onChange,
  label = "Source object",
  filter,
}: {
  choices: ObjectChoice[];
  value: OpenResearchObject | null;
  onChange: (v: OpenResearchObject | null) => void;
  label?: string;
  filter?: (o: ObjectChoice) => boolean;
}) {
  const shown = choices.filter(filter ?? (() => true));
  return (
    <Field label={label}>
      <select
        className={input}
        aria-label={label}
        value={value ? key(value) : ""}
        onChange={(e) =>
          onChange(
            shown.find((o) => key(o.reference) === e.target.value)?.reference ??
              null,
          )
        }
      >
        <option value="">Choose retained evidence</option>
        {shown.map((o) => (
          <option key={key(o.reference)} value={key(o.reference)}>
            {o.title} · {o.reference.kind} · {o.reference.revision.slice(0, 8)}
          </option>
        ))}
      </select>
    </Field>
  );
}
export function Sources({
  sources,
  choices,
  onChange,
  onOpen,
}: {
  sources: OpenResearchObject[];
  choices: ObjectChoice[];
  onChange: (r: OpenResearchObject[]) => void;
  onOpen: DeskProps["onOpen"];
}) {
  return (
    <div className="space-y-2">
      <ObjectSelect
        choices={choices}
        value={null}
        label="Add exact source"
        onChange={(r) => {
          if (r && !sources.some((s) => key(s) === key(r)))
            onChange([...sources, r]);
        }}
      />
      {sources.map((s) => (
        <div key={key(s)} className="flex items-center gap-2 text-xs">
          <button
            className="flex-1 text-left underline"
            onClick={() => onOpen(s)}
          >
            {choices.find((c) => key(c.reference) === key(s))?.title ?? s.kind}{" "}
            · {s.revision.slice(0, 8)}
          </button>
          <button
            className={button}
            onClick={() => onChange(sources.filter((r) => key(r) !== key(s)))}
          >
            Remove
          </button>
        </div>
      ))}
    </div>
  );
}
export function Artifacts({
  workspaceId,
  recordId,
  artifacts,
  choices,
  onError,
}: {
  workspaceId: string;
  recordId: string;
  artifacts: Artifact[];
  choices: Choices | null;
  onError: DeskProps["onError"];
}) {
  const [preview, setPreview] = useState<{
      artifact: Artifact;
      base64: string;
    } | null>(null),
    [checkpoint, setCheckpoint] = useState(""),
    [path, setPath] = useState(""),
    [expected, setExpected] = useState(""),
    [selected, setSelected] = useState(""),
    [busy, setBusy] = useState(false),
    [notice, setNotice] = useState("");
  const run = async (fn: () => Promise<unknown>) => {
    if (busy) return;
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      onError(workbenchErrorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap gap-2">
        {artifacts.map((a) => (
          <div key={a.id} className="flex gap-1">
            <button
              disabled={busy}
              className={button}
              onClick={() =>
                void run(async () => {
                  setSelected(a.id);
                  setPreview({
                    artifact: a,
                    ...(await programClient.call<{ base64: string }>(
                      workspaceId,
                      { action: "artifact", id: a.id },
                    )),
                  });
                })
              }
            >
              Preview {a.extension.toUpperCase()}
            </button>
            <button
              disabled={busy}
              className={button}
              onClick={() =>
                void run(async () => {
                  const path = await save({
                    defaultPath: `research-asset.${a.extension}`,
                  });
                  if (path)
                    await programClient.call(workspaceId, {
                      action: "exportArtifact",
                      id: a.id,
                      path,
                    });
                })
              }
            >
              Export
            </button>
          </div>
        ))}
      </div>
      {preview &&
        (["png", "svg"].includes(preview.artifact.extension) ? (
          <img
            className="max-h-[36rem] max-w-full bg-white"
            alt="Generated scientific figure from retained numeric sources"
            src={`data:image/${preview.artifact.extension === "svg" ? "svg+xml" : "png"};base64,${preview.base64}`}
          />
        ) : ["md", "tex", "csv", "json"].includes(
            preview.artifact.extension,
          ) ? (
          <pre className="max-h-80 overflow-auto whitespace-pre-wrap text-xs">
            {new TextDecoder().decode(
              Uint8Array.from(atob(preview.base64), (c) => c.charCodeAt(0)),
            )}
          </pre>
        ) : (
          <p className={muted}>Export the PDF to open it in your PDF reader.</p>
        ))}
      <details className="space-y-2 text-xs">
        <summary>Stage selected artifact for comparison and acceptance</summary>
        <select
          className={input}
          aria-label="Artifact to stage"
          value={selected}
          onChange={(e) => setSelected(e.target.value)}
        >
          <option value="">Choose artifact</option>
          {artifacts.map((a) => (
            <option key={a.id} value={a.id}>
              {a.extension.toUpperCase()}
            </option>
          ))}
        </select>
        <select
          className={input}
          aria-label="Publication task copy"
          value={checkpoint}
          onChange={(e) => setCheckpoint(e.target.value)}
        >
          <option value="">Choose an open task copy</option>
          {choices?.checkpoints
            .filter((c) => ["isolated", "review"].includes(c.body.state))
            .map((c) => (
              <option key={c.id} value={c.id}>
                {c.body.taskId} · {c.body.state}
              </option>
            ))}
        </select>
        <Field label="Relative destination file">
          <input
            className={input}
            value={path}
            onChange={(e) => setPath(e.target.value)}
            placeholder="figures/result.pdf"
          />
        </Field>
        <Field label="Current file hash (blank for a new file)">
          <input
            className={input}
            value={expected}
            onChange={(e) => setExpected(e.target.value)}
          />
        </Field>
        <button
          className={button}
          disabled={busy || !selected || !checkpoint || !path}
          onClick={() =>
            void run(async () => {
              await programClient.call(workspaceId, {
                action: "stage",
                recordId,
                artifactId: selected,
                checkpointId: checkpoint,
                path,
                expectedHash: expected || null,
              });
              setNotice(
                "Staged in the task copy. Open Edits & acceptance to capture, compare, check, and accept the change.",
              );
            })
          }
        >
          Stage artifact
        </button>
      </details>
      {notice && (
        <p role="status" className={muted}>
          {notice}
        </p>
      )}
    </div>
  );
}
