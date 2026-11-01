import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ComparePage from "./ComparePage";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

function report(text: string) {
  return {
    orientation: {},
    step_outputs: [
      {
        step_id: "review",
        step_label: "Review",
        raw_text: text,
        phase: "parallel",
        agent: "codex",
        structured_json: false,
      },
    ],
    report_date: "2026-08-23",
    paper_hash: "hash",
  };
}

function manifest(id: string, created: string, provider: string) {
  return {
    run_id: id,
    created,
    input_path: "/paper.pdf",
    input_mode: "document",
    profile_name: "Review",
    provider,
    artifacts: [
      {
        rel_path: "report.md",
        label: "Report",
        kind: "markdown",
        bytes: 10,
        sha256: "abc",
        group: "report",
      },
    ],
  };
}

describe("ComparePage", () => {
  beforeEach(() => invoke.mockReset());

  it("labels older/newer provenance, summarizes changes, and exports the diff", async () => {
    invoke.mockImplementation((command: string, args?: { runId?: string }) => {
      if (command === "get_run_report")
        return Promise.resolve(
          report(args?.runId === "older" ? "before" : "after"),
        );
      if (command === "get_run_manifest")
        return Promise.resolve(
          args?.runId === "older"
            ? manifest("older", "2026-08-01T00:00:00Z", "claude")
            : manifest("newer", "2026-08-23T00:00:00Z", "codex"),
        );
      if (command === "save_text_file") return Promise.resolve("/tmp/diff.md");
      return Promise.resolve(undefined);
    });
    const user = userEvent.setup();
    render(<ComparePage runA="older" runB="newer" onBack={vi.fn()} />);

    expect(
      await screen.findByRole("heading", { name: "Older report" }),
    ).toBeVisible();
    expect(screen.getByRole("heading", { name: "Newer report" })).toBeVisible();
    expect(
      screen.getByText(/1 changed · 0 added · 0 removed · 0 unchanged/),
    ).toBeVisible();
    expect(screen.getByText(/AI comparison sends both reports/)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Export diff…" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_text_file", {
        content: expect.stringContaining("Older run: older"),
        suggestedName: "pipeline-diff-older-newer.md",
      }),
    );
  });

  const backend = (reportFor: (runId?: string) => unknown) =>
    invoke.mockImplementation((command: string, args?: { runId?: string }) => {
      if (command === "get_run_report") return reportFor(args?.runId);
      if (command === "get_run_manifest")
        return Promise.resolve(
          manifest(args?.runId ?? "", "2026-08-01T00:00:00Z", "codex"),
        );
      if (command === "save_text_file") return Promise.resolve("/tmp/diff.md");
      return Promise.resolve(undefined);
    });

  it("offers nothing to export or compare when a report failed to load", async () => {
    backend((runId) =>
      runId === "failed"
        ? Promise.reject(new Error("report.json is missing"))
        : Promise.resolve(report("text")),
    );
    render(<ComparePage runA="failed" runB="newer" onBack={vi.fn()} />);
    expect(await screen.findByText("report.json is missing")).toBeVisible();
    expect(screen.getByRole("button", { name: "Export diff…" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: /Compare findings with AI/ }),
    ).toBeDisabled();
    expect(screen.queryByText("Untitled report")).not.toBeInTheDocument();
    expect(screen.queryByText(/0 changed/)).not.toBeInTheDocument();
  });

  it("offers no export when neither report has any output", async () => {
    backend(() => Promise.resolve({ ...report(""), step_outputs: [] }));
    render(<ComparePage runA="older" runB="newer" onBack={vi.fn()} />);
    expect(await screen.findByText(/no difference to/)).toBeVisible();
    expect(screen.getByRole("button", { name: "Export diff…" })).toBeDisabled();
  });

  it("keeps a code fence in a step output inside one exported block", async () => {
    const output = (word: string) =>
      `before\n\`\`\`\ncode ${word}\n\`\`\`\nafter`;
    backend((runId) =>
      Promise.resolve(report(output(runId === "older" ? "old" : "new"))),
    );
    render(<ComparePage runA="older" runB="newer" onBack={vi.fn()} />);
    await userEvent
      .setup()
      .click(await screen.findByRole("button", { name: "Export diff…" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "save_text_file",
        expect.objectContaining({ content: expect.anything() }),
      ),
    );
    const { content } = invoke.mock.calls.find(
      ([command]) => command === "save_text_file",
    )![1] as { content: string };
    const lines = content.split("\n");
    const open = lines.findIndex((line) => line === "````diff");
    expect(open).toBeGreaterThan(-1);
    // CommonMark: only a run at least as long as the opener closes it.
    const close = lines.findIndex(
      (line, index) => index > open && /^ {0,3}`{4,}\s*$/.test(line),
    );
    expect(
      lines
        .slice(close + 1)
        .join("")
        .trim(),
    ).toBe("");
    expect(lines.slice(open + 1, close).join("\n")).toContain("- code old");
    expect(lines.slice(open + 1, close).join("\n")).toContain("+ code new");
  });

  it("marks every line of an unchanged output in the export", async () => {
    backend(() => Promise.resolve(report("one\ntwo")));
    render(<ComparePage runA="older" runB="newer" onBack={vi.fn()} />);
    await userEvent
      .setup()
      .click(await screen.findByRole("button", { name: "Export diff…" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_text_file", {
        content: expect.stringContaining("  one\n  two"),
        suggestedName: expect.any(String),
      }),
    );
  });

  it("keeps a wide table in the AI comparison inside its card", async () => {
    invoke.mockImplementation((command: string, args?: { runId?: string }) => {
      if (command === "get_run_report") return Promise.resolve(report("text"));
      if (command === "get_run_manifest")
        return Promise.resolve(
          manifest(args?.runId ?? "", "2026-08-01T00:00:00Z", "codex"),
        );
      if (command === "get_settings") return Promise.resolve({});
      return Promise.resolve(
        "| Addressed | New |\n| --- | --- |\n| one | two |",
      );
    });
    const view = render(
      <ComparePage runA="older" runB="newer" onBack={vi.fn()} />,
    );
    await userEvent
      .setup()
      .click(
        await screen.findByRole("button", { name: /Compare findings with AI/ }),
      );
    await waitFor(() =>
      expect(
        view.container.querySelector("div.overflow-x-auto > table"),
      ).not.toBeNull(),
    );
  });

  it("renders math in the AI comparison as the Report tab does", async () => {
    invoke.mockImplementation((command: string, args?: { runId?: string }) => {
      if (command === "get_run_report") return Promise.resolve(report("text"));
      if (command === "get_run_manifest")
        return Promise.resolve(
          manifest(args?.runId ?? "", "2026-08-01T00:00:00Z", "codex"),
        );
      if (command === "get_settings") return Promise.resolve({});
      return Promise.resolve("The estimate $x^2$ changed.");
    });
    const user = userEvent.setup();
    const view = render(
      <ComparePage runA="older" runB="newer" onBack={vi.fn()} />,
    );
    await user.click(
      await screen.findByRole("button", { name: /Compare findings with AI/ }),
    );
    await waitFor(() =>
      expect(view.container.querySelector(".katex")).not.toBeNull(),
    );
    expect(screen.queryByText(/\$x\^2\$/)).not.toBeInTheDocument();
  });
});
