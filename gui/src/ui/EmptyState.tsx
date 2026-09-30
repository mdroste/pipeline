import type { ReactNode } from "react";

/** The one empty state: a short statement of what belongs here and, when the
 * user can act, exactly one way to start. Voice: "No X yet. <Action>." */
export default function EmptyState({
  title,
  description,
  action,
  compact = false,
  icon,
}: {
  title: string;
  description?: string;
  /** A single starting action (a Button or link). */
  action?: ReactNode;
  /** Inline list version for sidebars and small panes. */
  compact?: boolean;
  icon?: ReactNode;
}) {
  if (compact) {
    return (
      <p className="px-1 py-1 text-xs leading-5 text-gray-400 dark:text-gray-500">
        {title}
        {description ? ` ${description}` : ""}
      </p>
    );
  }
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-6 py-10 text-center">
      {icon && (
        <span aria-hidden="true" className="text-gray-300 dark:text-gray-600">
          {icon}
        </span>
      )}
      <p className="text-sm font-medium text-gray-700 dark:text-gray-300">
        {title}
      </p>
      {description && (
        <p className="max-w-sm text-xs leading-5 text-gray-500 dark:text-gray-400">
          {description}
        </p>
      )}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}
