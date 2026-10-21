/// <reference types="node" />
import { execFileSync } from "node:child_process";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { expect, it } from "vitest";
import { localDateTime, parseLocalDateTime } from "./localDateTime";

it.each(["America/Los_Angeles", "Europe/Berlin", "Asia/Tokyo"])(
  "round-trips target wall times across seasons in %s",
  (timezone) => {
    const result = execFileSync(
      process.execPath,
      [
        "--input-type=module",
        "-e",
        `
    import { localDateTime, parseLocalDateTime } from ${JSON.stringify(pathToFileURL(resolve("src/lib/localDateTime.ts")).href)};
    import assert from 'node:assert/strict';
    for (const [today, target] of [['2026-07-21T09:00', '2026-11-21T09:00'], ['2026-01-21T09:00', '2026-07-21T09:00']]) {
      Date.now = () => new Date(today).getTime();
      assert.equal(localDateTime(new Date(target)), target);
      const edited = target.slice(0, -2) + '01';
      assert.equal(parseLocalDateTime(edited), new Date(edited).getTime() / 1000);
    }
    if (process.env.TZ === 'America/Los_Angeles') {
      assert.equal(parseLocalDateTime('2026-03-08T02:30'), null);
      assert.equal(parseLocalDateTime('2026-11-01T01:30'), Date.parse('2026-11-01T01:30:00-07:00') / 1000);
    }
    console.log('passed');
  `,
      ],
      { env: { ...process.env, TZ: timezone }, encoding: "utf8" },
    );
    expect(result.trim()).toBe("passed");
  },
);

it("refuses empty or invalid local dates", () => {
  expect(localDateTime(new Date(NaN))).toBe("");
  expect(parseLocalDateTime("")).toBeNull();
  expect(parseLocalDateTime("not a date")).toBeNull();
});
