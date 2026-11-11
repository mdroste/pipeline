import { expect, it } from "vitest";
import {
  intervalUnit,
  scheduledTime,
  schedulePreset,
  zoneLabel,
} from "./schedulePresentation";

it("shows the selected zone across daylight-saving changes", () => {
  for (const instant of ["2026-10-30T13:00:00Z", "2026-11-02T14:00:00Z"]) {
    const at = Date.parse(instant) / 1000;
    const shown = scheduledTime(at, "America/New_York");
    expect(shown).toBe(
      new Intl.DateTimeFormat(undefined, {
        timeZone: "America/New_York",
        dateStyle: "medium",
        timeStyle: "short",
      }).format(new Date(at * 1000)),
    );
    expect(shown).not.toBe(scheduledTime(at, "UTC"));
  }
  expect(zoneLabel("America/New_York")).toContain("America / New York");
  expect(
    zoneLabel("America/New_York", Date.parse("2026-10-30T13:00:00Z") / 1000),
  ).toContain("GMT-4");
  expect(
    zoneLabel("America/New_York", Date.parse("2026-11-02T14:00:00Z") / 1000),
  ).toContain("GMT-5");
});
it("keeps calendar presets in their selected zone and chooses exact interval units", () => {
  expect(schedulePreset("weekly", "Asia/Tokyo")).toMatchObject({
    kind: "calendar",
    timezone: "Asia/Tokyo",
    weekdays: [0],
    hour: 9,
  });
  expect(intervalUnit(86400)).toBe(86400);
  expect(intervalUnit(7200)).toBe(3600);
  expect(intervalUnit(5400)).toBe(60);
});
