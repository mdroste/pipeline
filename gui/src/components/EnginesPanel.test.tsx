import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import EnginesPanel from "./EnginesPanel";
import type { EngineDownloadProgress, EngineStatus } from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
const confirmDialog = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./DialogService", () => ({ confirmDialog, notify: vi.fn() }));

type Handler = (event: { payload: unknown }) => void;
const handlers = vi.hoisted(() => new Map<string, Handler>());
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/event", () => ({
  listen,
}));

function engine(overrides: Partial<EngineStatus> = {}): EngineStatus {
  return {
    id: "paddleocr-vl-parser",
    label: "PaddleOCR-VL 1.6 Full Parser",
    description: "Layout-aware local extraction.",
    installed: false,
    version: "",
    entry_path: "",
    system_path: "",
    est_download_mb: 2900,
    est_disk_mb: 3800,
    managed_stack_mb: 0,
    installing: false,
    install_progress: null,
    ...overrides,
  };
}

describe("EnginesPanel", () => {
  beforeEach(() => {
    invoke.mockReset();
    confirmDialog.mockReset();
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
    expect(
      await screen.findByText("PaddleOCR-VL 1.6 Full Parser"),
    ).toBeInTheDocument();
    expect(screen.getByText("not installed")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Install \(~2\.9 GB\)/ }),
    ).toBeInTheDocument();
  });

  it("explains and disables an engine unavailable on this platform", async () => {
    invoke.mockResolvedValue([
      engine({
        id: "paddleocr-vl-parser",
        label: "PaddleOCR-VL 1.6 Full Parser",
        available: false,
        unavailable_reason: "The official runtime has no Intel macOS wheel.",
      }),
    ]);
    render(<EnginesPanel />);
    expect(await screen.findByText("unavailable")).toBeInTheDocument();
    expect(screen.getByText(/no Intel macOS wheel/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Install/ })).toBeDisabled();
  });

  it("shows installed state with version, disk usage, and Uninstall", async () => {
    invoke.mockResolvedValue([
      engine({ installed: true, version: "3.7.0", managed_stack_mb: 3800 }),
    ]);
    render(<EnginesPanel />);
    expect(await screen.findByText("installed v3.7.0")).toBeInTheDocument();
    expect(screen.getByText(/Disk usage: 3\.8 GB/)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Uninstall" }),
    ).toBeInTheDocument();
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
      return Promise.resolve();
    });
    const user = userEvent.setup();
    render(<EnginesPanel />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Local engine status could not be loaded: engine catalog unavailable",
    );
    await user.click(screen.getByRole("button", { name: "Retry status" }));

    expect(
      await screen.findByText("PaddleOCR-VL 1.6 Full Parser"),
    ).toBeVisible();
    await waitFor(() =>
      expect(
        screen.queryByText(/engine catalog unavailable/),
      ).not.toBeInTheDocument(),
    );
  });

  it("surfaces a failure to open the Pipeline data folder", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([engine()]);
      if (cmd === "open_pipeline_dir") {
        return Promise.reject(new Error("shell integration unavailable"));
      }
      return Promise.resolve();
    });
    const user = userEvent.setup();
    render(<EnginesPanel />);

    await screen.findByText("PaddleOCR-VL 1.6 Full Parser");
    await user.click(
      screen.getByRole("button", {
        name: "More information about Local Engines",
      }),
    );
    await user.click(screen.getByRole("button", { name: "~/.pipeline/" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not open ~/.pipeline/: shell integration unavailable",
    );
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
      engineId: "paddleocr-vl-parser",
    });

    // While installing: steps update from events, log lines stream.
    act(() => {
      handlers.get("engines:phase")!({
        payload: {
          engine: "paddleocr-vl-parser",
          phase: "models",
          status: "running",
        },
      });
      handlers.get("engines:log")!({
        payload: { line: "Downloading model weights..." },
      });
    });
    expect(
      await screen.findByText("Downloading model weights..."),
    ).toBeInTheDocument();
    expect(screen.getByText("Layout model").closest("li")).toHaveAttribute(
      "aria-current",
      "step",
    );
    expect(screen.getByText("Installing the layout model…")).toBeVisible();
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

  async function startInstall() {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([engine()]);
      if (cmd === "install_engine") return new Promise(() => {});
      return Promise.resolve();
    });
    render(<EnginesPanel />);
    await userEvent.click(
      await screen.findByRole("button", { name: /Install/ }),
    );
  }

  function emitDownload(
    download: Partial<EngineDownloadProgress> | null,
  ): void {
    act(() => {
      handlers.get("engines:download")!({
        payload: {
          download: download && {
            label: "llama.cpp runtime",
            received_bytes: 0,
            total_bytes: null,
            bytes_per_second: 0,
            retrying: false,
            ...download,
          },
        },
      });
    });
  }

  it("shows the size, speed, and time left of the current download", async () => {
    await startInstall();

    emitDownload({
      received_bytes: 412_000_000,
      total_bytes: 936_000_000,
      bytes_per_second: 8_400_000,
    });

    const bar = screen.getByRole("progressbar", {
      name: "Downloading llama.cpp runtime",
    });
    expect(bar).toHaveAttribute("aria-valuenow", "44");
    expect(screen.getByText("44%")).toBeVisible();
    expect(
      screen.getByText("412 MB of 936 MB · 8.4 MB/s · about 2 min left"),
    ).toBeVisible();

    emitDownload(null);
    expect(screen.queryByText("44%")).toBeNull();
  });

  it("keeps an unknown-size download indeterminate", async () => {
    await startInstall();

    emitDownload({ received_bytes: 25_000_000, bytes_per_second: 500_000 });

    expect(screen.getByRole("progressbar")).not.toHaveAttribute(
      "aria-valuenow",
    );
    expect(screen.getByText("25 MB · 500 KB/s")).toBeVisible();
  });

  it("explains a dropped connection while it reconnects", async () => {
    await startInstall();

    emitDownload({
      received_bytes: 1_250_000_000,
      total_bytes: 2_000_000_000,
      retrying: true,
    });

    expect(
      screen.getByText(
        "Connection interrupted. Reconnecting and continuing from 1.25 GB.",
      ),
    ).toBeVisible();
  });

  it("keeps the installer log collapsed until it is requested", async () => {
    await startInstall();
    act(() => {
      handlers.get("engines:log")!({ payload: { line: "Verified runtime" } });
    });

    const line = screen.getByText("Verified runtime");
    expect(line).not.toBeVisible();

    await userEvent.click(screen.getByText("Show details"));

    expect(line).toBeVisible();
  });

  it("restores an in-progress install after the panel is remounted", async () => {
    let listRequests = 0;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") {
        listRequests += 1;
        return Promise.resolve([
          listRequests === 1
            ? engine()
            : engine({
                installing: true,
                install_progress: {
                  engine_id: "paddleocr-vl-parser",
                  phases: { recognition: "running" },
                  log_lines: ["Downloading model weights in the background..."],
                  download: {
                    label: "PaddleOCR-VL Q8 model",
                    received_bytes: 500_000_000,
                    total_bytes: 1_000_000_000,
                    bytes_per_second: 0,
                    retrying: false,
                  },
                },
              }),
        ]);
      }
      if (cmd === "install_engine") return new Promise(() => {});
      return Promise.resolve();
    });

    const firstView = render(<EnginesPanel />);
    await userEvent.click(
      await screen.findByRole("button", { name: /Install/ }),
    );
    firstView.unmount();

    render(<EnginesPanel />);

    expect(
      await screen.findByRole("progressbar", {
        name: "Downloading PaddleOCR-VL Q8 model",
      }),
    ).toHaveAttribute("aria-valuenow", "50");
    expect(screen.getByRole("status")).toHaveTextContent(
      "Installation continues in the background if you leave Settings.",
    );
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
    expect(
      invoke.mock.calls.filter(([command]) => command === "install_engine"),
    ).toHaveLength(1);
  });

  it("polls an inherited install until it ends and then shows the result", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    try {
      const running = engine({
        installing: true,
        install_progress: {
          engine_id: "paddleocr-vl-parser",
          phases: { runtime: "done", packages: "done", models: "running" },
          log_lines: ["Downloading model weights in the background..."],
        },
      });
      let status = running;
      invoke.mockImplementation((cmd: string) =>
        cmd === "list_engines" ? Promise.resolve([status]) : Promise.resolve(),
      );
      const onSystemChange = vi.fn();
      const onEngineStatusChange = vi.fn();
      render(
        <EnginesPanel
          onSystemChange={onSystemChange}
          onEngineStatusChange={onEngineStatusChange}
        />,
      );
      expect(
        await screen.findByRole("button", { name: "Cancel" }),
      ).toBeEnabled();

      status = engine({ installed: true, version: "3.7.0" });
      await act(() => vi.advanceTimersByTimeAsync(2500));

      expect(await screen.findByText("installed v3.7.0")).toBeVisible();
      expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
      expect(onSystemChange).toHaveBeenCalledTimes(1);
      expect(onEngineStatusChange).toHaveBeenLastCalledWith([status]);
      const polls = invoke.mock.calls.length;
      await act(() => vi.advanceTimersByTimeAsync(10000));
      expect(invoke.mock.calls).toHaveLength(polls);
    } finally {
      vi.useRealTimers();
    }
  });

  it("reports an inherited install that ended without installing", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    try {
      let status = engine({ installing: true });
      invoke.mockImplementation((cmd: string) =>
        cmd === "list_engines" ? Promise.resolve([status]) : Promise.resolve(),
      );
      render(<EnginesPanel />);
      await screen.findByRole("button", { name: "Cancel" });
      status = engine();
      await act(() => vi.advanceTimersByTimeAsync(2500));
      expect(await screen.findByRole("alert")).toHaveTextContent(
        "PaddleOCR-VL 1.6 Full Parser was not installed.",
      );
      expect(
        screen.getByRole("button", { name: /Install \(~2\.9 GB\)/ }),
      ).toBeEnabled();
    } finally {
      vi.useRealTimers();
    }
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

  it("keeps the failed step and log visible after an install fails", async () => {
    let failInstall!: (reason: string) => void;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([engine()]);
      if (cmd === "install_engine")
        return new Promise((_, reject) => {
          failInstall = reject;
        });
      return Promise.resolve();
    });
    render(<EnginesPanel />);
    await userEvent.click(
      await screen.findByRole("button", { name: /Install/ }),
    );
    act(() => {
      handlers.get("engines:phase")!({
        payload: {
          engine: "paddleocr-vl-parser",
          phase: "recognition",
          status: "failed",
        },
      });
      handlers.get("engines:log")!({ payload: { line: "github.com: 404" } });
    });

    await act(async () => failInstall("llama.cpp runtime download failed"));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "llama.cpp runtime download failed",
    );
    expect(screen.getByText("OCR model").closest("li")).toHaveClass(
      "text-red-700",
    );
    expect(screen.getByText("Show details")).toBeVisible();
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it("describes a running uninstall without install progress", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines")
        return Promise.resolve([engine({ installed: true })]);
      if (cmd === "uninstall_engine") return new Promise(() => {});
      return Promise.resolve();
    });
    confirmDialog.mockResolvedValueOnce(true);
    render(<EnginesPanel />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Uninstall" }),
    );

    expect(await screen.findByRole("status")).toHaveTextContent(
      "Removing PaddleOCR-VL 1.6 Full Parser…",
    );
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it("uninstall asks for confirmation and skips when declined", async () => {
    invoke.mockResolvedValue([engine({ installed: true, version: "3.7.0" })]);
    confirmDialog.mockResolvedValueOnce(false);
    render(<EnginesPanel />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Uninstall" }),
    );
    expect(confirmDialog).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith(
      "uninstall_engine",
      expect.anything(),
    );
  });

  it("uninstall proceeds when confirmed", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines")
        return Promise.resolve([engine({ installed: true })]);
      return Promise.resolve();
    });
    confirmDialog.mockResolvedValueOnce(true);
    render(<EnginesPanel />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Uninstall" }),
    );
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("uninstall_engine", {
        engineId: "paddleocr-vl-parser",
      }),
    );
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
      return Promise.resolve();
    });
    render(<EnginesPanel />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "event permission denied",
    );
    expect(cleanup).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: /Install/ })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Retry connection" }),
    ).toBeEnabled();
  });
});
