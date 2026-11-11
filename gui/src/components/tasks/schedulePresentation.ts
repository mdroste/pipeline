import type { Trigger } from "../../lib/taskTypes";

export const localZone = () =>
  Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
export function zoneLabel(zone: string, at = Date.now() / 1000) {
  try {
    const offset = new Intl.DateTimeFormat(undefined, {
      timeZone: zone,
      timeZoneName: "shortOffset",
    })
      .formatToParts(new Date(at * 1000))
      .find((part) => part.type === "timeZoneName")?.value;
    return `${zone.replaceAll("_", " ").replaceAll("/", " / ")} (${offset})`;
  } catch {
    return zone;
  }
}
export function scheduledTime(
  at: number | null | undefined,
  zone = localZone(),
) {
  if (at == null) return "Not scheduled";
  try {
    return new Intl.DateTimeFormat(undefined, {
      timeZone: zone,
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(at * 1000));
  } catch {
    return "Choose a valid time zone";
  }
}
export function schedulePreset(
  preset: string,
  timezone = localZone(),
): Trigger {
  const now = Math.floor(Date.now() / 1000);
  if (preset === "hourly")
    return { kind: "interval", seconds: 3600, anchor: now + 3600 };
  return {
    kind: "calendar",
    timezone,
    hour: 9,
    minute: 0,
    weekdays:
      preset === "daily"
        ? [0, 1, 2, 3, 4, 5, 6]
        : preset === "weekly"
          ? [0]
          : [0, 1, 2, 3, 4],
  };
}
export function intervalUnit(seconds: number) {
  return seconds % 86400 === 0 ? 86400 : seconds % 3600 === 0 ? 3600 : 60;
}
