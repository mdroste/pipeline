import { useId, useState } from "react";
import useModalDialog from "../../hooks/useModalDialog";

export default function AgentProfileDialog({
  title,
  defaultName,
  workspaceId,
  workspaceName,
  busy,
  error,
  onSubmit,
  onCancel,
}: {
  title: string;
  defaultName: string;
  workspaceId: string | null;
  workspaceName: string;
  busy: boolean;
  error: string | null;
  onSubmit: (name: string, workspaceId: string | null) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(defaultName);
  const [scope, setScope] = useState(workspaceId ?? "global");
  const titleId = useId();
  const ref = useModalDialog<HTMLDivElement>(() => {
    if (!busy) onCancel();
  });
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className="w-96 max-w-[90vw] rounded-xl bg-white p-5 text-gray-900 shadow-xl dark:bg-gray-800 dark:text-gray-100"
      >
        <h2 id={titleId} className="text-sm font-semibold">
          {title}
        </h2>
        {error && (
          <p
            role="alert"
            className="mt-3 text-xs text-red-700 dark:text-red-300"
          >
            {error}
          </p>
        )}
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!busy && name.trim())
              onSubmit(name.trim(), scope === "global" ? null : scope);
          }}
        >
          <fieldset disabled={busy}>
            <label className="mt-4 block text-xs">
              Profile name
              <input
                autoFocus
                data-autofocus
                value={name}
                onChange={(event) => setName(event.target.value)}
                maxLength={300}
                className="mt-1 w-full rounded border border-gray-300 bg-transparent p-2 dark:border-gray-600"
              />
            </label>
            <label className="mt-3 block text-xs">
              Available in
              <select
                value={scope}
                onChange={(event) => setScope(event.target.value)}
                className="mt-1 w-full rounded border border-gray-300 bg-white p-2 dark:border-gray-600 dark:bg-gray-800"
              >
                {workspaceId && (
                  <option value={workspaceId}>{workspaceName}</option>
                )}
                <option value="global">All Workspaces</option>
              </select>
            </label>
            <p className="mt-3 text-xs text-gray-500">
              The new profile will be selected for this conversation. Edit its
              prompt and tools in the next screen.
            </p>
            <div className="mt-4 flex justify-end gap-2 text-xs">
              <button
                type="button"
                onClick={onCancel}
                className="rounded px-3 py-2"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={!name.trim()}
                className="rounded bg-gray-900 px-3 py-2 text-white disabled:opacity-50 dark:bg-gray-100 dark:text-gray-900"
              >
                Create profile
              </button>
            </div>
          </fieldset>
        </form>
      </div>
    </div>
  );
}
