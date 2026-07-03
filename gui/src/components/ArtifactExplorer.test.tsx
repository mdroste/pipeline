import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ArtifactExplorer from "./ArtifactExplorer";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const openExternal = vi.hoisted(() => vi.fn(() => Promise.resolve()));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openExternal }));

const manifest = {
  run_id: "abc123_20260703-010000",
  created: "2026-07-03T01:00:00Z",
  input_path: "/papers/draft.pdf",
  input_mode: "document",
  profile_name: "Deep Review",
  provider: "claude",
  artifacts: [
    { rel_path: "report.md", label: "Report", kind: "markdown", bytes: 100, sha256: "aa", group: "report" },
    { rel_path: "context/orientation.json", label: "Orientation map", kind: "json", bytes: 50, sha256: "bb", group: "context" },
    { rel_path: "artifacts/01_technical.md", label: "Technical", kind: "markdown", bytes: 80, sha256: "cc", group: "step" },
    { rel_path: "artifacts/02_analysis.py", label: "Analysis script", kind: "code", bytes: 60, sha256: "dd", group: "step" },
    { rel_path: "artifacts/03_results.csv", label: "Results table", kind: "csv", bytes: 40, sha256: "ee", group: "step" },
    { rel_path: "artifacts/04_model.pdf", label: "Model PDF", kind: "binary", bytes: 5000, sha256: "ff", group: "step" },
  ],
};

function textContent(kind: string, text: string) {
  return { kind, bytes: text.length, text, base64: null, truncated: false, abs_path: `/runs/x/${kind}` };
}

function mockBackend(contents: Record<string, unknown>) {
  invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "get_run_manifest") return Promise.resolve(manifest);
    if (cmd === "read_artifact") {
      const rel = args?.relPath as string;
      if (rel in contents) return Promise.resolve(contents[rel]);
      return Promise.reject(new Error(`no content for ${rel}`));
    }
    return Promise.reject(new Error(`unexpected command: ${cmd}`));
  });
}

describe("ArtifactExplorer", () => {
  beforeEach(() => {
    invoke.mockReset();
    openExternal.mockClear();
  });

  it("renders the manifest tree grouped by section and shows the report by default", async () => {
    mockBackend({ "report.md": textContent("markdown", "# Final Report\n\nAll good.") });
    render(<ArtifactExplorer runId={manifest.run_id} fallbackMarkdown="fallback" />);

    // Group headings (h4) — "Report" also names the artifact button, so
    // query by role to disambiguate.
    expect(
      await screen.findByRole("heading", { name: "Report", level: 4 }),
    ).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Context", level: 4 })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Steps", level: 4 })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Analysis script" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Final Report" })).toBeInTheDocument();
  });

  it("renders code artifacts with syntax highlighting", async () => {
    const user = userEvent.setup();
    mockBackend({
      "report.md": textContent("markdown", "# R"),
      "artifacts/02_analysis.py": textContent("code", "def f():\n    return 42"),
    });
    const { container } = render(
      <ArtifactExplorer runId={manifest.run_id} fallbackMarkdown="" />,
    );
    await user.click(await screen.findByRole("button", { name: "Analysis script" }));

    await waitFor(() => {
      expect(container.querySelector("pre code")).not.toBeNull();
    });
    expect(container.querySelector("pre code")?.textContent).toContain("return 42");
    // highlight.js wraps tokens in .hljs-* spans
    expect(container.querySelector("[class*='hljs-']")).not.toBeNull();
  });

  it("renders CSV artifacts as a table", async () => {
    const user = userEvent.setup();
    mockBackend({
      "report.md": textContent("markdown", "# R"),
      "artifacts/03_results.csv": textContent("csv", "name,value\nalpha,1\nbeta,2"),
    });
    render(<ArtifactExplorer runId={manifest.run_id} fallbackMarkdown="" />);
    await user.click(await screen.findByRole("button", { name: "Results table" }));

    expect(await screen.findByRole("table")).toBeInTheDocument();
    expect(screen.getByText("alpha")).toBeInTheDocument();
    expect(screen.getByText("beta")).toBeInTheDocument();
  });

  it("shows an open-externally card for binary artifacts", async () => {
    const user = userEvent.setup();
    mockBackend({
      "report.md": textContent("markdown", "# R"),
      "artifacts/04_model.pdf": {
        kind: "binary", bytes: 5000, text: null, base64: null,
        truncated: false, abs_path: "/runs/x/artifacts/04_model.pdf",
      },
    });
    render(<ArtifactExplorer runId={manifest.run_id} fallbackMarkdown="" />);
    await user.click(await screen.findByRole("button", { name: "Model PDF" }));

    const open = await screen.findByRole("button", { name: "Open in system viewer" });
    await user.click(open);
    expect(openExternal).toHaveBeenCalledWith("/runs/x/artifacts/04_model.pdf");
  });

  it("falls back to the plain report when the manifest cannot load", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "get_run_manifest"
        ? Promise.reject(new Error("no manifest"))
        : Promise.reject(new Error("unexpected")),
    );
    render(
      <ArtifactExplorer runId="missing" fallbackMarkdown={"# Fallback Report"} />,
    );
    expect(
      await screen.findByRole("heading", { name: "Fallback Report" }),
    ).toBeInTheDocument();
  });
});
