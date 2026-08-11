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

function folderSetup() {
  return {
    ...setup("document"),
    configuredInputMode: "folder",
    inputInterpretation: "document",
  };
}

describe("BatchPanel profile semantics", () => {
  beforeEach(() => {
    invoke.mockReset();
    open.mockReset();
    listen.mockReset();
    listen.mockResolvedValue(() => {});
  });

  it("shows a preloaded active workflow while status synchronization continues", () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_batch_status") {
        return new Promise(() => {});
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });

    render(
      <BatchPanel
        onClose={vi.fn()}
        onOpenRun={vi.fn()}
        preloadedSetup={setup()}
      />,
    );

    expect(screen.getByRole("textbox", { name: "Tone" })).toHaveValue("neutral");
    expect(screen.queryByText("Loading active workflow…")).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("get_execution_plan", expect.anything());
  });

  it("collects and passes the full active profile options", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_batch_status") return Promise.resolve([]);
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
      inputInterpretation: "document",
    }));
  });

  it("accepts queued documents for a folder-profile batch", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_execution_plan") return Promise.resolve(folderSetup());
      if (command === "start_batch") return Promise.resolve();
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    open
      .mockResolvedValueOnce("/inputs/rubric.pdf")
      .mockResolvedValueOnce(["/papers/paper.pdf"]);
    const user = userEvent.setup();
    render(<BatchPanel onClose={vi.fn()} onOpenRun={vi.fn()} />);

    await screen.findByRole("button", { name: "Add files…" });
    await user.click(screen.getByRole("button", { name: "Choose…" }));
    await user.click(screen.getByRole("button", { name: "Add files…" }));
    await user.click(screen.getByRole("button", { name: "Start batch (1)" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith(
      "start_batch",
      expect.objectContaining({ paths: ["/papers/paper.pdf"] }),
    ));
  });

  it("gates batch processing only for workflows with no primary input", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_execution_plan") return Promise.resolve(setup("none"));
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    render(<BatchPanel onClose={vi.fn()} onOpenRun={vi.fn()} />);

    expect(await screen.findByText("This workflow does not accept batch inputs.")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Add files…" })).not.toBeInTheDocument();
  });

  it("shows the exact captured profile and snapshot for a batch", async () => {
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
      if (command === "get_execution_plan") return Promise.resolve(setup());
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    render(<BatchPanel onClose={vi.fn()} onOpenRun={vi.fn()} />);

    expect(await screen.findByText("batch-snapshot-exact")).toBeVisible();
    expect(screen.getByText(/variables: Tone/)).toBeVisible();
  });

  it("cleans successful listeners and keeps mutations disabled after partial registration failure", async () => {
    const cleanups = [vi.fn(), vi.fn()];
    let registration = 0;
    listen.mockImplementation(() => {
      const index = registration++;
      return index === 1
        ? Promise.reject(new Error("batch listener denied"))
        : Promise.resolve(cleanups[index]);
    });
    render(<BatchPanel onClose={vi.fn()} onOpenRun={vi.fn()} />);

    expect(await screen.findByRole("alert")).toHaveTextContent("batch listener denied");
    expect(cleanups[0]).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Start batch (0)" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Retry setup" })).toBeEnabled();
  });

  it("rejects a stale profile schema immediately before starting", async () => {
    let plans = 0;
    invoke.mockImplementation((command: string) => {
      if (command === "get_batch_status") return Promise.resolve([]);
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

});
