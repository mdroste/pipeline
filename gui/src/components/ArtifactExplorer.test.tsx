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
  page_artifacts: { count: 1, digit_width: 1, extension: "jpg", total_bytes: 500 },
  artifacts: [
    { rel_path: "report.md", label: "Report", kind: "markdown", bytes: 100, sha256: "aa", group: "report" },
    { rel_path: "context/document_bundle.json", label: "Document bundle", kind: "json", bytes: 500, sha256: "ab", group: "document" },
    { rel_path: "context/orientation.json", label: "Orientation map", kind: "json", bytes: 50, sha256: "bb", group: "context" },
    { rel_path: "artifacts/figures/figure-1.png", label: "Figure 1 image", kind: "image", bytes: 500, sha256: "bd", group: "figures" },
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
    if (cmd === "read_page_artifact") {
      const key = `page:${args?.page}`;
      if (key in contents) return Promise.resolve(contents[key]);
      return Promise.reject(new Error(`no content for ${key}`));
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

  it("can defer all artifact reads until the user chooses a source", async () => {
    const user = userEvent.setup();
    mockBackend({
      "artifacts/01_technical.md": textContent("markdown", "# Technical source"),
    });
    render(
      <ArtifactExplorer
        runId={manifest.run_id}
        fallbackMarkdown=""
        deferInitialArtifact
      />,
    );

    expect(await screen.findByText("Select an artifact to preview it.")).toBeVisible();
    expect(
      invoke.mock.calls.filter(([command]) => command === "read_artifact"),
    ).toHaveLength(0);

    await user.click(screen.getByRole("button", { name: "Technical" }));
    expect(await screen.findByRole("heading", { name: "Technical source" })).toBeVisible();
    expect(invoke).toHaveBeenCalledWith("read_artifact", {
      runId: manifest.run_id,
      relPath: "artifacts/01_technical.md",
    });
  });

  it("uses a searchable compact page browser and lets the artifact rail collapse", async () => {
    const user = userEvent.setup();
    mockBackend({
      "report.md": textContent("markdown", "# R"),
      "page:1": {
        kind: "image",
        bytes: 500,
        text: null,
        base64: "aGVsbG8=",
        truncated: false,
        abs_path: "/runs/x/page-1.jpg",
      },
    });
    render(<ArtifactExplorer runId={manifest.run_id} fallbackMarkdown="" />);

    expect(await screen.findByRole("textbox", { name: "Search pages" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Page 1" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Page 1" }));
    expect(await screen.findByRole("img", { name: "Page 1" })).toBeVisible();
    expect(invoke).toHaveBeenCalledWith("read_page_artifact", {
      runId: manifest.run_id,
      page: 1,
    });
    await user.click(screen.getByRole("button", { name: "Hide artifact browser" }));
    expect(screen.queryByRole("textbox", { name: "Search pages" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Show artifact browser" }));
    expect(screen.getByRole("textbox", { name: "Search pages" })).toBeVisible();
  });

  it("materializes only one page-index window for long papers", async () => {
    const user = userEvent.setup();
    const longManifest = {
      ...manifest,
      page_artifacts: {
        count: 200,
        digit_width: 3,
        extension: "jpg",
        total_bytes: 30_000_000,
      },
    };
    invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_run_manifest") return Promise.resolve(longManifest);
      if (cmd === "read_artifact" && args?.relPath === "report.md") {
        return Promise.resolve(textContent("markdown", "# R"));
      }
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    render(<ArtifactExplorer runId={manifest.run_id} fallbackMarkdown="" />);

    expect(await screen.findByRole("button", { name: "Page 1" })).toBeVisible();
    expect(
      screen.getAllByRole("button", { name: /^Page \d+$/ }),
    ).toHaveLength(25);
    expect(screen.queryByRole("button", { name: "Page 26" })).not.toBeInTheDocument();

    await user.selectOptions(screen.getByRole("combobox", { name: "Page range" }), "25");
    expect(screen.getByRole("button", { name: "Page 26" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Page 1" })).not.toBeInTheDocument();
  });

  it("keeps legacy per-page manifest entries readable", async () => {
    const user = userEvent.setup();
    const legacyManifest = {
      ...manifest,
      page_artifacts: undefined,
      artifacts: [
        ...manifest.artifacts,
        {
          rel_path: "artifacts/pages/page-1.png",
          label: "Page 1",
          kind: "image",
          bytes: 600,
          sha256: "legacy",
          group: "pages",
        },
      ],
    };
    invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_run_manifest") return Promise.resolve(legacyManifest);
      if (cmd === "read_artifact" && args?.relPath === "report.md") {
        return Promise.resolve(textContent("markdown", "# R"));
      }
      if (cmd === "read_artifact" && args?.relPath === "artifacts/pages/page-1.png") {
        return Promise.resolve({
          kind: "image",
          bytes: 600,
          text: null,
          base64: "aGVsbG8=",
          truncated: false,
          abs_path: "/runs/x/page-1.png",
        });
      }
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    render(<ArtifactExplorer runId={manifest.run_id} fallbackMarkdown="" />);

    await user.click(await screen.findByRole("button", { name: "Page 1" }));
    expect(await screen.findByRole("img", { name: "Page 1" })).toBeVisible();
    expect(invoke).toHaveBeenCalledWith("read_artifact", {
      runId: manifest.run_id,
      relPath: "artifacts/pages/page-1.png",
    });
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

  it("shows every document artifact group and renders a human-readable bundle inspection", async () => {
    const user = userEvent.setup();
    const bundle = {
      schema_version: "1.0",
      bundle_id: "doc-0123456789abcdef",
      source_kind: "pdf",
      origins: [{ id: "origin-primary", kind: "pdf", path: "/papers/draft.pdf", role: "primary" }],
      pages: [{ number: 1, label: "Page 1", asset_id: "asset-page" }],
      nodes: [
        {
          id: "figure-00002",
          kind: "figure",
          order: 2,
          page: 1,
          label: "Figure 1",
          number: "1",
          text: "Impulse responses after a monetary policy shock.",
          asset_ids: ["asset-figure"],
          representations: [{ format: "orientation_summary", content: { what_it_shows: "Output falls." } }],
          provenance: { origin_id: "origin-primary", method: "marker", confidence: 0.9 },
        },
      ],
      assets: [
        {
          id: "asset-page",
          kind: "page",
          label: "Page 1",
          rel_path: "artifacts/pages/page-1.jpg",
          media_type: "image/jpeg",
          page: 1,
          width: 1200,
          height: 1600,
          provenance: { origin_id: "origin-primary", method: "pdftoppm", confidence: 1 },
        },
        {
          id: "asset-figure",
          kind: "figure",
          label: "Figure 1 image",
          rel_path: "artifacts/figures/figure-1.png",
          media_type: "image/png",
          page: 1,
          provenance: { origin_id: "origin-primary", method: "marker", confidence: 1 },
        },
      ],
      links: [],
      extraction: { method: "marker", source_path: "/papers/draft.pdf", paper_hash: "0123456789abcdef" },
      quality: [{ severity: "warning", scope: "page 1", message: "OCR confidence was low." }],
    };
    mockBackend({
      "report.md": textContent("markdown", "# R"),
      "context/document_bundle.json": textContent("json", JSON.stringify(bundle)),
    });
    render(<ArtifactExplorer runId={manifest.run_id} fallbackMarkdown="" />);

    expect(await screen.findByRole("heading", { name: "Document", level: 4 })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Pages", level: 4 })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Figures", level: 4 })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Document bundle" }));

    expect(await screen.findByRole("heading", { name: "DocumentBundle inspection" })).toBeInTheDocument();
    expect(screen.getAllByText("Impulse responses after a monetary policy shock.").length).toBeGreaterThan(0);
    expect(screen.getByText("OCR confidence was low.")).toBeInTheDocument();
    expect(screen.getAllByText("Figure 1 image").length).toBeGreaterThan(0);
    // Page renders already have a paged browser in the artifact rail; the
    // bundle inspector must not duplicate hundreds of page cards.
    expect(screen.queryByText(/1200×1600 · image\/jpeg/)).not.toBeInTheDocument();
  });

  it("clears the previous artifact while the next one is loading", async () => {
    const user = userEvent.setup();
    let resolveNext: ((value: unknown) => void) | undefined;
    invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_run_manifest") return Promise.resolve(manifest);
      if (cmd === "read_artifact" && args?.relPath === "report.md") {
        return Promise.resolve(textContent("markdown", "# Previous report"));
      }
      if (cmd === "read_artifact" && args?.relPath === "artifacts/02_analysis.py") {
        return new Promise((resolve) => { resolveNext = resolve; });
      }
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    render(<ArtifactExplorer runId={manifest.run_id} fallbackMarkdown="" />);
    expect(await screen.findByRole("heading", { name: "Previous report" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Analysis script" }));
    await waitFor(() => {
      expect(screen.queryByRole("heading", { name: "Previous report" })).not.toBeInTheDocument();
    });
    resolveNext?.(textContent("code", "return 42"));
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

  it("surfaces system-viewer plugin failures for binary artifacts", async () => {
    const user = userEvent.setup();
    openExternal.mockRejectedValueOnce(new Error("shell plugin unavailable"));
    mockBackend({
      "report.md": textContent("markdown", "# R"),
      "artifacts/04_model.pdf": {
        kind: "binary", bytes: 5000, text: null, base64: null,
        truncated: false, abs_path: "/runs/x/artifacts/04_model.pdf",
      },
    });
    render(<ArtifactExplorer runId={manifest.run_id} fallbackMarkdown="" />);
    await user.click(await screen.findByRole("button", { name: "Model PDF" }));
    await user.click(
      await screen.findByRole("button", { name: "Open in system viewer" }),
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not open this artifact in the system viewer: shell plugin unavailable",
    );
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
    expect(screen.getByRole("alert")).toHaveTextContent("no manifest");
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });

  it("retries a failed manifest request", async () => {
    let attempts = 0;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_run_manifest" && attempts++ === 0) {
        return Promise.reject(new Error("temporary failure"));
      }
      if (cmd === "get_run_manifest") return Promise.resolve(manifest);
      if (cmd === "read_artifact") return Promise.resolve(textContent("markdown", "# Recovered"));
      return Promise.reject(new Error("unexpected"));
    });
    render(<ArtifactExplorer runId={manifest.run_id} fallbackMarkdown="" />);
    await userEvent.setup().click(await screen.findByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("heading", { name: "Recovered" })).toBeInTheDocument();
  });
});
