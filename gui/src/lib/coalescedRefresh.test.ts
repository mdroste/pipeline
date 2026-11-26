import { expect, it, vi } from "vitest";
import { coalescedRefresh } from "./coalescedRefresh";
it("coalesces a burst into one follow-up and publishes only the fresh result", async () => {
  let finish!: (value: number) => void;
  const read = vi
    .fn()
    .mockImplementationOnce(
      () =>
        new Promise<number>((resolve) => {
          finish = resolve;
        }),
    )
    .mockResolvedValue(2);
  const publish = vi.fn();
  const queue = coalescedRefresh(read, publish);
  const first = queue.refresh();
  await Promise.all([queue.refresh(), queue.refresh(), queue.refresh()]);
  expect(read).toHaveBeenCalledTimes(1);
  finish(1);
  await first;
  expect(read).toHaveBeenCalledTimes(2);
  expect(publish.mock.calls).toEqual([[2]]);
});
it("retries failures and drops work after disposal", async () => {
  const read = vi
    .fn()
    .mockRejectedValueOnce(new Error("disk"))
    .mockResolvedValue(2);
  const publish = vi.fn();
  const queue = coalescedRefresh(read, publish);
  await queue.refresh();
  expect(publish).not.toHaveBeenCalled();
  await queue.refresh();
  expect(publish).toHaveBeenCalledWith(2);
  queue.dispose();
  await queue.refresh();
  expect(read).toHaveBeenCalledTimes(2);
});
it("does not publish or start queued reads after unmount", async () => {
  let finish!: (value: number) => void;
  const read = vi.fn(
    () =>
      new Promise<number>((resolve) => {
        finish = resolve;
      }),
  );
  const publish = vi.fn();
  const queue = coalescedRefresh(read, publish);
  const first = queue.refresh();
  void queue.refresh();
  queue.dispose();
  finish(1);
  await first;
  expect(read).toHaveBeenCalledOnce();
  expect(publish).not.toHaveBeenCalled();
});
