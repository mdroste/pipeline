// Access settings as an inheritance table: one row per value, one column per
// scope. The winning cell is highlighted; "Inherit" removes an override rather
// than copying the parent's value.

import { memo } from "react";
import type { EffectiveHarness } from "../../lib/workbenchTypes";
import {
  ACCESS_ROWS,
  ACCESS_SCOPES,
  type AccessKey,
  type AccessRow,
  type AccessScope,
  effectiveAccessValue,
  formatAccessValue,
  scopeValue,
  winningScope,
} from "./harnessHelpers";

interface Props {
  effective: EffectiveHarness;
  globalBody: Record<string, unknown>;
  /** Null when the conversation is unfiled. */
  workspaceBody: Record<string, unknown> | null;
  workspaceName: string | null;
  conversationOverrides: Record<string, unknown>;
  busy: boolean;
  successor: boolean;
  onSet: (scope: Exclude<AccessScope, "builtIn">, key: AccessKey, value: string | number | boolean | undefined) => void;
}

const controlClass = "rounded border border-gray-300 bg-white px-1.5 py-0.5 text-xs dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200";

function Cell({ row, scope, value, winning, editable, unavailable, busy, effectiveValue, onSet }: {
  row: AccessRow;
  scope: AccessScope;
  value: unknown;
  winning: boolean;
  editable: boolean;
  unavailable: string | null;
  busy: boolean;
  effectiveValue: string | number | boolean;
  onSet: Props["onSet"];
}) {
  const scopeLabel = ACCESS_SCOPES.find((item) => item.id === scope)!.label;
  const body = (() => {
    if (scope === "builtIn") return <span className="text-gray-600 dark:text-gray-300">{formatAccessValue(row, value)}</span>;
    if (unavailable) return <span className="text-gray-400" title={unavailable}>—</span>;
    if (!editable) return <span className="text-gray-400">{value === undefined ? "inherits" : formatAccessValue(row, value)}</span>;
    if (value === undefined) {
      return (
        <button
          type="button"
          disabled={busy}
          aria-label={`Override ${row.label} for ${scopeLabel}`}
          onClick={() => onSet(scope, row.key, effectiveValue)}
          className="rounded border border-dashed border-gray-300 px-2 py-0.5 text-[11px] text-gray-500 hover:border-gray-400 hover:text-gray-700 dark:border-gray-600 dark:text-gray-400"
        >
          inherits · set
        </button>
      );
    }
    const control = row.kind === "boolean" ? (
      <label className="inline-flex items-center gap-1 text-xs">
        <input
          type="checkbox"
          aria-label={`${row.label} (${scopeLabel})`}
          checked={value === true}
          disabled={busy}
          onChange={(event) => onSet(scope, row.key, event.target.checked)}
        />
        {formatAccessValue(row, value)}
      </label>
    ) : (
      <select
        aria-label={`${row.label} (${scopeLabel})`}
        value={String(value)}
        disabled={busy}
        onChange={(event) => {
          const option = row.options?.find((candidate) => String(candidate.value) === event.target.value);
          if (option) onSet(scope, row.key, option.value);
        }}
        className={controlClass}
      >
        {row.options?.map((option) => <option key={String(option.value)} value={String(option.value)}>{option.label}</option>)}
      </select>
    );
    return (
      <span className="inline-flex items-center gap-1">
        {control}
        <button
          type="button"
          disabled={busy}
          aria-label={`Inherit ${row.label} for ${scopeLabel}`}
          title="Remove this override and inherit from the scope to the left"
          onClick={() => onSet(scope, row.key, undefined)}
          className="rounded px-1 text-xs text-gray-400 hover:bg-gray-100 hover:text-gray-700 dark:hover:bg-gray-800"
        >
          ×
        </button>
      </span>
    );
  })();
  return (
    <td
      data-winning={winning || undefined}
      className={`px-2 py-2 align-top ${winning ? "rounded bg-emerald-50 ring-1 ring-inset ring-emerald-300 dark:bg-emerald-950/30 dark:ring-emerald-700" : ""}`}
    >
      {winning && <div className="mb-0.5 text-[9px] font-semibold uppercase tracking-wide text-emerald-700 dark:text-emerald-300">In effect</div>}
      {body}
    </td>
  );
}

function HarnessAccessTable({ effective, globalBody, workspaceBody, workspaceName, conversationOverrides, busy, successor, onSet }: Props) {
  const bodies = { global: globalBody, workspace: workspaceBody, conversation: conversationOverrides };
  return (
    <div className="space-y-3">
      <p className="text-xs text-gray-500 dark:text-gray-400">
        Each value comes from the rightmost scope that sets it. Global applies to every conversation, Workspace to conversations filed in {workspaceName ? <strong>{workspaceName}</strong> : "a workspace"}, and This conversation only here. Inherit removes an override instead of copying today's parent value.
      </p>
      <div className="overflow-x-auto rounded-xl border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
        <table className="w-full min-w-[40rem] text-left text-xs">
          <thead>
            <tr className="border-b border-gray-200 text-[10px] font-semibold uppercase tracking-wide text-gray-500 dark:border-gray-700 dark:text-gray-400">
              <th scope="col" className="px-3 py-2">Setting</th>
              {ACCESS_SCOPES.map((scope) => <th key={scope.id} scope="col" className="px-2 py-2">{scope.label}</th>)}
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-100 dark:divide-gray-800">
            {ACCESS_ROWS.map((row) => {
              const winner = row.locked ? "builtIn" : winningScope(effective, row.key);
              const effectiveValue = effectiveAccessValue(effective, row.key);
              return (
                <tr key={row.key}>
                  <th scope="row" className="px-3 py-2 align-top font-normal">
                    <div className="text-sm font-medium text-gray-800 dark:text-gray-200">{row.label}</div>
                    <div className="text-[11px] text-gray-500 dark:text-gray-400">{row.description}</div>
                    {row.locked && <div className="mt-1 text-[11px] text-amber-700 dark:text-amber-300">{row.locked}</div>}
                  </th>
                  {ACCESS_SCOPES.map((scope) => (
                    <Cell
                      key={scope.id}
                      row={row}
                      scope={scope.id}
                      value={scopeValue(row.key, scope.id, bodies)}
                      winning={winner === scope.id}
                      editable={!row.locked && scope.id !== "builtIn"}
                      unavailable={scope.id === "workspace" && workspaceBody === null ? "Workspace defaults apply only to conversations filed in a project. This conversation is unfiled; use Move to project… in the conversation list to file it." : null}
                      busy={busy}
                      effectiveValue={effectiveValue}
                      onSet={onSet}
                    />
                  ))}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <p className="text-[11px] text-gray-500 dark:text-gray-400">
        Access changes apply to your next message. Native permission profile in effect: <span className="font-mono">{effective.permissionProfile}</span>.
        {successor && " Because this setup differs from the active native thread, the next message starts a successor thread with a deterministic handoff from accepted Workspace records."}
      </p>
    </div>
  );
}

export default memo(HarnessAccessTable);
