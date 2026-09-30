/** Status dot with the app-wide attention vocabulary:
 * running = accent (blue in light, gray in dark), needs-you = amber,
 * ok = emerald, error = red. */
export default function Badge({
  kind,
  label,
}: {
  kind: "running" | "attention" | "ok" | "error" | "idle";
  label: string;
}) {
  const color =
    kind === "running"
      ? "bg-blue-500"
      : kind === "attention"
        ? "bg-amber-500"
        : kind === "ok"
          ? "bg-emerald-500"
          : kind === "error"
            ? "bg-red-500"
            : "bg-gray-400";
  return (
    <span
      aria-label={label}
      className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full ${color}`}
    />
  );
}
