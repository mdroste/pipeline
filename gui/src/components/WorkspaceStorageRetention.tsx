import { useRef, useState } from "react";
import { workbenchClient } from "../lib/workbenchClient";
import type { StorageReport, PrunePlan } from "../lib/workbenchTypes";
const button =
  "rounded border px-3 py-1.5 text-xs hover:bg-gray-50 disabled:opacity-40 dark:border-neutral-700 dark:hover:bg-neutral-800";
const mib = (bytes: number) =>
  `${(bytes / 1048576).toFixed(bytes < 1048576 ? 2 : 1)} MiB`;
export default function WorkspaceStorageRetention({
  busy,
  onAction,
}: {
  busy: boolean;
  onAction: (action: () => Promise<void>) => void;
}) {
  const [storage, setStorage] = useState<StorageReport | null>(null);
  const [pruneKeys, setPruneKeys] = useState<string[]>([]);
  const [prunePlan, setPrunePlan] = useState<PrunePlan | null>(null);
  const pruneGeneration = useRef(0);
  const [notice, setNotice] = useState("");
  const toggle = (
    list: string[],
    set: (v: string[]) => void,
    key: string,
    on: boolean,
  ) => set(on ? [...list, key] : list.filter((k) => k !== key));
  return (
    <div className="min-w-0 overflow-x-auto">
      {notice && (
        <p role="status" className="mb-3 text-sm">
          {notice}
        </p>
      )}
      <section>
        <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
          Storage and retention
        </h3>
        <button
          type="button"
          className={`${button} mt-2`}
          disabled={busy}
          onClick={() =>
            onAction(async () => {
              setStorage(await workbenchClient.storageReport());
              setPrunePlan(null);
              pruneGeneration.current += 1;
            })
          }
        >
          Inspect disk use
        </button>
        {storage && (
          <div className="mt-2 space-y-2 text-xs">
            <p className="text-gray-600 dark:text-gray-400">{storage.note}</p>
            <table className="w-full text-left">
              <thead>
                <tr>
                  <th>Category</th>
                  <th>Size</th>
                  <th>Entries</th>
                  <th>Prune</th>
                </tr>
              </thead>
              <tbody>
                {storage.categories.map((c) => (
                  <tr
                    key={c.key}
                    className="border-t align-top dark:border-neutral-800"
                  >
                    <td className="py-1 pr-2">
                      {c.label}
                      <p className="text-gray-500">{c.note}</p>
                    </td>
                    <td className="py-1 pr-2">{mib(c.bytes)}</td>
                    <td className="py-1 pr-2">{c.entries}</td>
                    <td className="py-1">
                      {c.disposable ? (
                        <input
                          type="checkbox"
                          aria-label={`Prune ${c.label}`}
                          checked={pruneKeys.includes(c.key)}
                          onChange={(e) => {
                            pruneGeneration.current += 1;
                            setPrunePlan(null);
                            toggle(
                              pruneKeys,
                              setPruneKeys,
                              c.key,
                              e.target.checked,
                            );
                          }}
                        />
                      ) : (
                        <span className="text-gray-500">retained</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                className={button}
                disabled={busy || !pruneKeys.length}
                onClick={() =>
                  onAction(async () => {
                    const generation = ++pruneGeneration.current;
                    setPrunePlan(null);
                    const plan = await workbenchClient.pruneStorage(
                      pruneKeys,
                      false,
                    );
                    if (generation === pruneGeneration.current)
                      setPrunePlan(plan);
                  })
                }
              >
                Preview deletion
              </button>
              <button
                type="button"
                className={button}
                disabled={busy || !prunePlan || prunePlan.applied}
                onClick={() =>
                  onAction(async () => {
                    const plan = await workbenchClient.pruneStorage(
                      pruneKeys,
                      true,
                      prunePlan?.previewToken,
                    );
                    setPrunePlan(plan);
                    setStorage(await workbenchClient.storageReport());
                    setNotice(
                      `Moved ${plan.entries.length} item(s), ${mib(plan.bytes)}, to trash. Restore or empty trash below.`,
                    );
                  })
                }
              >
                Move to trash
              </button>
              <button
                type="button"
                className={button}
                disabled={busy || !storage.trash.length}
                onClick={() =>
                  onAction(async () => {
                    if (
                      !window.confirm(
                        `Permanently delete ${storage.trash.length} trashed item(s)? This cannot be undone.`,
                      )
                    )
                      return;
                    await workbenchClient.emptyTrash();
                    setStorage(await workbenchClient.storageReport());
                  })
                }
              >
                Empty trash
              </button>
            </div>
            {prunePlan && !prunePlan.applied && (
              <p>
                {prunePlan.entries.length} item(s), {mib(prunePlan.bytes)},
                would move to trash. Nothing has been moved yet.
              </p>
            )}
            {storage.trash.map((t) => (
              <div
                key={t.id}
                className="flex items-center gap-2 border-t pt-1 dark:border-neutral-800"
              >
                <span className="min-w-0 flex-1 truncate">
                  {t.category} · {mib(t.sizeBytes)} · {t.originalPath}
                </span>
                <button
                  type="button"
                  className={button}
                  disabled={busy}
                  onClick={() =>
                    onAction(async () => {
                      await workbenchClient.restoreTrash(t.id);
                      setStorage(await workbenchClient.storageReport());
                    })
                  }
                >
                  Restore
                </button>
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
