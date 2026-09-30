/** The one loading indicator. Always labeled with what is loading. */
export default function Spinner({
  label,
  inline = false,
}: {
  /** Action-specific ("Loading report…"), never a bare spinner. */
  label: string;
  /** Inline chip next to content instead of a centered block. */
  inline?: boolean;
}) {
  const circle = (
    <span
      aria-hidden="true"
      className="inline-block h-4 w-4 animate-spin rounded-full border-2 border-gray-300 border-t-gray-600 dark:border-gray-700 dark:border-t-gray-300"
    />
  );
  if (inline) {
    return (
      <span
        role="status"
        className="inline-flex items-center gap-2 text-xs text-gray-500 dark:text-gray-400"
      >
        {circle}
        {label}
      </span>
    );
  }
  return (
    <div
      role="status"
      className="flex h-full min-h-24 flex-col items-center justify-center gap-3 text-gray-400"
    >
      <span
        aria-hidden="true"
        className="h-6 w-6 animate-spin rounded-full border-2 border-gray-300 border-t-gray-600 dark:border-gray-700 dark:border-t-gray-300"
      />
      <span className="text-xs">{label}</span>
    </div>
  );
}
