import { useRef, useState } from "react";
import { releaseClient } from "../lib/workbench/releaseClient";
import { confirmDialog } from "./DialogService";
import type { StorageReport, PrunePlan } from "../lib/workbenchTypes";
const button =
  "rounded border px-3 py-1.5 text-xs hover:bg-gray-50 disabled:opacity-40 dark:border-neutral-700 dark:hover:bg-neutral-800";
const mb = (bytes: number) =>
  `${(bytes / 1_000_000).toFixed(bytes < 1_000_000 ? 2 : 1)} MB`;
const plural = (count: number, noun: string) =>
  `${count} ${noun}${count === 1 ? "" : "s"}`;
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
          Disk use
        </h3>
        <button
          type="button"
          className={`${button} mt-2`}
          disabled={busy}
          onClick={() =>
            onAction(async () => {
              setStorage(await releaseClient.storageReport());
              setPrunePlan(null);
              pruneGeneration.current += 1;
            })
          }
        >
          Show disk use
        </button>
        {storage && (
          <div className="mt-2 space-y-2 text-xs">
            <p className="text-gray-600 dark:text-gray-400">{storage.note}</p>
            <table className="w-full text-left">
              <thead>
                <tr>
                  <th>Category</th>
                  <th>Size</th>
                  <th>Items</th>
                  <th>Remove</th>
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
                    <td className="py-1 pr-2">
                      {c.partial ? "at least " : ""}
                      {mb(c.bytes)}
                    </td>
                    <td className="py-1 pr-2">
                      {c.partial ? "at least " : ""}
                      {c.entries}
                    </td>
                    <td className="py-1">
                      {c.disposable ? (
                        <input
                          type="checkbox"
                          aria-label={`Remove ${c.label}`}
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
                        <span className="text-gray-500">Kept</span>
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
                    const plan = await releaseClient.pruneStorage(
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
                    const plan = await releaseClient.pruneStorage(
                      pruneKeys,
                      true,
                      prunePlan?.previewToken,
                    );
                    setPrunePlan(plan);
                    setStorage(await releaseClient.storageReport());
                    setNotice(
                      `Moved ${plural(plan.entries.length, "item")} (${mb(plan.bytes)}) to Trash. Restore or empty Trash below.`,
                    );
                  })
                }
              >
                Move to Trash
              </button>
              <button
                type="button"
                className={button}
                disabled={busy || !storage.trash.length}
                onClick={() =>
                  onAction(async () => {
                    if (
                      !(await confirmDialog(
                        `Permanently delete ${plural(storage.trash.length, "item")} in Trash? This cannot be undone.`,
                        {
                          title: "Empty Trash",
                          confirmLabel: "Delete permanently",
                          destructive: true,
                        },
                      ))
                    )
                      return;
                    await releaseClient.emptyTrash();
                    setStorage(await releaseClient.storageReport());
                  })
                }
              >
                Empty Trash
              </button>
            </div>
            {prunePlan && !prunePlan.applied && (
              <p>
                Choose Move to Trash to move{" "}
                {plural(prunePlan.entries.length, "item")} (
                {mb(prunePlan.bytes)}).
              </p>
            )}
            {storage.trash.map((t) => (
              <div
                key={t.id}
                className="flex items-center gap-2 border-t pt-1 dark:border-neutral-800"
              >
                <span className="min-w-0 flex-1 truncate">
                  {storage.categories.find((c) => c.key === t.category)
                    ?.label ?? t.category}{" "}
                  · {mb(t.sizeBytes)} · {t.originalPath}
                </span>
                <button
                  type="button"
                  className={button}
                  disabled={busy}
                  onClick={() =>
                    onAction(async () => {
                      await releaseClient.restoreTrash(t.id);
                      setStorage(await releaseClient.storageReport());
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
