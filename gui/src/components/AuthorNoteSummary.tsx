/** Plain text only: the note is author input, not generated report markup. */
export default function AuthorNoteSummary({
  note,
  batchCount = 1,
  onEdit,
}: {
  note?: string | null;
  batchCount?: number;
  onEdit?: () => void;
}) {
  if (!note) return null;
  return (
    <section
      aria-label="Additional instructions"
      className="rounded-xl border border-gray-200 bg-gray-50/70 p-4 dark:border-gray-800 dark:bg-gray-800/30"
    >
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-medium text-gray-900 dark:text-gray-100">
          Additional instructions
        </h2>
        {onEdit && (
          <button
            type="button"
            onClick={onEdit}
            className="rounded text-xs text-gray-600 underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 dark:text-gray-300"
          >
            Edit
          </button>
        )}
      </div>
      <p className="mt-3 whitespace-pre-wrap break-words text-sm leading-6 text-gray-800 dark:text-gray-200">
        {note}
      </p>
      <p className="mt-3 text-xs leading-5 text-gray-500 dark:text-gray-400">
        {batchCount > 1
          ? `Shared with every review agent for all ${batchCount} documents.`
          : "Included at the start of every review agent’s context."}
      </p>
    </section>
  );
}
