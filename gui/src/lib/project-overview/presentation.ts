import type { OverviewBlock, OverviewRow } from "./types";

export const plural = (count: number, one: string, many = `${one}s`) =>
  `${count} ${count === 1 ? one : many}`;
export const clip = (text: string, length = 140) => {
  const flat = text.replace(/\s+/g, " ").trim();
  return flat.length > length ? `${flat.slice(0, length - 1)}…` : flat;
};
export const time = (value: string | number | null | undefined) => {
  if (value === null || value === undefined) return Number.NaN;
  // Automation timestamps are epoch seconds; everything else is ISO text.
  return typeof value === "number" ? value * 1000 : Date.parse(value);
};
export const after = (
  value: string | number | null | undefined,
  since: number,
) => {
  const at = time(value);
  return Number.isFinite(at) && at > since;
};
const day = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
});
export function overviewDay(value: string | number | null | undefined) {
  const at = time(value);
  return Number.isFinite(at) ? day.format(new Date(at)) : "";
}
export const block = (
  id: OverviewBlock["id"],
  title: string,
  rows: OverviewRow[],
): OverviewBlock | null => (rows.length ? { id, title, rows } : null);
