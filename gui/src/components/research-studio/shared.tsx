import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { confirmDialog } from "../DialogService";
import type { ProjectHome } from "../../lib/projectClient";
import type { ReviewHandoff } from "../../lib/workbenchTypes";
import { button, input } from "../../ui/classes";
import "../project-surface/ProjectPage.css";
export { button, input };
export const panel = "project-section";
export interface StudioProps {
  workspaceId: string;
  data: ProjectHome;
  onRefresh: () => Promise<void>;
  /** Open a document, at a byte range when the caller has one. */
  onDocument: (
    revision: string,
    passage?: { start: number; end: number },
  ) => void;
  onReviewHandoff?: (handoff: ReviewHandoff) => void;
  /** False while this retained view is hidden. */
  active?: boolean;
  /** Open one saved selection in the reader and optionally pin a second beside it. */
  onAnchors?: (
    primaryAnchorId: string,
    secondaryAnchorId: string | null,
  ) => void;
}
export function Field({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <label className="block space-y-1 text-xs text-gray-600 dark:text-gray-400">
      <span>{label}</span>
      {children}
    </label>
  );
}
export function Text({
  label,
  value,
  onChange,
  multiline = false,
}: {
  label: string;
  value: string;
  onChange: (s: string) => void;
  multiline?: boolean;
}) {
  return (
    <Field label={label}>
      {multiline ? (
        <textarea
          className={input}
          rows={3}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      ) : (
        <input
          className={input}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </Field>
  );
}
export function Select({
  label,
  value,
  onChange,
  options,
  placeholder = "Select…",
}: {
  label: string;
  value: string;
  onChange: (s: string) => void;
  options: { value: string; label: string }[];
  placeholder?: string;
}) {
  return (
    <Field label={label}>
      <select
        className={input}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      >
        <option value="">{placeholder}</option>
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </Field>
  );
}
export function useAction() {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const gate = useRef(false);
  const run = useCallback(async (work: () => Promise<unknown>) => {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    setError(null);
    try {
      await work();
    } catch (e) {
      setError(workbenchErrorMessage(e));
    } finally {
      gate.current = false;
      setBusy(false);
    }
  }, []);
  return { error, busy, run, setError };
}
/**
 * Remembers what a form was loaded from, so choosing another record can ask
 * before it replaces text the researcher has not saved.
 */
export function useDraftGuard(draft: unknown) {
  const saved = useRef(JSON.stringify(draft));
  const latest = useRef(draft);
  latest.current = draft;
  const markSaved = useCallback((value: unknown) => {
    saved.current = JSON.stringify(value);
  }, []);
  const replace = useCallback((what: string, load: () => void) => {
    if (JSON.stringify(latest.current) === saved.current) {
      load();
      return;
    }
    void confirmDialog(
      `Your changes to this ${what} haven't been saved and will be lost.`,
      {
        title: "Discard unsaved changes?",
        confirmLabel: "Discard changes",
        cancelLabel: "Keep editing",
        destructive: true,
      },
    ).then((confirmed) => {
      if (confirmed) load();
    });
  }, []);
  return { markSaved, replace };
}
export function ErrorNotice({ error }: { error: string | null }) {
  const alert = useRef<HTMLParagraphElement>(null);
  // These views scroll, and the button that failed can be far below this notice.
  useEffect(() => {
    if (error) alert.current?.scrollIntoView?.({ block: "nearest" });
  }, [error]);
  return error ? (
    <p
      ref={alert}
      role="alert"
      className="rounded border border-red-300 bg-red-50 p-3 text-sm text-red-900 dark:bg-red-950/40 dark:text-red-200"
    >
      {error}
    </p>
  ) : null;
}
export const lines = (s: string) =>
  s
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
export async function exportText(text: string, filename: string) {
  const path = await save({ defaultPath: filename });
  if (path)
    await invoke("workbench_studio_export_file", { path, content: text });
}
export function Inspect({
  value,
  label = "Inspect details",
}: {
  value: unknown;
  label?: string;
}) {
  return (
    <details className="text-xs">
      <summary className="cursor-pointer">{label}</summary>
      <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap break-words">
        {JSON.stringify(value, null, 2)}
      </pre>
    </details>
  );
}
