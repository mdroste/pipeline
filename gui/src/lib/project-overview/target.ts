// ---- Target date ----------------------------------------------------------

export function targetSummary(
  settings: { targetDate?: string | null; targetLabel?: string | null },
  now: Date,
): { label: string; date: string; days: number } | null {
  if (!settings.targetDate) return null;
  const [year, month, dayOfMonth] = settings.targetDate.split("-").map(Number);
  const target = new Date(year, month - 1, dayOfMonth);
  if (Number.isNaN(target.getTime())) return null;
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  return {
    label: settings.targetLabel?.trim() || "Target",
    date: target.toLocaleDateString(undefined, { dateStyle: "medium" }),
    days: Math.round((target.getTime() - today.getTime()) / 86_400_000),
  };
}
