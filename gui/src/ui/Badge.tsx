/** Status dot with the app-wide attention vocabulary:
 * running = accent (blue in light, gray in dark), needs-you = amber,
 * ok = emerald, error = red. */
export type StatusTone = "running" | "attention" | "ok" | "error" | "idle";
const tones: Record<StatusTone, string> = {
  running: "ui-status--running",
  attention: "ui-status--attention",
  ok: "ui-status--ok",
  error: "ui-status--error",
  idle: "ui-status--idle",
};

export default function Badge({
  kind,
  label,
  showLabel = false,
  className = "",
}: {
  kind: StatusTone;
  label: string;
  /** Runtime owners supply the label and tone; this component owns no state. */
  showLabel?: boolean;
  className?: string;
}) {
  if (showLabel)
    return (
      <span className={`ui-status ${tones[kind]} ${className}`}>
        <i aria-hidden="true" className="ui-status-dot" />
        {label}
      </span>
    );
  return (
    <span
      aria-label={label}
      className={`ui-status-dot ${tones[kind]} ${className}`}
    />
  );
}
