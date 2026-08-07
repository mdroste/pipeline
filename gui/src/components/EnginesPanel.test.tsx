import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import EnginesPanel from "./EnginesPanel";
import type { EngineStatus } from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

type Handler = (event: { payload: unknown }) => void;
const handlers = vi.hoisted(() => new Map<string, Handler>());
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/event", () => ({
  listen,
}));

function engine(overrides: Partial<EngineStatus> = {}): EngineStatus {
  return {
    id: "paddleocr-vl",
    label: "PaddleOCR-VL 1.6 Q8",
    description: "Compact native extraction.",
    installed: false,
    version: "",
    entry_path: "",
    system_path: "",
    est_download_mb: 1900,
    est_disk_mb: 2300,
    managed_stack_mb: 0,
    installing: false,
    ...overrides,
  };
}

describe("EnginesPanel", () => {
  beforeEach(() => {
    invoke.mockReset();
    handlers.clear();
    listen.mockReset();
    listen.mockImplementation((name: string, cb: Handler) => {
      handlers.set(name, cb);
      return Promise.resolve(() => handlers.delete(name));
    });
  });

  it("lists engines with an Install button and size estimate", async () => {
    invoke.mockResolvedValue([engine()]);
    render(<EnginesPanel />);
    expect(await screen.findByText("PaddleOCR-VL 1.6 Q8")).toBeInTheDocument();
    expect(screen.getByText("not installed")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Install \(~1\.9 GB\)/ }),
    ).toBeInTheDocument();
  });

  it("shows installed state with version, disk usage, and Uninstall", async () => {
    invoke.mockResolvedValue([
      engine({ installed: true, version: "1.6 Q8", managed_stack_mb: 2300 }),
    ]);
    render(<EnginesPanel />);
    expect(await screen.findByText("installed v1.6 Q8")).toBeInTheDocument();
    expect(screen.getByText(/Disk usage: 2\.3 GB/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeInTheDocument();
  });

  it("surfaces an engine-list failure and retries it", async () => {
    let listAttempts = 0;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") {
        listAttempts += 1;
        return listAttempts === 1
          ? Promise.reject(new Error("engine catalog unavailable"))
          : Promise.resolve([engine()]);
      }
      if (cmd === "retired_marker_status") {
        return Promise.resolve({ present: false, bytes: 0 });
      }
      return Promise.resolve();
    });
    const user = userEvent.setup();
    render(<EnginesPanel />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Local engine status could not be loaded: engine catalog unavailable",
    );
    await user.click(screen.getByRole("button", { name: "Retry status" }));

    expect(await screen.findByText("PaddleOCR-VL 1.6 Q8")).toBeVisible();
    await waitFor(() =>
      expect(screen.queryByText(/engine catalog unavailable/)).not.toBeInTheDocument(),
    );
  });

  it("surfaces a retired-environment status failure without hiding loaded engines", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([engine()]);
      if (cmd === "retired_marker_status") {
        return Promise.reject(new Error("marker inspection denied"));
      }
      return Promise.resolve();
    });
    render(<EnginesPanel />);

    expect(await screen.findByText("PaddleOCR-VL 1.6 Q8")).toBeVisible();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Retired Marker status could not be checked: marker inspection denied",
    );
    expect(screen.getByRole("button", { name: "Retry status" })).toBeEnabled();
  });

  it("surfaces a failure to open the Pipeline data folder", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([engine()]);
      if (cmd === "retired_marker_status") {
        return Promise.resolve({ present: false, bytes: 0 });
      }
      if (cmd === "open_pipeline_dir") {
        return Promise.reject(new Error("shell integration unavailable"));
      }
      return Promise.resolve();
    });
    const user = userEvent.setup();
    render(<EnginesPanel />);

    await screen.findByText("PaddleOCR-VL 1.6 Q8");
    await user.click(screen.getByRole("button", { name: "~/.pipeline/" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not open ~/.pipeline/: shell integration unavailable",
    );
  });

  it("offers explicit removal for an old managed Marker environment", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([engine()]);
      if (cmd === "retired_marker_status") {
        return Promise.resolve({ present: true, bytes: 3_200_000_000 });
      }
      if (cmd === "remove_retired_marker") return Promise.resolve();
      return Promise.resolve();
    });
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<EnginesPanel />);

    expect(
      await screen.findByText("Retired Marker environment found"),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Remove old Marker files" }),
    );
    expect(confirmSpy).toHaveBeenCalled();
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("remove_retired_marker"),
    );
    confirmSpy.mockRestore();
  });

  it("install click invokes install_engine and streams phases and log lines", async () => {
    let resolveInstall!: () => void;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([engine()]);
      if (cmd === "install_engine")
        return new Promise<void>((r) => {
          resolveInstall = r;
        });
      return Promise.resolve();
    });
    render(<EnginesPanel />);
    await userEvent.click(
      await screen.findByRole("button", { name: /Install/ }),
    );
    expect(invoke).toHaveBeenCalledWith("install_engine", {
      engineId: "paddleocr-vl",
    });

    // While installing: phase chips update from events, log lines stream.
    act(() => {
      handlers.get("engines:phase")!({
        payload: { engine: "paddleocr-vl", phase: "models", status: "running" },
      });
      handlers.get("engines:log")!({
        payload: { line: "Downloading model weights..." },
      });
    });
    expect(await screen.findByText("Downloading model weights...")).toBeInTheDocument();
    expect(screen.getByText("Models")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();

    await act(async () => {
      resolveInstall();
    });
    // After completion the list refreshes.
    await waitFor(() =>
      expect(
        invoke.mock.calls.filter(([command]) => command === "list_engines"),
      ).toHaveLength(2),
    );
  });

  it("cancel button invokes cancel_engine_install", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([engine()]);
      if (cmd === "install_engine") return new Promise(() => {});
      return Promise.resolve();
    });
    render(<EnginesPanel />);
    await userEvent.click(
      await screen.findByRole("button", { name: /Install/ }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(invoke).toHaveBeenCalledWith("cancel_engine_install");
  });

  it("surfaces a failed engine cancellation", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([engine()]);
      if (cmd === "retired_marker_status") {
        return Promise.resolve({ present: false, bytes: 0 });
      }
      if (cmd === "install_engine") return new Promise(() => {});
      if (cmd === "cancel_engine_install") {
        return Promise.reject(new Error("cancellation channel unavailable"));
      }
      return Promise.resolve();
    });
    const user = userEvent.setup();
    render(<EnginesPanel />);
    await user.click(await screen.findByRole("button", { name: /Install/ }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Engine cancellation failed: cancellation channel unavailable",
    );
  });

  it("shows the error when an install fails", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([engine()]);
      if (cmd === "install_engine")
        return Promise.reject("Not enough disk space");
      return Promise.resolve();
    });
    render(<EnginesPanel />);
    await userEvent.click(
      await screen.findByRole("button", { name: /Install/ }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Not enough disk space",
    );
  });

  it("uninstall asks for confirmation and skips when declined", async () => {
    invoke.mockResolvedValue([engine({ installed: true, version: "1.6 Q8" })]);
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<EnginesPanel />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Uninstall" }),
    );
    expect(confirmSpy).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith(
      "uninstall_engine",
      expect.anything(),
    );
    confirmSpy.mockRestore();
  });

  it("uninstall proceeds when confirmed", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines")
        return Promise.resolve([engine({ installed: true })]);
      return Promise.resolve();
    });
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<EnginesPanel />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Uninstall" }),
    );
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("uninstall_engine", {
        engineId: "paddleocr-vl",
      }),
    );
    confirmSpy.mockRestore();
  });

  it("ignores an older engine-list response after a newer refresh completes", async () => {
    let resolveOld!: (value: EngineStatus[]) => void;
    const oldList = new Promise<EngineStatus[]>((resolve) => {
      resolveOld = resolve;
    });
    let listRequests = 0;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") {
        listRequests += 1;
        return listRequests === 1
          ? oldList
          : Promise.resolve([engine({ installed: true, version: "new" })]);
      }
      if (cmd === "retired_marker_status") {
        return Promise.resolve({ present: true, bytes: 1 });
      }
      if (cmd === "remove_retired_marker") return Promise.resolve();
      return Promise.resolve();
    });
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const user = userEvent.setup();
    render(<EnginesPanel />);

    await user.click(await screen.findByRole("button", { name: "Remove old Marker files" }));
    expect(await screen.findByText("installed vnew")).toBeVisible();
    await act(async () => resolveOld([engine({ installed: false })]));
    expect(screen.getByText("installed vnew")).toBeVisible();
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeVisible();
    confirmSpy.mockRestore();
  });

  it("fails closed and cleans partial registrations when a listener is unavailable", async () => {
    const cleanup = vi.fn();
    let registration = 0;
    listen.mockImplementation(() => {
      registration += 1;
      return registration === 1
        ? Promise.resolve(cleanup)
        : Promise.reject(new Error("event permission denied"));
    });
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([engine()]);
      if (cmd === "retired_marker_status") return Promise.resolve({ present: false, bytes: 0 });
      return Promise.resolve();
    });
    render(<EnginesPanel />);

    expect(await screen.findByRole("alert")).toHaveTextContent("event permission denied");
    expect(cleanup).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: /Install/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Retry connection" })).toBeEnabled();
  });
});
