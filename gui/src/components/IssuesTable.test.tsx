import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import IssuesTable from "./IssuesTable";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));

const issues = [
  { id: "i1", title: "First issue", severity: "high", section: "1", body: "Details" },
];

describe("IssuesTable annotation lifecycle", () => {
  beforeEach(() => invoke.mockReset());

  it("ignores a stale annotation load after switching runs", async () => {
    const resolvers = new Map<string, (value: string) => void>();
    invoke.mockImplementation((command: string, args: { runId: string }) => {
      if (command !== "get_annotations") return Promise.resolve();
      return new Promise<string>((resolve) => resolvers.set(args.runId, resolve));
    });

    const view = render(<IssuesTable issues={issues} runId="run-a" />);
    view.rerender(<IssuesTable issues={issues} runId="run-b" />);
    await act(async () => resolvers.get("run-b")?.(JSON.stringify({ i1: { status: "", note: "new" } })));
    await userEvent.setup().click(screen.getByRole("button", { name: /First issue/i }));
    expect(screen.getByPlaceholderText("Add a note…")).toHaveValue("new");

    await act(async () => resolvers.get("run-a")?.(JSON.stringify({ i1: { status: "", note: "stale" } })));
    expect(screen.getByPlaceholderText("Add a note…")).toHaveValue("new");
  });

  it("flushes a dirty annotation when the component unmounts", async () => {
    invoke.mockImplementation((command: string) =>
      command === "get_annotations" ? Promise.resolve("{}") : Promise.resolve(),
    );
    const view = render(<IssuesTable issues={issues} runId="run-a" />);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_annotations", { runId: "run-a" }));
    await userEvent.setup().click(screen.getByTitle("accept"));
    view.unmount();
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "save_annotations",
        expect.objectContaining({ runId: "run-a" }),
      ),
    );
  });
});
