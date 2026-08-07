import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import BatchPanel from "./BatchPanel";

const invoke = vi.hoisted(() => vi.fn());
const open = vi.hoisted(() => vi.fn());
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open }));
vi.mock("@tauri-apps/api/event", () => ({
  listen,
}));

function setup(inputMode = "document") {
  return {
    profileId: "review",
    profileConfigSnapshotId: "config-exact",
    profileSnapshotId: "runtime-exact",
    inputMode,
    variables: [{ key: "tone", label: "Tone", kind: "text", default: "neutral" }],
    inputSlots: [{ key: "rubric", label: "Rubric", mode: "document", required: true }],
    readiness: { deps: [], ready: true },
    stages: [],
  };
}

describe("BatchPanel profile semantics", () => {
  beforeEach(() => {
    invoke.mockReset();
    open.mockReset();
    listen.mockReset();
    listen.mockResolvedValue(() => {});
  });

  it("collects and passes the full active profile options", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_watch_status") {
        return Promise.resolve({ active: false, folder: "", processed: [], processed_total: 0, failed_total: 0 });
      }
      if (command === "get_execution_plan") return Promise.resolve(setup());
      if (command === "start_batch") return Promise.resolve();
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    open
      .mockResolvedValueOnce("/inputs/rubric.pdf")
      .mockResolvedValueOnce(["/papers/paper.pdf"]);
    const user = userEvent.setup();
    render(<BatchPanel onClose={vi.fn()} onOpenRun={vi.fn()} />);

    const tone = await screen.findByRole("textbox", { name: "Tone" });
    await user.clear(tone);
    await user.type(tone, "strict");
    await user.click(screen.getByRole("button", { name: "Choose…" }));
    await user.click(screen.getByRole("button", { name: "Add files…" }));
    await user.click(screen.getByRole("button", { name: "Start batch (1)" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_batch", {
        paths: ["/papers/paper.pdf"],
        variables: { tone: "strict" },
        extraInputs: { rubric: "/inputs/rubric.pdf" },
        expectedProfileConfigSnapshotId: "config-exact",
      });
    });
  });

  it("surfaces a file-picker plugin failure without suggesting a setup retry", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_watch_status") {
        return Promise.resolve({ active: false, folder: "", processed: [], processed_total: 0, failed_total: 0 });
      }
      if (command === "get_execution_plan") return Promise.resolve(setup());
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    open.mockRejectedValueOnce(new Error("dialog plugin unavailable"));
    const user = userEvent.setup();
    render(<BatchPanel onClose={vi.fn()} onOpenRun={vi.fn()} />);

    await screen.findByRole("textbox", { name: "Tone" });
    await user.click(screen.getByRole("button", { name: "Add files…" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not open the file picker: dialog plugin unavailable",
    );
    expect(screen.queryByRole("button", { name: "Retry setup" })).not.toBeInTheDocument();
  });

  it("identifies the field when a named-input picker fails", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_watch_status") {
        return Promise.resolve({ active: false, folder: "", processed: [], processed_total: 0, failed_total: 0 });
      }
      if (command === "get_execution_plan") return Promise.resolve(setup());
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    open.mockRejectedValueOnce(new Error("dialog permission denied"));
    const user = userEvent.setup();
    render(<BatchPanel onClose={vi.fn()} onOpenRun={vi.fn()} />);

    await screen.findByRole("textbox", { name: "Tone" });
    await user.click(screen.getByRole("button", { name: "Choose…" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not open the file picker for “Rubric”: dialog permission denied",
    );
  });

  it("preflights a PDF-capable representative when a mixed batch starts with DOCX", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_watch_status") {
        return Promise.resolve({ active: false, folder: "", processed: [], processed_total: 0, failed_total: 0 });
      }
      if (command === "get_execution_plan") return Promise.resolve(setup());
      if (command === "start_batch") return Promise.resolve();
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    open
      .mockResolvedValueOnce("/inputs/rubric.pdf")
      .mockResolvedValueOnce(["/papers/first.docx", "/papers/second.pdf"]);
    const user = userEvent.setup();
    render(<BatchPanel onClose={vi.fn()} onOpenRun={vi.fn()} />);

    await screen.findByRole("textbox", { name: "Tone" });
    await user.click(screen.getByRole("button", { name: "Choose…" }));
    await user.click(screen.getByRole("button", { name: "Add files…" }));
    await user.click(screen.getByRole("button", { name: "Start batch (2)" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_execution_plan", {
      variables: { tone: "neutral" },
      extraInputs: { rubric: "/inputs/rubric.pdf" },
      expectedProfileConfigSnapshotId: "config-exact",
      diff: false,
      paperPath: "/papers/second.pdf",
    }));
  });

  it.each(["folder", "none"])(
    "explicitly gates batch and watch for %s workflows",
    async (inputMode) => {
      invoke.mockImplementation((command: string) => {
        if (command === "get_batch_status") return Promise.resolve([]);
        if (command === "get_watch_status") {
          return Promise.resolve({ active: false, folder: "", processed: [], processed_total: 0, failed_total: 0 });
        }
        if (command === "get_execution_plan") return Promise.resolve(setup(inputMode));
        return Promise.reject(new Error(`unexpected command: ${command}`));
      });
      render(<BatchPanel onClose={vi.fn()} onOpenRun={vi.fn()} />);

      expect(
        await screen.findByText("Batch and watch require a document-input workflow."),
      ).toBeVisible();
      expect(screen.queryByRole("button", { name: "Add files…" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Watch folder…" })).not.toBeInTheDocument();
    },
  );

  it("shows the exact captured profile and snapshot for batch and watch", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_batch_status") {
        return Promise.resolve([{
          path: "/papers/a.pdf",
          name: "a.pdf",
          status: "done",
          run_id: "run-a",
          error: null,
          duration_secs: 1,
          profile_id: "review",
          profile_snapshot_id: "batch-snapshot-exact",
        }]);
      }
      if (command === "get_watch_status") {
        return Promise.resolve({
          active: true,
          folder: "/watch",
          profile_id: "review",
          profile_snapshot_id: "watch-snapshot-exact",
          processed: [],
          processed_total: 0,
          failed_total: 0,
        });
      }
      if (command === "get_execution_plan") return Promise.resolve(setup());
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    render(<BatchPanel onClose={vi.fn()} onOpenRun={vi.fn()} />);

    expect(await screen.findByText("batch-snapshot-exact")).toBeVisible();
    expect(screen.getByText("watch-snapshot-exact")).toBeVisible();
    expect(screen.getAllByText(/variables: Tone/).length).toBeGreaterThan(0);
    expect(screen.queryByDisplayValue("neutral")).not.toBeInTheDocument();
  });

  it("cleans successful listeners and keeps mutations disabled after partial registration failure", async () => {
    const cleanups = [vi.fn(), vi.fn(), vi.fn()];
    let registration = 0;
    listen.mockImplementation(() => {
      const index = registration++;
      return index === 1
        ? Promise.reject(new Error("watch listener denied"))
        : Promise.resolve(cleanups[index]);
    });
    render(<BatchPanel onClose={vi.fn()} onOpenRun={vi.fn()} />);

    expect(await screen.findByRole("alert")).toHaveTextContent("watch listener denied");
    expect(cleanups[0]).toHaveBeenCalledTimes(1);
    expect(cleanups[2]).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Start batch (0)" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Watch folder…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Retry setup" })).toBeEnabled();
  });

  it("rejects a stale profile schema immediately before starting", async () => {
    let plans = 0;
    invoke.mockImplementation((command: string) => {
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_watch_status") {
        return Promise.resolve({ active: false, folder: "", processed: [], processed_total: 0, failed_total: 0 });
      }
      if (command === "get_execution_plan") {
        plans += 1;
        return plans === 1
          ? Promise.resolve(setup())
          : Promise.reject(new Error("profile config snapshot changed"));
      }
      if (command === "start_batch") return Promise.resolve();
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    open
      .mockResolvedValueOnce("/inputs/rubric.pdf")
      .mockResolvedValueOnce(["/papers/paper.pdf"]);
    const user = userEvent.setup();
    render(<BatchPanel onClose={vi.fn()} onOpenRun={vi.fn()} />);

    await screen.findByRole("textbox", { name: "Tone" });
    await user.click(screen.getByRole("button", { name: "Choose…" }));
    await user.click(screen.getByRole("button", { name: "Add files…" }));
    await user.click(screen.getByRole("button", { name: "Start batch (1)" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("profile config snapshot changed");
    expect(screen.getByRole("alert")).toHaveTextContent("review its options");
    expect(invoke).not.toHaveBeenCalledWith("start_batch", expect.anything());
  });

  it("shows when watching is paused or stopped by a terminal folder error", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_watch_status") {
        return Promise.resolve({
          active: true,
          paused: true,
          error: "Folder is no longer readable",
          folder: "/watch",
          processed: [],
          processed_total: 0,
          failed_total: 0,
        });
      }
      if (command === "get_execution_plan") return Promise.resolve(setup());
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    render(<BatchPanel onClose={vi.fn()} onOpenRun={vi.fn()} />);

    expect(await screen.findByText("Paused while another run is active.")).toBeVisible();
    expect(screen.getByText(/Watch stopped: Folder is no longer readable/)).toBeVisible();
  });

  it("uses the authoritative stopped status instead of waiting for a later event", async () => {
    const activeWatch = {
      active: true,
      paused: false,
      error: null,
      folder: "/watch",
      profile_id: "review",
      profile_snapshot_id: "watch-snapshot",
      processed: [],
      processed_total: 0,
      failed_total: 0,
    };
    const stoppedWatch = {
      ...activeWatch,
      active: false,
      folder: "",
      profile_id: "",
      profile_snapshot_id: "",
    };
    invoke.mockImplementation((command: string) => {
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_watch_status") return Promise.resolve(activeWatch);
      if (command === "get_execution_plan") return Promise.resolve(setup());
      if (command === "stop_watch") return Promise.resolve(stoppedWatch);
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<BatchPanel onClose={vi.fn()} onOpenRun={vi.fn()} />);

    await user.click(await screen.findByRole("button", { name: "Stop watching" }));

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Watch folder…" })).toBeEnabled(),
    );
    expect(invoke).toHaveBeenCalledWith("stop_watch");
  });
});
