import type { TrashedRun } from "../../lib/reviewTypes";
import { fmtDate } from "./presentation";

interface Props {
  runs: TrashedRun[];
  loading: boolean;
  refresh: () => Promise<void>;
  restore: (run: TrashedRun) => Promise<void>;
  remove: (run: TrashedRun) => Promise<void>;
}

export default function ReviewHistoryTrash({
  runs,
  loading,
  refresh,
  restore,
  remove,
}: Props) {
  return (
    <section
      id="report-trash"
      aria-label="Report Trash"
      className="px-[var(--ui-page-gutter)] py-3 border-b border-line bg-sunken shrink-0"
    >
      <div className="flex flex-wrap items-center gap-3 mb-2">
        <h3 className="text-sm font-medium text-gray-900 dark:text-gray-100">
          Report Trash
        </h3>
        <span className="text-xs text-gray-500 dark:text-gray-400">
          Restore reports or delete them permanently.
        </span>
        <button
          onClick={refresh}
          className="ml-auto text-xs text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100"
        >
          Refresh Trash
        </button>
      </div>
      {loading ? (
        <p className="text-xs text-gray-500 dark:text-gray-400">
          Loading Trash…
        </p>
      ) : runs.length === 0 ? (
        <p className="text-xs text-gray-500 dark:text-gray-400">
          Trash is empty.
        </p>
      ) : (
        <ul className="space-y-1 max-h-40 overflow-auto">
          {runs.map((run) => (
            <li
              key={run.run_id}
              className="flex items-center gap-3 rounded border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 px-3 py-2"
            >
              <div className="min-w-0 flex-1">
                <p className="text-sm text-gray-900 dark:text-gray-100 truncate">
                  {run.title || run.input_name || run.run_id}
                </p>
                <p className="text-xs text-gray-500 dark:text-gray-400">
                  Moved {fmtDate(run.deleted_at)}
                </p>
              </div>
              <button
                onClick={() => restore(run)}
                className="px-2 py-1 text-xs rounded border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
              >
                Restore
              </button>
              <button
                onClick={() => remove(run)}
                className="px-2 py-1 text-xs rounded text-red-600 hover:text-red-800 dark:text-red-400 dark:hover:text-red-300"
              >
                Delete forever
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
