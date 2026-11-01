import { describe, expect, it, vi } from "vitest";
import { render, renderHook, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import findingsV2 from "../../tests/fixtures/workflow/findings-v2.json";
import type { ArtifactSelectionRequest } from "../lib/artifactTypes";
import type { PipelineReport } from "../lib/types";
import useArtifactSelection from "./artifact-explorer/useArtifactSelection";
import ReportWorkspace from "./ReportWorkspace";

const invoke = vi.hoisted(() => vi.fn());
const selection = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./ArtifactExplorer", () => ({
  default: ({
    selectionRequest,
  }: {
    selectionRequest: ArtifactSelectionRequest | null;
  }) => {
    selection(selectionRequest);
    return null;
  },
}));

const cited = "context/source-evidence/analysis/main.do";

function pageCitation(pageManifest: Record<string, unknown>) {
  invoke.mockImplementation((command: string) => {
    if (command === "get_annotations") return Promise.resolve("{}");
    if (command === "get_run_manifest")
      return Promise.resolve({ run_id: "run", artifacts: [], ...pageManifest });
    return Promise.reject(new Error(command));
  });
  const report = {
    products: {
      schema_version: 1,
      primary_step_id: "synthesis",
      findings: {
        findings: [
          {
            title: "Sign error",
            severity: "high",
            problem: "The sign flips.",
            what_would_help: "Fix it.",
            evidence: [{ page: 412, quote: "the sign flips" }],
          },
        ],
      },
    },
    step_outputs: [],
  } as unknown as PipelineReport;
  render(<ReportWorkspace runId="run" markdown="# Report" report={report} />);
}

describe("an evidence link to a page", () => {
  const openIssue = async () => {
    const user = userEvent.setup();
    await user.click(await screen.findByRole("tab", { name: /Issues/ }));
    await user.click(
      await screen.findByRole(
        "button",
        { name: /^Sign error/ },
        { timeout: 5_000 },
      ),
    );
    await screen.findByText("Page 412");
    // Lets the manifest request settle before the chip is read.
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("get_run_manifest", {
        runId: "run",
      }),
    );
  };

  it("stays plain text when the run saved no page images", async () => {
    pageCitation({});
    await openIssue();
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: "View Page 412 evidence" }),
      ).not.toBeInTheDocument(),
    );
  });

  it("stays a link when the run saved page images", async () => {
    pageCitation({
      page_artifacts: {
        count: 500,
        digit_width: 3,
        extension: "jpg",
        total_bytes: 1,
      },
    });
    await openIssue();
    expect(
      screen.getByRole("button", { name: "View Page 412 evidence" }),
    ).toBeVisible();
  });
});

describe("an evidence link to a source file", () => {
  it("opens the file at the cited line", async () => {
    invoke.mockImplementation((command: string) =>
      command === "get_annotations"
        ? Promise.resolve("{}")
        : Promise.reject(new Error(command)),
    );
    const report = {
      products: {
        schema_version: 1,
        primary_step_id: "synthesis",
        findings: findingsV2.published,
      },
      step_outputs: findingsV2.step_outputs,
    } as unknown as PipelineReport;
    const user = userEvent.setup();
    render(<ReportWorkspace runId="run" markdown="# Report" report={report} />);

    await user.click(screen.getByRole("tab", { name: /Issues/ }));
    await user.click(
      await screen.findByRole(
        "button",
        { name: /^The sample window/ },
        { timeout: 5_000 },
      ),
    );
    await user.click(
      screen.getByRole("button", {
        name: "View analysis/main.do:120–135 evidence",
      }),
    );

    await waitFor(() =>
      expect(selection).toHaveBeenLastCalledWith(
        expect.objectContaining({ relPath: cited, line: 120 }),
      ),
    );
    const request = selection.mock.lastCall![0];

    invoke.mockImplementation((command: string) =>
      command === "get_run_manifest"
        ? Promise.resolve({
            run_id: "run",
            artifacts: [
              {
                rel_path: cited,
                label: "main.do",
                kind: "text",
                bytes: 10,
                sha256: "",
                group: "context",
              },
            ],
          })
        : Promise.resolve({ kind: "text", text: "", bytes: 0 }),
    );
    const { result } = renderHook(() =>
      useArtifactSelection({
        runId: "run",
        deferInitialArtifact: true,
        preload: undefined,
        selectionRequest: request,
      }),
    );
    await waitFor(() =>
      expect(result.current.fileTab).toMatchObject({ path: cited, line: 120 }),
    );
  });
});
