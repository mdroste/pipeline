import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import EnginesPanel from "./EnginesPanel";
import type { EngineStatus } from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

type Handler = (event: { payload: unknown }) => void;
const handlers = vi.hoisted(() => new Map<string, Handler>());
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, cb: Handler) => {
    handlers.set(name, cb);
    return Promise.resolve(() => handlers.delete(name));
  },
}));

function engine(overrides: Partial<EngineStatus> = {}): EngineStatus {
  return {
    id: "marker",
    label: "marker-pdf",
    description: "Local PDF-to-Markdown conversion.",
    installed: false,
    version: "",
    entry_path: "",
    system_path: "",
    est_download_mb: 3500,
    est_disk_mb: 6000,
    managed_stack_mb: 0,
    installing: false,
    ...overrides,
  };
}

describe("EnginesPanel", () => {
  beforeEach(() => {
    invoke.mockReset();
    handlers.clear();
  });

  it("lists engines with an Install button and size estimate", async () => {
    invoke.mockResolvedValue([engine()]);
    render(<EnginesPanel />);
    expect(await screen.findByText("marker-pdf")).toBeInTheDocument();
    expect(screen.getByText("not installed")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Install \(~3\.5 GB\)/ }),
    ).toBeInTheDocument();
  });

  it("lists PaddleOCR-VL as a separate compact managed engine", async () => {
    invoke.mockResolvedValue([
      engine(),
      engine({
        id: "paddleocr-vl",
        label: "PaddleOCR-VL 1.6 Q8",
        description: "Compact native extraction.",
        est_download_mb: 1900,
        est_disk_mb: 2300,
      }),
    ]);
    render(<EnginesPanel />);
    expect(await screen.findByText("PaddleOCR-VL 1.6 Q8")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Install \(~1\.9 GB\)/ }),
    ).toBeInTheDocument();
  });

  it("notes a system copy (e.g. anaconda) when no managed install exists", async () => {
    invoke.mockResolvedValue([
      engine({ system_path: "/opt/anaconda3/bin/marker_single" }),
    ]);
    render(<EnginesPanel />);
    expect(
      await screen.findByText("/opt/anaconda3/bin/marker_single"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Extraction uses it until you install a managed copy/),
    ).toBeInTheDocument();
    // Install is still offered.
    expect(screen.getByRole("button", { name: /Install/ })).toBeInTheDocument();
  });

  it("marks a system copy as ignored once the managed install exists", async () => {
    invoke.mockResolvedValue([
      engine({
        installed: true,
        version: "1.10.2",
        system_path: "/opt/anaconda3/bin/marker_single",
      }),
    ]);
    render(<EnginesPanel />);
    expect(
      await screen.findByText(/is ignored in favor of this managed install/),
    ).toBeInTheDocument();
  });

  it("shows installed state with version, disk usage, and Uninstall", async () => {
    invoke.mockResolvedValue([
      engine({ installed: true, version: "1.10.2", managed_stack_mb: 5200 }),
    ]);
    render(<EnginesPanel />);
    expect(await screen.findByText("installed v1.10.2")).toBeInTheDocument();
    expect(screen.getByText(/Disk usage: 5\.2 GB/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeInTheDocument();
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
      engineId: "marker",
    });

    // While installing: phase chips update from events, log lines stream.
    handlers.get("engines:phase")!({
      payload: { engine: "marker", phase: "packages", status: "running" },
    });
    handlers.get("engines:log")!({
      payload: { line: "Downloading torch..." },
    });
    expect(await screen.findByText("Downloading torch...")).toBeInTheDocument();
    expect(screen.getByText("Packages")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();

    resolveInstall();
    // After completion the list refreshes.
    await waitFor(() =>
      expect(invoke).toHaveBeenLastCalledWith("list_engines"),
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
    invoke.mockResolvedValue([engine({ installed: true, version: "1.10.2" })]);
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
        engineId: "marker",
      }),
    );
    confirmSpy.mockRestore();
  });
});
