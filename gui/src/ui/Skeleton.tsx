const WIDTHS = ["92%", "78%", "86%", "64%"];

/** A placeholder for content that is loading, labeled with what is loading. */
export default function Skeleton({
  label,
  lines = 3,
  className = "",
}: {
  label: string;
  lines?: number;
  className?: string;
}) {
  return (
    <div role="status" aria-label={label} className={`space-y-2 ${className}`}>
      {Array.from({ length: lines }, (_, index) => (
        <div
          key={index}
          aria-hidden="true"
          style={{ width: WIDTHS[index % WIDTHS.length] }}
          className="h-3 animate-ui-shimmer rounded-ui-sm bg-[linear-gradient(90deg,var(--ui-sunken)_25%,var(--ui-border)_45%,var(--ui-sunken)_65%)] bg-[length:200%_100%]"
        />
      ))}
    </div>
  );
}
