import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import type { FileHunk } from "../../lib/fileDiff";
import SourceDiff from "./SourceDiff";

const jobs = vi.hoisted(
  () =>
    [] as {
      finish: (hunks: FileHunk[]) => void;
      cancel: ReturnType<typeof vi.fn>;
    }[],
);
vi.mock("../../lib/fileDiffClient", () => ({
  startFileDiff: () => {
    let finish!: (hunks: FileHunk[]) => void;
    const result = new Promise<FileHunk[]>((resolve) => {
      finish = resolve;
    });
    const cancel = vi.fn();
    jobs.push({ finish, cancel });
    return { result, cancel };
  },
}));
beforeEach(() => {
  jobs.length = 0;
});

it("pages a large whole-file preview while accepting its complete text", async () => {
  const before = "a".repeat(13_000);
  const after = "b".repeat(11_999) + "😀" + "tail";
  const accept = vi.fn(async () => {});
  const view = render(
    <SourceDiff before={before} after={after} path="x.txt" onAccept={accept} />,
  );
  await act(async () =>
    jobs[0].finish([
      { id: 0, from: 0, to: before.length, before, after, line: 1 },
    ]),
  );
  expect(view.container.querySelectorAll("code")[1].textContent).toBe(
    "b".repeat(11_999) + "😀",
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Line 1 after: next preview section" }),
  );
  expect(view.container.querySelectorAll("code")[1].textContent).toBe("😀tail");
  fireEvent.click(
    screen.getByRole("button", { name: "Accept 1 selected change" }),
  );
  await screen.findByText("Accepted 1 change.");
  expect(accept).toHaveBeenCalledWith(after);
});

it("disables acceptance while comparing and ignores a replaced result", async () => {
  const accept = vi.fn(async () => {});
  const view = render(
    <SourceDiff before="a" after="b" path="x.txt" onAccept={accept} />,
  );
  expect(
    screen.getByRole("button", { name: /Accept 0 selected/ }),
  ).toBeDisabled();
  view.rerender(
    <SourceDiff before="a" after="c" path="x.txt" onAccept={accept} />,
  );
  expect(jobs[0].cancel).toHaveBeenCalledOnce();
  await act(async () =>
    jobs[0].finish([
      { id: 0, from: 0, to: 1, before: "a", after: "b", line: 1 },
    ]),
  );
  expect(screen.getByText("Comparing…")).toBeVisible();
  await act(async () =>
    jobs[1].finish([
      { id: 0, from: 0, to: 1, before: "a", after: "c", line: 1 },
    ]),
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Accept 1 selected change" }),
  );
  await screen.findByText("Accepted 1 change.");
  expect(accept).toHaveBeenCalledWith("c");
  view.unmount();
  expect(jobs[1].cancel).toHaveBeenCalledOnce();
});

it("bounds rendered hunks while preserving choices on other pages", async () => {
  const accept = vi.fn(async () => {});
  render(
    <SourceDiff
      before={"a".repeat(85)}
      after={"b".repeat(85)}
      path="x.txt"
      onAccept={accept}
    />,
  );
  await act(async () =>
    jobs[0].finish(
      Array.from({ length: 85 }, (_, id) => ({
        id,
        from: id,
        to: id + 1,
        before: "a",
        after: "b",
        line: id + 1,
      })),
    ),
  );
  expect(screen.getAllByRole("checkbox")).toHaveLength(40);
  fireEvent.click(screen.getByLabelText("Accept change at line 1"));
  fireEvent.click(screen.getByRole("button", { name: "Next changes" }));
  expect(screen.getAllByRole("checkbox")).toHaveLength(40);
  fireEvent.click(screen.getByLabelText("Accept change at line 41"));
  fireEvent.click(screen.getByRole("button", { name: "Next changes" }));
  expect(screen.getAllByRole("checkbox")).toHaveLength(5);
  fireEvent.click(
    screen.getByRole("button", { name: "Accept 83 selected changes" }),
  );
  await screen.findByText("Accepted 83 changes.");
  expect(accept).toHaveBeenCalledWith(
    "a" + "b".repeat(39) + "a" + "b".repeat(44),
  );
});
