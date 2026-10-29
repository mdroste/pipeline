import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import ToolchainPicker from "./ToolchainPicker";
import type { ToolchainReport } from "../../lib/workbench/toolchains";

const mocks = vi.hoisted(() => ({ detectToolchains: vi.fn() }));
vi.mock("../../lib/workbench/executionClient", () => ({
  executionClient: mocks,
}));

const report: ToolchainReport = {
  platform: "macos",
  tools: [
    {
      kind: "stata",
      adapter: "stata",
      label: "StataNow MP",
      path: "/Applications/StataNow/StataMP.app/Contents/MacOS/stata-mp",
      source: "standardLocation",
      version: "19.5",
      versionText: null,
      versionSource: "install folder (Info.plist)",
      sha256: "a".repeat(64),
      sizeBytes: 10,
      note: "Detection does not start Stata. A profile test confirms the license and batch mode.",
    },
    {
      kind: "python",
      adapter: "python",
      label: ".venv (this project)",
      path: "/research/.venv/bin/python",
      source: "projectEnvironment",
      version: "3.12.4",
      versionText: "Python 3.12.4",
      versionSource: "python --version",
      sha256: null,
      sizeBytes: null,
      note: null,
    },
  ],
  project: {
    root: "/research",
    files: [
      {
        path: "renv.lock",
        ecosystem: "R",
        sha256: "b".repeat(64),
        sizeBytes: 2,
      },
    ],
    pythonEnvironments: [],
    stataAdoFolder: false,
  },
  boundary: "Detection installed nothing.",
};

beforeEach(() => {
  mocks.detectToolchains.mockReset();
});
afterEach(cleanup);

it("lists matching tools and fills the program from a detected path", async () => {
  mocks.detectToolchains.mockResolvedValue(report);
  const setArgv = vi.fn();
  render(
    <ToolchainPicker
      workspaceId="project"
      adapter="stata"
      argv={JSON.stringify(["stata-mp", "-b", "do", "main.do"])}
      setArgv={setArgv}
      disabled={false}
      onError={vi.fn()}
    />,
  );
  await act(async () => {
    fireEvent.click(
      screen.getByRole("button", { name: "Detect installed tools" }),
    );
  });
  expect(mocks.detectToolchains).toHaveBeenCalledExactlyOnceWith("project");
  expect(screen.getByText("StataNow MP")).toBeInTheDocument();
  expect(screen.getByText(/· 19\.5/)).toBeInTheDocument();
  expect(screen.queryByText(".venv (this project)")).not.toBeInTheDocument();
  expect(
    screen.getByText(/Environment files in this project: renv\.lock/),
  ).toBeInTheDocument();
  expect(screen.getByText("Detection installed nothing.")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Use StataNow MP" }));
  expect(JSON.parse(setArgv.mock.calls[0][0])).toEqual([
    report.tools[0].path,
    "-b",
    "do",
    "main.do",
  ]);
});

it("explains when nothing matches and reports detection errors", async () => {
  mocks.detectToolchains.mockResolvedValueOnce({ ...report, tools: [] });
  const onError = vi.fn();
  render(
    <ToolchainPicker
      workspaceId="project"
      adapter="stata"
      argv="[]"
      setArgv={vi.fn()}
      disabled={false}
      onError={onError}
    />,
  );
  const detect = screen.getByRole("button", { name: "Detect installed tools" });
  await act(async () => {
    fireEvent.click(detect);
  });
  expect(
    screen.getByText(/No Stata installation was found/),
  ).toBeInTheDocument();
  mocks.detectToolchains.mockRejectedValueOnce({
    code: "worker",
    message: "Toolchain detection stopped",
    retryable: true,
    recovery: null,
  });
  await act(async () => {
    fireEvent.click(detect);
  });
  expect(onError).toHaveBeenCalledWith(
    expect.stringContaining("Toolchain detection stopped"),
  );
});

it("is not offered for LaTeX, whose engines are chosen by name", () => {
  const { container } = render(
    <ToolchainPicker
      workspaceId="project"
      adapter="latex"
      argv="[]"
      setArgv={vi.fn()}
      disabled={false}
      onError={vi.fn()}
    />,
  );
  expect(container).toBeEmptyDOMElement();
});
