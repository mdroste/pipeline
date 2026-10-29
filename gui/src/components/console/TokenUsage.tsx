import type { TokenTotals } from "../../lib/reviewRuntimeTypes";
import {
  freshInputTokens,
  usageDescription,
  fmtTokens,
} from "../../lib/consolePresentation";

/** Logical input split into fresh, cache-read, and cache-write tokens, with a
 *  proportional bar. Cache traffic is a subset of input, never additional. */
export default function TokenUsage({
  usage,
  label,
}: {
  usage: TokenTotals;
  label: string;
}) {
  const fresh = freshInputTokens(usage);
  const parts = [
    {
      name: "fresh",
      value: fresh,
      bar: "bg-gray-400 dark:bg-gray-500",
      text: "text-gray-700 dark:text-gray-300",
    },
    {
      name: "cache read",
      value: usage.cached,
      bar: "bg-emerald-500 dark:bg-emerald-500",
      text: "text-emerald-700 dark:text-emerald-400",
    },
    {
      name: "cache write",
      value: usage.cacheWrite,
      bar: "bg-blue-500 dark:bg-blue-400",
      text: "text-blue-700 dark:text-blue-400",
    },
  ].filter((part) => part.value > 0);
  const description = usageDescription(usage);
  return (
    <div
      className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 tabular-nums"
      aria-label={description}
      title={description}
      tabIndex={0}
    >
      <span className="text-gray-500 dark:text-gray-400">{label}</span>
      <span className="text-gray-800 dark:text-gray-200">
        <span className="font-medium">{fmtTokens(usage.input)}</span> input
      </span>
      {usage.input > 0 && (
        <span
          aria-hidden
          className="flex h-1.5 w-20 overflow-hidden rounded-full bg-gray-200 dark:bg-gray-800"
        >
          {parts.map((part) => (
            <span
              key={part.name}
              className={part.bar}
              style={{ width: `${(part.value / usage.input) * 100}%` }}
            />
          ))}
        </span>
      )}
      {parts.map((part) => (
        <span key={part.name} className={part.text}>
          {fmtTokens(part.value)} {part.name}
        </span>
      ))}
      <span className="text-gray-800 dark:text-gray-200">
        → <span className="font-medium">{fmtTokens(usage.output)}</span> output
      </span>
    </div>
  );
}
