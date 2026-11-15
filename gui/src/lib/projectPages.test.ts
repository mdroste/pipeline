import { it, expect, vi } from "vitest";
import { allProjectPages } from "./projectPages";

it("reads the page after the highest ID even when the last row is older", async () => {
  const first = Array.from({ length: 500 }, (_, i) => ({
    id: String(500 - i).padStart(3, "0"),
    updatedAt: "2026-01-01",
  }));
  const extra = { id: "501", updatedAt: "2026-02-01" };
  const read = vi
    .fn()
    .mockResolvedValueOnce(first)
    .mockResolvedValueOnce([extra]);
  const all = await allProjectPages(read);
  expect(read.mock.calls).toEqual([[undefined], ["500"]]);
  expect(all).toHaveLength(501);
  expect(all[0]).toEqual(extra);
});
